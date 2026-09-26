use clap::{Args, Parser, Subcommand};
use llmbase_cli::{
    Gateway, error_payload,
    http::{self, HttpState, Surface},
    mcp, model_env, native_tool_available, operation_catalog, static_ui,
};
use llmbase_core::{
    Error, LibraryConfig,
    application::{Application, Principal, Request},
    ask::{AskKind, AskRequest},
};
use llmbase_model::CancellationToken;
use serde_json::{Value, json};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    name = "llmbase",
    version,
    about = "Native, scoped knowledge-library CLI, HTTP and MCP"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    base_dir: PathBuf,
    #[arg(long, global = true, default_value = "default")]
    library_id: String,
    /// Additional configured library: ID=/absolute/or/relative/path.
    #[arg(long = "library", global = true)]
    libraries: Vec<String>,
    #[arg(long, global = true)]
    json: bool,
    #[arg(short='v', long, global=true, action=clap::ArgAction::Count)]
    verbose: u8,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Native operation, with its versioned core payload printed as JSON.
    Op {
        name: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    /// Backward-compatible operation inspection and invocation.
    Ops {
        #[command(subcommand)]
        command: OpsCommand,
    },
    Search {
        #[command(subcommand)]
        command: SearchCommand,
    },
    Compile {
        #[command(subcommand)]
        command: CompileCommand,
    },
    Ingest {
        #[command(subcommand)]
        command: IngestCommand,
    },
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    Lint {
        #[command(subcommand)]
        command: LintCommand,
    },
    Jobs {
        #[command(subcommand)]
        command: JobsCommand,
    },
    Index,
    Get {
        slug: String,
    },
    Stats,
    Doctor,
    /// Established compiled-wiki query, with explicit optional write-back.
    Query(QueryArgs),
    /// Evidence-first, read-only native question answering. English by default.
    Ask(AskArgs),
    /// Copy-first local administration; never exposed as remote operations.
    Migration {
        #[command(subcommand)]
        command: MigrationCommand,
    },
    /// Start the Agent HTTP surface; bearer authorization remains required.
    Serve(ServeArgs),
    /// Start the Web API surface.
    Web(WebArgs),
    /// Start the official MCP stdio protocol. Stdout contains protocol only.
    Mcp,
}

#[derive(Subcommand)]
enum OpsCommand {
    List,
    Call {
        name: String,
        #[arg(long, default_value = "{}")]
        json_args: String,
    },
}

#[derive(Subcommand)]
enum SearchCommand {
    /// Serve the lightweight search reader and its scoped search API.
    Serve(SearchServeArgs),
    Query {
        query_text: String,
        #[arg(long, default_value_t = 10)]
        top_k: usize,
        #[arg(long)]
        json_output: bool,
    },
    Raw {
        query_text: String,
        #[arg(long, default_value_t = 10)]
        top_k: usize,
    },
}

#[derive(Subcommand)]
enum CompileCommand {
    Index,
    /// Compile and accept pending sources using the configured language profile.
    New {
        #[arg(long)]
        batch_size: Option<usize>,
    },
    /// Recompile all sources, including previously accepted source receipts.
    All,
    /// Compile one raw source into reviewable drafts; does not accept them yet.
    Source {
        source_id: String,
    },
    /// Inspect the exact bytes and source revision of a draft.
    Draft {
        draft_id: String,
    },
    /// Accept a reviewed draft; updates require its reviewed base content hash.
    Apply {
        draft_id: String,
        #[arg(long)]
        expected_hash: Option<String>,
    },
}

#[derive(Subcommand)]
enum IngestCommand {
    File {
        file_path: PathBuf,
    },
    Dir {
        dir_path: PathBuf,
    },
    Url {
        url: String,
    },
    Pdf {
        pdf_path: PathBuf,
        #[arg(long, default_value_t = 20)]
        chunk_pages: usize,
    },
    /// Browser extraction uses the explicitly configured browse process adapter.
    Browse {
        url: String,
    },
    List,
}

