use llmbase_core::{
    Error, LibraryConfig, Result,
    application::{Application, Principal, Request},
};
use llmbase_rust_extension_example::inspect_extension;
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err(Error::Invalid(
            "usage: llmbase-rust-extension-example <library-root> <library-id> <source-id-or-slug|--list>"
                .into(),
        ));
    }
    let library_id = &args[1];
    let app = Application::with_extensions(
        vec![LibraryConfig::load(&args[0], library_id)?],
        vec![inspect_extension(library_id)?],
    )?;
    if args[2] == "--list" {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &app.extension_descriptors(&Principal::public(library_id), library_id)?
            )?
        );
        return Ok(());
    }
    let output = app
        .execute_extension(
            &Principal::public(library_id),
            Request {
                library_id: library_id.clone(),
                operation: "example.inspect".into(),
                args: json!({"source_id": args[2]}),
            },
            CancellationToken::new(),
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
