//! Read DTOs for the established Web surface. No transport or unscoped file access.
use crate::{
    Error, Library, Result, Source, config::scoped_path, document::hash, profile::LanguageProfile,
    store::WriterLock,
};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;

pub fn execute(
    library: &mut Library,
    operation: &str,
    args: &Value,
    private: bool,
) -> Result<Value> {
    if !args.is_object() {
        return Err(Error::Invalid("web arguments must be an object".into()));
    }
    if operation == "web_config" {
        return config(library, args);
    }
    if operation == "web_tones" {
        return Ok(json!({"tones":[
            {"id":"default","label":"Default","label_zh":"默认","icon":"chat"},
            {"id":"caveman","label":"Caveman","label_zh":"原始人","icon":"pets"},
            {"id":"wenyan","label":"文言文","label_zh":"文言文","icon":"history_edu"},
            {"id":"scholar","label":"Scholar","label_zh":"学术","icon":"school"},
            {"id":"eli5","label":"ELI5","label_zh":"幼儿园","icon":"child_care"}
        ]}));
    }
    if operation == "web_refs_plugins" {
        // Historical reference providers are descriptors/permalink resolvers;
        // they are separate from operator-configured ingestion processes.
        return Ok(json!({"plugins":[
            {"id":"cbeta","name":{"en":"CBETA Buddhist Canon","zh":"CBETA 大藏经","ja":"CBETA 大蔵経"}},
            {"id":"wikisource","name":{"en":"Wikisource","zh":"维基文库","ja":"ウィキソース"}},
            {"id":"ctext","name":{"en":"Chinese Text Project","zh":"中国哲学书电子化计划","ja":"中国哲学書電子化計画"}}
        ]}));
    }
    if operation == "web_worker_status" {
        if !private {
            return Err(Error::Scope);
        }
        let jobs = library.jobs()?;
        let active = jobs
            .iter()
            .filter(|j| matches!(j.status, crate::jobs::JobStatus::Running))
            .count();
        return Ok(json!({"busy":active>0,"active_jobs":active,"scope":"native_persistent_jobs"}));
    }
    if operation == "web_trails" {
        let _lock = WriterLock::acquire(&library.config)?;
        let document = read_meta(library, "trails.json")?.unwrap_or(json!({"trails":[]}));
        if !private {
            return if document
                .get("trails")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                Ok(json!({"trails":[]}))
            } else {
                Err(Error::Scope)
            };
        }
        return Ok(document);
    }
    // Validate the indexed snapshot against source bytes before serving any
    // derived DTO. A public→private external edit cannot expose an old revision.
    let sources = {
        let _lock = WriterLock::acquire(&library.config)?;
        live_sources(library, private)?
    };
    let compiled: Vec<&Source> = sources.iter().filter(|s| s.kind == "compiled").collect();
    match operation {
        "web_articles" => articles(library, &compiled, args),
        "web_article" => article(library, &compiled, args, private),
        "web_aliases" => Ok(json!({"aliases":library.maintenance_article_aliases(private)?})),
        "web_collections" => collections(library, &compiled),
        "web_sources" => Ok(json!({"documents":raw_list(&sources)})),
        "web_source" => raw_detail(library, &sources, required(args, "slug")?),
        "web_stats" => stats(library, &sources, private),
        "web_health" => {
            let (report, withheld) = health(library, private)?;
            Ok(json!({"report":report,"withheld":withheld}))
        }
        "web_search" => {
            let query = required(args, "query")?;
            let top_k = args.get("top_k").and_then(Value::as_u64).unwrap_or(10);
            if top_k > 100 {
                return Err(Error::Invalid("top_k must be 0..100".into()));
            }
            let search = library.search(query, top_k as usize, Some("compiled"), private)?;
            let terms = library.normalizer.tokens(query);
            let mut result = serde_json::to_value(search)?;
            for hit in result["results"].as_array_mut().unwrap() {
                if let Some(source) = compiled.iter().find(|s| hit["source_id"] == s.source_id) {
                    hit["summary"] = get(&source.metadata, "summary", json!(""));
                    let normalized = library.normalizer.normalize(&source.content);
                    hit["matched_terms"] = json!(
                        terms
                            .iter()
                            .filter(|term| normalized.contains(term.as_str()))
                            .collect::<Vec<_>>()
                    );
                }
            }
            Ok(json!({"query":query,"results":result["results"]}))
        }
        "web_lint" => library.lint(serde_json::from_value(args.clone())?, private),
        "web_wiki_export" => Ok(
            json!({"articles":compiled.iter().map(|s|(s.slug.clone(),json!({"metadata":s.metadata,"content":body(s)}))).collect::<BTreeMap<_,_>>(),"count":compiled.len()}),
        ),
        "web_taxonomy" => library.taxonomy(&resolve_lang(library, args)?, private),
        "web_xici" => library.xici(&resolve_lang(library, args)?, private),
        "web_entities" => library.entities(private),
        "web_export_article" => {
            library.export_article(required(args, "slug")?, &language(library)?.0, private)
        }
        "web_export_tag" => library.export_tag(required(args, "tag")?, private),
        "web_export_graph" => library.export_graph(
            required(args, "slug")?,
            args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize,
            private,
        ),
        _ => Err(Error::Capability(format!(
            "unknown web read operation: {operation}"
        ))),
    }
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}
fn get(metadata: &Value, key: &str, default: Value) -> Value {
    metadata.get(key).cloned().unwrap_or(default)
}
fn body(source: &Source) -> String {
    source.content[source.body_start..]
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned()
}

