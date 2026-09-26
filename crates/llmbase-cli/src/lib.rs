//! Thin CLI, HTTP and official MCP SDK adapters over the native application service.
pub mod http;
pub mod ingest_local;
pub mod mcp;
pub mod model_env;
pub mod static_ui;

use llmbase_core::{
    Error, Source,
    application::{Application, Principal, Request},
};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};

/// Historical tool schemas are data, frozen from the Python operation registry.
pub fn operation_catalog() -> &'static [Value] {
    static CATALOG: OnceLock<Vec<Value>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("operations.json"))
            .expect("checked-in operation schemas must be valid JSON")
    })
}

/// The first vertical slice exposes only implemented native tools in discovery.
/// Known-but-unavailable legacy calls receive an explicit capability error.
pub fn native_tool_available(name: &str) -> bool {
    matches!(
        name,
        "kb_search"
            | "kb_search_raw"
            | "kb_get"
            | "kb_get_sections"
            | "kb_list"
            | "kb_stats"
            | "kb_rebuild_index"
            | "kb_taxonomy"
            | "kb_xici"
            | "kb_export_article"
            | "kb_export_tag"
            | "kb_export_graph"
            | "kb_export"
            | "kb_ask"
            | "kb_compile"
            | "kb_backlinks"
            | "kb_lint"
            | "kb_lint_fix"
            | "kb_ingest"
    )
}

#[derive(Clone)]
pub struct Gateway {
    pub application: Arc<Application>,
    pub default_library: String,
    pub models: Option<llmbase_core::ask::AskModels>,
}

impl Gateway {
    pub fn new(application: Application, default_library: String) -> Self {
        Self {
            application: Arc::new(application),
            default_library,
            models: None,
        }
    }

    pub fn with_models(mut self, models: Option<llmbase_core::ask::AskModels>) -> Self {
        self.models = models;
        self
    }

