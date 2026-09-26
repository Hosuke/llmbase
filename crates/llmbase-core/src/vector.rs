//! Durable embedding caches, atomic vector generations, and true lexical/vector RRF.

use std::collections::HashMap;

use llmbase_model::{CancellationToken, Client, EmbeddingRequest, ModelError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{Error, Evidence, Library, Result, SearchHit, SearchResult, document::hash};

const MAX_PASSAGES: usize = 100_000;
const MAX_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;
const MAX_DIMENSION: usize = 32_768;
const CANDIDATES: usize = 100;
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS embedding_cache (
 library_id TEXT NOT NULL, spec_hash TEXT NOT NULL, input_hash TEXT NOT NULL,
 dimension INTEGER NOT NULL, vector BLOB NOT NULL,
 PRIMARY KEY(library_id,spec_hash,input_hash));
CREATE TABLE IF NOT EXISTS vector_generations (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, spec_hash TEXT NOT NULL,
 spec_json TEXT NOT NULL, snapshot_hash TEXT NOT NULL, passage_count INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('building','ready','failed')),
 UNIQUE(library_id,spec_hash,snapshot_hash));
CREATE TABLE IF NOT EXISTS vector_members (
 library_id TEXT NOT NULL, generation_id TEXT NOT NULL, passage_id TEXT NOT NULL,
 spec_hash TEXT NOT NULL, input_hash TEXT NOT NULL,
 PRIMARY KEY(library_id,generation_id,passage_id));
CREATE TABLE IF NOT EXISTS vector_active (
 library_id TEXT PRIMARY KEY, generation_id TEXT NOT NULL);
"#;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingSpec {
    /// Must equal Client::endpoint_identity(); credentials/query are never stored.
    pub provider: String,
    pub model: String,
    /// Operator-declared model revision. Change when an alias changes weights.
    pub revision: String,
    pub dimension: usize,
}

impl EmbeddingSpec {
    pub fn new(
        client: &Client,
        model: impl Into<String>,
        revision: impl Into<String>,
        dimension: usize,
    ) -> Self {
        Self {
            provider: client.endpoint_identity().to_owned(),
            model: model.into(),
            revision: revision.into(),
            dimension,
        }
    }

    fn validate(&self, client: &Client) -> Result<()> {
        if self.provider != client.endpoint_identity() {
            return Err(Error::Invalid(
                "embedding provider differs from index specification".into(),
            ));
        }
        if self.model.trim().is_empty()
            || self.model.len() > 1024
            || self.revision.trim().is_empty()
            || self.revision.len() > 1024
            || !(1..=MAX_DIMENSION).contains(&self.dimension)
        {
            return Err(Error::Invalid(
                "invalid embedding model, revision, or dimension".into(),
            ));
        }
        Ok(())
    }

