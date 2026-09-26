//! Pure compatibility primitives. Offsets here are Unicode scalar indices,
//! matching Python strings; indexed evidence keeps its separate UTF-8 byte unit.
use crate::{Error, Result, compat_kb};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::sync::LazyLock;

fn field<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}
fn whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

struct ParagraphPatterns {
    heading: regex::Regex,
    list: regex::Regex,
    thematic: regex::Regex,
    html: regex::Regex,
    reference: regex::Regex,
    fence_open: regex::Regex,
    fence_close: regex::Regex,
}

static PARAGRAPH_PATTERNS: LazyLock<ParagraphPatterns> = LazyLock::new(|| {
    let pattern = |s| regex::Regex::new(s).expect("constant paragraph pattern");
    ParagraphPatterns {
        heading: pattern(r"^ {0,3}#{1,6}(?:[ \t]|$)"),
        list: pattern(r"^ {0,3}(?:[-*+]|\d{1,9}[.)])[\s\x1c-\x1f]"),
        thematic: pattern(r"^ {0,3}(?:-[ -]{2,}|\*[ *]{2,}|_[ _]{2,})[\s\x1c-\x1f]*$"),
        html: pattern(concat!(
            r"^ {0,3}(?:<(?:script|pre|style|textarea)(?:[\s\x1c-\x1f]|>|$)",
            r"|<!--|<\?|<![a-z]|<!\[cdata\[",
            r"|</?(?:address|article|aside|base|basefont|blockquote|body|caption|center|",
            r"col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|",
            r"footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|",
            r"link|main|menu|menuitem|nav|noframes|ol|optgroup|option|p|param|section|",
            r"source|summary|table|tbody|td|tfoot|th|thead|title|tr|track|ul)",
            r"(?:[\s\x1c-\x1f]|/?>|$))"
        )),
        reference: pattern(r"^ {0,3}\[[^\]\n]+\]:[\s\x1c-\x1f]*[^\s\x1c-\x1f]+"),
        fence_open: pattern(r"^ {0,3}(`{3,}|~{3,})"),
        fence_close: pattern(r"^ {0,3}([`~]{3,})[\s\x1c-\x1f]*$"),
    }
});

impl ParagraphPatterns {
    fn html(&self, line: &str) -> bool {
        // Python IGNORECASE for the ASCII tag patterns also folds these four
        // Unicode letters; Rust regex's case folding is not identical.
        let folded: String = line
            .chars()
            .map(|c| match c {
                'İ' | 'ı' => 'i',
                'ſ' => 's',
                'K' => 'k',
                _ => c.to_ascii_lowercase(),
            })
            .collect();
        self.html.is_match(&folded)
    }

    fn structural(&self, line: &str) -> bool {
        let stripped = line.trim_start_matches(whitespace);
        self.heading.is_match(line)
            || self.list.is_match(line)
            || stripped.starts_with(['>', '|'])
            || self.thematic.is_match(line)
            || self.html(line)
            || self.reference.is_match(line)
            || self.fence_open.is_match(line)
    }

    fn container(&self, line: &str) -> bool {
        self.list.is_match(line)
            || line.trim_start_matches(whitespace).starts_with('>')
            || self.html(line)
    }
}

/// Python str.splitlines without keepends. CRLF is one delimiter; a terminal
/// delimiter does not add an empty item. U+001F is whitespace, not a delimiter.
fn paragraph_lines(body: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut chars = body.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            result.push(&body[start..index]);
            start = index + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                chars.next();
                start += 1;
            }
        }
    }
    if start < body.len() {
        result.push(&body[start..]);
    }
    result
}

fn paragraph_indented(line: &str) -> bool {
    line.starts_with("    ") || line.starts_with('\t')
}

