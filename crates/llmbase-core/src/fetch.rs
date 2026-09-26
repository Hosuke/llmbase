//! Bounded public-web fetching. DNS validation and the HTTP connection use the
//! same addresses; redirects never escape this policy. No proxy, shell or cookies.
use crate::{Error, Result};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
pub use tokio_util::sync::CancellationToken;

const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FetchOptions {
    pub max_bytes: usize,
    pub timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub max_redirects: usize,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            max_bytes: 8 * 1024 * 1024,
            timeout_ms: 30_000,
            connect_timeout_ms: 10_000,
            max_redirects: 5,
        }
    }
}

impl FetchOptions {
    fn validate(&self) -> Result<()> {
        if self.max_bytes == 0
            || self.max_bytes > MAX_BYTES
            || self.timeout_ms == 0
            || self.timeout_ms > 120_000
            || self.connect_timeout_ms == 0
            || self.connect_timeout_ms > self.timeout_ms
            || self.max_redirects > 10
        {
            return Err(Error::Invalid(
                "fetch limits exceed supported bounds".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct FetchResponse {
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub body_bytes: Vec<u8>,
    pub redirects: usize,
    pub http_attempts: usize,
}

/// Conservative public-unicast policy based on the IANA special-use registries.
/// IPv6 transition/mapped mechanisms are denied, including public embedded IPv4:
/// their ultimate routing destination cannot be verified by a socket address.
pub fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(a) => {
            let [a, b, c, _] = a.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 192 && b == 168)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(a) => {
            let s = a.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

fn parse_url(input: &str) -> Result<Url> {
    // Reject parser normalization of controls/whitespace and all userinfo,
    // including an empty username before @. Never include the input in errors.
    if input.len() > 16_384
        || input
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return Err(Error::Invalid("invalid fetch URL".into()));
    }
    let mut url = Url::parse(input).map_err(|_| Error::Invalid("invalid fetch URL".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || input.split_once("://").is_some_and(|(_, rest)| {
            rest.split(['/', '?', '#'])
                .next()
                .unwrap_or("")
                .contains('@')
        })
        || url.port_or_known_default().is_none_or(|port| port == 0)
    {
        return Err(Error::Invalid(
            "fetch requires an HTTP(S) URL without userinfo".into(),
        ));
    }
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err(Error::Invalid("fetch destination is not public".into()));
    }
    url.set_fragment(None);
    Ok(url)
}

struct Target {
    url: Url,
    addresses: Vec<SocketAddr>,
}

async fn resolve_target(url: Url) -> Result<Target> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::Invalid("missing fetch hostname".into()))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let literal = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>();
    let mut addresses = match literal {
        Ok(address) => vec![SocketAddr::new(address, port)],
        Err(_) => tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| Error::Invalid("fetch DNS resolution failed".into()))?
            .take(257)
            .collect(),
    };
    // Reject the truncated answer set before deduplication: duplicates must not
    // hide a later answer which was not examined by this fetcher.
    if addresses.len() > 256 {
        return Err(Error::Invalid("fetch DNS answer limit exceeded".into()));
    }
    addresses.sort_unstable();
    addresses.dedup();
    validate_addresses(&addresses)?;
    Ok(Target { url, addresses })
}

fn validate_addresses(addresses: &[SocketAddr]) -> Result<()> {
    if addresses.is_empty()
        || addresses.len() > 256
        || addresses.iter().any(|a| !is_public_address(a.ip()))
    {
        return Err(Error::Invalid("fetch destination is not public".into()));
    }
    Ok(())
}

fn client_for(target: &Target, options: &FetchOptions) -> Result<Client> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .timeout(Duration::from_millis(options.timeout_ms))
        .connect_timeout(Duration::from_millis(options.connect_timeout_ms))
        .resolve_to_addrs(target.url.host_str().unwrap_or(""), &target.addresses)
        .user_agent("LLMBase/1.0")
        .build()
        .map_err(|_| Error::Capability("fetch HTTP client unavailable".into()))
}

async fn fetch_resolved<F, Fut>(
    input: &str,
    options: &FetchOptions,
    resolve: F,
) -> Result<FetchResponse>
where
    F: Fn(Url) -> Fut,
    Fut: Future<Output = Result<Target>>,
{
    let mut url = parse_url(input)?;
    for hop in 0..=options.max_redirects {
        let target = resolve(url).await?;
        let mut response = client_for(&target, options)?
            .get(target.url.clone())
            .header("Accept-Encoding", "identity")
            .send()
            .await
            .map_err(|_| Error::Invalid("fetch HTTP request failed".into()))?;
        // Defence in depth: the connected peer must be one of our pinned IPs.
        if response
            .remote_addr()
            .is_none_or(|peer| !target.addresses.iter().any(|a| a.ip() == peer.ip()))
        {
            return Err(Error::Invalid(
                "fetch connected to an unvalidated address".into(),
            ));
        }
        if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            if hop == options.max_redirects {
                return Err(Error::Invalid("fetch redirect limit exceeded".into()));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or_else(|| Error::Invalid("fetch redirect has no valid location".into()))?;
            if location
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
                || location
                    .split_once("://")
                    .map(|(_, rest)| rest)
                    .or_else(|| location.strip_prefix("//"))
                    .is_some_and(|authority| {
                        authority
                            .split(['/', '?', '#'])
                            .next()
                            .unwrap_or("")
                            .contains('@')
                    })
            {
                return Err(Error::Invalid("invalid fetch redirect".into()));
            }
            let next = target
                .url
                .join(location)
                .map_err(|_| Error::Invalid("invalid fetch redirect".into()))?;
            url = parse_url(next.as_str())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(Error::Invalid(format!(
                "fetch returned HTTP {}",
                response.status().as_u16()
            )));
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|v| !v.as_bytes().eq_ignore_ascii_case(b"identity"))
        {
            return Err(Error::Invalid(
                "fetch requires an uncompressed response".into(),
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > options.max_bytes as u64)
        {
            return Err(Error::Invalid("fetch body size limit exceeded".into()));
        }
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let mut body_bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Invalid("fetch response body failed".into()))?
        {
            if chunk.len() > options.max_bytes - body_bytes.len() {
                return Err(Error::Invalid("fetch body size limit exceeded".into()));
            }
            body_bytes.extend_from_slice(&chunk);
        }
        return Ok(FetchResponse {
            url: target.url.to_string(),
            status,
            content_type,
            body_bytes,
            redirects: hop,
            http_attempts: hop + 1,
        });
    }
    unreachable!("redirect loop is bounded above")
}

/// Resolve and inspect a URL without sending HTTP. This is not a replacement for
/// fetch(): only fetch() pins those addresses into the subsequent connection.
pub async fn validate_url(input: &str, cancellation: CancellationToken) -> Result<()> {
    bounded(30_000, cancellation, async {
        resolve_target(parse_url(input)?).await.map(|_| ())
    })
    .await
}

async fn bounded<T>(
    timeout_ms: u64,
    cancellation: CancellationToken,
    work: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(Error::Invalid("fetch cancelled".into())),
        result = tokio::time::timeout(Duration::from_millis(timeout_ms), work) =>
            result.map_err(|_| Error::Invalid("fetch total timeout exceeded".into()))?,
    }
}

pub async fn fetch(
    input: &str,
    options: FetchOptions,
    cancellation: CancellationToken,
) -> Result<FetchResponse> {
    options.validate()?;
    bounded(
        options.timeout_ms,
        cancellation,
        fetch_resolved(input, &options, resolve_target),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(
        responses: Vec<String>,
    ) -> (u16, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = [0u8; 8192];
                let count = stream.read(&mut bytes).await.unwrap();
                recorded
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&bytes[..count]).into_owned());
                if response == "stall" {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                } else {
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        });
        (port, requests, task)
    }

