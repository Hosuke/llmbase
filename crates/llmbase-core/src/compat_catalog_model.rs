//! Local compatibility catalog sequence. Python supplies live callables, never storage logic.
use crate::{Error, Library, LibraryConfig, Result, Source, document::hash,
    maintenance_model::{inputs, check_inputs, read_cache, fingerprint, now, finish},
    store::{WriterLock, WriterSession}, recovery::SourceGuard};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn text<'a>(v: &'a Value, key: &str) -> &'a str { v.get(key).and_then(Value::as_str).unwrap_or("") }
fn array(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }
fn strings(v: &Value) -> Vec<String> { v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect() }
fn bound(v: &Value) -> Result<()> {
    if serde_json::to_vec(v)?.len() > 16*1024*1024 { return Err(Error::Invalid("catalog input exceeds 16 MiB".into())); }
    fn depth(v:&Value,n:usize)->Result<()> { if n>64{return Err(Error::Invalid("catalog nesting exceeds 64".into()))} match v {Value::Array(a)=>for v in a {depth(v,n+1)?},Value::Object(o)=>for v in o.values(){depth(v,n+1)?},_=>{}} Ok(()) }
    depth(v,0)
}
fn empty_entities()->Value { json!({"people":[],"events":[],"places":[]}) }
fn other(slugs:Vec<Value>)->Value {json!({"id":"other","label":{"en":"Other","zh":"其他","ja":"その他"},"children":[],"article_slugs":slugs})}
fn add_other(nodes:&mut Vec<Value>,slug:Value){
    if let Some(n)=nodes.iter_mut().find(|n|matches!(text(n,"id"),"other"|"其他")){append_slug(n,slug)}else{nodes.push(other(vec![slug]))}
}
fn append_slug(n:&mut Value,slug:Value){if !n["article_slugs"].is_array(){n["article_slugs"]=json!([])}n["article_slugs"].as_array_mut().unwrap().push(slug)}
fn labels(nodes:&mut [Value],keys:&[String]){
    for n in nodes {
        let id=n.get("id").cloned().unwrap_or(json!(""));
        if n["label"].is_object(){
            let fallback=keys.iter().find_map(|k|n["label"].get(k).and_then(Value::as_str).filter(|s|!s.is_empty())).map(|s|json!(s)).unwrap_or(id.clone());
            for k in keys {if n["label"].get(k).is_none(){n["label"][k]=fallback.clone()} if !n["label"][k].as_str().is_some_and(|s|!s.is_empty()){n["label"][k]=id.clone()}}
        }else{
            let label=n.get("label").cloned().unwrap_or(Value::Null);
            let value=if crate::compat_compile::truthy(&label){match label {Value::String(_)=>label,_=>json!(crate::compat_kb::python_string(&label))}}else{n.get("id").cloned().unwrap_or(json!("unknown"))};
            n["label"]=json!({});for k in keys {n["label"][k]=value.clone()}
        }
        let children=if n["children"].is_object(){vec![n["children"].clone()]}else{array(&n["children"])};
        let mut children:Vec<_>=children.into_iter().filter(Value::is_object).collect();labels(&mut children,keys);n["children"]=json!(children);
    }
}
fn parse(text:&str,kind:&str,keys:&[String])->Value{
    let (left,right)=if kind=="taxonomy"{('[',']')}else{('{','}')};
    let Some(start)=text.find(left) else{return if kind=="taxonomy"{Value::Null}else{empty_entities()}};
    let Some(end)=text.rfind(right) else{return if kind=="taxonomy"{Value::Null}else{empty_entities()}};
    let mut v:Value=if end>=start{serde_json::from_str(&text[start..=end]).unwrap_or(Value::Null)}else{Value::Null};
    if kind=="taxonomy" {
        let Some(nodes)=v.as_array_mut() else{return Value::Null};
        if nodes.iter().any(|n|!n.is_object()||n.get("id").is_none()||n.get("label").is_none()){return Value::Null}
        labels(nodes,keys);v
    }else{
        if !v.is_object(){return empty_entities()}
        for key in ["people","events","places"]{if !v[key].is_array(){v[key]=json!([])}}v
    }
}
fn dedup(nodes:&mut [Value],seen:&mut BTreeSet<String>,children_first:bool){
    for n in nodes {
        if children_first && let Some(c)=n["children"].as_array_mut(){dedup(c,seen,children_first)}
        n["article_slugs"]=json!(strings(&n["article_slugs"]).into_iter().filter(|s|seen.insert(s.clone())).collect::<Vec<_>>());
        if !children_first && let Some(c)=n["children"].as_array_mut(){dedup(c,seen,children_first)}
    }
}
fn assigned(nodes:&[Value],out:&mut BTreeSet<String>){for n in nodes {out.extend(strings(&n["article_slugs"]));assigned(&array(&n["children"]),out)}}
fn ensure(nodes:&mut Vec<Value>,articles:&[Value]){
    let mut seen=BTreeSet::new();assigned(nodes,&mut seen);
    let missing:BTreeSet<_>=articles.iter().map(|a|text(a,"slug").to_owned()).filter(|s|!seen.contains(s)).collect();
    for s in missing{add_other(nodes,json!(s))}dedup(nodes,&mut BTreeSet::new(),false)
}
fn counts(articles:&[Value],normalize:bool)->Vec<(String,usize)>{
    let mut counts=Vec::<(String,usize)>::new();
    for a in articles {for tag in strings(&a["tags"]){let t=if normalize{tag.to_lowercase().replace(' ',"-")}else{tag};if let Some((_,n))=counts.iter_mut().find(|(k,_)|k==&t){*n+=1}else{counts.push((t,1))}}}
    counts.sort_by(|a,b|b.1.cmp(&a.1));counts
}
fn fallback(articles:&[Value])->Vec<Value>{
    if articles.is_empty(){return vec![]}
    let top:Vec<_>=counts(articles,true).into_iter().take(10).filter(|(_,c)|*c>=2).map(|(t,_)|t).collect();
    if top.is_empty(){return vec![json!({"id":"all","label":{"en":"All Articles","zh":"所有文章","ja":"すべての記事"},"children":[],"article_slugs":articles.iter().map(|a|a["slug"].clone()).collect::<Vec<_>>()})]}
    let mut used=BTreeSet::new();let mut nodes=Vec::new();
    for tag in top {let slugs:Vec<_>=articles.iter().filter(|a|!used.contains(text(a,"slug"))&&strings(&a["tags"]).iter().any(|t|t.to_lowercase().replace(' ',"-")==tag)).map(|a|a["slug"].clone()).collect();for s in &slugs{used.insert(s.as_str().unwrap_or("").to_owned());}if !slugs.is_empty(){nodes.push(json!({"id":tag,"label":{"en":tag.replace('-'," "),"zh":tag.replace('-'," "),"ja":tag.replace('-'," ")},"children":[],"article_slugs":slugs}))}}
    let rest:Vec<_>=articles.iter().filter(|a|!used.contains(text(a,"slug"))).map(|a|a["slug"].clone()).collect();if !rest.is_empty(){nodes.push(other(rest))}nodes
}
fn localized_title(title:&str,lang:&str)->String{
    if lang=="zh-en"||!title.contains('/'){return title.into()}
    let parts:Vec<_>=title.split('/').map(str::trim).filter(|p|!p.is_empty()).collect();if parts.len()<2{return title.into()}
    let cjk=|s:&str|s.chars().any(|c|matches!(c,'\u{3400}'..='\u{4dbf}'|'\u{4e00}'..='\u{9fff}'|'\u{3040}'..='\u{30ff}'));
    if matches!(lang,"zh"|"ja"){parts.iter().find(|p|cjk(p)).unwrap_or(parts.last().unwrap()).to_string()}else{parts.iter().find(|p|!cjk(p)).unwrap_or(&parts[0]).to_string()}
}
fn localize(nodes:&[Value],lang:&str,titles:&Value)->Vec<Value>{nodes.iter().map(|n|{
    let children=localize(&array(&n["children"]),lang,titles);let slugs=strings(&n["article_slugs"]);
    let articles:Vec<_>=slugs.iter().map(|s|json!({"slug":s,"title":localized_title(titles.get(s).and_then(Value::as_str).unwrap_or(s),lang)})).collect();
    let label=if n["label"].is_string(){n["label"].clone()}else{n["label"].get(lang).or_else(||n["label"].get("en")).or_else(||n["label"].get("zh")).unwrap_or(&n["id"]).clone()};
    json!({"id":n["id"],"label":label,"count":articles.len(),"total":articles.len()+children.iter().map(|c|c["total"].as_u64().unwrap_or(0) as usize).sum::<usize>(),"articles":articles,"children":children})
}).collect()}
type MatchIndex=Vec<(String,Vec<usize>,usize)>;
fn index_nodes(nodes:&[Value],path:&[usize],tags:&mut MatchIndex,keywords:&mut MatchIndex){
    for (i,n) in nodes.iter().enumerate(){let mut p=path.to_vec();p.push(i);for (field,out) in [("match_tags",&mut *tags),("match_title_keywords",&mut *keywords)]{for word in strings(&n[field]){let key=word.to_lowercase();if let Some(entry)=out.iter_mut().find(|e|e.0==key){if path.len()>entry.2{*entry=(key,p.clone(),path.len())}}else{out.push((key,p.clone(),path.len()))}}}index_nodes(&array(&n["children"]),&p,tags,keywords)}
}
fn at<'a>(nodes:&'a mut [Value],path:&[usize])->&'a mut Value {let n=&mut nodes[path[0]];if path.len()==1{n}else{at(n["children"].as_array_mut().unwrap(),&path[1..])}}
fn assign(nodes:&mut Vec<Value>,articles:&[Value]){
    let(mut tags,mut keywords)=(Vec::new(),Vec::new());index_nodes(nodes,&[],&mut tags,&mut keywords);
    let mut other_slugs=Vec::new();for a in articles {let mut best=None;let mut score=0;let title=text(a,"title").to_lowercase();for(k,p,d)in &keywords{if title.contains(k)&&100+d>score{best=Some(p);score=100+d}}
        if score<100{for tag in strings(&a["tags"]){let t=tag.to_lowercase();if t.starts_with("category:"){continue}if let Some((_,p,d))=tags.iter().find(|(k,_,_)|k==&t)&&*d>score{score=*d;best=Some(p)}}}
        if let Some(p)=best{append_slug(at(nodes,p),a["slug"].clone())}else{other_slugs.push(a["slug"].clone())}}
    if !other_slugs.is_empty(){nodes.push(other(other_slugs))}
    fn clean(nodes:&mut [Value]){for n in nodes{n.as_object_mut().unwrap().remove("match_tags");n.as_object_mut().unwrap().remove("match_title_keywords");if !n["article_slugs"].is_array(){n["article_slugs"]=json!([])}if let Some(c)=n["children"].as_array_mut(){clean(c)}}}clean(nodes)
}
fn entity_dedup(v:&Value)->Value{
    let entries=array(v).into_iter().filter(Value::is_object).map(|mut n|{if !n["articles"].is_array(){n["articles"]=json!([])}n}).collect();json!(crate::entities_model::dedup_entities(entries))
}
/// Pure helpers never initialize storage or configuration.
pub fn transform(action:&str,args:&Value)->Result<Value>{
    bound(args)?;let articles=array(&args["articles"]);let mut tree=array(&args["tree"]);let keys=args.get("label_keys").map(strings).unwrap_or(vec!["en".into(),"zh".into(),"ja".into()]);
    Ok(match action {
        "parse_taxonomy"=>parse(text(args,"text"),"taxonomy",&keys),
        "parse_entities"=>parse(text(args,"text"),"entities",&keys),
        "dedup_entities"=>entity_dedup(&args["entities"]),
        "fix_labels"=>{labels(&mut tree,&keys);json!(tree)},
        "dedup_tree"=>{let mut seen: BTreeSet<_>=strings(&args["seen"]).into_iter().collect();dedup(&mut tree,&mut seen,true);json!({"tree":tree,"seen":seen})},
        "ensure_assignment"=>{ensure(&mut tree,&articles);json!(tree)},
        "assign_tree"=>{assign(&mut tree,&articles);json!(tree)},
        "add_other"=>{add_other(&mut tree,args["slug"].clone());json!(tree)},
        "fallback"=>json!(fallback(&articles)),
        "localize_title"=>json!(localized_title(text(args,"title"),text(args,"lang"))),
        "localize_tree"=>json!(localize(&tree,text(args,"lang"),&args["titles"])),
        "entity_lines"=>json!(articles.iter().map(|a|if articles.len()<=80{format!("- {}: {} | {}",text(a,"slug"),text(a,"title"),text(a,"summary"))}else{format!("- {}: {} | tags: {}",text(a,"slug"),text(a,"title"),strings(&a["tags"]).into_iter().take(3).collect::<Vec<_>>().join(", "))}).collect::<Vec<_>>()),
        "taxonomy_lines"=>json!(articles.iter().map(|a|{let tags=strings(&a["tags"]);format!("- {} | {} | {}",text(a,"slug"),text(a,"title"),if tags.is_empty(){"none".into()}else{tags.into_iter().take(5).collect::<Vec<_>>().join(", ")})}).collect::<Vec<_>>()),
        "taxonomy_summary"=>taxonomy_summary(&articles),
        "xici_context"=>xici_context(&articles),
        _=>return Err(Error::Capability("unknown catalog transform".into()))
    })
}
fn taxonomy_summary(articles:&[Value])->Value{
    let mut frequencies=Vec::<(String,usize,Vec<String>)>::new();for a in articles{for tag in strings(&a["tags"]){let tag=tag.to_lowercase();if tag.starts_with("category:"){continue}if let Some((_,count,titles))=frequencies.iter_mut().find(|x|x.0==tag){*count+=1;if titles.len()<2{titles.push(text(a,"title").into())}}else{frequencies.push((tag,1,vec![text(a,"title").into()]))}}}
    frequencies.sort_by(|a,b|b.1.cmp(&a.1));let mut lines=Vec::new();for(t,n,s)in frequencies{if n<2&&lines.len()>=60{break}lines.push(format!("- {t} ({n}): {}",s.join("; ")))}json!({"tag_text":lines.join("\n"),"titles":articles.iter().map(|a|a["title"].clone()).collect::<Vec<_>>()})
}
fn xici_context(articles:&[Value])->Value{
    let themes:Vec<_>=counts(articles,false).into_iter().take(7).map(|(t,_)|t).collect();
    let overview=if articles.len()<=80{articles.iter().map(|a|format!("- {}: {}",text(a,"title"),text(a,"summary"))).collect::<Vec<_>>().join("\n")}else{
        let mut groups=Vec::<(String,Vec<String>)>::new();for a in articles{for tag in strings(&a["tags"]).into_iter().take(3){if tag.starts_with("category:"){continue}if let Some((_,titles))=groups.iter_mut().find(|x|x.0==tag){if titles.len()<4{titles.push(text(a,"title").into())}}else{groups.push((tag,vec![text(a,"title").into()]))}}}groups.sort_by(|a,b|b.1.len().cmp(&a.1.len()));format!("Knowledge base has {} articles across these themes:\n{}",articles.len(),groups.into_iter().take(15).map(|(t,s)|format!("- {t} ({} articles): {}",s.len(),s.into_iter().take(3).collect::<Vec<_>>().join(", "))).collect::<Vec<_>>().join("\n"))};
    json!({"themes":themes,"overview":overview})
}