fn normalize_paragraphs(body: &str, terminators: &str, wrappers: &str) -> String {
    let patterns = &*PARAGRAPH_PATTERNS;
    let nl = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = Vec::<String>::new();
    let mut fence: Option<&str> = None;
    let mut container = false;
    let mut indented = false;
    let mut continuation_pending = false;
    for line in paragraph_lines(body) {
        if let Some(opener) = fence {
            out.push(line.to_owned());
            if let Some(m) = patterns.fence_close.captures(line) {
                let closer = m.get(1).unwrap().as_str();
                if closer.as_bytes()[0] == opener.as_bytes()[0] && closer.len() >= opener.len() {
                    fence = None;
                }
            }
            continue;
        }
        if let Some(m) = patterns.fence_open.captures(line) {
            out.push(line.to_owned());
            fence = Some(m.get(1).unwrap().as_str());
            indented = false;
            continue;
        }
        if line.trim_matches(whitespace).is_empty() {
            out.push(line.to_owned());
            if container {
                continuation_pending = true;
            }
            container = false;
            indented = false;
            continue;
        }
        if continuation_pending {
            continuation_pending = false;
            if line.starts_with([' ', '\t']) {
                container = true;
            }
        }
        if paragraph_indented(line)
            && (indented
                || out.last().is_none_or(|prev| {
                    prev.trim_matches(whitespace).is_empty() || patterns.structural(prev)
                }))
        {
            out.push(line.to_owned());
            indented = true;
            continue;
        }
        indented = false;
        if patterns.structural(line) {
            out.push(line.to_owned());
            container = patterns.container(line);
            continue;
        }
        let Some(prev) = out.last_mut() else {
            out.push(line.to_owned());
            continue;
        };
        let tail = prev
            .trim_end_matches(whitespace)
            .trim_end_matches(|c| wrappers.contains(c));
        let terminated = tail.chars().last().is_none_or(|c| terminators.contains(c));
        if container
            || prev.trim_matches(whitespace).is_empty()
            || patterns.structural(prev)
            || paragraph_indented(prev)
            || terminated
        {
            out.push(line.to_owned());
        } else {
            *prev = format!(
                "{}{}",
                prev.trim_end_matches(whitespace),
                line.trim_start_matches(whitespace)
            );
        }
    }
    let mut trail = 0;
    let mut original = body;
    while let Some(prefix) = original
        .strip_suffix("\r\n")
        .or_else(|| original.strip_suffix('\n'))
    {
        original = prefix;
        trail += 1;
    }
    let joined = out.join(nl);
    format!("{}{}", joined.trim_end_matches(nl), nl.repeat(trail))
}

fn headings(body: &str) -> Vec<Value> {
    fn flatten(nodes: &Value, output: &mut Vec<Value>) {
        for node in nodes.as_array().unwrap() {
            output.push(json!([node["level"], node["start"], node["title"]]));
            flatten(&node["children"], output);
        }
    }
    let mut result = Vec::new();
    flatten(&compat_kb::parse_sections(body), &mut result);
    result
}

static HEAD_PATTERNS: LazyLock<(regex::Regex, regex::Regex)> = LazyLock::new(|| {
    (
        regex::Regex::new(
            r"^ {0,3}(#{1,6})[\s\x1c-\x1f]+(.+?)(?:[\s\x1c-\x1f]+#+)?[\s\x1c-\x1f]*$",
        )
        .expect("constant heading pattern"),
        regex::Regex::new(
            r"^( {0,3})(#{1,6})([\s\x1c-\x1f]+)(.+?)((?:[\s\x1c-\x1f]+#+)?)[\s\x1c-\x1f]*$",
        )
        .expect("constant heading rewrite pattern"),
    )
});

/// Normalize headings with a caller-supplied title classifier. The scanner and
/// rewrite are native; a compatibility host may supply Python regex matching.
pub fn normalize_heads_with(
    body: &str,
    enabled: bool,
    mut level_for: impl FnMut(&str) -> Result<Option<u8>>,
) -> Result<String> {
    if body.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid("heading body exceeds 16 MiB".into()));
    }
    if !enabled {
        return Ok(body.to_owned());
    }
    let patterns = &*PARAGRAPH_PATTERNS;
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    let mut html = false;
    for line in paragraph_lines(body) {
        let mut rewritten = None;
        if let Some(opener) = fence {
            if let Some(m) = patterns.fence_close.captures(line) {
                let closer = m.get(1).unwrap().as_str();
                if closer.as_bytes()[0] == opener.as_bytes()[0] && closer.len() >= opener.len() {
                    fence = None;
                }
            }
        } else if let Some(m) = patterns.fence_open.captures(line) {
            fence = Some(m.get(1).unwrap().as_str());
            html = false;
        } else if line.trim_matches(whitespace).is_empty() {
            html = false;
        } else if !html {
            if patterns.html(line) {
                html = true;
            } else if let Some(m) = HEAD_PATTERNS.0.captures(line) {
                let title = m.get(2).unwrap().as_str().trim_matches(whitespace);
                if let Some(level) = level_for(title)? {
                    if !(1..=6).contains(&level) {
                        return Err(Error::Invalid("heading level must be 1..6".into()));
                    }
                    if let Some(parts) = HEAD_PATTERNS.1.captures(line) {
                        rewritten = Some(format!(
                            "{}{}{}{}{}",
                            &parts[1],
                            "#".repeat(usize::from(level)),
                            &parts[3],
                            &parts[4],
                            &parts[5],
                        ));
                    }
                }
            }
        }
        out.push(rewritten.unwrap_or_else(|| line.to_owned()));
    }
    let nl = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let mut trail = 0;
    let mut original = body;
    while let Some(prefix) = original
        .strip_suffix("\r\n")
        .or_else(|| original.strip_suffix('\n'))
    {
        original = prefix;
        trail += 1;
    }
    let joined = out.join(nl);
    Ok(format!(
        "{}{}",
        joined.trim_end_matches(nl),
        nl.repeat(trail)
    ))
}