pub(crate) fn live_sources(library: &Library, private: bool) -> Result<Vec<Source>> {
    let (count,size):(u64,u64)=library.db.query_row("SELECT count(*),COALESCE(sum(length(r.original)),0) FROM sources s JOIN revisions r ON r.library_id=s.library_id AND r.source_id=s.id AND r.revision=s.revision WHERE s.library_id=?1 AND s.deleted=0 AND (?2 OR s.visibility='public')",rusqlite::params![library.config.library_id,private],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if count > 20_000 || size > MAX_BYTES {
        return Err(Error::Capability(
            "web snapshot exceeds 20000 sources or 64 MiB".into(),
        ));
    }
    let sources = library.list(None, private)?;
    for source in &sources {
        let path = scoped_path(library.config.root_for(&source.kind)?, &source.rel_path)?;
        let bytes = fs::read(path).map_err(|_| {
            Error::Conflict("source changed since indexing; sync before reading".into())
        })?;
        if bytes.len() as u64 > MAX_BYTES || hash(&bytes) != source.content_hash {
            return Err(Error::Conflict(
                "source changed since indexing; sync before reading".into(),
            ));
        }
    }
    Ok(sources)
}

fn read_meta(library: &Library, name: &str) -> Result<Option<Value>> {
    let path = scoped_path(&library.config.meta, name)?;
    match fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
                return Err(Error::Invalid(
                    "web metadata exceeds 4 MiB or is not a file".into(),
                ));
            }
            Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
        }
    }
}

fn summary(source: &Source) -> Value {
    json!({"slug":source.slug,"title":source.title,"summary":get(&source.metadata,"summary",json!("")),"tags":get(&source.metadata,"tags",json!([]))})
}
fn tag_set(value: &Value) -> BTreeSet<String> {
    match value {
        Value::Array(v) => v
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Value::String(s) => BTreeSet::from([s.clone()]),
        _ => BTreeSet::new(),
    }
}
fn articles(library: &Library, sources: &[&Source], args: &Value) -> Result<Value> {
    let tag = args
        .get("tag")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if args.get("lite") == Some(&json!(true)) {
        let Some(index) = read_meta(library, "index.json")? else {
            return Ok(json!({"articles":[],"total":0}));
        };
        let entries = index
            .as_array()
            .ok_or_else(|| Error::Invalid("index.json must be an array".into()))?;
        let visible: BTreeMap<_, _> = sources.iter().map(|s| (s.slug.as_str(), *s)).collect();
        let mut result = Vec::new();
        for entry in entries {
            if let Some(source) = entry
                .get("slug")
                .and_then(Value::as_str)
                .and_then(|slug| visible.get(slug))
                && tag.is_none_or(|tag| tag_set(&source.metadata["tags"]).contains(tag))
            {
                result.push(json!({"slug":source.slug,"title":source.title}));
            }
        }
        return Ok(json!({"total":result.len(),"articles":result}));
    }
    let new_mode = ["limit", "cursor", "tag", "q", "fields"]
        .iter()
        .any(|k| args.get(k).is_some());
    let limit = args
        .get("limit")
        .map(|v| {
            v.as_u64()
                .filter(|n| (1..=1000).contains(n))
                .ok_or_else(|| Error::Invalid("limit must be 1..1000".into()))
        })
        .transpose()?;
    let cursor = args.get("cursor").and_then(Value::as_str).unwrap_or("");
    let query = args
        .get("q")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let fields: BTreeSet<_> = args
        .get("fields")
        .and_then(Value::as_str)
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut selected: Vec<_> = sources
        .iter()
        .copied()
        .filter(|s| s.slug.as_str() > cursor)
        .filter(|s| tag.is_none_or(|tag| tag_set(&s.metadata["tags"]).contains(tag)))
        .filter(|s| {
            query.is_empty()
                || format!(
                    "{} {}",
                    s.title,
                    s.metadata
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                )
                .to_lowercase()
                .contains(&query)
        })
        .collect();
    selected.sort_by(|a, b| a.slug.cmp(&b.slug));
    let has_more = limit.is_some_and(|n| selected.len() > n as usize);
    if let Some(limit) = limit {
        selected.truncate(limit as usize);
    }
    let next = if has_more {
        selected.last().map(|s| s.slug.clone())
    } else {
        None
    };
    let result: Vec<_> = selected
        .iter()
        .map(|s| {
            let mut row = summary(s);
            if !fields.is_empty() {
                row.as_object_mut()
                    .unwrap()
                    .retain(|k, _| fields.contains(k.as_str()));
            }
            row
        })
        .collect();
    if new_mode {
        Ok(
            json!({"total":sources.len(),"count":result.len(),"articles":result,"next_cursor":next,"filters":{"tag":args.get("tag"),"q":args.get("q")}}),
        )
    } else {
        Ok(json!({"articles":result}))
    }
}

