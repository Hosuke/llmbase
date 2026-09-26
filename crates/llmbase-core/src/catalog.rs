//! Scoped catalog projections and durable accepted classifications/relations.
//! Projection reads never rewrite Markdown or legacy metadata JSON.
use crate::document::hash;
use crate::store::WriterLock;
use crate::{Error, Evidence, Library, ResolvedEvidence, Result, Source};
use regex::Regex;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

const MAX_SCAN: usize = 20_000;
const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS catalog_documents (
 library_id TEXT NOT NULL, kind TEXT NOT NULL, revision INTEGER NOT NULL,
 payload TEXT NOT NULL, PRIMARY KEY(library_id,kind));
CREATE TABLE IF NOT EXISTS catalog_document_revisions (
 library_id TEXT NOT NULL, kind TEXT NOT NULL, revision INTEGER NOT NULL,
 payload TEXT NOT NULL, PRIMARY KEY(library_id,kind,revision));
CREATE TABLE IF NOT EXISTS catalog_relations (
 library_id TEXT NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL,
 deleted INTEGER NOT NULL DEFAULT 0, payload TEXT NOT NULL,
 PRIMARY KEY(library_id,id));
CREATE TABLE IF NOT EXISTS catalog_relation_revisions (
 library_id TEXT NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL,
 deleted INTEGER NOT NULL, payload TEXT NOT NULL,
 PRIMARY KEY(library_id,id,revision));