#[derive(Subcommand)]
enum ExportCommand {
    Article {
        slug: String,
    },
    Tag {
        tag: String,
    },
    Graph {
        slug: String,
        #[arg(long, default_value_t = 2)]
        depth: usize,
    },
}

#[derive(Subcommand)]
enum LintCommand {
    Check,
    Deep,
    Fix,
    Clean,
    Dedup,
    Heal,
    /// Normalize synonymous tags with a model and journal each accepted change.
    NormalizeTags,
}

#[derive(Subcommand)]
enum JobsCommand {
    List,
    Get {
        job_id: String,
    },
    Events {
        job_id: String,
    },
    Cancel {
        job_id: String,
    },
    Retry {
        job_id: String,
    },
    Enqueue {
        operation: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    /// Execute at most one ready job and exit; suitable for a supervisor/timer.
    RunOne {
        #[arg(long, default_value = "native-worker")]
        worker_id: String,
        #[arg(long)]
        job_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum MigrationCommand {
    /// Inspect original files without creating a database, lock, or directory.
    DryRun,
    /// Copy the configured library to a separate new target and index the copy.
    Copy { target: PathBuf },
    /// Verify a copied library against its migration receipt.
    Verify { target: PathBuf },
    /// Create a verified backup in a separate target.
    Backup { target: PathBuf },
    /// Restore a verified snapshot into a separate new target.
    Restore { snapshot: PathBuf, target: PathBuf },
    /// Verify the original remains preserved; does not switch any runtime.
    Rollback { snapshot: PathBuf },
}

#[derive(Args)]
struct QueryArgs {
    question: String,
    #[arg(long="format", default_value="markdown", value_parser=["markdown","marp","chart"])]
    output_format: String,
    #[arg(long, default_value="default", value_parser=["default","caveman","wenyan","scholar","eli5"])]
    tone: String,
    #[arg(long)]
    file_back: bool,
    #[arg(long)]
    deep: bool,
}

#[derive(Args)]
struct AskArgs {
    question: String,
    #[arg(long, default_value="raw", value_parser=["raw","compiled","both"])]
    kind: String,
    #[arg(long, default_value_t = 8)]
    top_k: usize,
    #[arg(long, default_value_t = 8192)]
    context_token_budget: usize,
    #[arg(long, default_value_t = 2048)]
    max_tokens: u32,
    #[arg(long)]
    short_answer: bool,
    #[arg(long)]
    max_sentences: Option<usize>,
    #[arg(long)]
    answer_language: Option<String>,
    #[arg(long)]
    session_id: Option<String>,
    /// Emit provider-backed structured events as JSONL while generation runs.
    #[arg(long)]
    stream: bool,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(long, default_value_t = 5556)]
    port: u16,
    /// Permit unauthenticated reads from the default library only.
    #[arg(long)]
    public_read: bool,
}

#[derive(Args)]
struct WebArgs {
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(long, default_value_t = 5555)]
    port: u16,
    #[arg(long)]
    public_read: bool,
}

#[derive(Args)]
struct SearchServeArgs {
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,
    /// Defaults to config.search.port, or 5555 when absent.
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    public_read: bool,
}

fn args_object(text: &str) -> Result<Value, Error> {
    let args: Value = serde_json::from_str(text)
        .map_err(|_| Error::Invalid("arguments must be valid JSON".into()))?;
    if !args.is_object() {
        return Err(Error::Invalid("arguments must be a JSON object".into()));
    }
    Ok(args)
}

async fn run(cli: Cli) -> Result<(), Error> {
    // Administration must precede Application/Library construction: dry-run is
    // strictly read-only, and restore/verify do not depend on the current cwd.
    if let Command::Migration { ref command } = cli.command {
        use llmbase_core::migration;
        let value = match command {
            MigrationCommand::DryRun => serde_json::to_value(migration::inspect(
                &LibraryConfig::load(&cli.base_dir, &cli.library_id)?,
            )?)?,
            MigrationCommand::Copy { target } => serde_json::to_value(migration::copy_first(
                &LibraryConfig::load(&cli.base_dir, &cli.library_id)?,
                target,
            )?)?,
            MigrationCommand::Verify { target } => {
                serde_json::to_value(migration::verify_copy(target)?)?
            }
            MigrationCommand::Backup { target } => serde_json::to_value(migration::backup(
                &LibraryConfig::load(&cli.base_dir, &cli.library_id)?,
                target,
            )?)?,
            MigrationCommand::Restore { snapshot, target } => {
                serde_json::to_value(migration::restore(snapshot, target)?)?
            }
            MigrationCommand::Rollback { snapshot } => {
                serde_json::to_value(migration::verify_rollback(snapshot)?)?
            }
        };
        print_value(&value, cli.json)?;
        return Ok(());
    }
    let primary_config = LibraryConfig::load(&cli.base_dir, &cli.library_id)?;
    let mut configs = vec![primary_config.clone()];
    for entry in &cli.libraries {
        let (id, path) = entry
            .split_once('=')
            .ok_or_else(|| Error::Invalid("--library must be ID=PATH".into()))?;
        configs.push(LibraryConfig::load(path, id)?);
    }
    let gateway = Gateway::new(Application::new(configs)?, cli.library_id.clone());
    let principal = Principal::local(&cli.library_id);
    let native = |operation: &str, args: Value| Request {
        library_id: cli.library_id.clone(),
        operation: operation.into(),
        args,
    };
    let value = match cli.command {
        Command::Ingest{command}=>match command {
            IngestCommand::File{file_path}=>llmbase_cli::ingest_local::file(&gateway,principal,&cli.library_id,&file_path).await?,
            IngestCommand::Dir{dir_path}=>llmbase_cli::ingest_local::directory(&gateway,principal,&cli.library_id,&dir_path).await?,
            IngestCommand::Url{url}=>gateway.execute(principal,native("ingest_url",json!({"url":url}))).await?,
            IngestCommand::Pdf{pdf_path,chunk_pages}=>llmbase_cli::ingest_local::pdf(&gateway,principal,&cli.library_id,&pdf_path,chunk_pages).await?,
            IngestCommand::Browse{url}=>gateway.execute(principal,native("plugin_ingest",json!({"adapter":"browse","input":{"url":url}}))).await?,
            IngestCommand::List=>gateway.execute(principal,native("web_sources",json!({}))).await?,
        },
        Command::Export { command } => {
            let (name, args) = match command {
                ExportCommand::Article { slug } => ("kb_export_article", json!({"slug":slug})),
                ExportCommand::Tag { tag } => ("kb_export_tag", json!({"tag":tag})),
                ExportCommand::Graph { slug, depth } => {
                    ("kb_export_graph", json!({"slug":slug,"depth":depth}))
                }
            };
            gateway
                .legacy(principal, cli.library_id.clone(), name, args)
                .await?
        }
        Command::Lint { command } => {
            let (operation,args) = match command {
                LintCommand::Check => ("web_lint",json!({})),
                LintCommand::Deep => ("maintenance_repair",json!({"mode":"deep_lint"})),
                LintCommand::Fix => ("maintenance_repair",json!({"mode":"auto_fix"})),
                LintCommand::Clean => ("maintenance_repair",json!({"mode":"clean"})),
                LintCommand::Dedup => ("maintenance_repair",json!({"mode":"deduplicate"})),
                LintCommand::Heal => ("maintenance_repair",json!({"mode":"heal"})),
                LintCommand::NormalizeTags => ("maintenance_repair",json!({"mode":"normalize_tags"})),
            };
            gateway
                .execute(principal, native(operation, args))
                .await?
        }
        Command::Jobs { command } => {
            let (operation, args) = match command {
                JobsCommand::List => ("job_list", json!({})),
                JobsCommand::Get { job_id } => ("job_get", json!({"job_id":job_id})),
                JobsCommand::Events { job_id } => ("job_events", json!({"job_id":job_id})),
                JobsCommand::Cancel { job_id } => ("job_cancel", json!({"job_id":job_id})),
                JobsCommand::Retry { job_id } => ("job_retry", json!({"job_id":job_id})),
                JobsCommand::Enqueue { operation, args } => (
                    "job_enqueue",
                    json!({"operation":operation,"args":args_object(&args)?}),
                ),
                JobsCommand::RunOne { worker_id, job_id } => {
                    ("job_run_one", json!({"worker_id":worker_id,"job_id":job_id}))
                }
            };
            gateway.execute(principal, native(operation, args)).await?
        }
        Command::Migration { .. } => unreachable!("migration handled before opening library"),
        Command::Op { name, args } => {
            gateway
                .execute(principal, native(&name, args_object(&args)?))
                .await?
        }
        Command::Ops {
            command: OpsCommand::List,
        } => {
            json!({"operations":operation_catalog().iter().map(|op| { let mut op=op.clone(); op["native_available"]=json!(native_tool_available(op["name"].as_str().unwrap())); op }).collect::<Vec<_>>()})
        }
        Command::Ops {
            command: OpsCommand::Call { name, json_args },
        } => {
            if !operation_catalog().iter().any(|op| op["name"] == name) {
                return Err(Error::NotFound("unknown operation".into()));
            }
            gateway
                .legacy(
                    principal,
                    cli.library_id.clone(),
                    &name,
                    args_object(&json_args)?,
                )
                .await?
        }
        Command::Search { command: SearchCommand::Serve(args) } => {
            let port = match args.port {
                Some(port) => port,
                None => match primary_config.settings.pointer("/search/port") {
                    Some(value) => value.as_u64().and_then(|value| u16::try_from(value).ok())
                        .ok_or_else(|| Error::Invalid("search.port must be an integer in 0..65535".into()))?,
                    None => 5555,
                },
            };
            let grants = http::grants_from_environment(&cli.library_id)?;
            static_ui::serve(
                http::search_router(HttpState::new(gateway, Surface::Web, args.public_read, grants)),
                SocketAddr::new(args.host, port),
            ).await?;
            return Ok(());
        }
        Command::Search {
            command: SearchCommand::Query {
                query_text, top_k, ..
            },
        } => {
            gateway
                .legacy(
                    principal,
                    cli.library_id.clone(),
                    "kb_search",
                    json!({"query":query_text,"top_k":top_k}),
                )
                .await?
        }
        Command::Search {
            command: SearchCommand::Raw { query_text, top_k },
        } => {
            gateway
                .legacy(
                    principal,
                    cli.library_id.clone(),
                    "kb_search_raw",
                    json!({"query":query_text,"top_k":top_k}),
                )
                .await?
        }
        Command::Compile {
            command: CompileCommand::Index,
        } => {
            gateway
                .legacy(
                    principal,
                    cli.library_id.clone(),
                    "kb_rebuild_index",
                    json!({}),
                )
                .await?
        }
        Command::Compile {
            command: CompileCommand::New { batch_size },
        } => {
            let mut args = json!({"mode":"new"});
            if let Some(batch_size) = batch_size { args["batch_size"] = json!(batch_size); }
            gateway.execute(principal, native("legacy_compile", args)).await?
        }
        Command::Compile {
            command: CompileCommand::All,
        } => gateway.execute(principal, native("legacy_compile", json!({"mode":"all"}))).await?,
        Command::Compile {
            command: CompileCommand::Source { source_id },
        } => {
            gateway
                .execute(principal, native("compile", json!({"source_id":source_id})))
                .await?
        }
        Command::Compile {
            command: CompileCommand::Draft { draft_id },
        } => {
            gateway
                .execute(principal, native("draft_get", json!({"draft_id":draft_id})))
                .await?
        }
        Command::Compile {
            command:
                CompileCommand::Apply {
                    draft_id,
                    expected_hash,
                },
        } => {
            gateway
                .execute(
                    principal,
                    native(
                        "draft_apply",
                        json!({"draft_id":draft_id,"expected_hash":expected_hash}),
                    ),
                )
                .await?
        }
        Command::Index => {
            gateway
                .execute(principal, native("index", json!({})))
                .await?
        }
        Command::Get { slug } => {
            gateway
                .legacy(
                    principal,
                    cli.library_id.clone(),
                    "kb_get",
                    json!({"slug":slug}),
                )
                .await?
        }
        Command::Stats => {
            gateway
                .legacy(principal, cli.library_id.clone(), "kb_stats", json!({}))
                .await?
        }
        Command::Doctor => {
            gateway
                .execute(principal, native("doctor", json!({})))
                .await?
        }
        Command::Query(args) => {
            gateway.execute(principal, native("legacy_query", json!({
                "question":args.question,"mode":if args.deep{"deep"}else{"basic"},
                "tone":args.tone,"output_format":args.output_format,"file_back":args.file_back,
            }))).await?
        }
        Command::Ask(args) => {
            let models = model_env::from_environment()?.ok_or_else(|| {
                Error::Capability(
                    "configure LLMBASE_BASE_URL and LLMBASE_MODEL for question answering".into(),
                )
            })?;
            let request = AskRequest {
                question: args.question,
                library_id: cli.library_id.clone(),
                kind: match args.kind.as_str() {
                    "compiled" => AskKind::Compiled,
                    "both" => AskKind::Both,
                    _ => AskKind::Raw,
                },
                top_k: args.top_k,
                context_token_budget: args.context_token_budget,
                max_tokens: args.max_tokens,
                short_answer: args.short_answer,
                max_sentences: args.max_sentences,
                answer_language: args.answer_language,
                session_id: args.session_id,
                ..AskRequest::default()
            };
            let mut stream = gateway.application.start_ask(
                &principal,
                models,
                request,
                CancellationToken::new(),
            )?;
            if args.stream {
                while let Some(event) = stream.recv().await {
                    println!("{}", serde_json::to_string(&event)?);
                }
                // Collect checks the supervised terminal result even after the
                // event receiver was drained, so partial failure exits nonzero.
                stream.collect().await?;
                return Ok(());
            }
            serde_json::to_value(stream.collect().await?)?
        }
        Command::Mcp => {
            mcp::serve(gateway).await.map_err(|_| {
                Error::Capability("MCP transport stopped with a protocol or I/O error".into())
            })?;
            return Ok(());
        }
        Command::Serve(args) => {
            let grants = http::grants_from_environment(&cli.library_id)?;
            http::serve(
                HttpState::new(gateway, Surface::Agent, args.public_read, grants)
                    .with_models(model_env::from_environment()?),
                SocketAddr::new(args.host, args.port),
            )
            .await?;
            return Ok(());
        }
        Command::Web(args) => {
            let grants = http::grants_from_environment(&cli.library_id)?;
            let state =
                HttpState::new(gateway, Surface::Web, args.public_read, grants)
                    .with_models(model_env::from_environment()?);
            static_ui::serve(
                static_ui::web_router(http::router(state), &primary_config),
                SocketAddr::new(args.host, args.port),
            )
            .await?;
            return Ok(());
        }
    };
    // JSON is also the default first-slice renderer; human commands retain the
    // same semantic payload. Never place logging or progress on stdout.
    print_value(&value, cli.json)
}

fn print_value(value: &Value, compact: bool) -> Result<(), Error> {
    println!(
        "{}",
        if compact {
            serde_json::to_string(value)?
        } else {
            serde_json::to_string_pretty(value)?
        }
    );
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", json!({"error":error_payload(&error)}));
            ExitCode::from(match error {
                Error::Invalid(_) => 2,
                Error::Busy => 3,
                _ => 1,
            })
        }
    }
}