fn normalize_heads(args: &Value) -> Result<Value> {
    let body = field(args, "body")?;
    let rules = args
        .get("rules")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("heading rules must be an array".into()))?;
    if rules.len() > 256 {
        return Err(Error::Invalid("heading rules exceed 256".into()));
    }
    let mut compiled = Vec::with_capacity(rules.len());
    let mut pattern_bytes = 0;
    for rule in rules {
        let level = rule
            .get("level")
            .and_then(Value::as_u64)
            .filter(|level| (1..=6).contains(level))
            .ok_or_else(|| Error::Invalid("heading level must be an integer 1..6".into()))?;
        let pattern = field(rule, "pattern")?;
        pattern_bytes += pattern.len();
        if pattern_bytes > 64 * 1024 {
            return Err(Error::Invalid("heading patterns exceed 64 KiB".into()));
        }
        let regex = regex::RegexBuilder::new(pattern)
            .size_limit(1024 * 1024)
            .build()
            .map_err(|err| Error::Invalid(format!("invalid native heading regex: {err}")))?;
        compiled.push((regex, level as u8));
    }
    Ok(json!(normalize_heads_with(
        body,
        !compiled.is_empty(),
        |title| {
            Ok(compiled
                .iter()
                .find(|(pattern, _)| pattern.is_match(title))
                .map(|(_, level)| *level))
        }
    )?))
}

fn find_path(nodes: &Value, anchor: &str, path: &mut Vec<usize>) -> Result<bool> {
    let nodes = nodes
        .as_array()
        .ok_or_else(|| Error::Invalid("sections must be an array".into()))?;
    for (index, node) in nodes.iter().enumerate() {
        path.push(index);
        if node.get("anchor") == Some(&json!(anchor)) {
            return Ok(true);
        }
        if find_path(node.get("children").unwrap_or(&json!([])), anchor, path)? {
            return Ok(true);
        }
        path.pop();
    }
    Ok(false)
}

fn normalize_span(content: &str, level: &str) -> Result<(String, Vec<usize>)> {
    if !matches!(level, "none" | "punct" | "punct_spaces") {
        return Err(Error::Invalid(format!("unknown normalize level: {level}")));
    }
    const PUNCT: &str = "！。，、；：？「」『』（）〔〕【】《》〈〉·・—–―…!.,;:?()[]{}\"'";
    let mut text = String::new();
    let mut mapping = Vec::new();
    for (index, ch) in content.chars().enumerate() {
        if level != "none" && (PUNCT.contains(ch) || level == "punct_spaces" && whitespace(ch)) {
            continue;
        }
        text.push(ch);
        mapping.push(index);
    }
    Ok((text, mapping))
}

fn locate(args: &Value) -> Result<Value> {
    let content = field(args, "content")?;
    let head = field(args, "head")?;
    let tail = args.get("tail").and_then(Value::as_str).unwrap_or("");
    if head.is_empty() {
        return Err(Error::Invalid("head must not be empty".into()));
    }
    if content.is_empty() {
        return Ok(Value::Null);
    }
    let level = args
        .get("normalize")
        .and_then(Value::as_str)
        .unwrap_or("punct_spaces");
    let (normalized, map) = normalize_span(content, level)?;
    let (head, _) = normalize_span(head, level)?;
    if head.is_empty() {
        return Ok(Value::Null);
    }
    let Some(start) = normalized.find(&head) else {
        return Ok(Value::Null);
    };
    let start_original = map[normalized[..start].chars().count()];
    if !tail.is_empty() {
        let (tail, _) = normalize_span(tail, level)?;
        if !tail.is_empty()
            && let Some(offset) = normalized[start + head.len()..].find(&tail)
        {
            let end_byte = start + head.len() + offset + tail.len();
            let end_original = map[normalized[..end_byte].chars().count() - 1] + 1;
            return Ok(json!({"start":start_original,"end":end_original,"strategy":"exact"}));
        }
    }
    Ok(json!({"start":start_original,"end":content.chars().count(),"strategy":"head_only"}))
}

