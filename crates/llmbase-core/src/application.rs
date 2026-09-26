//! All transport adapters dispatch through this scope/capability boundary.
use crate::{Error, Evidence, Library, LibraryConfig, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Read,
    Write,
    Model,
    Private,
}

#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub libraries: HashSet<String>,
    pub capabilities: HashSet<Capability>,
}
impl Principal {
    pub fn local(library: &str) -> Self {
        Self {
            id: "local".into(),
            libraries: HashSet::from([library.into()]),
            capabilities: HashSet::from([
                Capability::Read,
                Capability::Write,
                Capability::Model,
                Capability::Private,
            ]),
        }
    }
    pub fn public(library: &str) -> Self {
        Self {
            id: "public".into(),
            libraries: HashSet::from([library.into()]),
            capabilities: HashSet::from([Capability::Read]),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub library_id: String,
    pub operation: String,
    #[serde(default = "empty_args")]
    pub args: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileRequest {
    pub source_id: String,
    /// Omitted profiles use this library's current configured language profile.
    #[serde(default)]
    pub profile: Option<crate::profile::CompileProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromoteRequest {
    pub answer_id: String,
    #[serde(default)]
    pub profile: Option<crate::answers::PromotionProfile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorBuildRequest {
    pub revision: String,
    pub dimension: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HybridSearchRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    pub kind: Option<String>,
}
fn default_top_k() -> usize {
    10
}
fn empty_args() -> Value {
    json!({})
}

#[derive(Clone)]
pub struct Application {
    libraries: HashMap<String, LibraryConfig>,
    /// Test-only barrier fired before a claimed operation opens its library
    /// handle; never set in production.
    operation_open_barrier: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}
impl Application {
    pub(crate) fn library_config(&self, library_id: &str) -> Result<LibraryConfig> {
        self.libraries.get(library_id).cloned().ok_or(Error::Scope)
    }

    /// Test-only hook installing the operation-open barrier.
    #[doc(hidden)]
    pub fn set_operation_open_barrier(
        &mut self,
        barrier: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    ) {
        self.operation_open_barrier = barrier;
    }

    pub fn new(configs: Vec<LibraryConfig>) -> Result<Self> {
        let mut libraries = HashMap::new();
        for cfg in configs {
            if libraries.insert(cfg.library_id.clone(), cfg).is_some() {
                return Err(Error::Conflict("duplicate library configuration".into()));
            }
        }
        if libraries.is_empty() {
            return Err(Error::Invalid("at least one library is required".into()));
        }
        Ok(Self {
            libraries,
            operation_open_barrier: None,
        })
    }

    pub fn start_ask(
        &self,
        principal: &Principal,
        models: crate::ask::AskModels,
        request: crate::ask::AskRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::ask::AskStream> {
        self.authorize(
            principal,
            &Request {
                library_id: request.library_id.clone(),
                operation: "get".into(),
                args: json!({}),
            },
        )?;
        if !principal.capabilities.contains(&Capability::Model) {
            return Err(Error::Scope);
        }
        let config = self
            .libraries
            .get(&request.library_id)
            .ok_or(Error::Scope)?
            .clone();
        crate::ask::start_ask(
            config,
            models,
            crate::ask::AskAccess {
                include_private: principal.capabilities.contains(&Capability::Private),
            },
            request,
            cancel,
        )
    }

    pub async fn ask(
        &self,
        principal: &Principal,
        models: crate::ask::AskModels,
        request: crate::ask::AskRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::ask::AskResponse> {
        self.start_ask(principal, models, request, cancel)?
            .collect()
            .await
    }

    pub async fn compile_source(
        &self,
        principal: &Principal,
        library_id: &str,
        client: llmbase_model::Client,
        request: CompileRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::compile::CompileReport> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "compile".into(),
                args: json!({}),
            },
        )?;
        if request.source_id.is_empty() {
            return Err(Error::Invalid("source_id is required".into()));
        }
        let config = self.libraries.get(library_id).ok_or(Error::Scope)?.clone();
        let profile = match request.profile {
            Some(profile) => profile,
            None => crate::profile::CompileProfile::from_config(&config.settings)?,
        };
        profile.validate()?;
        // Dropping a request cancels its provider work even while the blocking
        // storage worker is awaiting HTTP on the runtime handle.
        let _cancel_on_drop = cancel.clone().drop_guard();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let mut library = Library::open(config)?;
            library.sync()?;
            runtime.block_on(library.compile_source(&client, &request.source_id, &profile, cancel))
        })
        .await
        .map_err(|_| Error::Capability("compile worker failed".into()))?
    }

    /// Native host for the accepted legacy workflows; the underlying sequencing
    /// and journal are shared with the Python callback facade.
    pub async fn legacy_workflow(
        &self,
        principal: &Principal,
        library_id: &str,
        client: llmbase_model::Client,
        operation: &str,
        args: Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Value> {
        if !matches!(operation, "legacy_query" | "legacy_compile") {
            return Err(Error::Invalid("unknown legacy workflow".into()));
        }
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: operation.into(),
                args: args.clone(),
            },
        )?;
        let config = self.library_config(library_id)?;
        let mut args = args;
        let mut take_string = |key: &str, limit: usize| -> Result<Option<String>> {
            match args.as_object_mut().expect("authorized object").remove(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(value)) if value.len() <= limit && !value.trim().is_empty() => {
                    Ok(Some(value))
                }
                _ => Err(Error::Invalid(format!("invalid {key} override"))),
            }
        };
        let model = take_string("model", 200)?;
        let key = take_string("api_key", 500)?.map(llmbase_model::SecretKey::new);
        let answer_client = client.scoped_overrides(model, key)?;
        let operation = operation.to_owned();
        let runtime = tokio::runtime::Handle::current();
        let _guard = cancel.clone().drop_guard();
        tokio::task::spawn_blocking(move || {
            let response = if operation == "legacy_query" {
                crate::compat_workflows::query_with_clients(
                    config,
                    client,
                    answer_client,
                    args,
                    cancel,
                    runtime,
                )?
            } else {
                crate::compat_workflows::compile(config, client, args, cancel, runtime)?
            };
            Ok(response["value"].clone())
        })
        .await
        .map_err(|_| Error::Capability("legacy workflow worker failed".into()))?
    }

    pub async fn repair(
        &self,
        principal: &Principal,
        library_id: &str,
        client: Option<llmbase_model::Client>,
        mut args: Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Value> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "maintenance_repair".into(),
                args: args.clone(),
            },
        )?;
        let config = self.library_config(library_id)?;
        if args.get("profile").is_none() {
            args["profile"] = serde_json::to_value(crate::profile::LanguageProfile::from_config(
                &config.settings,
            )?)?;
        }
        let request: crate::maintenance_repair::RepairRequest = serde_json::from_value(args)?;
        let private = principal.capabilities.contains(&Capability::Private);
        let runtime = tokio::runtime::Handle::current();
        let _guard = cancel.clone().drop_guard();
        let open_barrier = self.operation_open_barrier.clone();
        tokio::task::spawn_blocking(move || {
            let mut library = open_operation_library(&config, &cancel, open_barrier)?;
            runtime.block_on(library.repair(client.as_ref(), &request, private, cancel))
        })
        .await
        .map_err(|_| Error::Capability("maintenance repair worker failed".into()))?
    }

    pub async fn generate_maintenance(
        &self,
        principal: &Principal,
        library_id: &str,
        client: llmbase_model::Client,
        operation: &str,
        args: Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Value> {
        if !matches!(
            operation,
            "taxonomy_generate" | "xici_generate" | "entities_generate"
        ) {
            return Err(Error::Capability(
                "unknown generated maintenance operation".into(),
            ));
        }
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: operation.into(),
                args: args.clone(),
            },
        )?;
        let config = self.library_config(library_id)?;
        let operation = operation.to_owned();
        let private = principal.capabilities.contains(&Capability::Private);
        let runtime = tokio::runtime::Handle::current();
        let _guard = cancel.clone().drop_guard();
        tokio::task::spawn_blocking(move || {
            let mut library = Library::open(config)?;
            runtime.block_on(async {
                if operation == "taxonomy_generate" {
                    library
                        .generate_taxonomy(&client, &serde_json::from_value(args)?, private, cancel)
                        .await
                } else if operation == "entities_generate" {
                    library
                        .generate_entities(&client, &serde_json::from_value(args)?, private, cancel)
                        .await
                } else {
                    library
                        .generate_xici(&client, &serde_json::from_value(args)?, private, cancel)
                        .await
                }
            })
        })
        .await
        .map_err(|_| Error::Capability("maintenance generation worker failed".into()))?
    }

    pub async fn promote_answer(
        &self,
        principal: &Principal,
        library_id: &str,
        client: llmbase_model::Client,
        request: PromoteRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::answers::PromotionReport> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "answer_promote".into(),
                args: json!({}),
            },
        )?;
        let config = self.libraries.get(library_id).ok_or(Error::Scope)?.clone();
        let profile = match request.profile {
            Some(profile) => profile,
            None => crate::answers::PromotionProfile {
                compile: crate::profile::CompileProfile::from_config(&config.settings)?,
                ..Default::default()
            },
        };
        let _cancel_on_drop = cancel.clone().drop_guard();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let mut library = Library::open(config)?;
            library.sync()?;
            runtime.block_on(library.promote_answer_with_profile(
                &client,
                &request.answer_id,
                &profile,
                cancel,
            ))
        })
        .await
        .map_err(|_| Error::Capability("promotion worker failed".into()))?
    }

    pub async fn build_vectors(
        &self,
        principal: &Principal,
        library_id: &str,
        client: llmbase_model::Client,
        request: VectorBuildRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::vector::VectorBuildReport> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "vector_build".into(),
                args: json!({}),
            },
        )?;
        let config = self.libraries.get(library_id).ok_or(Error::Scope)?.clone();
        let spec = crate::vector::EmbeddingSpec::new(
            &client,
            client.model(),
            request.revision,
            request.dimension,
        );
        let _cancel_on_drop = cancel.clone().drop_guard();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let mut library = Library::open(config)?;
            library.sync()?;
            runtime.block_on(library.build_vectors(&client, &spec, cancel))
        })
        .await
        .map_err(|_| Error::Capability("vector worker failed".into()))?
    }

    pub async fn search_hybrid(
        &self,
        principal: &Principal,
        library_id: &str,
        client: Option<llmbase_model::Client>,
        request: HybridSearchRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::SearchResult> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "hybrid_search".into(),
                args: json!({}),
            },
        )?;
        if client.is_some() && !principal.capabilities.contains(&Capability::Model) {
            return Err(Error::Scope);
        }
        if request.top_k > 100 {
            return Err(Error::Invalid("top_k must be 0..100".into()));
        }
        let config = self.libraries.get(library_id).ok_or(Error::Scope)?.clone();
        let private = principal.capabilities.contains(&Capability::Private);
        let _cancel_on_drop = cancel.clone().drop_guard();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let library = Library::open(config)?;
            runtime.block_on(library.search_hybrid(
                &request.query,
                request.top_k,
                request.kind.as_deref(),
                private,
                client.as_ref(),
                cancel,
            ))
        })
        .await
        .map_err(|_| Error::Capability("retrieval worker failed".into()))?
    }

    pub fn authorize(&self, principal: &Principal, request: &Request) -> Result<()> {
        if !principal.libraries.contains(&request.library_id)
            || !self.libraries.contains_key(&request.library_id)
        {
            return Err(Error::Scope);
        }
        if !request.args.is_object() {
            return Err(Error::Invalid(
                "operation arguments must be an object".into(),
            ));
        }
        let required = match request.operation.as_str() {
            "maintenance_repair"
                if request.args.get("mode").and_then(Value::as_str) == Some("deep_lint") =>
            {
                Capability::Model
            }
            "web_write_article_delete"
            | "web_write_wiki_clean"
            | "web_write_taxonomy_update"
            | "web_write_trail_save"
            | "web_write_trail_delete" => Capability::Write,
            "index" | "kb_index" | "source_write" | "source_delete" | "source_rename"
            | "job_enqueue" | "job_cancel" | "job_retry" | "draft_apply" | "answer_save"
            | "taxonomy_set" | "relation_add" | "relation_remove" | "maintenance_repair" => {
                Capability::Write
            }
            "rebuild_metadata"
            | "lint_fix"
            | "job_run_one"
            | "compat_compile_write"
            | "compat_compile_mark"
            | "compat_compile_reset"
            | "compat_compile_rebuild"
            | "compat_compile_backlinks"
            | "compat_pipeline_append"
            | "compat_pipeline_cache_put"
            | "compat_pipeline_cache_clear" => Capability::Write,
            "compile" | "answer_promote" | "vector_build" | "ask" | "plugin_ingest"
            | "taxonomy_generate" | "xici_generate" | "entities_generate" | "legacy_query"
            | "legacy_compile" => Capability::Model,
            "transcription_ask" => Capability::Model,
            "transcription_prepare" => Capability::Read,
            "compat_kb_raw_write"
            | "ingest_text"
            | "ingest_url"
            | "ingest_existing"
            | "ingest_asset" => Capability::Write,
            "job_get" | "job_list" | "job_events" => Capability::Private,
            "compat_compile_find"
            | "compat_compile_existing"
            | "compat_compile_read_raw"
            | "compat_compile_load_index"
            | "compat_compile_sources"
            | "compat_pipeline_events"
            | "compat_pipeline_tail"
            | "compat_pipeline_state"
            | "compat_pipeline_cache_get"
            | "web_config"
            | "web_articles"
            | "web_article"
            | "web_aliases"
            | "web_collections"
            | "web_sources"
            | "web_source"
            | "web_stats"
            | "web_health"
            | "web_wiki_export"
            | "web_taxonomy"
            | "web_xici"
            | "web_entities"
            | "web_export_article"
            | "web_export_tag"
            | "web_export_graph"
            | "web_trails"
            | "web_worker_status"
            | "web_search"
            | "web_lint" => Capability::Read,
            "web_tones" | "web_refs_plugins" => Capability::Read,
            "compat_search"
            | "compat_kb_get"
            | "compat_kb_list"
            | "compat_kb_sections"
            | "compat_kb_stats"
            | "compat_kb_raw_list"
            | "compat_kb_health"
            | "compat_kb_visibility"
            | "compat_kb_raw_hash"
            | "compat_kb_trails"
            | "compat_search_documents"
            | "kb_search"
            | "kb_search_raw"
            | "search"
            | "kb_get"
            | "get"
            | "kb_list"
            | "list"
            | "kb_stats"
            | "evidence"
            | "draft_get"
            | "answer_get"
            | "hybrid_search"
            | "aliases"
            | "backlinks"
            | "backlinks_compat"
            | "graph"
            | "taxonomy"
            | "entities"
            | "doctor" => Capability::Read,
            "export_article" | "export_tag" | "export_graph" | "lint" | "xici" => Capability::Read,
            _ => {
                return Err(Error::Capability(format!(
                    "unknown or unimplemented operation: {}",
                    request.operation
                )));
            }
        };
        if !principal.capabilities.contains(&required) {
            return Err(Error::Scope);
        }
        if matches!(
            request.operation.as_str(),
            "ingest_text" | "ingest_url" | "ingest_existing" | "ingest_asset"
        ) && (!principal.capabilities.contains(&Capability::Read)
            || !principal.capabilities.contains(&Capability::Private))
        {
            return Err(Error::Scope);
        }
        if request.operation == "maintenance_repair" {
            let mode = request
                .args
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("auto_fix");
            if !principal.capabilities.contains(&Capability::Read)
                || (!matches!(mode, "clean" | "heal_slugs")
                    && !principal.capabilities.contains(&Capability::Model))
                || (request.args.get("private_inputs") == Some(&json!(true))
                    && !principal.capabilities.contains(&Capability::Private))
            {
                return Err(Error::Scope);
            }
        }
        if matches!(
            request.operation.as_str(),
            "compile"
                | "answer_promote"
                | "vector_build"
                | "draft_get"
                | "draft_apply"
                | "plugin_ingest"
                | "taxonomy_generate"
                | "xici_generate"
                | "entities_generate"
                | "legacy_compile"
        ) {
            // Compilation considers existing articles for merge, including private
            // candidates. This operator boundary precedes storage and model work.
            for capability in [Capability::Read, Capability::Private] {
                if !principal.capabilities.contains(&capability) {
                    return Err(Error::Scope);
                }
            }
            if matches!(
                request.operation.as_str(),
                "compile"
                    | "answer_promote"
                    | "vector_build"
                    | "plugin_ingest"
                    | "taxonomy_generate"
                    | "xici_generate"
                    | "entities_generate"
                    | "legacy_compile"
            ) && !principal.capabilities.contains(&Capability::Write)
            {
                return Err(Error::Scope);
            }
        }
        if matches!(
            request.operation.as_str(),
            "ask"
                | "transcription_ask"
                | "legacy_query"
                | "answer_save"
                | "taxonomy_set"
                | "relation_add"
                | "relation_remove"
                | "lint_fix"
        ) && !principal.capabilities.contains(&Capability::Read)
        {
            return Err(Error::Scope);
        }
        if request.operation == "legacy_query" {
            let mode = request
                .args
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("basic");
            if !matches!(mode, "basic" | "deep") {
                return Err(Error::Invalid(
                    "legacy query mode must be basic or deep".into(),
                ));
            }
            if ["file_back", "promote"]
                .iter()
                .any(|key| request.args.get(key) == Some(&json!(true)))
                && (!principal.capabilities.contains(&Capability::Write)
                    || !principal.capabilities.contains(&Capability::Private))
            {
                // A legacy write can merge an existing article; restrict it to
                // operators even though default retrieval considers public text.
                return Err(Error::Scope);
            }
        }
        if request.operation.starts_with("web_write_")
            && !principal.capabilities.contains(&Capability::Read)
        {
            return Err(Error::Scope);
        }
        if request.operation.starts_with("job_")
            && !principal.capabilities.contains(&Capability::Private)
        {
            return Err(Error::Scope);
        }
        if (request.operation.starts_with("compat_search")
            || request.operation.starts_with("compat_kb_")
            || request.operation.starts_with("compat_compile_")
            || request.operation.starts_with("compat_pipeline_")
            || matches!(
                request.operation.as_str(),
                "web_worker_status" | "web_write_trail_save" | "web_write_trail_delete"
            ))
            && !principal.capabilities.contains(&Capability::Private)
        {
            return Err(Error::Scope);
        }
        Ok(())
    }

    pub fn execute(&self, principal: &Principal, request: Request) -> Result<Value> {
        self.authorize(principal, &request)?;
        let config = self
            .libraries
            .get(&request.library_id)
            .ok_or(Error::Scope)?
            .clone();
        let private = principal.capabilities.contains(&Capability::Private);
        if request.operation == "transcription_prepare" {
            let mut args = request.args;
            if args.get("library_id").is_some() {
                return Err(Error::Invalid(
                    "library_id belongs in the operation envelope".into(),
                ));
            }
            args["library_id"] = json!(request.library_id);
            return Ok(serde_json::to_value(self.prepare_transcription(
                principal,
                serde_json::from_value(args)?,
            )?)?);
        }
        if request.operation.starts_with("compat_kb_") {
            return crate::compat_kb::execute(&config, &request.operation, &request.args);
        }
        if request.operation.starts_with("compat_compile_") {
            return crate::compat_compile::execute(&config, &request.operation, &request.args);
        }
        if request.operation.starts_with("compat_pipeline_") {
            return crate::compat_pipeline::execute(&config, &request.operation, &request.args);
        }
        if request.operation == "compat_search_documents" {
            return crate::compat_search::documents(
                &config,
                required_string(&request.args, "kind")?,
                private && request.args.get("public_only") != Some(&json!(true)),
            );
        }
        if request.operation == "compat_search" {
            return crate::compat_search::search(&config, &request.args, private);
        }
        let mut library = Library::open(config)?;
        let args = &request.args;
        if request.operation.starts_with("web_write_") {
            return crate::web_write::execute(&mut library, &request.operation, args, private);
        }
        if request.operation.starts_with("web_") {
            return crate::web_read::execute(&mut library, &request.operation, args, private);
        }
        match request.operation.as_str() {
            "ingest_text" => library.ingest_text(serde_json::from_value(args.clone())?),
            "ingest_existing" => library.ingest_existing(required_string(args, "source")?),
            "ingest_asset" => library.write_raw_asset(
                required_string(args, "path")?,
                serde_json::from_value(
                    args.get("content_bytes")
                        .cloned()
                        .ok_or_else(|| Error::Invalid("content_bytes required".into()))?,
                )?,
                args.get("expected_hash").and_then(Value::as_str),
            ),
            "rebuild_metadata" => library.rebuild_metadata(),
            "export_article" => {
                let profile =
                    crate::profile::LanguageProfile::from_config(&library.config.settings)?;
                library.export_article(required_string(args, "slug")?, &profile, private)
            }
            "export_tag" => library.export_tag(required_string(args, "tag")?, private),
            "export_graph" => {
                let depth = args
                    .get("depth")
                    .map(|v| {
                        v.as_u64()
                            .filter(|d| *d <= 4)
                            .ok_or_else(|| Error::Invalid("depth must be 0..4".into()))
                    })
                    .transpose()?
                    .unwrap_or(2);
                library.export_graph(required_string(args, "slug")?, depth as usize, private)
            }
            "lint" => library.lint(serde_json::from_value(args.clone())?, private),
            "lint_fix" => library.fix_deterministic(serde_json::from_value(args.clone())?, private),
            "xici" => library.xici(
                args.get("lang").and_then(Value::as_str).unwrap_or("en"),
                private,
            ),
            "compile" | "answer_promote" | "vector_build" | "hybrid_search" | "ask"
            | "job_run_one" | "plugin_ingest" | "taxonomy_generate" | "xici_generate"
            | "entities_generate" | "legacy_query" | "legacy_compile" | "maintenance_repair"
            | "ingest_url" | "transcription_ask" => Err(Error::Capability(
                "operation requires the asynchronous model entry point".into(),
            )),
            "answer_save" => {
                let response = serde_json::from_value(
                    args.get("response")
                        .cloned()
                        .ok_or_else(|| Error::Invalid("response is required".into()))?,
                )?;
                let accepted = serde_json::from_value(
                    args.get("request")
                        .cloned()
                        .ok_or_else(|| Error::Invalid("request is required".into()))?,
                )?;
                Ok(serde_json::to_value({
                    library.sync()?;
                    library.save_answer(&response, &accepted, private)?
                })?)
            }
            "answer_get" => Ok(serde_json::to_value(
                library.saved_answer(required_string(args, "answer_id")?, private)?,
            )?),
            "aliases" => library.aliases(private),
            "backlinks" => library.backlinks(required_string(args, "target")?, private),
            "backlinks_compat" => library.legacy_backlinks(required_string(args, "slug")?, private),
            "graph" => library.graph(serde_json::from_value(args.clone())?, private),
            "taxonomy" => library.taxonomy(
                args.get("lang").and_then(Value::as_str).unwrap_or("en"),
                private,
            ),
            "taxonomy_set" => library.set_taxonomy(
                args.get("document")
                    .cloned()
                    .ok_or_else(|| Error::Invalid("document is required".into()))?,
                private,
            ),
            "entities" => library.entities(private),
            "relation_add" => library.add_relation(serde_json::from_value(args.clone())?, private),
            "relation_remove" => {
                library.remove_relation(required_string(args, "relation_id")?, private)
            }
            "draft_get" => Ok(serde_json::to_value(
                library.draft(required_string(args, "draft_id")?)?,
            )?),
            "draft_apply" => {
                let expected = match args.get("expected_hash") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(hash)) => Some(hash.as_str()),
                    _ => {
                        return Err(Error::Invalid(
                            "expected_hash must be a string or null".into(),
                        ));
                    }
                };
                Ok(serde_json::to_value(library.apply_draft(
                    required_string(args, "draft_id")?,
                    expected,
                )?)?)
            }
            "index" | "kb_index" => Ok(serde_json::to_value(library.sync()?)?),
            "source_write" => {
                let _writer = crate::store::WriterSession::acquire(&library.config)?;
                if !private {
                    library.sync()?;
                }
                let kind = required_string(args, "kind")?;
                let path = required_string(args, "path")?;
                let content = required_string(args, "content")?;
                let document = crate::document::Document::parse(content.as_bytes().to_vec())?;
                if !private {
                    if !crate::visibility::is_public(&document.metadata) {
                        return Err(Error::Scope);
                    }
                    for source in library.list(Some(kind), true)? {
                        if source.rel_path == path {
                            library.get(&source.source_id, None, false)?;
                        }
                    }
                }
                Ok(serde_json::to_value(library.write_source(
                    kind,
                    path,
                    content.as_bytes().to_vec(),
                    args.get("expected_hash").and_then(Value::as_str),
                )?)?)
            }
            "source_delete" | "source_rename" => {
                let _writer = crate::store::WriterSession::acquire(&library.config)?;
                if !private {
                    library.sync()?;
                }
                let id = required_string(args, "source_id")?;
                library.get(id, None, private)?;
                let expected = required_string(args, "expected_hash")?;
                let report = if request.operation == "source_delete" {
                    library.delete_source(id, expected)?
                } else {
                    library.rename_source(id, required_string(args, "destination")?, expected)?
                };
                Ok(serde_json::to_value(report)?)
            }
            "job_enqueue" => {
                let operation = required_string(args, "operation")?;
                if operation.starts_with("job_") {
                    return Err(Error::Invalid(
                        "nested job-management jobs are not allowed".into(),
                    ));
                }
                let job_args = args.get("args").cloned().unwrap_or_else(empty_args);
                self.authorize(
                    principal,
                    &Request {
                        library_id: request.library_id.clone(),
                        operation: operation.into(),
                        args: job_args.clone(),
                    },
                )?;
                let max_attempts = args
                    .get("max_attempts")
                    .map(|v| {
                        v.as_u64()
                            .filter(|v| *v <= 20)
                            .ok_or_else(|| Error::Invalid("invalid max_attempts".into()))
                    })
                    .transpose()?
                    .unwrap_or(3) as u32;
                Ok(serde_json::to_value(library.enqueue_job(
                    operation,
                    job_args,
                    max_attempts,
                )?)?)
            }
            "job_get" => Ok(serde_json::to_value(
                library.job(required_string(args, "job_id")?)?,
            )?),
            "job_list" => Ok(json!({"jobs":library.jobs()?})),
            "job_events" => {
                Ok(json!({"events":library.job_events(required_string(args,"job_id")?)?}))
            }
            "job_cancel" => Ok(serde_json::to_value(
                library.cancel_job(required_string(args, "job_id")?)?,
            )?),
            "job_retry" => Ok(serde_json::to_value(
                library.retry_job(required_string(args, "job_id")?)?,
            )?),
            "search" | "kb_search" | "kb_search_raw" => {
                let query = required_string(args, "query")?;
                let top_k = match args.get("top_k") {
                    None => 10,
                    Some(v) => v.as_u64().filter(|k| *k <= 100).ok_or_else(|| {
                        Error::Invalid("top_k must be an integer from 0 to 100".into())
                    })? as usize,
                };
                let kind = match request.operation.as_str() {
                    "kb_search_raw" => Some("raw"),
                    "kb_search" => Some("compiled"),
                    _ => args.get("kind").and_then(Value::as_str),
                };
                Ok(serde_json::to_value(
                    library.search(query, top_k, kind, private)?,
                )?)
            }
            "get" | "kb_get" => {
                let id = args
                    .get("source_id")
                    .or_else(|| args.get("slug"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Invalid("source_id or slug is required".into()))?;
                let revision = args
                    .get("revision")
                    .map(|v| {
                        v.as_u64()
                            .ok_or_else(|| Error::Invalid("revision must be an integer".into()))
                    })
                    .transpose()?;
                Ok(serde_json::to_value(library.get(id, revision, private)?)?)
            }
            "list" | "kb_list" => Ok(
                json!({"sources":library.list(args.get("kind").and_then(Value::as_str),private)?}),
            ),
            "kb_stats" => {
                let all = library.list(None, private)?;
                Ok(
                    json!({"library_id":request.library_id,"raw_count":all.iter().filter(|s|s.kind=="raw").count(),"article_count":all.iter().filter(|s|s.kind=="compiled").count(),"output_count":all.iter().filter(|s|s.kind=="note").count()}),
                )
            }
            "evidence" => {
                let evidence: Evidence = serde_json::from_value(
                    args.get("evidence")
                        .ok_or_else(|| Error::Invalid("evidence required".into()))?
                        .clone(),
                )?;
                Ok(serde_json::to_value(
                    library.resolve_evidence(&evidence, private)?,
                )?)
            }
            "doctor" => Ok(
                json!({"library_id":request.library_id,"core":"rust","schema_version":1,"fts5":true,"embedding":"unconfigured"}),
            ),
            _ => unreachable!("authorization rejects unavailable operations"),
        }
    }
}

