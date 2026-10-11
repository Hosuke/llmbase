//! MCP stdio framing, initialization and lifecycle come from the official SDK.
//! References: https://github.com/modelcontextprotocol/rust-sdk and
//! https://docs.rs/rmcp/3.4.0/rmcp/handler/server/trait.ServerHandler.html
use crate::{Gateway, error_payload, native_tool_available, operation_catalog};
use llmbase_core::application::{Principal, Request};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::stdio,
};
use serde_json::{Value, json};

fn native_tools() -> Vec<Tool> {
    [
        ("compile_source", "Compile one raw source into reviewable drafts without accepting them.",
            json!({"type":"object","properties":{"source_id":{"type":"string"},"profile":{"type":"object"}},"required":["source_id"],"additionalProperties":false})),
        ("get_draft", "Read the exact reviewed bytes and source revision of a compilation draft.",
            json!({"type":"object","properties":{"draft_id":{"type":"string"}},"required":["draft_id"],"additionalProperties":false})),
        ("apply_draft", "Accept a draft with an explicit compare-and-swap against its reviewed base hash.",
            json!({"type":"object","properties":{"draft_id":{"type":"string"},"expected_hash":{"type":["string","null"]}},"required":["draft_id"],"additionalProperties":false})),
        ("ask_sources", "Answer from original versioned evidence. Does not save or promote the answer.",
            json!({"type":"object","properties":{"question":{"type":"string"},"kind":{"type":"string","enum":["raw","compiled","both"],"default":"raw"},"top_k":{"type":"integer","minimum":0,"maximum":100},"answer_language":{"type":"string"},"short_answer":{"type":"boolean"},"session_id":{"type":"string"},"context_token_budget":{"type":"integer"},"max_tokens":{"type":"integer"}},"required":["question"],"additionalProperties":false})),
        ("prepare_transcription", "Validate supplied transcript text and map it into a read-only native ask; does not perform OCR or model calls.", transcription_schema()),
        ("ask_transcription", "Answer a supplied transcript using scoped original evidence; no source or answer writes. Images need a recognition adapter.", transcription_schema()),
        ("save_answer", "Explicitly save an accepted answer with validated citations and visibility.",
            json!({"type":"object","properties":{"response":{"type":"object"},"request":{"type":"object"}},"required":["response","request"],"additionalProperties":false})),
        ("get_saved_answer", "Read a saved answer and resolve its original evidence versions.",
            json!({"type":"object","properties":{"answer_id":{"type":"string"}},"required":["answer_id"],"additionalProperties":false})),
        ("promote_answer", "Judge a saved answer and optionally create a reviewable article draft; never applies it automatically.",
            json!({"type":"object","properties":{"answer_id":{"type":"string"},"profile":{"type":"object"}},"required":["answer_id"],"additionalProperties":false})),
        ("build_vectors", "Build or resume an embedding generation and activate it after complete snapshot validation.",
            json!({"type":"object","properties":{"revision":{"type":"string"},"dimension":{"type":"integer","minimum":1}},"required":["revision","dimension"],"additionalProperties":false})),
        ("search_hybrid", "Fuse lexical and actual vector retrieval; explicitly reports degradation when vectors are unavailable.",
            json!({"type":"object","properties":{"query":{"type":"string"},"top_k":{"type":"integer","minimum":0,"maximum":100},"kind":{"type":"string","enum":["raw","compiled","note"]}},"required":["query"],"additionalProperties":false})),
        ("resolve_evidence", "Resolve an exact original passage and report whether it is stale or deleted.",
            json!({"type":"object","properties":{"evidence":{"type":"object"}},"required":["evidence"],"additionalProperties":false})),
        ("knowledge_graph", "Inspect a bounded graph of wikilinks and evidence-supported manual relations.",
            json!({"type":"object","properties":{"root":{"type":"string"},"depth":{"type":"integer","minimum":0,"maximum":4},"max_neighbors":{"type":"integer","minimum":1,"maximum":200},"max_nodes":{"type":"integer","minimum":1,"maximum":5000},"max_edges":{"type":"integer","minimum":1,"maximum":20000}},"additionalProperties":false})),
        ("article_backlinks", "List visible articles linking to one scoped article or unambiguous alias.",
            json!({"type":"object","properties":{"target":{"type":"string"}},"required":["target"],"additionalProperties":false})),
        ("get_taxonomy", "Read accepted classification or its explicit tag fallback in the requested language.",
            json!({"type":"object","properties":{"lang":{"type":"string","default":"en"}},"additionalProperties":false})),
        ("set_taxonomy", "Accept a reviewed classification document with retained unknown metadata.",
            json!({"type":"object","properties":{"document":{"type":"object"}},"required":["document"],"additionalProperties":false})),
        ("generate_taxonomy", "Generate actual classification from scoped article metadata using the configured model.",
            json!({"type":"object","properties":{"private_inputs":{"type":"boolean","default":false},"label_languages":{"type":"array","items":{"type":"string"}},"sync_category_tags":{"type":"boolean","default":false},"max_tokens":{"type":"integer"},"context_bytes":{"type":"integer"}},"additionalProperties":false})),
        ("generate_introduction", "Generate and persist a guided introduction from a guarded source snapshot.",
            json!({"type":"object","properties":{"lang":{"type":"string","default":"en"},"private_inputs":{"type":"boolean","default":false},"strategy":{"type":"string","enum":["direct","legacy_zh_base"]},"max_tokens":{"type":"integer"},"context_bytes":{"type":"integer"}},"additionalProperties":false})),
        ("generate_entities", "Extract people, events and places from guarded article context, with separate private output.",
            json!({"type":"object","properties":{"private_inputs":{"type":"boolean","default":false},"force":{"type":"boolean","default":false},"max_tokens":{"type":"integer"},"context_bytes":{"type":"integer"}},"additionalProperties":false})),
        ("repair_library", "Run an explicit maintenance stage with journaled changes and durable partial/failure status.",
            json!({"type":"object","properties":{"mode":{"type":"string","enum":["clean","heal_slugs","dirty_tags","normalize_tags","metadata","broken_links","deduplicate","auto_fix","heal","deep_lint"]},"private_inputs":{"type":"boolean","default":false},"max_articles":{"type":"integer"},"max_merges":{"type":"integer"},"max_stubs":{"type":"integer"}},"required":["mode"],"additionalProperties":false})),
        ("run_job", "Execute at most one scoped queued job with a renewable durable lease.",
            json!({"type":"object","properties":{"worker_id":{"type":"string"},"job_id":{"type":"string"}},"additionalProperties":false})),
        ("ingest_with_adapter", "Run one operator-configured external parser and accept its scoped source documents through the native journal.",
            json!({"type":"object","properties":{"adapter":{"type":"string"},"input":{}},"required":["adapter","input"],"additionalProperties":false})),
        ("add_relation", "Accept a typed manual relation with original-version evidence supports.",
            json!({"type":"object","properties":{"source":{"type":"string"},"target":{"type":"string"},"kind":{"type":"string"},"supports":{"type":"array","items":{"type":"object"}},"metadata":{"type":"object"},"visibility":{"type":"string","enum":["public","internal","private"]}},"required":["source","target","kind"],"additionalProperties":false})),
        ("remove_relation", "Tombstone a manual relation while retaining its accepted revision history.",
            json!({"type":"object","properties":{"relation_id":{"type":"string"}},"required":["relation_id"],"additionalProperties":false})),
    ].into_iter().map(|(name,description,schema)| Tool::new(name,description,schema.as_object().unwrap().clone())).collect()
}