fn split(args: &Value) -> Result<Value> {
    let body = field(args, "body")?;
    let level = args
        .get("level")
        .and_then(Value::as_u64)
        .filter(|v| (1..=6).contains(v))
        .ok_or_else(|| Error::Invalid("level must be 1..6".into()))?;
    let heads = headings(body);
    let chars: Vec<char> = body.chars().collect();
    let mut result = Vec::new();
    for (index, head) in heads.iter().enumerate() {
        if head[0] != level {
            continue;
        }
        let start = head[1].as_u64().unwrap() as usize;
        let end = heads[index + 1..]
            .iter()
            .find(|h| h[0].as_u64().unwrap() <= level)
            .map(|h| h[1].as_u64().unwrap() as usize)
            .unwrap_or(chars.len());
        let newline = (start..end).find(|&i| matches!(chars[i], '\r' | '\n'));
        let (header_end, content_start) = match newline {
            Some(i) => (
                i,
                i + if chars[i] == '\r' && i + 1 < end && chars[i + 1] == '\n' {
                    2
                } else {
                    1
                },
            ),
            None => (end, end),
        };
        result.push(json!({"level":level,"title":head[2],"header_line":chars[start..header_end].iter().collect::<String>(),
            "start":start,"end":end,"content":chars[content_start..end].iter().collect::<String>().trim_matches(whitespace)}));
    }
    Ok(json!(result))
}

pub fn transform(action: &str, args: &Value) -> Result<Value> {
    if !args.is_object() {
        return Err(Error::Invalid("utility arguments must be an object".into()));
    }
    match action {
        "normalize_heads" => normalize_heads(args),
        "normalize_paragraphs" => Ok(json!(normalize_paragraphs(
            field(args, "body")?,
            field(args, "terminators")?,
            field(args, "wrappers")?
        ))),
        "normalize_title" => Ok(json!(compat_kb::normalized_title(field(args, "text")?))),
        "sha1" => Ok(json!(format!(
            "{:x}",
            Sha1::digest(field(args, "text")?.as_bytes())
        ))),
        "make_anchor" => {
            let titles = args
                .get("titles")
                .and_then(Value::as_array)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    Error::Invalid("ancestor_titles must contain at least the current title".into())
                })?;
            let normalized = titles
                .iter()
                .map(|t| {
                    t.as_str()
                        .map(compat_kb::normalized_title)
                        .ok_or_else(|| Error::Invalid("ancestor titles must be strings".into()))
                })
                .collect::<Result<Vec<_>>>()?;
            let max = args.get("slug_max").and_then(Value::as_u64).unwrap_or(20) as usize;
            let hash_len = args.get("hash_len").and_then(Value::as_u64).unwrap_or(6) as usize;
            let separator = args.get("separator").and_then(Value::as_str).unwrap_or("›");
            let fallback = args.get("fallback").and_then(Value::as_str).unwrap_or("_");
            let last = normalized.last().unwrap();
            let short = if last.is_empty() {
                fallback.into()
            } else {
                last.chars().take(max).collect::<String>()
            };
            let digest = format!("{:x}", Sha1::digest(normalized.join(separator).as_bytes()));
            let level = args
                .get("level")
                .ok_or_else(|| Error::Invalid("level is required".into()))?;
            Ok(json!(format!(
                "h{level}-{short}-{}",
                &digest[..hash_len.min(digest.len())]
            )))
        }
        "sections" => Ok(compat_kb::parse_sections(field(args, "body")?)),
        "headings" => Ok(json!(headings(field(args, "body")?))),
        "find_section_path" => {
            let mut path = Vec::new();
            if find_path(
                args.get("sections").unwrap_or(&json!([])),
                field(args, "anchor")?,
                &mut path,
            )? {
                Ok(json!(path))
            } else {
                Ok(Value::Null)
            }
        }
        "extract_section" => {
            let body = field(args, "body")?;
            let mut path = Vec::new();
            let mut nodes = args.get("sections").unwrap_or(&Value::Null);
            if !find_path(nodes, field(args, "anchor")?, &mut path)? {
                return Ok(Value::Null);
            }
            let mut found = &Value::Null;
            for index in path {
                found = &nodes[index];
                nodes = &found["children"];
            }
            let start = found["start"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("invalid section offset".into()))?
                as usize;
            let end = found["end"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("invalid section offset".into()))?
                as usize;
            Ok(json!(
                body.chars()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .collect::<String>()
            ))
        }
        "split" => split(args),
        "normalize_span" => Ok(json!(
            normalize_span(field(args, "text")?, field(args, "level")?)?.0
        )),
        "normalize_span_map" => {
            let (text, map) = normalize_span(field(args, "text")?, field(args, "level")?)?;
            Ok(json!([text, map]))
        }
        "locate_span" => locate(args),
        _ => Err(Error::Invalid("unknown utility transform".into())),
    }
}