    // Private test-only resolver supplies a pinned loopback fixture for one
    // otherwise unresolvable hostname. Other URLs use the production policy.
    // No public/config/env option can select this resolver or permit local IPs.
    async fn fixture_target(url: Url) -> Result<Target> {
        if url.host_str() == Some("pinned-fetch.invalid") {
            let address = SocketAddr::from(([127, 0, 0, 1], url.port().unwrap()));
            Ok(Target {
                url,
                addresses: vec![address],
            })
        } else {
            resolve_target(url).await
        }
    }

    #[tokio::test]
    async fn pinned_connection_ignores_proxy_and_revalidates_relative_redirect() {
        // Isolate proxy environment mutation in a new process, never in a
        // multithreaded test process. A working request proves no proxy was used.
        if std::env::var_os("LLMBASE_FETCH_PROXY_TEST_CHILD").is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "fetch::tests::pinned_connection_ignores_proxy_and_revalidates_relative_redirect", "--nocapture"])
                .env("LLMBASE_FETCH_PROXY_TEST_CHILD", "1")
                .env("HTTP_PROXY", "http://127.0.0.1:9")
                .env("http_proxy", "http://127.0.0.1:9")
                .env("ALL_PROXY", "http://127.0.0.1:9")
                .env("all_proxy", "http://127.0.0.1:9")
                .env("NO_PROXY", "").env("no_proxy", "").status().unwrap();
            assert!(result.success());
            return;
        }
        let (port, requests, server) = server(vec![
            "HTTP/1.1 302 Found\r\nLocation: /done\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\npong".into()]).await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let response = fetch_resolved(
            &format!("http://pinned-fetch.invalid:{port}/start"),
            &FetchOptions::default(),
            move |url| {
                seen.lock().unwrap().push(url.path().to_owned());
                fixture_target(url)
            },
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(response.body_bytes, b"pong");
        assert_eq!(response.redirects, 1);
        assert_eq!(response.http_attempts, 2);
        assert_eq!(*calls.lock().unwrap(), ["/start", "/done"]);
        assert!(requests.lock().unwrap().iter().all(|r| {
            r.to_lowercase()
                .contains(&format!("host: pinned-fetch.invalid:{port}"))
        }));
    }

    #[tokio::test]
    async fn redirect_to_private_or_userinfo_never_connects() {
        for location in [
            "http://127.0.0.1:9/secret",
            "http://[::ffff:127.0.0.1]/secret",
            "http://user:secret@8.8.8.8/",
            "http://@8.8.8.8/",
        ] {
            let (port, requests, server) = server(vec![format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n"
            )])
            .await;
            let err = fetch_resolved(
                &format!("http://pinned-fetch.invalid:{port}/start"),
                &FetchOptions::default(),
                fixture_target,
            )
            .await
            .unwrap_err();
            server.await.unwrap();
            assert!(
                err.to_string().contains("not public")
                    || err.to_string().contains("userinfo")
                    || err.to_string().contains("invalid fetch redirect")
            );
            assert!(!err.to_string().contains("secret"));
            assert_eq!(requests.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn bounded_chunked_declared_and_compressed_bodies() {
        for response in [
            "HTTP/1.1 200 OK\r\nContent-Length: 999999\r\n\r\n",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 0\r\n\r\n",
        ] {
            let (port, _, server) = server(vec![response.into()]).await;
            let options = FetchOptions {
                max_bytes: 4,
                ..Default::default()
            };
            let err = fetch_resolved(
                &format!("http://pinned-fetch.invalid:{port}/"),
                &options,
                fixture_target,
            )
            .await
            .unwrap_err();
            server.await.unwrap();
            assert!(
                err.to_string().contains("size limit") || err.to_string().contains("uncompressed")
            );
        }
    }

    #[tokio::test]
    async fn stalled_response_obeys_total_deadline_and_cancellation() {
        for cancel in [false, true] {
            let (port, _, server) = server(vec!["stall".into()]).await;
            let token = CancellationToken::new();
            if cancel {
                let trigger = token.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    trigger.cancel();
                });
            }
            let options = FetchOptions::default();
            let url = format!("http://pinned-fetch.invalid:{port}/");
            let start = std::time::Instant::now();
            let err = bounded(
                if cancel { 1000 } else { 30 },
                token,
                fetch_resolved(&url, &options, fixture_target),
            )
            .await
            .unwrap_err();
            assert!(start.elapsed() < Duration::from_secs(1));
            assert!(
                err.to_string()
                    .contains(if cancel { "cancelled" } else { "timeout" })
            );
            server.abort();
        }
    }

    #[tokio::test]
    async fn redirects_bounded_and_dns_mixed_answers_denied() {
        let (port, _, server) = server(vec![
            "HTTP/1.1 302 Found\r\nLocation: /again\r\nContent-Length: 0\r\n\r\n".into(),
        ])
        .await;
        let options = FetchOptions {
            max_redirects: 0,
            ..Default::default()
        };
        let err = fetch_resolved(
            &format!("http://pinned-fetch.invalid:{port}/"),
            &options,
            fixture_target,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("redirect limit"));
        server.await.unwrap();
        assert!(validate_addresses(&[]).is_err());
        assert!(
            validate_addresses(&[
                "8.8.8.8:80".parse().unwrap(),
                "10.0.0.1:80".parse().unwrap()
            ])
            .is_err()
        );
    }
}