"#;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GraphOptions {
    pub root: Option<String>,
    pub depth: usize,
    pub max_neighbors: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
}
impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            root: None,
            depth: 1,
            max_neighbors: 50,
            max_nodes: 1000,
            max_edges: 5000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationInput {
    pub source: String,
    pub target: String,
    pub kind: String,
    #[serde(default)]
    pub supports: Vec<Evidence>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
    #[serde(default = "public_visibility")]
    pub visibility: String,
}
fn empty_object() -> Value {
    json!({})
}
fn public_visibility() -> String {
    "public".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredRelation {
    id: String,
    source_id: String,
    target_id: String,
    kind: String,
    visibility: String,
    supports: Vec<Evidence>,
    metadata: Value,
    unconditional: bool,
}

#[derive(Clone, Debug, Serialize)]
struct Edge {
    id: String,
    source: String,
    target: String,
    kind: String,
    origin: String,
    supports: Vec<ResolvedEvidence>,
    metadata: Value,
    revision: Option<u64>,
}

impl Library {
    /// Unique aliases only; ambiguity is represented separately, never guessed.
    pub fn aliases(&self, include_private: bool) -> Result<Value> {
        let _lock = WriterLock::acquire(&self.config)?;
        let sources = self.catalog_sources(Some("compiled"), include_private)?;
        let aliases = AliasIndex::new(self, &sources);
        let by_id: HashMap<_, _> = sources.iter().map(|s| (s.source_id.as_str(), s)).collect();
        let mut unique = BTreeMap::new();
        let mut ambiguous = BTreeMap::new();
        for (alias, ids) in aliases.exact {
            if ids.len() == 1 {
                unique.insert(alias, by_id[ids.first().unwrap().as_str()].slug.clone());
            } else {
                ambiguous.insert(alias, ids);
            }
        }
        Ok(json!({"library_id":self.config.library_id,"aliases":unique,"ambiguous":ambiguous}))
    }

    pub fn backlinks(&self, target: &str, include_private: bool) -> Result<Value> {
        let _lock = WriterLock::acquire(&self.config)?;
        let sources = self.catalog_sources(Some("compiled"), include_private)?;
        let aliases = AliasIndex::new(self, &sources);
        let target = aliases
            .resolve(self, target)?
            .ok_or_else(|| Error::NotFound("article".into()))?;
        let pattern = link_pattern()?;
        let mut backlinks = Vec::new();
        for source in &sources {
            let found = pattern.captures_iter(&source.content).any(|capture| {
                aliases
                    .resolve(self, capture[1].trim())
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some(target.as_str())
            });
            if found {
                backlinks.push(
                    json!({"source_id":source.source_id,"slug":source.slug,"title":source.title}),
                );
            }
        }
        Ok(json!({"library_id":self.config.library_id,"source_id":target,"backlinks":backlinks}))
    }

    pub fn graph(&self, options: GraphOptions, include_private: bool) -> Result<Value> {
        if !(1..=5000).contains(&options.max_nodes)
            || !(1..=20_000).contains(&options.max_edges)
            || options.depth > 4
            || !(1..=200).contains(&options.max_neighbors)
        {
            return Err(Error::Invalid(
                "graph bounds: nodes 1..5000, edges 1..20000, depth 0..4, neighbors 1..200".into(),
            ));
        }
        let _lock = WriterLock::acquire(&self.config)?;
        let sources = self.catalog_sources(None, include_private)?;
        let aliases = AliasIndex::new(self, &sources);
        let root = options
            .root
            .as_deref()
            .map(|name| {
                aliases
                    .resolve(self, name)
                    .and_then(|id| id.ok_or_else(|| Error::NotFound("graph root".into())))
            })
            .transpose()?;
        let by_id: HashMap<_, _> = sources.iter().map(|s| (s.source_id.clone(), s)).collect();
        let pattern = link_pattern()?;
        let mut edges = BTreeMap::<String, Edge>::new();
        let mut truncated = false;
        // Manual content takes precedence in a bounded projection. Distinct
        // relation kinds have distinct identities, including contradictions.
        for (revision, relation) in self.catalog_relations()? {
            if !by_id.contains_key(&relation.source_id)
                || !by_id.contains_key(&relation.target_id)
                || (!include_private && relation.visibility != "public")
            {
                continue;
            }
            let supports = relation
                .supports
                .iter()
                .filter_map(|e| self.resolve_evidence(e, include_private).ok())
                .collect::<Vec<_>>();
            if !relation.unconditional && !supports.iter().any(|s| !s.deleted) {
                continue;
            }
            let edge = Edge {
                id: relation.id.clone(),
                source: relation.source_id,
                target: relation.target_id,
                kind: relation.kind,
                origin: "manual".into(),
                supports,
                metadata: relation.metadata,
                revision: Some(revision),
            };
            insert_edge(&mut edges, edge, options.max_edges, &mut truncated);
        }
        for source in &sources {
            for capture in pattern.captures_iter(&source.content[source.body_start..]) {
                let Some(target) = aliases.resolve(self, capture[1].trim()).ok().flatten() else {
                    continue;
                };
                let matched = capture.get(0).unwrap();
                let supports = self.range_supports(
                    source,
                    source.body_start + matched.start(),
                    source.body_start + matched.end(),
                )?;
                let id = edge_id(&source.source_id, &target, "wikilink", "derived");
                insert_edge(
                    &mut edges,
                    Edge {
                        id,
                        source: source.source_id.clone(),
                        target,
                        kind: "wikilink".into(),
                        origin: "wikilink".into(),
                        supports,
                        metadata: json!({}),
                        revision: None,
                    },
                    options.max_edges,
                    &mut truncated,
                );
            }
        }
        // Source references describe provenance, not semantic entailment.
        let raw_refs = source_reference_index(self, &sources);
        let mut cited_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for source in sources
            .iter()
            .filter(|s| s.kind == "compiled" || s.kind == "note")
        {
            for reference in metadata_references(&source.metadata) {
                let key = self.normalizer.normalize(reference.trim());
                let Some(ids) = raw_refs.get(&key).filter(|ids| ids.len() == 1) else {
                    continue;
                };
                let raw = ids.first().unwrap();
                cited_by
                    .entry(raw.clone())
                    .or_default()
                    .insert(source.source_id.clone());
                let support = self.first_support(by_id[raw])?;
                insert_edge(
                    &mut edges,
                    Edge {
                        id: edge_id(&source.source_id, raw, "source_reference", "derived"),
                        source: source.source_id.clone(),
                        target: raw.clone(),
                        kind: "source_reference".into(),
                        origin: "source_metadata".into(),
                        supports: support.into_iter().collect(),
                        metadata: json!({}),
                        revision: None,
                    },
                    options.max_edges,
                    &mut truncated,
                );
            }
        }
        for (raw_id, citing) in cited_by {
            let citing: Vec<_> = citing.into_iter().collect();
            let support = self.first_support(by_id[&raw_id])?;
            for (index, source) in citing.iter().enumerate() {
                for target in &citing[index + 1..] {
                    insert_edge(
                        &mut edges,
                        Edge {
                            id: edge_id(source, target, "shared_source", "derived"),
                            source: source.clone(),
                            target: target.clone(),
                            kind: "shared_source".into(),
                            origin: "source_metadata".into(),
                            supports: support.clone().into_iter().collect(),
                            metadata: json!({}),
                            revision: None,
                        },
                        options.max_edges,
                        &mut truncated,
                    );
                    if edges.len() >= options.max_edges {
                        truncated = true;
                        break;
                    }
                }
                if edges.len() >= options.max_edges {
                    break;
                }
            }
        }
        let selected = if let Some(root) = &root {
            let mut selected = BTreeSet::from([root.clone()]);
            let mut queue = VecDeque::from([(root.clone(), 0)]);
            while let Some((id, depth)) = queue.pop_front() {
                if depth >= options.depth {
                    continue;
                }
                let neighbors: BTreeSet<_> = edges
                    .values()
                    .filter_map(|edge| {
                        if edge.source == id {
                            Some(edge.target.clone())
                        } else if edge.target == id {
                            Some(edge.source.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                if neighbors.len() > options.max_neighbors {
                    truncated = true;
                }
                for neighbor in neighbors.into_iter().take(options.max_neighbors) {
                    if selected.contains(&neighbor) {
                        continue;
                    }
                    if selected.len() >= options.max_nodes {
                        truncated = true;
                        break;
                    }
                    selected.insert(neighbor.clone());
                    queue.push_back((neighbor, depth + 1));
                }
            }
            selected
        } else {
            if sources.len() > options.max_nodes {
                truncated = true;
            }
            sources
                .iter()
                .take(options.max_nodes)
                .map(|s| s.source_id.clone())
                .collect()
        };
        let nodes: Vec<_> = sources.iter().filter(|s| selected.contains(&s.source_id))
            .map(|s| json!({"id":s.source_id,"slug":s.slug,"title":s.title,"kind":s.kind,"revision":s.revision})).collect();
        let edges: Vec<_> = edges
            .into_values()
            .filter(|e| selected.contains(&e.source) && selected.contains(&e.target))
            .collect();
        Ok(
            json!({"version":1,"library_id":self.config.library_id,"root":root,"nodes":nodes,"edges":edges,"truncated":truncated}),
        )
    }

    pub fn add_relation(&mut self, input: RelationInput, include_private: bool) -> Result<Value> {
        if input.source.len() > 1024
            || input.target.len() > 1024
            || input.kind.is_empty()
            || input.kind.len() > 128
            || input.kind.chars().any(char::is_control)
            || input.supports.len() > 128
            || !input.metadata.is_object()
            || serde_json::to_vec(&input.metadata)?.len() > 16_384
            || !["public", "internal", "private"].contains(&input.visibility.as_str())
        {
            return Err(Error::Invalid("invalid or oversized relation".into()));
        }
        let _lock = WriterLock::acquire(&self.config)?;
        let source = self.live_source(&input.source, include_private)?;
        let target = self.live_source(&input.target, include_private)?;
        if input.visibility != "public" && !include_private {
            return Err(Error::Scope);
        }
        if input.visibility == "public" {
            self.live_source(&source.source_id, false)?;
            self.live_source(&target.source_id, false)?;
        }
        for support in &input.supports {
            self.resolve_evidence(support, include_private)?;
            if input.visibility == "public" {
                self.resolve_evidence(support, false)?;
            }
        }
        self.db.execute_batch(SCHEMA)?;
        let id = edge_id(
            &source.source_id,
            &target.source_id,
            &input.kind,
            &self.config.library_id,
        );
        let previous: Option<(u64, String)> = self
            .db
            .query_row(
                "SELECT revision,payload FROM catalog_relations WHERE library_id=?1 AND id=?2",
                params![self.config.library_id, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let revision = previous.as_ref().map_or(1, |(revision, _)| revision + 1);
        let mut relation = StoredRelation {
            id: id.clone(),
            source_id: source.source_id,
            target_id: target.source_id,
            kind: input.kind,
            visibility: input.visibility,
            supports: input.supports,
            metadata: input.metadata,
            unconditional: false,
        };
        relation.unconditional = relation.supports.is_empty();
        if let Some((_, payload)) = &previous {
            let old: StoredRelation = serde_json::from_str(payload)?;
            if old.visibility != "public" && !include_private {
                return Err(Error::Scope);
            }
            relation.visibility = stricter_visibility(&old.visibility, &relation.visibility).into();
            relation.unconditional |= old.unconditional;
            for evidence in old.supports {
                if !relation.supports.contains(&evidence) {
                    relation.supports.push(evidence);
                }
            }
            if relation.supports.len() > 128 {
                return Err(Error::Invalid(
                    "relation exceeds 128 retained evidence supports".into(),
                ));
            }
            let mut metadata = old.metadata.as_object().cloned().unwrap_or_default();
            metadata.extend(relation.metadata.as_object().unwrap().clone());
            relation.metadata = Value::Object(metadata);
        }
        if serde_json::to_vec(&relation.metadata)?.len() > 16_384 {
            return Err(Error::Invalid(
                "retained relation metadata exceeds 16 KiB".into(),
            ));
        }
        let payload = serde_json::to_string(&relation)?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO catalog_relations VALUES (?1,?2,?3,0,?4) ON CONFLICT(library_id,id) DO UPDATE SET revision=excluded.revision,deleted=0,payload=excluded.payload",params![self.config.library_id,id,revision,payload])?;
        tx.execute(
            "INSERT INTO catalog_relation_revisions VALUES (?1,?2,?3,0,?4)",
            params![self.config.library_id, id, revision, payload],
        )?;
        tx.commit()?;
        Ok(json!({"id":id,"revision":revision,"created":previous.is_none(),"relation":relation}))
    }

    pub fn remove_relation(&mut self, id: &str, include_private: bool) -> Result<Value> {
        let _lock = WriterLock::acquire(&self.config)?;
        if !self.catalog_table_exists("catalog_relations")? {
            return Err(Error::NotFound("relation".into()));
        }
        let (revision,payload):(u64,String) = self.db.query_row("SELECT revision,payload FROM catalog_relations WHERE library_id=?1 AND id=?2 AND deleted=0",params![self.config.library_id,id],|r| Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(|| Error::NotFound("relation".into()))?;
        let relation: StoredRelation = serde_json::from_str(&payload)?;
        if relation.visibility != "public" && !include_private {
            return Err(Error::NotFound("relation".into()));
        }
        self.get(&relation.source_id, None, include_private)?;
        self.get(&relation.target_id, None, include_private)?;
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE catalog_relations SET deleted=1,revision=?3 WHERE library_id=?1 AND id=?2",
            params![self.config.library_id, id, revision + 1],
        )?;
        tx.execute(
            "INSERT INTO catalog_relation_revisions VALUES (?1,?2,?3,1,?4)",
            params![self.config.library_id, id, revision + 1, payload],
        )?;
        tx.commit()?;
        Ok(json!({"id":id,"revision":revision+1,"deleted":true}))
    }

    pub fn taxonomy(&self, lang: &str, include_private: bool) -> Result<Value> {
        if lang.is_empty() || lang.len() > 64 || lang.chars().any(char::is_control) {
            return Err(Error::Invalid("invalid taxonomy language".into()));
        }
        let _lock = WriterLock::acquire(&self.config)?;
        let sources = self.catalog_sources(Some("compiled"), include_private)?;
        let (document, origin) = match self.catalog_document("taxonomy")? {
            Some((v, o)) => (v, o),
            None => (fallback_taxonomy(&sources), "tag_fallback".into()),
        };
        validate_taxonomy(&document)?;
        if !include_private {
            validate_public_aggregate(self, &document)?;
        }
        let by_slug = unique_slugs(&sources);
        let mut seen = HashSet::new();
        let categories = localize_nodes(
            document["categories"].as_array().unwrap(),
            lang,
            &by_slug,
            &mut seen,
            include_private,
        );
        Ok(
            json!({"library_id":self.config.library_id,"categories":categories,"origin":origin,"locked":document.get("locked").and_then(Value::as_bool).unwrap_or(false)}),
        )
    }

    /// Persist accepted human classification with every unknown field retained.
    pub fn set_taxonomy(&mut self, mut document: Value, include_private: bool) -> Result<Value> {
        validate_taxonomy(&document)?;
        let _lock = WriterLock::acquire(&self.config)?;
        let sources = self.catalog_sources(Some("compiled"), include_private)?;
        let visible = unique_slugs(&sources);
        let mut refs = Vec::new();
        taxonomy_references(&document["categories"], &mut refs);
        if refs.iter().any(|r| !visible.contains_key(*r)) {
            return Err(Error::Scope);
        }
        if !include_private {
            if taxonomy_contains_private(&document["categories"]) {
                return Err(Error::Scope);
            }
            if let Some((old, _)) = self.catalog_document("taxonomy")? {
                if taxonomy_contains_private(&old["categories"]) {
                    return Err(Error::Scope);
                }
                let mut old_refs = Vec::new();
                taxonomy_references(&old["categories"], &mut old_refs);
                if old_refs.iter().any(|r| !visible.contains_key(*r)) {
                    return Err(Error::Scope);
                }
            }
        }
        document["locked"] = json!(true);
        self.db.execute_batch(SCHEMA)?;
        let revision: u64 = self
            .db
            .query_row(
                "SELECT revision FROM catalog_documents WHERE library_id=?1 AND kind='taxonomy'",
                [&self.config.library_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0)
            + 1;
        let payload = serde_json::to_string(&document)?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO catalog_documents VALUES (?1,'taxonomy',?2,?3) ON CONFLICT(library_id,kind) DO UPDATE SET revision=excluded.revision,payload=excluded.payload",params![self.config.library_id,revision,payload])?;
        tx.execute(
            "INSERT INTO catalog_document_revisions VALUES (?1,'taxonomy',?2,?3)",
            params![self.config.library_id, revision, payload],
        )?;
        tx.commit()?;
        Ok(
            json!({"revision":revision,"categories":document["categories"].as_array().unwrap().len(),"locked":true}),
        )
    }

    /// Reference-only maintenance preserves the accepted lock, labels and extras.
    pub(crate) fn replace_taxonomy_references_locked(
        &mut self,
        expected: &Value,
        document: &Value,
    ) -> Result<()> {
        validate_taxonomy(document)?;
        if !self
            .catalog_document("taxonomy")?
            .is_some_and(|(current, origin)| origin == "accepted" && current == *expected)
        {
            return Err(Error::Conflict(
                "accepted taxonomy changed before reference maintenance".into(),
            ));
        }
        let revision: u64 = self.db.query_row(
            "SELECT revision FROM catalog_documents WHERE library_id=?1 AND kind='taxonomy'",
            [&self.config.library_id],
            |r| r.get(0),
        )?;
        let payload = serde_json::to_string(document)?;
        let tx = self.db.transaction()?;
        tx.execute("UPDATE catalog_documents SET revision=?2,payload=?3 WHERE library_id=?1 AND kind='taxonomy'",params![self.config.library_id,revision+1,payload])?;
        tx.execute(
            "INSERT INTO catalog_document_revisions VALUES (?1,'taxonomy',?2,?3)",
            params![self.config.library_id, revision + 1, payload],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Genuine cached-entity read, with conservative per-record provenance filtering.
    pub fn entities(&self, include_private: bool) -> Result<Value> {
        let _lock = WriterLock::acquire(&self.config)?;
        let sources = self.catalog_sources(Some("compiled"), include_private)?;
        let by_slug = unique_slugs(&sources);
        let private_document = if include_private {
            self.catalog_document("entities.private")?
        } else {
            None
        };
        let document = match private_document {
            Some(value) => Some(value),
            None => self.catalog_document("entities")?,
        };
        let Some((document, origin)) = document else {
            return Ok(
                json!({"people":[],"events":[],"places":[],"origin":"not_generated","article_count":sources.len()}),
            );
        };
        if !document.is_object() {
            return Err(Error::Invalid("entities cache must be an object".into()));
        }
        if !include_private && !crate::visibility::is_public(&document) {
            return Err(Error::Scope);
        }
        if !include_private {
            validate_public_aggregate(self, &document)?;
        }
        let mut result = if include_private {
            document.clone()
        } else {
            json!({})
        };
        for category in ["people", "events", "places"] {
            let entries = document
                .get(category)
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    Error::Invalid("entities cache needs people/events/places arrays".into())
                })?;
            if entries.len() > 10_000 {
                return Err(Error::Invalid(
                    "entity category exceeds 10000 records".into(),
                ));
            }
            let mut projected = Vec::new();
            for entry in entries {
                if !entry.is_object() {
                    return Err(Error::Invalid("entity entry must be an object".into()));
                }
                let refs = entry
                    .get("articles")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let visible: Vec<_> = refs
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|slug| by_slug.contains_key(*slug))
                    .map(str::to_owned)
                    .collect();
                let explicitly_private = !crate::visibility::is_public(entry);
                // Mixed public/private aggregate text has no field-level
                // provenance, so it is withheld in its entirety from public.
                if !include_private
                    && (explicitly_private || visible.is_empty() || visible.len() != refs.len())
                {
                    continue;
                }
                let mut item = entry.clone();
                item["articles"] = json!(visible);
                projected.push(item);
            }
            result[category] = json!(projected);
        }
        result["origin"] = json!(origin);
        result["article_count"] = json!(sources.len());
        Ok(result)
    }

    fn catalog_sources(&self, kind: Option<&str>, include_private: bool) -> Result<Vec<Source>> {
        let count:u64=self.db.query_row("SELECT count(*) FROM sources WHERE library_id=?1 AND deleted=0 AND (?2 IS NULL OR kind=?2) AND (?3 OR visibility='public')",params![self.config.library_id,kind,include_private],|r|r.get(0))?;
        if count > MAX_SCAN as u64 {
            return Err(Error::Capability(
                "catalog projection exceeds 20000 visible sources; narrow the library".into(),
            ));
        }
        let bytes:u64 = self.db.query_row("SELECT COALESCE(sum(length(r.original)),0) FROM sources s JOIN revisions r ON r.library_id=s.library_id AND r.source_id=s.id AND r.revision=s.revision WHERE s.library_id=?1 AND s.deleted=0 AND (?2 IS NULL OR s.kind=?2) AND (?3 OR s.visibility='public')", params![self.config.library_id,kind,include_private], |r|r.get(0))?;
        if bytes > MAX_SCAN_BYTES {
            return Err(Error::Capability(
                "catalog projection exceeds 64 MiB of visible source text; narrow the library"
                    .into(),
            ));
        }
        self.list(kind, include_private)
    }
    fn live_source(&self, name: &str, include_private: bool) -> Result<Source> {
        let source = self.get(name, None, include_private)?;
        if source.deleted {
            return Err(Error::NotFound("active source".into()));
        }
        Ok(source)
    }
    fn catalog_table_exists(&self, name: &str) -> Result<bool> {
        Ok(self
            .db
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                [name],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
    fn catalog_relations(&self) -> Result<Vec<(u64, StoredRelation)>> {
        if !self.catalog_table_exists("catalog_relations")? {
            return Ok(Vec::new());
        }
        let bytes:u64 = self.db.query_row("SELECT COALESCE(sum(length(payload)),0) FROM catalog_relations WHERE library_id=?1 AND deleted=0", [&self.config.library_id], |r|r.get(0))?;
        if bytes > MAX_SCAN_BYTES {
            return Err(Error::Capability(
                "catalog projection exceeds 64 MiB of retained relations".into(),
            ));
        }
        let mut stmt=self.db.prepare("SELECT revision,payload FROM catalog_relations WHERE library_id=?1 AND deleted=0 ORDER BY id LIMIT 20001")?;
        let records = stmt
            .query_map([&self.config.library_id], |r| {
                Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if records.len() > 20_000 {
            return Err(Error::Capability(
                "manual relation projection exceeds 20000 records".into(),
            ));
        }
        records
            .into_iter()
            .map(|(revision, payload)| Ok((revision, serde_json::from_str(&payload)?)))
            .collect()
    }
    pub(crate) fn catalog_document(&self, kind: &str) -> Result<Option<(Value, String)>> {
        if self.catalog_table_exists("catalog_documents")? {
            let payload: Option<String> = self
                .db
                .query_row(
                    "SELECT payload FROM catalog_documents WHERE library_id=?1 AND kind=?2",
                    params![self.config.library_id, kind],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(payload) = payload {
                return Ok(Some((serde_json::from_str(&payload)?, "accepted".into())));
            }
        }
        let path = crate::config::scoped_path(&self.config.meta, &format!("{kind}.json"))?;
        match std::fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
            Ok(meta) => {
                if meta.len() > MAX_DOCUMENT_BYTES as u64 {
                    return Err(Error::Invalid("catalog cache exceeds 4 MiB".into()));
                }
                let data = std::fs::read(path)?;
                Ok(Some((
                    serde_json::from_slice(&data)?,
                    "legacy_cache".into(),
                )))
            }
        }
    }
    fn range_supports(
        &self,
        source: &Source,
        start: usize,
        end: usize,
    ) -> Result<Vec<ResolvedEvidence>> {
        let mut stmt=self.db.prepare("SELECT id,start,end,quote,source_url,locator FROM passages WHERE library_id=?1 AND source_id=?2 AND revision=?3 AND active=1 AND start<?5 AND end>?4 ORDER BY start")?;
        let evidence = stmt
            .query_map(
                params![
                    self.config.library_id,
                    source.source_id,
                    source.revision,
                    start,
                    end
                ],
                |r| {
                    Ok(Evidence {
                        library_id: self.config.library_id.clone(),
                        source_id: source.source_id.clone(),
                        revision: source.revision,
                        content_hash: source.content_hash.clone(),
                        passage_id: r.get(0)?,
                        start: r.get(1)?,
                        end: r.get(2)?,
                        exact_quote: r.get(3)?,
                        source_url: r.get(4)?,
                        locator: r.get(5)?,
                        offset_unit: "utf8_byte".into(),
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(evidence
            .into_iter()
            .map(|evidence| ResolvedEvidence {
                evidence,
                stale: false,
                deleted: false,
                current_revision: source.revision,
            })
            .collect())
    }
    fn first_support(&self, source: &Source) -> Result<Option<ResolvedEvidence>> {
        Ok(self
            .range_supports(
                source,
                source.body_start,
                source.body_start.saturating_add(1),
            )?
            .into_iter()
            .next())
    }
}

fn edge_id(source: &str, target: &str, kind: &str, origin: &str) -> String {
    hash(
        serde_json::to_string(&(source, target, kind, origin))
            .unwrap()
            .as_bytes(),
    )
}
fn stricter_visibility<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a == "private" || b == "private" {
        "private"
    } else if a == "internal" || b == "internal" {
        "internal"
    } else {
        "public"
    }
}
fn insert_edge(edges: &mut BTreeMap<String, Edge>, edge: Edge, limit: usize, truncated: &mut bool) {
    if let Some(existing) = edges.get_mut(&edge.id) {
        for support in edge.supports {
            if !existing
                .supports
                .iter()
                .any(|s| s.evidence == support.evidence)
            {
                existing.supports.push(support);
            }
        }
    } else if edges.len() < limit {
        edges.insert(edge.id.clone(), edge);
    } else {
        *truncated = true;
    }
}
fn link_pattern() -> Result<Regex> {
    Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
        .map_err(|_| Error::Invalid("wiki-link parser".into()))
}

struct AliasIndex {
    exact: BTreeMap<String, BTreeSet<String>>,
}
impl AliasIndex {
    fn new(library: &Library, sources: &[Source]) -> Self {
        let mut exact: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for source in sources {
            let mut names = vec![
                source.source_id.clone(),
                source.slug.clone(),
                source.title.clone(),
            ];
            names.extend(source.title.split('/').map(str::trim).map(str::to_owned));
            for key in ["aliases", "merged_from"] {
                if let Some(values) = source.metadata.get(key).and_then(Value::as_array) {
                    names.extend(values.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            }
            for name in names {
                let alias = library.normalizer.normalize(name.trim());
                if !alias.is_empty() {
                    exact
                        .entry(alias)
                        .or_default()
                        .insert(source.source_id.clone());
                }
            }
        }
        Self { exact }
    }
    fn resolve(&self, library: &Library, name: &str) -> Result<Option<String>> {
        let key = library.normalizer.normalize(name.trim());
        for candidate in [&key, &key.replace(' ', "-"), &key.replace(' ', "")] {
            if let Some(ids) = self.exact.get(candidate) {
                return if ids.len() == 1 {
                    Ok(ids.first().cloned())
                } else {
                    Err(Error::Conflict(
                        "ambiguous catalog alias; use source ID".into(),
                    ))
                };
            }
        }
        Ok(None)
    }
}

fn source_reference_index(
    library: &Library,
    sources: &[Source],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut result: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for source in sources.iter().filter(|s| s.kind == "raw") {
        let mut refs = vec![
            source.source_id.clone(),
            source.slug.clone(),
            source.rel_path.clone(),
            source.title.clone(),
        ];
        for key in ["source", "source_url", "url", "work_id"] {
            if let Some(value) = source.metadata.get(key).and_then(Value::as_str) {
                refs.push(value.into());
            }
        }
        for reference in refs {
            result
                .entry(library.normalizer.normalize(reference.trim()))
                .or_default()
                .insert(source.source_id.clone());
        }
    }
    result
}
fn metadata_references(metadata: &Value) -> Vec<&str> {
    let mut refs = Vec::new();
    if let Some(items) = metadata.get("sources").and_then(Value::as_array) {
        for item in items {
            if let Some(text) = item.as_str() {
                refs.push(text);
            } else {
                for key in ["source_id", "path", "url", "source", "title", "work_id"] {
                    if let Some(text) = item.get(key).and_then(Value::as_str) {
                        refs.push(text);
                    }
                }
            }
        }
    }
    refs
}

fn validate_public_aggregate(library: &Library, document: &Value) -> Result<()> {
    let refs = if let Some(consulted) = document.get("consulted_sources") {
        Some(consulted)
    } else if document.get("source_fingerprint").is_some() {
        document.get("source_ids")
    } else {
        None
    };
    let Some(refs) = refs else { return Ok(()) };
    let refs = refs
        .as_array()
        .ok_or_else(|| Error::Invalid("invalid aggregate source provenance".into()))?;
    if refs.len() > 20000 {
        return Err(Error::Invalid(
            "too many aggregate source references".into(),
        ));
    }
    let mut seen = HashSet::new();
    for reference in refs {
        let id = reference
            .as_str()
            .or_else(|| reference.get("source_id").and_then(Value::as_str))
            .ok_or_else(|| Error::Invalid("aggregate source ID required".into()))?;
        if !seen.insert(id) {
            return Err(Error::Invalid(
                "duplicate aggregate source provenance".into(),
            ));
        }
        let source = library.get(id, None, false).map_err(|_| {
            Error::Conflict("cached generation has an unavailable consulted source".into())
        })?;
        if source.deleted {
            return Err(Error::Conflict(
                "cached generation has a deleted consulted source".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_taxonomy(document: &Value) -> Result<()> {
    if serde_json::to_vec(document)?.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::Invalid("taxonomy exceeds 4 MiB".into()));
    }
    let categories = document
        .get("categories")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("taxonomy needs categories array".into()))?;
    fn visit(
        nodes: &[Value],
        depth: usize,
        count: &mut usize,
        ids: &mut HashSet<String>,
    ) -> Result<()> {
        if depth > 32 {
            return Err(Error::Invalid("taxonomy exceeds depth 32".into()));
        }
        for node in nodes {
            *count += 1;
            if *count > 4096 {
                return Err(Error::Invalid("taxonomy exceeds 4096 categories".into()));
            }
            let id = node
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 256)
                .ok_or_else(|| Error::Invalid("taxonomy category needs bounded ID".into()))?;
            if !ids.insert(id.into()) {
                return Err(Error::Conflict("duplicate taxonomy category ID".into()));
            }
            let valid_label = match node.get("label") {
                Some(Value::String(s)) => s.len() <= 4096,
                Some(Value::Object(map)) => {
                    map.len() <= 32
                        && map
                            .values()
                            .all(|v| v.as_str().is_some_and(|s| s.len() <= 4096))
                }
                _ => false,
            };
            if !valid_label {
                return Err(Error::Invalid(
                    "taxonomy label must be text or language map".into(),
                ));
            }
            if let Some(slugs) = node.get("article_slugs") {
                let slugs = slugs
                    .as_array()
                    .ok_or_else(|| Error::Invalid("article_slugs must be an array".into()))?;
                if slugs.len() > MAX_SCAN
                    || slugs
                        .iter()
                        .any(|v| v.as_str().is_none_or(|s| s.is_empty() || s.len() > 1024))
                {
                    return Err(Error::Invalid("invalid taxonomy article references".into()));
                }
            }
            if let Some(children) = node.get("children") {
                visit(
                    children.as_array().ok_or_else(|| {
                        Error::Invalid("taxonomy children must be an array".into())
                    })?,
                    depth + 1,
                    count,
                    ids,
                )?;
            }
        }
        Ok(())
    }
    visit(categories, 0, &mut 0, &mut HashSet::new())
}
fn taxonomy_references<'a>(nodes: &'a Value, refs: &mut Vec<&'a str>) {
    if let Some(nodes) = nodes.as_array() {
        for node in nodes {
            if let Some(slugs) = node.get("article_slugs").and_then(Value::as_array) {
                refs.extend(slugs.iter().filter_map(Value::as_str));
            }
            taxonomy_references(&node["children"], refs);
        }
    }
}

fn taxonomy_contains_private(nodes: &Value) -> bool {
    nodes.as_array().is_some_and(|nodes| {
        nodes.iter().any(|node| {
            !crate::visibility::is_public(node) || taxonomy_contains_private(&node["children"])
        })
    })
}
fn unique_slugs(sources: &[Source]) -> HashMap<&str, &Source> {
    let mut grouped: HashMap<&str, Vec<&Source>> = HashMap::new();
    for source in sources {
        grouped.entry(&source.slug).or_default().push(source);
    }
    grouped
        .into_iter()
        .filter_map(|(slug, items)| {
            if items.len() == 1 {
                Some((slug, items[0]))
            } else {
                None
            }
        })
        .collect()
}
fn localize_nodes(
    nodes: &[Value],
    lang: &str,
    sources: &HashMap<&str, &Source>,
    seen: &mut HashSet<String>,
    include_private: bool,
) -> Vec<Value> {
    let mut result = Vec::new();
    for node in nodes {
        if !include_private
            && (!crate::visibility::is_public(node)
                || node
                    .get("article_slugs")
                    .and_then(Value::as_array)
                    .is_some_and(|slugs| {
                        slugs
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|slug| !sources.contains_key(slug))
                    }))
        {
            // An aggregate label with mixed/unknown provenance cannot be split
            // into public fields reliably. Withhold that entire subtree.
            continue;
        }
        let children = localize_nodes(
            node.get("children")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            lang,
            sources,
            seen,
            include_private,
        );
        let mut articles = Vec::new();
        if let Some(slugs) = node.get("article_slugs").and_then(Value::as_array) {
            for slug in slugs.iter().filter_map(Value::as_str) {
                if let Some(source) = sources.get(slug)
                    && seen.insert(slug.into())
                {
                    articles.push(json!({"slug":slug,"title":localized_title(source,lang)}));
                }
            }
        }
        let total = articles.len()
            + children
                .iter()
                .filter_map(|c| c["total"].as_u64())
                .sum::<u64>() as usize;
        if !include_private && total == 0 {
            continue;
        }
        let label = if node["label"].is_string() {
            node["label"].clone()
        } else {
            node["label"]
                .get(lang)
                .or_else(|| node["label"].get("en"))
                .or_else(|| node["label"].get("zh"))
                .cloned()
                .unwrap_or_else(|| node["id"].clone())
        };
        result.push(json!({"id":node["id"],"label":label,"count":articles.len(),"total":total,"articles":articles,"children":children}));
    }
    result
}
fn localized_title(source: &Source, lang: &str) -> String {
    if let Some(title) = source
        .metadata
        .get("titles")
        .and_then(|v| v.get(lang))
        .and_then(Value::as_str)
    {
        return title.into();
    }
    if lang == "zh-en" {
        return source.title.clone();
    }
    let parts = source
        .title
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return source.title.clone();
    }
    let cjk = |s: &&str| {
        s.chars()
            .any(|c| matches!(c as u32,0x3040..=0x30ff|0x3400..=0x9fff|0x20000..=0x3ffff))
    };
    if ["zh", "ja", "zh-Hant", "zh-Hans", "wenyan"].contains(&lang) {
        parts
            .iter()
            .find(|s| cjk(s))
            .unwrap_or(&parts[parts.len() - 1])
            .to_string()
    } else if lang == "en" {
        parts
            .iter()
            .find(|s| !cjk(s))
            .unwrap_or(&parts[0])
            .to_string()
    } else {
        source.title.clone()
    }
}
fn fallback_taxonomy(sources: &[Source]) -> Value {
    let mut tags: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for source in sources {
        if let Some(values) = source.metadata.get("tags").and_then(Value::as_array) {
            for tag in values.iter().filter_map(Value::as_str) {
                tags.entry(tag.to_lowercase().replace(' ', "-"))
                    .or_default()
                    .insert(source.slug.clone());
            }
        }
    }
    let mut tags: Vec<_> = tags
        .into_iter()
        .filter(|(_, slugs)| slugs.len() >= 2)
        .collect();
    tags.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    tags.truncate(10);
    if tags.is_empty() {
        return json!({"categories":[{"id":"all","label":{"en":"All Articles","zh":"全部文章","ja":"全記事"},"article_slugs":sources.iter().map(|s|&s.slug).collect::<Vec<_>>(),"children":[]}],"generated":false});
    }
    let mut assigned = HashSet::new();
    let mut categories = Vec::new();
    for (tag, slugs) in tags {
        let slugs: Vec<_> = slugs
            .into_iter()
            .filter(|s| assigned.insert(s.clone()))
            .collect();
        if !slugs.is_empty() {
            categories.push(json!({"id":tag,"label":tag,"article_slugs":slugs,"children":[]}));
        }
    }
    let other: Vec<_> = sources
        .iter()
        .filter(|s| !assigned.contains(&s.slug))
        .map(|s| &s.slug)
        .collect();
    if !other.is_empty() {
        categories.push(json!({"id":"other","label":{"en":"Other","zh":"其他","ja":"その他"},"article_slugs":other,"children":[]}));
    }
    json!({"categories":categories,"generated":false})
}