    fn fingerprint(&self) -> Result<String> {
        Ok(hash(serde_json::to_vec(self)?.as_slice()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorBuildReport {
    pub generation_id: String,
    pub passages: usize,
    /// Number of unique uncached input texts successfully persisted this run.
    pub embedded: usize,
    pub cache_hits: usize,
    pub reused_active: bool,
}

#[derive(Serialize)]
struct Passage {
    id: String,
    input_hash: String,
    quote: String,
    source_id: String,
    revision: u64,
    source_hash: String,
    parser_fingerprint: String,
    visibility: String,
    kind: String,
    start: usize,
    end: usize,
}

struct Snapshot {
    passages: Vec<Passage>,
    fingerprint: String,
}

impl Library {
    /// Resume/cache successful batches, then atomically activate the complete
    /// generation only if the original passage snapshot is still current.
    pub async fn build_vectors(
        &mut self,
        client: &Client,
        spec: &EmbeddingSpec,
        cancel: CancellationToken,
    ) -> Result<VectorBuildReport> {
        spec.validate(client)?;
        cancelled(&cancel)?;
        self.db.execute_batch(SCHEMA)?;
        let snapshot = read_snapshot(&self.db, &self.config.library_id)?;
        let spec_hash = spec.fingerprint()?;
        let generation = hash(
            format!(
                "{}:{spec_hash}:{}",
                self.config.library_id, snapshot.fingerprint
            )
            .as_bytes(),
        );
        let mut report = VectorBuildReport {
            generation_id: generation.clone(),
            passages: snapshot.passages.len(),
            embedded: 0,
            cache_hits: 0,
            reused_active: false,
        };
        let active: Option<String> = self
            .db
            .query_row(
                "SELECT generation_id FROM vector_active WHERE library_id=?1",
                [&self.config.library_id],
                |r| r.get(0),
            )
            .optional()?;
        let mut pending = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for passage in &snapshot.passages {
            cancelled(&cancel)?;
            if !seen.insert(passage.input_hash.clone()) {
                continue;
            }
            if read_cache(
                &self.db,
                &self.config.library_id,
                &spec_hash,
                &passage.input_hash,
                spec.dimension,
            )?
            .is_some()
            {
                report.cache_hits += 1;
            } else {
                pending.push(passage);
            }
        }
        if active.as_deref() == Some(generation.as_str()) && pending.is_empty() {
            // Validate membership as well as cache content before accepting reuse.
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if read_snapshot(&tx, &self.config.library_id)?.fingerprint != snapshot.fingerprint {
                return Err(Error::Conflict(
                    "source index changed while validating vector reuse".into(),
                ));
            }
            validate_members(
                &tx,
                &self.config.library_id,
                &generation,
                &snapshot,
                &spec_hash,
            )?;
            tx.commit()?;
            report.reused_active = true;
            return Ok(report);
        }
        self.db.execute("INSERT INTO vector_generations(id,library_id,spec_hash,spec_json,snapshot_hash,passage_count,state) VALUES (?1,?2,?3,?4,?5,?6,'building') ON CONFLICT(id) DO UPDATE SET state=CASE WHEN state='ready' THEN 'ready' ELSE 'building' END",
            params![generation,self.config.library_id,spec_hash,serde_json::to_string(spec)?,snapshot.fingerprint,snapshot.passages.len()])?;
        let result = self
            .populate_generation(
                client,
                spec,
                &spec_hash,
                &snapshot,
                &pending,
                &cancel,
                &mut report,
            )
            .await;
        if result.is_err() {
            // Cache writes are already durable; failed generation can resume.
            // Never change the previous active generation on this failure path.
            let _ = self.db.execute("UPDATE vector_generations SET state='failed' WHERE library_id=?1 AND id=?2 AND id NOT IN (SELECT generation_id FROM vector_active WHERE library_id=?1)", params![self.config.library_id,generation]);
        }
        result?;
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)] // Internal build inputs share one generation/snapshot.
    async fn populate_generation(
        &mut self,
        client: &Client,
        spec: &EmbeddingSpec,
        spec_hash: &str,
        snapshot_before: &Snapshot,
        pending: &[&Passage],
        cancel: &CancellationToken,
        report: &mut VectorBuildReport,
    ) -> Result<()> {
        for batch in pending.chunks(32) {
            cancelled(cancel)?;
            let request = EmbeddingRequest {
                input: batch.iter().map(|p| p.quote.clone()).collect(),
                model: Some(spec.model.clone()),
                dimensions: Some(spec.dimension),
            };
            let response = client.embed(request, cancel.clone()).await?;
            if response.dimensions != spec.dimension || response.embeddings.len() != batch.len() {
                return Err(Error::Conflict(
                    "embedding response does not match generation".into(),
                ));
            }
            let vectors = response
                .embeddings
                .iter()
                .map(|v| encode_vector(v, spec.dimension))
                .collect::<Result<Vec<_>>>()?;
            cancelled(cancel)?;
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (passage, bytes) in batch.iter().zip(vectors) {
                tx.execute("INSERT OR IGNORE INTO embedding_cache(library_id,spec_hash,input_hash,dimension,vector) VALUES (?1,?2,?3,?4,?5)", params![self.config.library_id,spec_hash,passage.input_hash,spec.dimension,bytes])?;
            }
            tx.commit()?;
            report.embedded += batch.len();
        }
        cancelled(cancel)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_snapshot(&tx, &self.config.library_id)?;
        if current.fingerprint != snapshot_before.fingerprint {
            return Err(Error::Conflict(
                "source index changed during embedding build; cached batches retained, rerun build"
                    .into(),
            ));
        }
        tx.execute(
            "DELETE FROM vector_members WHERE library_id=?1 AND generation_id=?2",
            params![self.config.library_id, report.generation_id],
        )?;
        for passage in &current.passages {
            cancelled(cancel)?;
            read_cache(
                &tx,
                &self.config.library_id,
                spec_hash,
                &passage.input_hash,
                spec.dimension,
            )?
            .ok_or_else(|| Error::Conflict("embedding cache missing during activation".into()))?;
            tx.execute("INSERT INTO vector_members(library_id,generation_id,passage_id,spec_hash,input_hash) VALUES (?1,?2,?3,?4,?5)", params![self.config.library_id,report.generation_id,passage.id,spec_hash,passage.input_hash])?;
        }
        validate_members(
            &tx,
            &self.config.library_id,
            &report.generation_id,
            &current,
            spec_hash,
        )?;
        tx.execute(
            "UPDATE vector_generations SET state='ready' WHERE library_id=?1 AND id=?2",
            params![self.config.library_id, report.generation_id],
        )?;
        tx.execute("INSERT INTO vector_active(library_id,generation_id) VALUES (?1,?2) ON CONFLICT(library_id) DO UPDATE SET generation_id=excluded.generation_id", params![self.config.library_id,report.generation_id])?;
        cancelled(cancel)?;
        tx.commit()?;
        Ok(())
    }

    /// RRF over independently ranked lexical and actual cosine-vector results.
    /// Missing/stale vectors degrade explicitly; configured provider failures do
    /// not silently claim successful hybrid retrieval.
    #[allow(clippy::too_many_arguments)] // Matches lexical scope/filter inputs plus model/cancellation.
    pub async fn search_hybrid(
        &self,
        query: &str,
        top_k: usize,
        kind: Option<&str>,
        include_private: bool,
        client: Option<&Client>,
        cancel: CancellationToken,
    ) -> Result<SearchResult> {
        cancelled(&cancel)?;
        // Validate normal search constraints before any outbound model call.
        let initial = self.search(query, CANDIDATES, kind, include_private)?;
        if query.trim().is_empty() || top_k == 0 {
            return Ok(SearchResult {
                results: Vec::new(),
                ..initial
            });
        }
        let Some(client) = client else {
            return Ok(degrade(initial, "no embedding client configured", top_k));
        };
        let tables: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='vector_active')", [], |r| r.get(0))?;
        if !tables {
            return Ok(degrade(
                initial,
                "embedding generation has not been built",
                top_k,
            ));
        }
        let active = active_generation(&self.db, &self.config.library_id)?;
        let Some((generation, spec_json, expected_snapshot)) = active else {
            return Ok(degrade(
                initial,
                "embedding generation has not been built",
                top_k,
            ));
        };
        let spec: EmbeddingSpec = serde_json::from_str(&spec_json)?;
        spec.validate(client)?;
        if read_snapshot(&self.db, &self.config.library_id)?.fingerprint != expected_snapshot {
            return Ok(degrade(
                initial,
                "embedding generation is stale; rebuild required",
                top_k,
            ));
        }
        let spec_hash = spec.fingerprint()?;
        let query_hash = hash(query.as_bytes());
        let query_vector = if let Some(vector) = read_cache(
            &self.db,
            &self.config.library_id,
            &spec_hash,
            &query_hash,
            spec.dimension,
        )? {
            vector
        } else {
            let response = client
                .embed(
                    EmbeddingRequest {
                        input: vec![query.to_owned()],
                        model: Some(spec.model.clone()),
                        dimensions: Some(spec.dimension),
                    },
                    cancel.clone(),
                )
                .await?;
            let vector = response
                .embeddings
                .into_iter()
                .next()
                .ok_or_else(|| Error::Conflict("missing query embedding".into()))?;
            let bytes = encode_vector(&vector, spec.dimension)?;
            cancelled(&cancel)?;
            self.db.execute("INSERT OR IGNORE INTO embedding_cache(library_id,spec_hash,input_hash,dimension,vector) VALUES (?1,?2,?3,?4,?5)", params![self.config.library_id,spec_hash,query_hash,spec.dimension,bytes])?;
            vector
        };
        cancelled(&cancel)?;
        // Hold one SQLite read snapshot only for local ranking, never provider I/O.
        let tx = self.db.unchecked_transaction()?;
        let now = active_generation(&tx, &self.config.library_id)?;
        let mut lexical = self.search(query, CANDIDATES, kind, include_private)?;
        if now.as_ref().map(|v| v.0.as_str()) != Some(generation.as_str())
            || read_snapshot(&tx, &self.config.library_id)?.fingerprint != expected_snapshot
        {
            return Ok(degrade(
                lexical,
                "embedding generation changed during query; retry required",
                top_k,
            ));
        }
        let mut stmt = tx.prepare("SELECT p.id,s.id,s.slug,s.title,s.kind,p.revision,r.hash,p.start,p.end,p.quote,p.source_url,p.locator,c.vector,c.dimension FROM vector_members m JOIN embedding_cache c ON c.library_id=m.library_id AND c.spec_hash=m.spec_hash AND c.input_hash=m.input_hash JOIN passages p ON p.id=m.passage_id AND p.library_id=m.library_id JOIN sources s ON s.id=p.source_id AND s.library_id=p.library_id JOIN revisions r ON r.library_id=p.library_id AND r.source_id=p.source_id AND r.revision=p.revision WHERE m.library_id=?1 AND m.generation_id=?2 AND p.active=1 AND s.deleted=0 AND (?3 IS NULL OR s.kind=?3) AND (?4 OR s.visibility='public') ORDER BY p.id")?;
        let mut rows = stmt.query(params![
            self.config.library_id,
            generation,
            kind,
            include_private
        ])?;
        let mut semantic = Vec::<SearchHit>::new();
        while let Some(row) = rows.next()? {
            cancelled(&cancel)?;
            let bytes: Vec<u8> = row.get(12)?;
            let dimension: usize = row.get(13)?;
            if dimension != spec.dimension {
                return Err(Error::Conflict("mixed embedding dimensions".into()));
            }
            let vector = decode_vector(&bytes, dimension)?;
            let score = cosine(&query_vector, &vector)?;
            if semantic.len() == CANDIDATES
                && score
                    < semantic
                        .last()
                        .map(|h| h.score)
                        .unwrap_or(f64::NEG_INFINITY)
            {
                continue;
            }
            let quote: String = row.get(9)?;
            semantic.push(SearchHit {
                source_id: row.get(1)?,
                slug: row.get(2)?,
                title: row.get(3)?,
                source_kind: row.get(4)?,
                score,
                snippet: quote.chars().take(240).collect(),
                evidence: Evidence {
                    library_id: self.config.library_id.clone(),
                    source_id: row.get(1)?,
                    revision: row.get(5)?,
                    content_hash: row.get(6)?,
                    passage_id: row.get(0)?,
                    start: row.get(7)?,
                    end: row.get(8)?,
                    offset_unit: "utf8_byte".into(),
                    exact_quote: quote,
                    source_url: row.get(10)?,
                    locator: row.get(11)?,
                },
            });
            semantic.sort_by(|a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.evidence.passage_id.cmp(&b.evidence.passage_id))
            });
            semantic.truncate(CANDIDATES);
        }
        let mut fused: HashMap<String, SearchHit> = HashMap::new();
        for ranking in [&lexical.results, &semantic] {
            for (rank, hit) in ranking.iter().enumerate() {
                let score = 1.0 / (60.0 + (rank + 1) as f64);
                fused
                    .entry(hit.evidence.passage_id.clone())
                    .and_modify(|h| h.score += score)
                    .or_insert_with(|| {
                        let mut h = hit.clone();
                        h.score = score;
                        h
                    });
            }
        }
        lexical.results = fused.into_values().collect();
        if !include_private {
            for hit in &lexical.results {
                self.validate_public_source_snapshot(&hit.source_id)?;
            }
        }
        lexical.results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.evidence.passage_id.cmp(&b.evidence.passage_id))
        });
        lexical.results.truncate(top_k.min(CANDIDATES));
        lexical.mode = "hybrid".into();
        lexical.degradation = None;
        Ok(lexical)
    }
}

fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(ModelError::Cancelled.into());
    }
    Ok(())
}

fn degrade(mut result: SearchResult, reason: &str, top_k: usize) -> SearchResult {
    result.mode = "lexical".into();
    result.degradation = Some(reason.into());
    result.results.truncate(top_k.min(CANDIDATES));
    result
}

fn active_generation(db: &Connection, library: &str) -> Result<Option<(String, String, String)>> {
    Ok(db.query_row("SELECT g.id,g.spec_json,g.snapshot_hash FROM vector_active a JOIN vector_generations g ON g.id=a.generation_id AND g.library_id=a.library_id WHERE a.library_id=?1 AND g.state='ready'", [library], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)
}

fn read_snapshot(db: &Connection, library: &str) -> Result<Snapshot> {
    let mut stmt = db.prepare("SELECT p.id,p.quote,p.source_id,p.revision,s.hash,s.fingerprint,s.visibility,s.kind,p.start,p.end FROM passages p JOIN sources s ON s.id=p.source_id AND s.library_id=p.library_id WHERE p.library_id=?1 AND p.active=1 AND s.deleted=0 ORDER BY p.id LIMIT ?2")?;
    let mut rows = stmt.query(params![library, MAX_PASSAGES + 1])?;
    let mut passages = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = rows.next()? {
        let quote: String = row.get(1)?;
        bytes = bytes
            .checked_add(quote.len())
            .ok_or_else(|| Error::Invalid("vector snapshot too large".into()))?;
        if passages.len() >= MAX_PASSAGES || bytes > MAX_SNAPSHOT_BYTES {
            return Err(Error::Capability(
                "vector baseline supports at most 100000 passages / 128 MiB text per generation"
                    .into(),
            ));
        }
        passages.push(Passage {
            id: row.get(0)?,
            input_hash: hash(quote.as_bytes()),
            quote,
            source_id: row.get(2)?,
            revision: row.get(3)?,
            source_hash: row.get(4)?,
            parser_fingerprint: row.get(5)?,
            visibility: row.get(6)?,
            kind: row.get(7)?,
            start: row.get(8)?,
            end: row.get(9)?,
        });
    }
    let fingerprint = hash(&serde_json::to_vec(&passages)?);
    Ok(Snapshot {
        passages,
        fingerprint,
    })
}

fn validate_members(
    db: &Connection,
    library: &str,
    generation: &str,
    snapshot: &Snapshot,
    spec_hash: &str,
) -> Result<()> {
    let count: usize = db.query_row(
        "SELECT count(*) FROM vector_members WHERE library_id=?1 AND generation_id=?2",
        params![library, generation],
        |r| r.get(0),
    )?;
    if count != snapshot.passages.len() {
        return Err(Error::Conflict("incomplete vector generation".into()));
    }
    for passage in &snapshot.passages {
        let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM vector_members WHERE library_id=?1 AND generation_id=?2 AND passage_id=?3 AND spec_hash=?4 AND input_hash=?5)", params![library,generation,passage.id,spec_hash,passage.input_hash], |r| r.get(0))?;
        if !exists {
            return Err(Error::Conflict(
                "vector generation membership mismatch".into(),
            ));
        }
    }
    Ok(())
}

