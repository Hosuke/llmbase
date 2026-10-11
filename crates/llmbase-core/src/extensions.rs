//! Explicit, application-scoped extensions compiled into a trusted Rust host.
//!
//! This is not a sandbox or a dynamic ABI. Serde validates typed inputs; schemas
//! describe those types, not an additional arbitrary JSON Schema validation layer.
//! Use `#[serde(deny_unknown_fields)]` when unknown input fields must be rejected.
use crate::application::{Application, Capability, Principal, Request};
use crate::{Error, LibraryConfig, Result, profile::LanguageProfile};
pub use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{collections::HashSet, future::Future, pin::Pin, sync::Arc};
use tokio_util::sync::CancellationToken;

pub const EXTENSION_API_VERSION: u32 = 1;

/// Registration policy supplied by the host, not by transport callers.
pub struct ExtensionSpec {
    pub name: String,
    pub description: String,
    pub contract_version: u32,
    /// Explicit enablement. No wildcard or implicit all-library registration.
    pub libraries: Vec<String>,
    /// Also the maximum capabilities available through the handler's context.
    pub required_capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExtensionDescriptor {
    pub name: String,
    pub description: String,
    pub contract_version: u32,
    pub input_schema: Value,
    pub output_schema: Value,
    pub required_capabilities: Vec<Capability>,
}

type HandlerFuture = Pin<Box<dyn Future<Output = Result<Value>> + Send>>;
type Handler = dyn Fn(ExtensionContext, Value) -> Result<HandlerFuture> + Send + Sync;

#[derive(Clone)]
pub struct Extension {
    descriptor: ExtensionDescriptor,
    libraries: HashSet<String>,
    handler: Arc<Handler>,
}

impl Extension {
    /// Both wire types must have object schemas. Prefer ordinary Serde structs;
    /// authors of custom serializers/JsonSchema implementations must keep them consistent.
    pub fn typed<I, O, F, Fut>(spec: ExtensionSpec, handler: F) -> Result<Self>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + Send + 'static,
        F: Fn(ExtensionContext, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O>> + Send + 'static,
    {
        if spec.contract_version != EXTENSION_API_VERSION {
            return Err(Error::Invalid(
                "unsupported extension contract version".into(),
            ));
        }
        // Built-ins have no dot; reserved engine namespaces cannot be registered.
        let segments: Vec<_> = spec.name.split('.').collect();
        if spec.name.len() > 64
            || segments.len() < 2
            || matches!(segments[0], "llmbase" | "core")
            || segments.iter().any(|segment| {
                !segment
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_lowercase)
                    || !segment.bytes().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    })
            })
        {
            return Err(Error::Invalid(
                "extension name must be a non-reserved lowercase namespace.name (max 64 bytes)"
                    .into(),
            ));
        }
        let libraries: HashSet<_> = spec.libraries.iter().cloned().collect();
        if libraries.is_empty() || libraries.len() != spec.libraries.len() {
            return Err(Error::Invalid(
                "extension needs distinct explicit library IDs".into(),
            ));
        }
        for id in &libraries {
            crate::config::validate_id(id)?;
        }
        if spec.required_capabilities.is_empty()
            || spec
                .required_capabilities
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != spec.required_capabilities.len()
        {
            return Err(Error::Invalid(
                "extension needs distinct required capabilities".into(),
            ));
        }
        let input_schema = schemars::schema_for!(I).to_value();
        let output_schema = schemars::generate::SchemaSettings::default()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<O>()
            .to_value();
        if input_schema.get("type").and_then(Value::as_str) != Some("object")
            || output_schema.get("type").and_then(Value::as_str) != Some("object")
        {
            return Err(Error::Invalid(
                "extension input and output schemas must be objects".into(),
            ));
        }
        Ok(Self {
            descriptor: ExtensionDescriptor {
                name: spec.name,
                description: spec.description,
                contract_version: spec.contract_version,
                input_schema,
                output_schema,
                required_capabilities: spec.required_capabilities,
            },
            libraries,
            handler: Arc::new(move |context, args| {
                let input = serde_json::from_value::<I>(args)
                    .map_err(|e| Error::Invalid(format!("invalid extension input: {e}")))?;
                let future = handler(context, input);
                Ok(Box::pin(async move {
                    let value = serde_json::to_value(future.await?)?;
                    if !value.is_object() {
                        return Err(Error::Invalid("extension output must be an object".into()));
                    }
                    Ok(value)
                }))
            }),
        })
    }

    pub(crate) fn authorize(&self, principal: &Principal, library_id: &str) -> Result<()> {
        if !self.libraries.contains(library_id)
            || !self
                .descriptor
                .required_capabilities
                .iter()
                .all(|cap| principal.capabilities.contains(cap))
        {
            return Err(Error::Scope);
        }
        Ok(())
    }
}