    pub async fn execute(&self, principal: Principal, request: Request) -> Result<Value, Error> {
        // Authorize before scheduling work or opening any storage.
        self.application.authorize(&principal, &request)?;
        if request.operation == "transcription_ask" {
            let mut args = request.args;
            if args.get("library_id").is_some() {
                return Err(Error::Invalid(
                    "library_id belongs in the operation envelope".into(),
                ));
            }
            args["library_id"] = json!(request.library_id);
            let request: llmbase_core::transcription::TranscriptionRequest =
                serde_json::from_value(args)?;
            // Validate supplied text/unsupported image capability before any model I/O.
            self.application
                .prepare_transcription(&principal, request.clone())?;
            let models = match self.models.clone() {
                Some(models) => models,
                None => model_env::from_environment()?.ok_or_else(|| {
                    Error::Capability("generation model is not configured".into())
                })?,
            };
            return Ok(serde_json::to_value(
                self.application
                    .start_transcription_ask(
                        &principal,
                        models,
                        request,
                        llmbase_model::CancellationToken::new(),
                    )?
                    .collect()
                    .await?,
            )?);
        }
        if request.operation == "ingest_url" {
            let url = request
                .args
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Invalid("url is required".into()))?;
            return self
                .application
                .ingest_url(
                    &principal,
                    &request.library_id,
                    url,
                    llmbase_model::CancellationToken::new(),
                )
                .await;
        }
        if request.operation == "maintenance_repair" {
            let no_model = matches!(
                request.args.get("mode").and_then(Value::as_str),
                Some("clean" | "heal_slugs")
            );
            let client = if no_model {
                None
            } else {
                Some(match self.models.clone() {
                    Some(models) => models.generation,
                    None => {
                        model_env::from_environment()?
                            .ok_or_else(|| {
                                Error::Capability("generation model is not configured".into())
                            })?
                            .generation
                    }
                })
            };
            return self
                .application
                .repair(
                    &principal,
                    &request.library_id,
                    client,
                    request.args,
                    llmbase_model::CancellationToken::new(),
                )
                .await;
        }
        if matches!(
            request.operation.as_str(),
            "legacy_query" | "legacy_compile"
        ) {
            let models = match self.models.clone() {
                Some(models) => models,
                None => model_env::from_environment()?.ok_or_else(|| {
                    Error::Capability("generation model is not configured".into())
                })?,
            };
            return self
                .application
                .legacy_workflow(
                    &principal,
                    &request.library_id,
                    models.generation,
                    &request.operation,
                    request.args,
                    llmbase_model::CancellationToken::new(),
                )
                .await;
        }
        if matches!(
            request.operation.as_str(),
            "taxonomy_generate" | "xici_generate" | "entities_generate"
        ) {
            let models = match self.models.clone() {
                Some(models) => models,
                None => model_env::from_environment()?.ok_or_else(|| {
                    Error::Capability("generation model is not configured".into())
                })?,
            };
            return self
                .application
                .generate_maintenance(
                    &principal,
                    &request.library_id,
                    models.generation,
                    &request.operation,
                    request.args,
                    llmbase_model::CancellationToken::new(),
                )
                .await;
        }
        if request.operation == "plugin_ingest" {
            return self
                .application
                .ingest_with_adapter(
                    &principal,
                    &request.library_id,
                    serde_json::from_value(request.args)?,
                    llmbase_model::CancellationToken::new(),
                )
                .await;
        }
        if request.operation == "job_run_one" {
            let models = match self.models.clone() {
                Some(models) => Some(models),
                None => model_env::from_environment()?,
            };
            let worker_models = llmbase_core::worker::WorkerModels {
                generation: models.as_ref().map(|m| m.generation.clone()),
                embedding: match models.and_then(|m| m.embedding) {
                    Some(client) => Some(client),
                    None => model_env::embedding_from_environment()?,
                },
            };
            let worker_id = request
                .args
                .get("worker_id")
                .and_then(Value::as_str)
                .unwrap_or("native-worker");
            return Ok(serde_json::to_value(
                self.application
                    .run_job(
                        &principal,
                        &request.library_id,
                        worker_models,
                        llmbase_core::worker::WorkerSelection {
                            worker_id: worker_id.into(),
                            job_id: request
                                .args
                                .get("job_id")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                        },
                        llmbase_model::CancellationToken::new(),
                    )
                    .await?,
            )?);
        }
        if matches!(request.operation.as_str(), "vector_build" | "hybrid_search") {
            let client = match self
                .models
                .as_ref()
                .and_then(|models| models.embedding.clone())
            {
                Some(client) => Some(client),
                None => model_env::embedding_from_environment()?,
            };
            if request.operation == "vector_build" {
                let client = client.ok_or_else(|| {
                    Error::Capability("configure an embedding model before building vectors".into())
                })?;
                return Ok(serde_json::to_value(
                    self.application
                        .build_vectors(
                            &principal,
                            &request.library_id,
                            client,
                            serde_json::from_value(request.args)?,
                            llmbase_model::CancellationToken::new(),
                        )
                        .await?,
                )?);
            }
            return Ok(serde_json::to_value(
                self.application
                    .search_hybrid(
                        &principal,
                        &request.library_id,
                        client,
                        serde_json::from_value(request.args)?,
                        llmbase_model::CancellationToken::new(),
                    )
                    .await?,
            )?);
        }
        if matches!(
            request.operation.as_str(),
            "compile" | "answer_promote" | "ask"
        ) {
            let models = match self.models.clone() {
                Some(models) => models,
                None => model_env::from_environment()?.ok_or_else(|| {
                    Error::Capability(
                        "configure LLMBASE_BASE_URL and LLMBASE_MODEL for generation".into(),
                    )
                })?,
            };
            if request.operation == "ask" {
                let mut args = request.args;
                if args.get("library_id").is_some() {
                    return Err(Error::Invalid(
                        "library_id belongs in the operation envelope".into(),
                    ));
                }
                args["library_id"] = json!(request.library_id);
                return Ok(serde_json::to_value(
                    self.application
                        .ask(
                            &principal,
                            models,
                            serde_json::from_value(args)?,
                            llmbase_model::CancellationToken::new(),
                        )
                        .await?,
                )?);
            }
            if request.operation == "answer_promote" {
                let args = serde_json::from_value(request.args)?;
                return Ok(serde_json::to_value(
                    self.application
                        .promote_answer(
                            &principal,
                            &request.library_id,
                            models.generation,
                            args,
                            llmbase_model::CancellationToken::new(),
                        )
                        .await?,
                )?);
            }
            let args = serde_json::from_value(request.args)?;
            return Ok(serde_json::to_value(
                self.application
                    .compile_source(
                        &principal,
                        &request.library_id,
                        models.generation,
                        args,
                        llmbase_model::CancellationToken::new(),
                    )
                    .await?,
            )?);
        }
        let application = self.application.clone();
        tokio::task::spawn_blocking(move || application.execute(&principal, request))
            .await
            .map_err(|_| Error::Capability("application worker failed".into()))?
    }

    pub async fn legacy(
        &self,
        principal: Principal,
        library_id: String,
        name: &str,
        args: Value,
    ) -> Result<Value, Error> {
        validate_legacy_args(name, &args)?;
        let mut native_args = args.clone();
        let operation = match name {
            "kb_ingest" => {
                let source = args
                    .get("source")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        args.get("url")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                    })
                    .ok_or_else(|| Error::Invalid("source or url is required".into()))?;
                if source.starts_with("https://") || source.starts_with("http://") {
                    native_args = json!({"url":source});
                    "ingest_url"
                } else {
                    native_args = json!({"source":source});
                    "ingest_existing"
                }
            }
            "kb_lint_fix" => {
                native_args = json!({"mode":"auto_fix"});
                "maintenance_repair"
            }
            "kb_lint" => {
                if args.get("fix") == Some(&json!(true)) {
                    native_args = json!({"mode":"auto_fix"});
                    "maintenance_repair"
                } else if args.get("deep") == Some(&json!(true)) {
                    native_args = json!({"mode":"deep_lint"});
                    "maintenance_repair"
                } else {
                    native_args = json!({});
                    "web_lint"
                }
            }
            "kb_backlinks" => "backlinks_compat",
            "kb_ask" => {
                let deep = args.get("deep").and_then(Value::as_bool).unwrap_or(true);
                native_args
                    .as_object_mut()
                    .expect("validated object")
                    .remove("deep");
                native_args["mode"] = json!(if deep { "deep" } else { "basic" });
                native_args["return_context"] = json!(deep);
                "legacy_query"
            }
            "kb_compile" => {
                native_args =
                    json!({"mode":if args.get("full")==Some(&json!(true)){"all"}else{"new"}});
                "legacy_compile"
            }
            "kb_rebuild_index" => "rebuild_metadata",
            "kb_get_sections" => {
                native_args["sections"] = json!(true);
                "web_article"
            }
            "kb_taxonomy" => {
                if native_args.get("lang").is_none_or(Value::is_null) {
                    native_args["lang"] = json!("zh");
                }
                "web_taxonomy"
            }
            "kb_xici" => {
                if native_args.get("lang").is_none_or(Value::is_null) {
                    native_args["lang"] = json!("zh");
                }
                "web_xici"
            }
            "kb_export_article" => "web_export_article",
            "kb_export_tag" => "web_export_tag",
            "kb_export_graph" => "web_export_graph",
            "kb_export" => match args["type"].as_str() {
                Some("article") => "web_export_article",
                Some("tag") => {
                    native_args = json!({"tag":args["slug"]});
                    "web_export_tag"
                }
                Some("graph") => "web_export_graph",
                _ => return Err(Error::Invalid("unknown export type".into())),
            },
            _ => name,
        };
        let request = Request {
            library_id: library_id.clone(),
            operation: operation.into(),
            args: native_args,
        };
        let result = self.execute(principal.clone(), request).await;
        if (matches!(name, "kb_get" | "kb_get_sections" | "kb_export_article")
            || name == "kb_export" && args["type"] == "article")
            && matches!(result, Err(Error::NotFound(_)))
        {
            return Ok(json!({"found":false,"slug":args.get("slug")}));
        }
        let value = result?;
        match name {
            "kb_lint" if args.get("fix") != Some(&json!(true)) => {
                let report = if args.get("deep") == Some(&json!(true)) {
                    value
                        .pointer("/stages/0/value/report")
                        .cloned()
                        .ok_or_else(|| Error::Invalid("deep lint returned no report".into()))?
                } else {
                    value
                };
                Ok(json!({"report":report}))
            }
            "kb_ask" => Ok(if value.is_object() {
                value
            } else {
                json!({"answer":value})
            }),
            "kb_taxonomy" => Ok(json!({"categories":value["categories"]})),
            "kb_get_sections" => Ok(
                json!({"found":true,"slug":value["slug"],"title":value["title"],"sections":value["sections"]}),
            ),
            "kb_search" | "kb_search_raw" => Ok(json!({"results":value["results"]})),
            "kb_get" => {
                let source: Source = serde_json::from_value(value)?;
                let mut value = article(&source, true);
                if let Some(section) = args
                    .get("section")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    let body = value["content"].as_str().unwrap_or("");
                    let sections = llmbase_core::compat_kb::parse_sections(body);
                    let span = llmbase_core::compat_kb::find_section(&sections, section);
                    let content = span.map(|(start, end)| {
                        body.chars()
                            .skip(start)
                            .take(end - start)
                            .collect::<String>()
                    });
                    value
                        .as_object_mut()
                        .expect("article object")
                        .remove("content");
                    value["section"] = json!(section);
                    value["section_found"] = json!(span.is_some());
                    if let Some(content) = content {
                        value["content"] = json!(content);
                    }
                }
                Ok(value)
            }
            "kb_list" => {
                let sources: Vec<Source> = serde_json::from_value(value["sources"].clone())?;
                let tag = args.get("tag").and_then(Value::as_str);
                let articles: Vec<_> = sources
                    .iter()
                    .filter(|s| s.kind == "compiled")
                    .filter(|s| {
                        tag.is_none_or(|tag| {
                            s.metadata
                                .get("tags")
                                .and_then(Value::as_array)
                                .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(tag)))
                        })
                    })
                    .map(article_summary)
                    .collect();
                Ok(json!({"articles":articles}))
            }
            "kb_stats" => {
                // Old total_words counts whitespace-separated tokens from the original
                // concept markdown, not CJK tokens or vector-index dimensions.
                let sources = self
                    .execute(
                        principal,
                        Request {
                            library_id,
                            operation: "list".into(),
                            args: json!({"kind":"compiled"}),
                        },
                    )
                    .await?;
                let sources: Vec<Source> = serde_json::from_value(sources["sources"].clone())?;
                let words: usize = sources
                    .iter()
                    .map(|s| s.content.split_whitespace().count())
                    .sum();
                Ok(
                    json!({"articles":value["article_count"],"raw_documents":value["raw_count"],"filed_outputs":value["output_count"],"total_words":words}),
                )
            }
            "kb_rebuild_index" => {
                let stats = self
                    .execute(
                        principal,
                        Request {
                            library_id,
                            operation: "kb_stats".into(),
                            args: json!({}),
                        },
                    )
                    .await?;
                Ok(json!({"article_count":stats["article_count"]}))
            }
            _ => Ok(value),
        }
    }
}

