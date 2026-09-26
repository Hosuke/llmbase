# Public-web fetch and URL ingest

The default `llmwiki.ingest_url` now calls `llmbase_core::fetch::fetch` through the
in-process PyO3 bridge. BeautifulSoup, markdownify, live title/extension helpers,
surrogate cleanup and `ingested` hooks remain Python format/extension boundaries.
Accepted article Markdown and exact image bytes are written by
`compat_kb_raw_write` inside the shared native writer lease. Network and conversion
finish before that lease is acquired. Individual writes are atomic; the group is
not a multi-file transaction (a failed final write may leave downloaded assets).
The original response is also retained byte-for-byte as `source.original`, with
`original_file`, `original_sha256` and optional `original_media_type` metadata.
This opaque original is not fed into the Markdown text index. Its write uses
`ingest_asset` with a native current-hash read and CAS under the writer lease,
retaining exact versions in durable write-intent history. It initializes native
storage for original revision history, without forcing unrelated Markdown indexing.

`fetch(&str, FetchOptions, CancellationToken)` returns final URL, HTTP status,
content type, original body bytes, redirects and actual HTTP attempt count. It
does not initialize a library. The native CLI can use this same API and then the
shared raw writer; HTML conversion must explicitly select an adapter. The Python
bridge returns a receipt containing the native adapter, attempt/byte counts and a
sequence, without URLs, headers, credentials or response content in the trace.

For every request, including each redirect, Rust resolves the hostname, requires
every answer to pass the address policy, and gives those exact addresses to
reqwest `resolve_to_addrs`. It also checks the connected peer against that set.
IP literals take the same policy path. Invalid/empty DNS results fail closed; an
oversized answer set is rejected before deduplication. New clients per hop avoid
connection-pool/DNS reuse. Environment/system proxies, automatic redirects,
automatic retries, decompression and cookie storage are disabled. TLS certificate
and hostname verification remain enabled. No command or shell is involved.

The policy accepts public unicast only. It rejects IPv4 private, loopback,
link-local, shared carrier, documentation, benchmarking, multicast, reserved and
unspecified ranges. IPv6 is conservatively limited to global unicast `2000::/3`,
excluding special IETF `2001::/23`, documentation and 6to4 ranges. Mapped IPv4,
compatible IPv4, NAT64 and other transition mechanisms are rejected even when an
embedded IPv4 address is public. This intentionally also excludes some globally
reachable special-purpose assignments. URL userinfo, local hostnames, controls,
backslashes and non-HTTP(S) schemes are rejected. Errors omit the submitted URL.

Defaults are 8 MiB, 30 seconds total (including DNS, redirects and streaming),
10 seconds connection timeout and five redirects. Valid caller limits cannot
exceed 32 MiB, 120 seconds or ten redirects. Length headers and streamed chunks
are both checked. Compressed responses are rejected even if a server ignores
`Accept-Encoding: identity`, avoiding decompression bombs and preserving bytes.
Cancellation drops the in-flight future. Platform DNS threads may outlive a
cancelled resolver, but their result is unused and bridge shutdown does not wait
for them or start a connection afterward.

URL ingest applies the same fetch path to images, resolving relative URLs against
the final page URL. It permits at most 32 images, 4 MiB per image and 32 MiB total
assets, within a 120-second network budget. Failed images retain the old skip
behavior. A cancellation aborts before persistence. The default URL fetch requires
a built native extension even in a development checkout; it never falls back to
the old unpinned requests path.

A live replacement of `requests.get` remains an explicit trusted Python adapter.
The actual callable runs, receives `allow_redirects=False`, and gets an explicit
`core: python` receipt. Its redirects pass the live `_validate_url` hook again;
response size is checked after the callback returns. An arbitrary Python adapter
can perform its own network work or ignore arguments, so native pinning, timeout
and cancellation guarantees are not claimed for this extension boundary.
Replacing `_validate_url` alone cannot bypass the default native fetcher's policy.

Tests include public API address/URL spellings, pre-cancellation, mixed DNS
answers, an actual pinned HTTP connection, proxy environment isolation, relative
redirect re-resolution, private/userinfo redirect refusal, chunked/declared size
bounds and stalled-body cancellation/deadline. The loopback fixture is injected
only by private `cfg(test)` code; production exposes no private-network override.
Candidate Python tests additionally cover live callback replacement, image
validation, cancellation and native byte-preserving persistence with Python file
writers disabled.

Installed candidate v10 passed all seven fetch facade tests and the full 569-test
Python suite. Native fetch integration tests (three), internal pinning/deadline
tests (five, plus the proxy child fixture), and ingest/history tests (five) passed.
The immutable installed-package receipt is
[`python-native-candidate-v10.json`](results/python-native-candidate-v10.json).

Policy references: [IANA IPv4 special-purpose registry](https://www.iana.org/assignments/iana-ipv4-special-registry),
[IANA IPv6 special-purpose registry](https://www.iana.org/assignments/iana-ipv6-special-registry),
and [reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html).
The conservative address table was checked against these registries on 2026-09-20.