fn article(library: &Library, sources: &[&Source], args: &Value, private: bool) -> Result<Value> {
    let requested = required(args, "slug")?;
    scoped_path(&library.config.concepts, &format!("{requested}.md"))?;
    let resolved = library
        .maintenance_resolve_article(requested, private)?
        .ok_or_else(|| Error::NotFound("article".into()))?;
    let source = sources
        .iter()
        .find(|s| s.slug == resolved)
        .ok_or_else(|| Error::NotFound("article".into()))?;
    let content = body(source);
    if args.get("sections") == Some(&json!(true)) {
        return Ok(
            json!({"status":"ok","slug":source.slug,"title":source.title,"sections":crate::compat_kb::parse_sections(&content)}),
        );
    }
    let exported = library.export_article(&resolved, &language(library)?.0, private)?;
    let safe_sources: Vec<_> = source
        .metadata
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| v.is_object())
        .cloned()
        .map(|mut v| {
            if v.get("url").and_then(Value::as_str).is_some_and(|u| {
                !u.is_empty() && !u.starts_with("https://") && !u.starts_with("http://")
            }) {
                v["url"] = json!("");
            }
            v
        })
        .collect();
    Ok(
        json!({"status":"ok","slug":source.slug,"title":source.title,"summary":get(&source.metadata,"summary",json!("")),"tags":get(&source.metadata,"tags",json!([])),"sources":safe_sources,"content":content,"backlinks":exported["backlinks"]}),
    )
}

fn title_case(text: &str) -> String {
    let mut boundary = true;
    let mut result = String::new();
    for c in text.chars() {
        if boundary {
            result.extend(c.to_uppercase());
        } else {
            result.extend(c.to_lowercase());
        }
        boundary = !c.is_alphabetic();
    }
    result
}
fn collections(library: &Library, sources: &[&Source]) -> Result<Value> {
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for source in sources {
        let mut tags = tag_set(&source.metadata["tags"]);
        if tags.is_empty() {
            tags.insert("uncategorized".into());
        }
        for tag in tags {
            groups.entry(tag).or_default().push(json!({"slug":source.slug,"title":source.title,"summary":get(&source.metadata,"summary",json!(""))}));
        }
    }
    Ok(
        json!({"collections":groups.into_iter().map(|(tag,articles)|json!({"id":tag,"label":library.config.settings["collections"][&tag].get("label").cloned().unwrap_or(json!(title_case(&tag))),"count":articles.len(),"articles":articles})).collect::<Vec<_>>()}),
    )
}