/// The historical registry invoked Python keyword arguments. Unknown argument
/// names therefore failed rather than disappearing silently. These schemas are
/// shallow primitive objects; native DTO validation still applies afterwards.
pub fn validate_legacy_args(name: &str, args: &Value) -> Result<(), Error> {
    let op = operation_catalog()
        .iter()
        .find(|op| op["name"] == name)
        .ok_or_else(|| Error::NotFound("operation".into()))?;
    let object = args
        .as_object()
        .ok_or_else(|| Error::Invalid("arguments must be an object".into()))?;
    let schema = &op["params"];
    if let Some(required) = schema["required"].as_array() {
        for key in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(Error::Invalid(format!("missing required argument: {key}")));
            }
        }
    }
    for (key, value) in object {
        let property = schema["properties"]
            .get(key)
            .ok_or_else(|| Error::Invalid("unknown operation argument".into()))?;
        let valid = match property["type"].as_str() {
            Some("string") => value.is_string(),
            Some("integer") => value.is_i64() || value.is_u64(),
            Some("boolean") => value.is_boolean(),
            _ => true,
        };
        // Optional Python kwargs historically accept None; used by facade
        // callers to mean default. Required values remain strict native DTOs.
        if !valid && !value.is_null() {
            return Err(Error::Invalid(format!("invalid argument type: {key}")));
        }
        if let Some(choices) = property["enum"].as_array()
            && !choices.contains(value)
        {
            return Err(Error::Invalid(format!("invalid argument value: {key}")));
        }
    }
    Ok(())
}

