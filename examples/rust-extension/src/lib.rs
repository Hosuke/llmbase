//! A separately compiled extension using only the public `llmbase-core` API.

use llmbase_core::{
    Error, Result,
    application::Capability,
    extensions::{EXTENSION_API_VERSION, Extension, ExtensionContext, ExtensionSpec, JsonSchema},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "llmbase_core::schemars")]
#[serde(deny_unknown_fields)]
pub struct InspectInput {
    pub source_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "llmbase_core::schemars")]
pub struct InspectOutput {
    pub library_id: String,
    pub default_lang: String,
    pub bare_body: bool,
    /// UTF-8 bytes of the complete original source, including frontmatter.
    pub content_bytes: usize,
    /// The native `get` result, including unmodified original source content.
    pub source: Value,
}

/// Register this operation for exactly one application-selected library.
pub fn inspect_extension(library_id: &str) -> Result<Extension> {
    Extension::typed::<InspectInput, InspectOutput, _, _>(
        ExtensionSpec {
            name: "example.inspect".into(),
            description: "Inspect a source and its library's language profile.".into(),
            contract_version: EXTENSION_API_VERSION,
            libraries: vec![library_id.into()],
            required_capabilities: vec![Capability::Read],
        },
        |context: ExtensionContext, input: InspectInput| async move {
            let profile = context.language_profile()?;
            let source = context
                .execute("get", json!({"source_id": input.source_id}))
                .await?;
            let content_bytes = source
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Invalid("native get result has no source content".into()))?
                .len();
            Ok(InspectOutput {
                library_id: context.library_id().into(),
                bare_body: profile.bare_body(),
                default_lang: profile.default_lang,
                content_bytes,
                source,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmbase_core::{
        Library, LibraryConfig,
        application::{Application, Principal, Request},
    };
    use std::fs;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    const LIBRARY_ID: &str = "example";
    const ENGLISH: &str = "---\ntitle: Original notes\n---\nKnowledge begins with evidence.\n";
    const WENYAN: &str =
        "---\r\ntitle: 學而\r\n---\r\n子曰：學而時習之，不亦說乎？\r\n𠀀 e\u{301}，禮。\n";
    const WENYAN_CONFIG: &str = "\
languages:
  default: wenyan
  profiles:
    wenyan:
      default_lang: lzh
      api_default_lang: lzh
      sections:
        - code: lzh
          key: 文言
          header: ''
";

    fn fixture(content: &str, settings: Option<&str>) -> (TempDir, LibraryConfig, String) {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join("raw")).unwrap();
        fs::write(root.path().join("raw/sample.md"), content.as_bytes()).unwrap();
        if let Some(settings) = settings {
            fs::write(root.path().join("config.yaml"), settings).unwrap();
        }
        let config = LibraryConfig::load(root.path(), LIBRARY_ID).unwrap();
        let mut library = Library::open(config.clone()).unwrap();
        library.sync().unwrap();
        let source_id = library.get("sample", None, false).unwrap().source_id;
        (root, config, source_id)
    }

    fn application(config: LibraryConfig) -> Application {
        Application::with_extensions(vec![config], vec![inspect_extension(LIBRARY_ID).unwrap()])
            .unwrap()
    }

    async fn inspect(app: &Application, source_id: &str) -> Result<Value> {
        app.execute_extension(
            &Principal::public(LIBRARY_ID),
            Request {
                library_id: LIBRARY_ID.into(),
                operation: "example.inspect".into(),
                args: json!({"source_id": source_id}),
            },
            CancellationToken::new(),
        )
        .await
    }

    #[tokio::test]
    async fn unconfigured_library_uses_english_and_returns_native_source() {
        let (_root, config, source_id) = fixture(ENGLISH, None);
        let app = application(config);
        let output = inspect(&app, &source_id).await.unwrap();
        assert_eq!(output["library_id"], LIBRARY_ID);
        assert_eq!(output["default_lang"], "en");
        assert_eq!(output["bare_body"], true);
        assert_eq!(output["content_bytes"], ENGLISH.len());
        let native = app
            .execute(
                &Principal::public(LIBRARY_ID),
                Request {
                    library_id: LIBRARY_ID.into(),
                    operation: "get".into(),
                    args: json!({"source_id": source_id}),
                },
            )
            .unwrap();
        assert_eq!(output["source"], native);
        assert_eq!(output["source"]["content"], ENGLISH);
    }

    #[tokio::test]
    async fn configured_single_section_wenyan_preserves_original_bytes() {
        let (root, config, source_id) = fixture(WENYAN, Some(WENYAN_CONFIG));
        let output = inspect(&application(config), &source_id).await.unwrap();
        assert_eq!(output["default_lang"], "lzh");
        assert_eq!(output["bare_body"], true);
        assert_eq!(output["content_bytes"], WENYAN.len());
        assert_eq!(
            output["source"]["content"].as_str().unwrap().as_bytes(),
            WENYAN.as_bytes()
        );
        assert_eq!(
            fs::read(root.path().join("raw/sample.md")).unwrap(),
            WENYAN.as_bytes()
        );
    }

    #[tokio::test]
    async fn applications_do_not_inherit_extension_registrations() {
        let (_first_root, first_config, first_source) = fixture(WENYAN, Some(WENYAN_CONFIG));
        let (_second_root, second_config, second_source) = fixture(ENGLISH, None);
        let first = application(first_config);
        let second = Application::new(vec![second_config]).unwrap();
        assert!(inspect(&first, &first_source).await.is_ok());
        assert!(inspect(&second, &second_source).await.is_err());
    }

    #[tokio::test]
    async fn applications_with_the_same_library_id_keep_separate_profiles() {
        let (_wenyan_root, wenyan_config, wenyan_source) = fixture(WENYAN, Some(WENYAN_CONFIG));
        let (_english_root, english_config, english_source) = fixture(ENGLISH, None);
        let wenyan = application(wenyan_config);
        let english = application(english_config);
        for _ in 0..2 {
            let first = inspect(&wenyan, &wenyan_source).await.unwrap();
            let second = inspect(&english, &english_source).await.unwrap();
            assert_eq!(first["default_lang"], "lzh");
            assert_eq!(first["source"]["content"], WENYAN);
            assert_eq!(second["default_lang"], "en");
            assert_eq!(second["source"]["content"], ENGLISH);
        }
    }
}