fn raw_list(sources: &[Source]) -> Vec<Value> {
    let mut groups: BTreeMap<&str, &Source> = BTreeMap::new();
    for source in sources.iter().filter(|s| s.kind == "raw") {
        let Some((folder, _)) = source.rel_path.split_once('/') else {
            continue;
        };
        if !groups.contains_key(folder) || source.rel_path == format!("{folder}/index.md") {
            groups.insert(folder, source);
        }
    }
    groups
        .into_iter()
        .map(|(folder, s)| {
            let mut entry = s.metadata.clone();
            entry["path"] = json!(folder);
            entry["title"] = get(&s.metadata, "title", json!(folder));
            entry["type"] = get(&s.metadata, "type", json!("unknown"));
            entry["compiled"] = get(&s.metadata, "compiled", json!(false));
            entry["ingested_at"] = get(&s.metadata, "ingested_at", json!(""));
            entry.as_object_mut().unwrap().retain(|key, v| {
                key == "path"
                    || !v.as_str().is_some_and(|s| {
                        s.starts_with('/')
                            || s.starts_with("~/")
                            || s.as_bytes().get(1) == Some(&b':')
                    })
            });
            entry
        })
        .collect()
}
fn raw_detail(library: &Library, sources: &[Source], slug: &str) -> Result<Value> {
    scoped_path(&library.config.raw, &format!("{slug}/index.md"))?;
    let path = format!("{slug}/index.md");
    let source = sources
        .iter()
        .find(|s| s.kind == "raw" && s.rel_path == path)
        .ok_or_else(|| Error::NotFound("raw document".into()))?;
    let cap = match library
        .config
        .settings
        .get("sources")
        .and_then(|v| v.get("max_content_chars"))
    {
        Some(Value::Null) => 500_000,
        Some(v) => v
            .as_i64()
            .map(|n| n.clamp(0, 500_000) as usize)
            .unwrap_or(50_000),
        None => 50_000,
    };
    let metadata: BTreeMap<_, _> = source
        .metadata
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            (
                k,
                match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(true) => "True".into(),
                    Value::Bool(false) => "False".into(),
                    Value::Null => "None".into(),
                    _ => v.to_string(),
                },
            )
        })
        .collect();
    Ok(
        json!({"slug":slug,"title":source.title,"type":get(&source.metadata,"type",json!("unknown")),"compiled":get(&source.metadata,"compiled",json!(false)),"content":body(source).chars().take(cap).collect::<String>(),"metadata":metadata}),
    )
}

fn health(library: &Library, private: bool) -> Result<(Value, bool)> {
    let Some(report) = read_meta(library, "health.json")? else {
        return Ok((Value::Null, false));
    };
    if !private {
        let hidden:u64=library.db.query_row("SELECT count(*) FROM revisions WHERE library_id=?1 AND ((json_type(metadata,'$.private') IS NOT NULL AND json_type(metadata,'$.private') NOT IN ('null','false')) OR (json_type(metadata,'$.visibility') IS NOT NULL AND (json_type(metadata,'$.visibility')!='text' OR json_extract(metadata,'$.visibility')!='public')))",[&library.config.library_id],|r|r.get(0))?;
        if hidden > 0 {
            return Ok((Value::Null, true));
        }
    }
    Ok((report, false))
}
fn stats(library: &Library, sources: &[Source], private: bool) -> Result<Value> {
    let articles: Vec<_> = sources.iter().filter(|s| s.kind == "compiled").collect();
    let raw: BTreeSet<_> = sources
        .iter()
        .filter(|s| s.kind == "raw")
        .map(|s| s.rel_path.split('/').next().unwrap())
        .collect();
    let outputs = sources.iter().filter(|s| s.kind == "note").count();
    let link = Regex::new(r"\[\[[^\]]+\]\]").map_err(|_| Error::Invalid("link regex".into()))?;
    let words: usize = articles
        .iter()
        .map(|s| s.content.split_whitespace().count())
        .sum();
    let links: usize = articles
        .iter()
        .map(|s| link.find_iter(&s.content).count())
        .sum();
    let (report, withheld) = health(library, private)?;
    let score = if articles.is_empty() || withheld {
        0
    } else {
        100 - report["results"]["total_issues"]
            .as_u64()
            .unwrap_or(0)
            .min(100)
    };
    Ok(
        json!({"raw_count":raw.len(),"article_count":articles.len(),"output_count":outputs,"total_words":words,"link_count":links,"health_score":score,"health_status":if withheld{"private_cache_withheld"}else if report.is_null(){"not_checked"}else{"cached"}}),
    )
}

fn config(library: &Library, args: &Value) -> Result<Value> {
    let branding = &library.config.settings["branding"];
    let languages = if args.get("field") == Some(&json!("branding")) {
        Value::Null
    } else {
        language(library)?.1
    };
    Ok(
        json!({"branding":{"name":get(branding,"name",json!("LLMBase")),"nameShort":get(branding,"name_short",json!("L")),"tagline":get(branding,"tagline",json!("Knowledge Base")),"poweredBy":{"label":get(branding,"powered_by_label",json!("Powered by LLMBase")),"url":get(branding,"powered_by_url",json!("https://github.com/Hosuke/llmbase"))}},"languages":languages}),
    )
}

