use axum::{Router, body::Body, http::Request as HttpRequest};
use http_body_util::BodyExt;
use llmbase_cli::{
    Gateway,
    http::{AuthGrant, HttpState, Surface, router},
    mcp::McpServer,
    operation_catalog,
};
use llmbase_core::{
    Error, LibraryConfig,
    application::{Application, Capability, Principal, Request},
    extensions::{Extension, ExtensionSpec},
    schemars::JsonSchema,
};
use rmcp::ServiceExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};
use tower::ServiceExt as _;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(crate = "llmbase_core::schemars")]
struct EchoInput {
    message: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "llmbase_core::schemars")]
struct EchoOutput {
    library: String,
    message: String,
}

fn spec(name: &str, library: &str, capabilities: Vec<Capability>) -> ExtensionSpec {
    ExtensionSpec {
        name: name.into(),
        description: "Return typed input with its authorized library.".into(),
        contract_version: 1,
        libraries: vec![library.into()],
        required_capabilities: capabilities,
    }
}

fn echo(spec: ExtensionSpec, calls: Arc<AtomicUsize>) -> Extension {
    Extension::typed(spec, move |context, input: EchoInput| {
        calls.fetch_add(1, Ordering::SeqCst);
        async move {
            Ok(EchoOutput {
                library: context.library_id().into(),
                message: input.message,
            })
        }
    })
    .unwrap()
}

struct Fixture {
    _root: TempDir,
    gateway: Gateway,
    calls: Arc<AtomicUsize>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut configs = Vec::new();
        for library in ["alpha", "beta"] {
            let path = root.path().join(library);
            std::fs::create_dir_all(&path).unwrap();
            configs.push(LibraryConfig::load(&path, library).unwrap());
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let extensions = vec![
            echo(
                spec("example.echo", "alpha", vec![Capability::Read]),
                calls.clone(),
            ),
            echo(
                spec(
                    "example.restricted",
                    "alpha",
                    vec![Capability::Read, Capability::Write],
                ),
                calls.clone(),
            ),
            echo(
                spec("example.other", "beta", vec![Capability::Read]),
                calls.clone(),
            ),
        ];
        Self {
            _root: root,
            gateway: Gateway::new(
                Application::with_extensions(configs, extensions).unwrap(),
                "alpha".into(),
            ),
            calls,
        }
    }
}

fn request(library: &str, operation: &str, args: Value) -> Request {
    Request {
        library_id: library.into(),
        operation: operation.into(),
        args,
    }
}