pub fn article_summary(source: &Source) -> Value {
    json!({"slug":source.slug,"title":source.title,
        "summary":source.metadata.get("summary").cloned().unwrap_or(json!("")),
        "tags":source.metadata.get("tags").cloned().unwrap_or(json!([]))})
}

pub fn article(source: &Source, found: bool) -> Value {
    let mut value = article_summary(source);
    value["sources"] = source.metadata.get("sources").cloned().unwrap_or(json!([]));
    value["content"] = json!(source.content.get(source.body_start..).unwrap_or(""));
    if found {
        value["found"] = json!(true);
    }
    value
}

/// Do not put filesystem paths, SQL text, credentials, or provider payloads into
/// remote errors. Local CLI diagnostics also use this stable, bounded envelope.
pub fn error_payload(error: &Error) -> Value {
    let (code, message) = match error {
        Error::Invalid(message) => ("invalid_request", message.clone()),
        Error::NotFound(_) => ("not_found", "resource not found".into()),
        Error::Conflict(_) => ("conflict", "resource conflicts with current state".into()),
        Error::Scope => ("forbidden", "library or capability denied".into()),
        Error::Busy => ("busy", "another write operation is running".into()),
        Error::Capability(message) => ("capability_unavailable", message.clone()),
        Error::Json(_) | Error::Yaml(_) => ("invalid_data", "invalid structured data".into()),
        Error::Io(_) | Error::Sql(_) => ("storage_error", "storage operation failed".into()),
        Error::Model(_) => ("model_error", "model operation failed".into()),
    };
    json!({"code":code,"message":message})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_catalog_is_exactly_the_audited_registry() {
        let audit: Value = serde_json::from_str(include_str!(
            "../../../docs/rust-migration/contracts/baseline.json"
        ))
        .unwrap();
        let expected = audit["operations"].as_array().unwrap();
        assert_eq!(operation_catalog().len(), 19);
        for (actual, old) in operation_catalog().iter().zip(expected) {
            for field in ["name", "description", "params", "writes", "category"] {
                assert_eq!(actual[field], old[field], "{}: {field}", actual["name"]);
            }
        }
        let ask = operation_catalog()
            .iter()
            .find(|op| op["name"] == "kb_ask")
            .unwrap();
        assert_eq!(ask["params"]["properties"]["deep"]["default"], true);
        assert_eq!(ask["params"]["properties"]["file_back"]["default"], false);
        assert_eq!(ask["params"]["properties"]["api_key"]["writeOnly"], true);
    }
}