/// Existing Web routes retain legacy defaults; native ask keeps English.
fn language(library: &Library) -> Result<(LanguageProfile, Value)> {
    let mut settings = library.config.settings.clone();
    if settings
        .get("languages")
        .is_none_or(|value| value.is_null() || value.as_object().is_some_and(|map| map.is_empty()))
    {
        settings["languages"] = json!({"default":"trilingual","profiles":{"trilingual":{"default_lang":"zh-en","api_default_lang":"zh","sections":[{"code":"en","key":"english","header":"## English","label":"English","icon":"EN","title_hint":"latin"},{"code":"zh","key":"中文","header":"## 中文","label":"中文","icon":"中","title_hint":"cjk"},{"code":"ja","key":"日本語","header":"## 日本語","label":"日本語","icon":"日","title_hint":"cjk"}],"views":[{"code":"zh-en","show":["en","zh"],"label":"中英双语","icon":"双","title_hint":"full","primary":"zh"}]}}});
    }
    let name = settings["languages"]["default"]
        .as_str()
        .ok_or_else(|| Error::Invalid("languages.default missing".into()))?
        .to_owned();
    let raw = &mut settings["languages"]["profiles"][&name];
    let sections = raw
        .get_mut("sections")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::Invalid("language sections missing".into()))?;
    for s in sections.iter_mut() {
        if s.get("code").is_none() {
            s["code"] = s["key"].clone();
        }
    }
    let first = sections
        .first()
        .and_then(|s| s.get("code"))
        .cloned()
        .ok_or_else(|| Error::Invalid("language sections are empty".into()))?;
    if raw.get("api_default_lang").is_none() {
        raw["api_default_lang"] = first;
    }
    let profile = LanguageProfile::from_config(&settings)?;
    let mut codes = Vec::new();
    let mut sections = Vec::new();
    for section in &profile.sections {
        codes.push(section.code.clone());
        let label = section
            .extra
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or(&section.key);
        sections.push(json!({"code":section.code,"key":section.key,"header":section.header,"label":label,"icon":section.extra.get("icon").cloned().unwrap_or(json!(label.chars().take(1).collect::<String>())),"title_hint":section.extra.get("title_hint").cloned().unwrap_or(json!("full"))}));
    }
    let section_codes: HashSet<_> = codes.iter().cloned().collect();
    let mut views = Vec::new();
    for view in &profile.views {
        let code = view
            .get("code")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .ok_or_else(|| Error::Invalid("language view code missing".into()))?;
        let show = view
            .get("show")
            .and_then(Value::as_array)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Error::Invalid("language view show missing".into()))?;
        if codes.iter().any(|c| c == code)
            || show
                .iter()
                .any(|c| !c.as_str().is_some_and(|c| section_codes.contains(c)))
        {
            return Err(Error::Invalid("invalid language view codes".into()));
        }
        let primary = view
            .get("primary")
            .cloned()
            .unwrap_or_else(|| show[0].clone());
        if !primary.as_str().is_some_and(|c| section_codes.contains(c)) {
            return Err(Error::Invalid("invalid view primary".into()));
        }
        let label = view.get("label").and_then(Value::as_str).unwrap_or(code);
        views.push(json!({"code":code,"show":show,"label":label,"icon":view.get("icon").cloned().unwrap_or(json!(label.chars().take(1).collect::<String>())) ,"title_hint":get(view,"title_hint",json!("full")),"primary":primary}));
        codes.push(code.to_owned());
    }
    if !codes.contains(&profile.default_lang) || !codes.contains(&profile.api_default_lang) {
        return Err(Error::Invalid(
            "language defaults must name a section or view".into(),
        ));
    }
    let dto = json!({"name":name,"sections":sections,"views":views,"codes":codes,"default_lang":profile.default_lang,"api_default_lang":profile.api_default_lang,"single_section":profile.sections.len()==1,"bare_body":profile.bare_body()});
    Ok((profile, dto))
}
fn resolve_lang(library: &Library, args: &Value) -> Result<String> {
    let (_, profile) = language(library)?;
    let lang = args
        .get("lang")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(profile["api_default_lang"].as_str().unwrap());
    if !profile["codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == lang)
    {
        return Err(Error::Invalid("invalid lang".into()));
    }
    Ok(lang.to_owned())
}
