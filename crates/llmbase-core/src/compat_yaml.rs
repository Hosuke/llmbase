//! Bounded YAML 1.1 safe-scalar resolver over yaml-rust2's native event parser.
//! Scalar styles are retained: quoted "yes" is never rewritten as a boolean.
use crate::{Error, Result};
use num_bigint::BigInt;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Number, Value};
use std::{
    collections::HashMap,
    str::{Chars, FromStr},
    sync::LazyLock,
};
use yaml_rust2::{
    parser::{Event, Parser, Tag},
    scanner::TScalarStyle,
};

const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_EXPANDED: usize = 32 * 1024 * 1024;

/// Every value is wrapped in a typed node; literal user maps cannot collide
/// with a transport marker. Python adapters may reconstruct date/nonfinite types.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Node {
    Null,
    Bool(bool),
    Integer(String),
    Float(String),
    String(String),
    Timestamp(String),
    Sequence(Vec<Node>),
    Mapping(Vec<(Node, Node)>),
    Merge,
}

impl Node {
    fn weight(&self) -> (usize, usize) {
        let mut count = 1;
        let mut size = 0;
        match self {
            Self::Integer(s) | Self::Float(s) | Self::String(s) | Self::Timestamp(s) => {
                size = s.len()
            }
            Self::Sequence(nodes) => {
                for n in nodes {
                    let (c, s) = n.weight();
                    count += c;
                    size += s;
                }
            }
            Self::Mapping(pairs) => {
                for (k, v) in pairs {
                    for n in [k, v] {
                        let (c, s) = n.weight();
                        count += c;
                        size += s;
                    }
                }
            }
            _ => {}
        }
        (count, size)
    }
    /// Native document metadata remains a JSON-domain DTO. Typed-only YAML
    /// values return an explicit capability error instead of becoming null,
    /// with one serde_yaml compatibility exception: a plain null mapping key
    /// becomes the JSON object key "null" (collision with a quoted "null"
    /// key is rejected explicitly).
    pub fn to_json(&self) -> Result<Value> {
        Ok(match self {
            Self::Null => Value::Null,
            Self::Bool(v) => Value::Bool(*v),
            Self::Integer(v) => Value::Number(
                Number::from_str(v).map_err(|_| Error::Invalid("invalid YAML integer".into()))?,
            ),
            Self::Float(v) => {
                let value = v
                    .parse::<f64>()
                    .map_err(|_| Error::Invalid("invalid YAML float".into()))?;
                Value::Number(Number::from_f64(value).ok_or_else(|| {
                    Error::Capability("non-finite YAML metadata requires a typed adapter".into())
                })?)
            }
            Self::String(s) | Self::Timestamp(s) => Value::String(s.clone()),
            Self::Sequence(nodes) => Value::Array(
                nodes
                    .iter()
                    .map(Node::to_json)
                    .collect::<Result<Vec<_>>>()?,
            ),
            Self::Mapping(pairs) => {
                let mut result = Map::new();
                let mut null_from_plain = false;
                let mut null_from_string = false;
                for (k, v) in pairs {
                    let key = match k {
                        Self::String(key) => {
                            if key == "null" {
                                null_from_string = true;
                                if null_from_plain {
                                    return Err(Error::Invalid(
                                        "YAML null mapping key collides with \"null\" string key"
                                            .into(),
                                    ));
                                }
                            }
                            key.clone()
                        }
                        // serde_yaml compatibility: a plain null mapping key is
                        // the JSON object key "null". parse_typed keeps the
                        // typed Null key for Python reconstruction; other
                        // non-string keys still require a typed adapter.
                        Self::Null => {
                            null_from_plain = true;
                            if null_from_string {
                                return Err(Error::Invalid(
                                    "YAML null mapping key collides with \"null\" string key"
                                        .into(),
                                ));
                            }
                            "null".to_owned()
                        }
                        _ => {
                            return Err(Error::Capability(
                                "non-string YAML mapping keys require a typed adapter".into(),
                            ));
                        }
                    };
                    result.insert(key, v.to_json()?);
                }
                Value::Object(result)
            }
            Self::Merge => {
                return Err(Error::Invalid(
                    "YAML merge marker must be a mapping key".into(),
                ));
            }
        })
    }
}

static INTEGER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(:[0-5]?[0-9])+)$").unwrap()
});
static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([-+]?[0-9][0-9_]*\.[0-9_]*([eE][-+][0-9]+)?|\.[0-9][0-9_]*([eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(inf|Inf|INF)|\.(nan|NaN|NAN))$").unwrap()
});
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{4}-[0-9]{2}-[0-9]{2}|[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}([Tt]|[ \t]+)[0-9]{1,2}:[0-9]{2}:[0-9]{2}(\.[0-9]*)?([ \t]*(Z|[-+][0-9]{1,2}(:[0-9]{2})?))?)$").unwrap()
});