fn read_cache(
    db: &Connection,
    library: &str,
    spec_hash: &str,
    input_hash: &str,
    dimension: usize,
) -> Result<Option<Vec<f32>>> {
    let cached: Option<(usize,Vec<u8>)> = db.query_row("SELECT dimension,vector FROM embedding_cache WHERE library_id=?1 AND spec_hash=?2 AND input_hash=?3", params![library,spec_hash,input_hash], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
    cached
        .map(|(stored_dimension, bytes)| {
            if stored_dimension != dimension {
                return Err(Error::Conflict(
                    "cached embedding dimension mismatch".into(),
                ));
            }
            decode_vector(&bytes, dimension)
        })
        .transpose()
}

fn encode_vector(vector: &[f32], dimension: usize) -> Result<Vec<u8>> {
    validate_vector(vector, dimension)?;
    Ok(vector.iter().flat_map(|n| n.to_le_bytes()).collect())
}

fn decode_vector(bytes: &[u8], dimension: usize) -> Result<Vec<f32>> {
    if !(1..=MAX_DIMENSION).contains(&dimension) || bytes.len() != dimension * 4 {
        return Err(Error::Conflict("invalid vector byte length".into()));
    }
    let vector = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .collect::<Vec<_>>();
    validate_vector(&vector, dimension)?;
    Ok(vector)
}

fn validate_vector(vector: &[f32], dimension: usize) -> Result<()> {
    if vector.len() != dimension
        || vector.iter().any(|n| !n.is_finite())
        || vector.iter().all(|n| *n == 0.0)
    {
        return Err(Error::Conflict(
            "embedding must have matching dimensions and finite nonzero norm".into(),
        ));
    }
    Ok(())
}

fn cosine(left: &[f32], right: &[f32]) -> Result<f64> {
    if left.len() != right.len() {
        return Err(Error::Conflict("incompatible vector dimensions".into()));
    }
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (&a, &b) in left.iter().zip(right) {
        dot += f64::from(a) * f64::from(b);
        left_norm += f64::from(a) * f64::from(a);
        right_norm += f64::from(b) * f64::from(b);
    }
    Ok(dot / (left_norm.sqrt() * right_norm.sqrt()))
}