fn transcription_schema() -> Value {
    json!({"type":"object","properties":{
        "version":{"type":"integer","const":1},
        "transcription_id":{"type":"string","minLength":1,"maxLength":128},
        "session_id":{"type":"string","minLength":1,"maxLength":128},
        "request_id":{"type":"string","minLength":1,"maxLength":128},
        "input_kind":{"type":"string","enum":["supplied_transcription","synthetic_transcription","image","handwriting"]},
        "text":{"type":"string","minLength":1,"maxLength":16384},
        "image_base64":{"type":"string","maxLength":8388608},
        "answer_preferences":{"type":"object","properties":{
            "short_answer":{"type":"boolean","default":true},
            "max_sentences":{"type":"integer","minimum":1,"maximum":100},
            "answer_language":{"type":"string","maxLength":64}
        },"additionalProperties":false}
    },"required":["version","transcription_id","session_id","input_kind"],"additionalProperties":false})
}

#[derive(Clone)]
pub struct McpServer {
    pub gateway: Gateway,
    pub principal: Principal,
}

impl McpServer {
    pub fn tools(&self) -> Vec<Tool> {
        let mut tools: Vec<_> = operation_catalog()
            .iter()
            .filter(|op| native_tool_available(op["name"].as_str().unwrap_or_default()))
            .map(|op| {
                Tool::new(
                    op["name"].as_str().unwrap().to_owned(),
                    op["description"].as_str().unwrap().to_owned(),
                    op["params"].as_object().unwrap().clone(),
                )
            })
            .collect();
        tools.extend(native_tools());
        if let Ok(descriptors) = self
            .gateway
            .application
            .extension_descriptors(&self.principal, &self.gateway.default_library)
        {
            tools.extend(descriptors.into_iter().filter_map(|descriptor| {
                // Registration accepts object schemas; keep discovery fail-closed
                // if an application ever supplies a non-object descriptor.
                let input = descriptor.input_schema.as_object()?.clone();
                let output = descriptor.output_schema.as_object()?.clone();
                Some(
                    Tool::new(descriptor.name, descriptor.description, input)
                        .with_raw_output_schema(output.into()),
                )
            }));
        }
        tools
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("llmbase", env!("CARGO_PKG_VERSION"));
        info.instructions = Some("Native library tools. Tool discovery lists available capabilities; unavailable legacy operations return explicit errors. The process is scoped to its configured library.".into());
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|r| r.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "this tool list has no continuation cursor",
                None,
            ));
        }
        Ok(ListToolsResult {
            tools: self.tools(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools().into_iter().find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let extension = self.gateway.application.has_extension(&request.name);
        let native = match request.name.as_ref() {
            "compile_source" => Some("compile"),
            "get_draft" => Some("draft_get"),
            "apply_draft" => Some("draft_apply"),
            "ask_sources" => Some("ask"),
            "prepare_transcription" => Some("transcription_prepare"),
            "ask_transcription" => Some("transcription_ask"),
            "save_answer" => Some("answer_save"),
            "get_saved_answer" => Some("answer_get"),
            "promote_answer" => Some("answer_promote"),
            "build_vectors" => Some("vector_build"),
            "search_hybrid" => Some("hybrid_search"),
            "resolve_evidence" => Some("evidence"),
            "knowledge_graph" => Some("graph"),
            "article_backlinks" => Some("backlinks"),
            "get_taxonomy" => Some("taxonomy"),
            "set_taxonomy" => Some("taxonomy_set"),
            "generate_taxonomy" => Some("taxonomy_generate"),
            "generate_introduction" => Some("xici_generate"),
            "generate_entities" => Some("entities_generate"),
            "repair_library" => Some("maintenance_repair"),
            "run_job" => Some("job_run_one"),
            "ingest_with_adapter" => Some("plugin_ingest"),
            "add_relation" => Some("relation_add"),
            "remove_relation" => Some("relation_remove"),
            _ if extension => Some(request.name.as_ref()),
            _ => None,
        };
        let known = native.is_some()
            || operation_catalog()
                .iter()
                .any(|op| op["name"].as_str() == Some(request.name.as_ref()));
        if !known {
            return Err(ErrorData::method_not_found::<
                rmcp::model::CallToolRequestMethod,
            >());
        }
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = if let Some(operation) = native {
            let execution = self.gateway.execute(
                self.principal.clone(),
                Request {
                    library_id: self.gateway.default_library.clone(),
                    operation: operation.into(),
                    args,
                },
            );
            if extension {
                // rmcp cancels the request token without aborting this task.
                // Dropping the Gateway future cancels the extension's child;
                // the biased branch also prevents dispatch when already cancelled.
                tokio::select! {
                    biased;
                    _ = context.ct.cancelled() => Err(llmbase_model::ModelError::Cancelled.into()),
                    result = execution => result,
                }
            } else {
                execution.await
            }
        } else {
            self.gateway
                .legacy(
                    self.principal.clone(),
                    self.gateway.default_library.clone(),
                    &request.name,
                    args,
                )
                .await
        };
        let result = match result {
            Ok(value) if extension => CallToolResult::structured(value),
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&value).expect("JSON value serialization"),
            )]),
            Err(error) => {
                CallToolResult::error(vec![ContentBlock::text(error_payload(&error).to_string())])
            }
        };
        Ok(result.into())
    }
}

pub async fn serve(gateway: Gateway) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let principal = Principal::local(&gateway.default_library);
    let service = McpServer { gateway, principal }.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