fn tag_name(tag: Option<&Tag>) -> Result<Option<&str>> {
    match tag {
        None => Ok(None),
        Some(tag) if tag.handle == "tag:yaml.org,2002:" || tag.handle == "!!" => {
            Ok(Some(&tag.suffix))
        }
        Some(tag) if tag.handle == "!" && tag.suffix.is_empty() => Ok(Some("str")),
        Some(_) => Err(Error::Invalid("unsupported YAML tag".into())),
    }
}

fn integer(text: &str) -> Result<String> {
    let text = text.replace('_', "");
    let negative = text.starts_with('-');
    let digits = text.trim_start_matches(['+', '-']);
    if digits.is_empty() {
        return Err(Error::Invalid("invalid YAML integer".into()));
    }
    let value = if digits.contains(':') {
        let mut value = BigInt::from(0);
        for digit in digits.split(':') {
            value = value * 60
                + BigInt::parse_bytes(digit.as_bytes(), 10)
                    .ok_or_else(|| Error::Invalid("invalid YAML sexagesimal integer".into()))?;
        }
        value
    } else {
        let (radix, digits) = if let Some(s) = digits.strip_prefix("0b") {
            (2, s)
        } else if let Some(s) = digits.strip_prefix("0x") {
            (16, s)
        } else if digits.starts_with('0') {
            (8, digits)
        } else {
            (10, digits)
        };
        BigInt::parse_bytes(digits.as_bytes(), radix)
            .ok_or_else(|| Error::Invalid("invalid YAML integer".into()))?
    };
    Ok((if negative { -value } else { value }).to_string())
}

fn float(text: &str) -> Result<String> {
    let text = text.replace('_', "").to_lowercase();
    let negative = text.starts_with('-');
    let digits = text.trim_start_matches(['+', '-']);
    if digits == ".inf" {
        return Ok(if negative { "-inf" } else { "inf" }.into());
    }
    if digits == ".nan" {
        return Ok("NaN".into());
    }
    let value = if digits.contains(':') {
        let mut value = 0.;
        for digit in digits.split(':') {
            value = value * 60.
                + digit
                    .parse::<f64>()
                    .map_err(|_| Error::Invalid("invalid YAML float".into()))?;
        }
        if negative { -value } else { value }
    } else {
        text.parse::<f64>()
            .map_err(|_| Error::Invalid("invalid YAML float".into()))?
    };
    Ok(value.to_string())
}

fn scalar(text: String, style: TScalarStyle, tag: Option<&Tag>) -> Result<Node> {
    let explicit = tag_name(tag)?;
    let implicit = style == TScalarStyle::Plain && explicit.is_none();
    let boolean = matches!(
        text.as_str(),
        "yes"
            | "Yes"
            | "YES"
            | "no"
            | "No"
            | "NO"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
            | "on"
            | "On"
            | "ON"
            | "off"
            | "Off"
            | "OFF"
    );
    let kind = explicit.unwrap_or_else(|| {
        if !implicit {
            "str"
        } else if boolean {
            "bool"
        } else if matches!(text.as_str(), "" | "~" | "null" | "Null" | "NULL") {
            "null"
        } else if text == "<<" {
            "merge"
        } else if INTEGER.is_match(&text) {
            "int"
        } else if FLOAT.is_match(&text) {
            "float"
        } else if TIMESTAMP.is_match(&text) {
            "timestamp"
        } else {
            "str"
        }
    });
    Ok(match kind {
        "str" | "value" => Node::String(text),
        "null" => Node::Null,
        "bool" => match text.to_lowercase().as_str() {
            "yes" | "true" | "on" => Node::Bool(true),
            "no" | "false" | "off" => Node::Bool(false),
            _ => return Err(Error::Invalid("invalid YAML boolean".into())),
        },
        "int" => Node::Integer(integer(&text)?),
        "float" => Node::Float(float(&text)?),
        "timestamp" => {
            if !TIMESTAMP.is_match(&text) {
                return Err(Error::Invalid("invalid YAML timestamp".into()));
            }
            Node::Timestamp(text)
        }
        "merge" => Node::Merge,
        _ => {
            return Err(Error::Capability(format!(
                "unsupported safe YAML scalar type: {kind}"
            )));
        }
    })
}