impl Application {
    pub fn with_extensions(
        configs: Vec<LibraryConfig>,
        extensions: Vec<Extension>,
    ) -> Result<Self> {
        let mut app = Self::new(configs)?;
        for extension in extensions {
            for id in &extension.libraries {
                let config = app.library_config(id)?;
                LanguageProfile::from_config(&config.settings)?;
            }
            let name = extension.descriptor.name.clone();
            if app.extensions.insert(name, extension).is_some() {
                return Err(Error::Conflict("duplicate extension operation".into()));
            }
        }
        Ok(app)
    }

    /// Registry membership for trusted host dispatch; use descriptors for authorized discovery.
    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.contains_key(name)
    }

    pub fn extension_descriptors(
        &self,
        principal: &Principal,
        library_id: &str,
    ) -> Result<Vec<ExtensionDescriptor>> {
        self.library_config(library_id)?;
        if !principal.libraries.contains(library_id) {
            return Err(Error::Scope);
        }
        Ok(self
            .extensions
            .values()
            .filter(|extension| extension.authorize(principal, library_id).is_ok())
            .map(|extension| extension.descriptor.clone())
            .collect())
    }

    /// Dropping this future or cancelling the caller token cancels a child token
    /// and drops the handler future. Already accepted writes are never rolled back.
    pub async fn execute_extension(
        &self,
        principal: &Principal,
        request: Request,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.authorize(principal, &request)?;
        let extension = self
            .extensions
            .get(&request.operation)
            .ok_or_else(|| Error::Capability("not a registered extension".into()))?;
        check_cancelled(&cancel)?;
        let child = cancel.child_token();
        let _cancel_on_drop = child.clone().drop_guard();
        let context = ExtensionContext {
            application: self.clone(),
            library_id: request.library_id.clone(),
            principal: Principal {
                id: principal.id.clone(),
                libraries: HashSet::from([request.library_id]),
                capabilities: extension
                    .descriptor
                    .required_capabilities
                    .iter()
                    .cloned()
                    .collect(),
            },
            cancel: child.clone(),
        };
        let future = (extension.handler)(context, request.args)?;
        tokio::select! {
            biased;
            _ = child.cancelled() => Err(llmbase_model::ModelError::Cancelled.into()),
            result = future => {
                check_cancelled(&child)?;
                result
            }
        }
    }
}

/// Library-scoped shared services with only the extension's declared grants.
/// No raw configuration, storage handle, credentials or filesystem paths are exposed.
#[derive(Clone)]
pub struct ExtensionContext {
    application: Application,
    library_id: String,
    principal: Principal,
    cancel: CancellationToken,
}

impl ExtensionContext {
    pub fn library_id(&self) -> &str {
        &self.library_id
    }

    pub fn language_profile(&self) -> Result<LanguageProfile> {
        LanguageProfile::from_config(&self.application.library_config(&self.library_id)?.settings)
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Minimal v1 service surface: get/list/search/evidence/doctor/kb_stats,
    /// index and journaled source_write/source_delete/source_rename. Other built-ins,
    /// models, job management and nested extensions require later explicit APIs.
    /// A blocking service already running may finish after cancellation; inspect
    /// its stored revisions before retrying a write.
    pub async fn execute(&self, operation: &str, args: Value) -> Result<Value> {
        check_cancelled(&self.cancel)?;
        if !matches!(
            operation,
            "get"
                | "list"
                | "search"
                | "evidence"
                | "doctor"
                | "kb_stats"
                | "index"
                | "source_write"
                | "source_delete"
                | "source_rename"
        ) {
            return Err(Error::Capability(
                "service is not exposed to v1 extensions".into(),
            ));
        }
        let context = self.clone();
        let request = Request {
            library_id: self.library_id.clone(),
            operation: operation.into(),
            args,
        };
        self.application.authorize(&self.principal, &request)?;
        tokio::task::spawn_blocking(move || {
            check_cancelled(&context.cancel)?;
            context.application.execute(&context.principal, request)
        })
        .await
        .map_err(|_| Error::Capability("extension service worker failed".into()))?
    }
}

fn check_cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(llmbase_model::ModelError::Cancelled.into());
    }
    Ok(())
}