#[tokio::test]
async fn gateway_authorizes_and_deserializes_before_calling_typed_handlers() {
    let fixture = Fixture::new();
    for (principal, library, operation) in [
        (Principal::public("alpha"), "alpha", "example.restricted"),
        (Principal::local("beta"), "beta", "example.echo"),
        (Principal::local("alpha"), "beta", "example.other"),
    ] {
        let result = fixture
            .gateway
            .execute(
                principal,
                request(library, operation, json!({"message":17})),
            )
            .await;
        assert!(matches!(result, Err(Error::Scope)), "{result:?}");
    }
    for args in [json!({"message":17}), json!({"message":"hi","extra":true})] {
        let result = fixture
            .gateway
            .execute(
                Principal::public("alpha"),
                request("alpha", "example.echo", args),
            )
            .await;
        assert!(matches!(result, Err(Error::Invalid(_) | Error::Json(_))));
    }
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    let result = fixture
        .gateway
        .execute(
            Principal::public("alpha"),
            request("alpha", "example.echo", json!({"message":"hello"})),
        )
        .await
        .unwrap();
    assert_eq!(result, json!({"library":"alpha","message":"hello"}));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dropping_gateway_execution_cancels_the_extension_context() {
    let root = tempfile::tempdir().unwrap();
    let (started, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let extension = Extension::typed(
        spec("example.wait", "alpha", vec![Capability::Read]),
        move |context, _: EchoInput| {
            started.send(context.cancellation()).unwrap();
            std::future::pending::<llmbase_core::Result<EchoOutput>>()
        },
    )
    .unwrap();
    let gateway = Gateway::new(
        Application::with_extensions(
            vec![LibraryConfig::load(root.path(), "alpha").unwrap()],
            vec![extension],
        )
        .unwrap(),
        "alpha".into(),
    );
    let task = tokio::spawn(async move {
        gateway
            .execute(
                Principal::public("alpha"),
                request("alpha", "example.wait", json!({"message":"wait"})),
            )
            .await
    });
    let cancellation = tokio::time::timeout(Duration::from_secs(5), observed.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(!cancellation.is_cancelled());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(cancellation.is_cancelled());
}

async fn http(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
    library: Option<&str>,
) -> (u16, Value) {
    let mut request = HttpRequest::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(library) = library {
        request = request.header("x-library-id", library);
    }
    let body = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

fn extension_names(items: &Value) -> Vec<&str> {
    let mut names: Vec<_> = items
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .filter(|name| name.starts_with("example."))
        .collect();
    names.sort_unstable();
    names
}

#[tokio::test]
async fn http_discovery_and_dispatch_use_the_selected_authorized_library() {
    let fixture = Fixture::new();
    let token = "extension-test-owner-token";
    let mut owner = Principal::local("alpha");
    owner.libraries.insert("beta".into());
    let app = router(HttpState::new(
        fixture.gateway.clone(),
        Surface::Agent,
        true,
        vec![AuthGrant::new(token, owner).unwrap()],
    ));
    let (status, public) = http(&app, "GET", "/api/op", None, None, None).await;
    assert_eq!(status, 200);
    assert_eq!(extension_names(&public["operations"]), ["example.echo"]);
    let operations = public["operations"].as_array().unwrap();
    let builtin = operations
        .iter()
        .find(|op| op["name"] == "kb_search")
        .unwrap();
    assert_eq!(
        builtin,
        operation_catalog()
            .iter()
            .find(|op| op["name"] == "kb_search")
            .unwrap()
    );
    let descriptor = operations
        .iter()
        .find(|op| op["name"] == "example.echo")
        .unwrap();
    assert_eq!(descriptor["contract_version"], 1);
    assert_eq!(descriptor["params"], descriptor["input_schema"]);
    assert_eq!(
        descriptor["output_schema"]["properties"]["library"]["type"],
        "string"
    );
    let (status, owner) = http(&app, "GET", "/api/op", None, Some(token), None).await;
    assert_eq!(status, 200);
    assert_eq!(
        extension_names(&owner["operations"]),
        ["example.echo", "example.restricted"]
    );
    let (status, other) = http(&app, "GET", "/api/op", None, Some(token), Some("beta")).await;
    assert_eq!(status, 200);
    assert_eq!(extension_names(&other["operations"]), ["example.other"]);
    assert_eq!(
        http(&app, "GET", "/api/op", None, None, Some("beta"))
            .await
            .0,
        403
    );
    for (path, args, auth, library, expected) in [
        (
            "/api/op/example.restricted",
            json!({"message":17}),
            None,
            None,
            403,
        ),
        (
            "/api/op/example.echo",
            json!({"message":17}),
            None,
            None,
            400,
        ),
        (
            "/api/op/example.echo",
            json!({"message":"hi","api_key":"forbidden"}),
            None,
            None,
            400,
        ),
        (
            "/api/op/example.echo",
            json!({"message":"hi"}),
            Some("invalid-test-bearer"),
            None,
            401,
        ),
        (
            "/api/op/example.echo",
            json!({"message":"hi"}),
            Some(token),
            Some("beta"),
            403,
        ),
    ] {
        let (status, _) = http(&app, "POST", path, Some(args), auth, library).await;
        assert_eq!(status, expected, "{path}");
    }
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    let (status, result) = http(
        &app,
        "POST",
        "/api/op/example.other",
        Some(json!({"message":"other"})),
        Some(token),
        Some("beta"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        result["result"],
        json!({"library":"beta","message":"other"})
    );
    let (status, result) = http(
        &app,
        "POST",
        "/api/v1/op",
        Some(json!({"library_id":"alpha","operation":"example.echo","args":{"message":"native"}})),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        result["result"],
        json!({"library":"alpha","message":"native"})
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
}

struct McpConnection {
    input: WriteHalf<DuplexStream>,
    output: Lines<BufReader<ReadHalf<DuplexStream>>>,
    task: tokio::task::JoinHandle<()>,
}

impl McpConnection {
    async fn new(server: McpServer) -> Self {
        let (server_io, client_io) = tokio::io::duplex(65536);
        let task = tokio::spawn(async move {
            server
                .serve(server_io)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (output, input) = tokio::io::split(client_io);
        let mut connection = Self {
            input,
            output: BufReader::new(output).lines(),
            task,
        };
        let response = connection.rpc(1, "initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"extensions-test","version":"1"}})).await;
        assert!(response.get("error").is_none(), "{response}");
        connection
            .input
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .unwrap();
        connection
    }

    async fn rpc(&mut self, id: u64, method: &str, params: Value) -> Value {
        let request = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.send(request).await;
        loop {
            let line = tokio::time::timeout(Duration::from_secs(5), self.output.next_line())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            if response["id"] == id {
                return response;
            }
        }
    }

    async fn send(&mut self, message: Value) {
        self.input
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn close(self) {
        drop(self.input);
        drop(self.output);
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn mcp_discovers_typed_schemas_and_dispatches_only_with_required_authority() {
    let fixture = Fixture::new();
    let mut connection = McpConnection::new(McpServer {
        gateway: fixture.gateway.clone(),
        principal: Principal::public("alpha"),
    })
    .await;
    let response = connection.rpc(2, "tools/list", json!({})).await;
    assert_eq!(
        extension_names(&response["result"]["tools"]),
        ["example.echo"]
    );
    let tool = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "example.echo")
        .unwrap();
    assert_eq!(
        tool["inputSchema"]["properties"]["message"]["type"],
        "string"
    );
    assert_eq!(
        tool["outputSchema"]["properties"]["library"]["type"],
        "string"
    );
    let forbidden = connection
        .rpc(
            3,
            "tools/call",
            json!({"name":"example.restricted","arguments":{"message":17}}),
        )
        .await;
    assert_eq!(forbidden["result"]["isError"], true);
    let error: Value =
        serde_json::from_str(forbidden["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["code"], "forbidden");
    let invalid = connection
        .rpc(
            4,
            "tools/call",
            json!({"name":"example.echo","arguments":{"message":17}}),
        )
        .await;
    assert_eq!(invalid["result"]["isError"], true);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    let response = connection
        .rpc(
            5,
            "tools/call",
            json!({"name":"example.echo","arguments":{"message":"mcp"}}),
        )
        .await;
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["structuredContent"],
        json!({"library":"alpha","message":"mcp"})
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    connection.close().await;
    let mut other_gateway = fixture.gateway.clone();
    other_gateway.default_library = "beta".into();
    let mut connection = McpConnection::new(McpServer {
        gateway: other_gateway,
        principal: Principal::local("beta"),
    })
    .await;
    let response = connection.rpc(2, "tools/list", json!({})).await;
    assert_eq!(
        extension_names(&response["result"]["tools"]),
        ["example.other"]
    );
    let forbidden = connection
        .rpc(
            3,
            "tools/call",
            json!({"name":"example.echo","arguments":{"message":"wrong library"}}),
        )
        .await;
    assert_eq!(forbidden["result"]["isError"], true);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    connection.close().await;
}

#[tokio::test]
async fn mcp_cancel_notification_cancels_retained_context_and_blocks_later_writes() {
    let root = tempfile::tempdir().unwrap();
    let (started, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    let extension = Extension::typed(
        spec(
            "example.wait",
            "alpha",
            vec![Capability::Read, Capability::Write],
        ),
        move |context, _: EchoInput| {
            handler_calls.fetch_add(1, Ordering::SeqCst);
            started.send(context).unwrap();
            std::future::pending::<llmbase_core::Result<EchoOutput>>()
        },
    )
    .unwrap();
    let gateway = Gateway::new(
        Application::with_extensions(
            vec![LibraryConfig::load(root.path(), "alpha").unwrap()],
            vec![extension],
        )
        .unwrap(),
        "alpha".into(),
    );
    let mut connection = McpConnection::new(McpServer {
        gateway,
        principal: Principal::local("alpha"),
    })
    .await;
    connection
        .send(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"example.wait","arguments":{"message":"wait"}}}))
        .await;
    let context = tokio::time::timeout(Duration::from_secs(5), observed.recv())
        .await
        .unwrap()
        .unwrap();
    let cancellation = context.cancellation();
    assert!(!cancellation.is_cancelled());
    connection
        .send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2}}))
        .await;
    tokio::time::timeout(Duration::from_secs(5), cancellation.cancelled())
        .await
        .unwrap();
    let result = context
        .execute(
            "source_write",
            json!({"kind":"raw","path":"raw/late.md","content":"This write must be denied."}),
        )
        .await;
    assert!(matches!(
        result,
        Err(Error::Model(llmbase_model::ModelError::Cancelled))
    ));
    assert!(!root.path().join("raw/late.md").exists());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // rmcp suppresses the cancelled response; the connection remains usable.
    assert!(
        connection
            .rpc(3, "ping", json!({}))
            .await
            .get("result")
            .is_some()
    );
    connection.close().await;
}