/// A claimed job opens its own library handle only here, strictly before any
/// model call or accepted write; a concurrent reader may briefly hold the
/// recovery writer lock. Retry just this pre-side-effect open with a bounded
/// delay under the same cancellation token — the operation itself and its
/// side effects are never replayed.
fn open_operation_library(
    config: &LibraryConfig,
    cancel: &tokio_util::sync::CancellationToken,
    barrier: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
) -> Result<Library> {
    const ATTEMPTS: u32 = 100;
    if let Some(barrier) = barrier {
        barrier();
    }
    for attempt in 0..ATTEMPTS {
        if cancel.is_cancelled() {
            return Err(llmbase_model::ModelError::Cancelled.into());
        }
        match Library::open(config.clone()) {
            Err(Error::Busy) if attempt + 1 < ATTEMPTS => {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!("the final attempt returns its result")
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Invalid(format!("{key} must be a nonempty string")))
}

impl Application {
    /// A read-scoped pure mapping; never creates a library or invokes a model.
    pub fn prepare_transcription(
        &self,
        principal: &Principal,
        request: crate::transcription::TranscriptionRequest,
    ) -> Result<crate::transcription::PreparedTranscription> {
        self.authorize(
            principal,
            &Request {
                library_id: request.library_id.clone(),
                operation: "get".into(),
                args: json!({}),
            },
        )?;
        crate::transcription::prepare_transcription(&request.library_id, &request)
    }

    /// Reuses the native ask authorization, private grants, and no-save policy.
    pub fn start_transcription_ask(
        &self,
        principal: &Principal,
        models: crate::ask::AskModels,
        request: crate::transcription::TranscriptionRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<crate::transcription::TranscriptionStream> {
        if !principal.capabilities.contains(&Capability::Model) {
            return Err(Error::Scope);
        }
        let prepared = self.prepare_transcription(principal, request)?;
        if cancel.is_cancelled() {
            return Err(llmbase_model::ModelError::Cancelled.into());
        }
        let ask = self.start_ask(principal, models, prepared.question, cancel)?;
        Ok(crate::transcription::TranscriptionStream::from_ask(
            prepared.transcription,
            ask,
        ))
    }
}