struct Reader<'a> {
    parser: Parser<Chars<'a>>,
    anchors: HashMap<usize, Node>,
    events: usize,
    expanded_nodes: usize,
    expanded_bytes: usize,
}
impl Reader<'_> {
    fn next(&mut self) -> Result<Event> {
        self.events += 1;
        if self.events > MAX_NODES * 3 {
            return Err(Error::Capability("YAML event limit exceeded".into()));
        }
        self.parser
            .next_token()
            .map(|v| v.0)
            .map_err(|_| Error::Invalid("invalid YAML syntax".into()))
    }
    fn node(&mut self, event: Event, depth: usize) -> Result<Node> {
        if depth > 128 {
            return Err(Error::Capability("YAML nesting limit exceeded".into()));
        }
        let (node, anchor) = match event {
            Event::Alias(id) => {
                let value = self
                    .anchors
                    .get(&id)
                    .ok_or_else(|| Error::Invalid("unresolved or cyclic YAML alias".into()))?;
                let (count, size) = value.weight();
                self.expanded_nodes += count;
                self.expanded_bytes += size;
                if self.expanded_nodes > MAX_NODES || self.expanded_bytes > MAX_EXPANDED {
                    return Err(Error::Capability(
                        "YAML alias expansion limit exceeded".into(),
                    ));
                }
                return Ok(value.clone());
            }
            Event::Scalar(text, style, anchor, tag) => (scalar(text, style, tag.as_ref())?, anchor),
            Event::SequenceStart(anchor, tag) => {
                if !matches!(tag_name(tag.as_ref())?, None | Some("seq")) {
                    return Err(Error::Capability("unsupported YAML sequence tag".into()));
                }
                let mut nodes = Vec::new();
                loop {
                    let e = self.next()?;
                    if e == Event::SequenceEnd {
                        break;
                    }
                    nodes.push(self.node(e, depth + 1)?);
                }
                (Node::Sequence(nodes), anchor)
            }
            Event::MappingStart(anchor, tag) => {
                if !matches!(tag_name(tag.as_ref())?, None | Some("map")) {
                    return Err(Error::Capability("unsupported YAML mapping tag".into()));
                }
                let mut merged = Vec::new();
                let mut own = Vec::new();
                loop {
                    let e = self.next()?;
                    if e == Event::MappingEnd {
                        break;
                    }
                    let key = self.node(e, depth + 1)?;
                    let e = self.next()?;
                    let value = self.node(e, depth + 1)?;
                    if matches!(key, Node::Merge) {
                        match value {
                            Node::Mapping(pairs) => merged.extend(pairs),
                            Node::Sequence(nodes) => {
                                for node in nodes.into_iter().rev() {
                                    let Node::Mapping(pairs) = node else {
                                        return Err(Error::Invalid(
                                            "YAML merge sequence must contain mappings".into(),
                                        ));
                                    };
                                    merged.extend(pairs);
                                }
                            }
                            _ => {
                                return Err(Error::Invalid(
                                    "YAML merge must contain mapping or sequence".into(),
                                ));
                            }
                        }
                    } else {
                        own.push((key, value));
                    }
                }
                merged.extend(own);
                (Node::Mapping(merged), anchor)
            }
            _ => return Err(Error::Invalid("unexpected YAML event".into())),
        };
        let (count, size) = node.weight();
        if count > MAX_NODES || size > MAX_EXPANDED {
            return Err(Error::Capability(
                "YAML expanded value exceeds limits".into(),
            ));
        }
        if anchor != 0 {
            self.anchors.insert(anchor, node.clone());
        }
        Ok(node)
    }
}

pub fn parse_typed(text: &str) -> Result<Node> {
    if text.len() > MAX_INPUT {
        return Err(Error::Capability("YAML input exceeds16 MiB".into()));
    }
    let mut reader = Reader {
        parser: Parser::new_from_str(text),
        anchors: HashMap::new(),
        events: 0,
        expanded_nodes: 0,
        expanded_bytes: 0,
    };
    if reader.next()? != Event::StreamStart {
        return Err(Error::Invalid("YAML stream expected".into()));
    }
    match reader.next()? {
        Event::StreamEnd => return Ok(Node::Null),
        Event::DocumentStart => {}
        _ => return Err(Error::Invalid("YAML document expected".into())),
    }
    let event = reader.next()?;
    let value = reader.node(event, 0)?;
    if reader.next()? != Event::DocumentEnd || reader.next()? != Event::StreamEnd {
        return Err(Error::Invalid(
            "exactly one YAML document is required".into(),
        ));
    }
    Ok(value)
}

pub fn parse(text: &str) -> Result<Value> {
    parse_typed(text)?.to_json()
}
