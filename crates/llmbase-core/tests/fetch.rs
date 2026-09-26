use llmbase_core::fetch::{
    CancellationToken, FetchOptions, fetch, is_public_address, validate_url,
};

#[test]
fn public_unicast_policy_covers_special_ranges_and_transition_forms() {
    for address in [
        "0.0.0.0",
        "0.1.2.3",
        "10.3.4.5",
        "100.64.0.1",
        "100.127.255.254",
        "127.9.8.7",
        "169.254.169.254",
        "172.16.0.1",
        "172.31.255.1",
        "192.0.0.1",
        "192.0.2.1",
        "192.88.99.1",
        "192.168.1.1",
        "198.18.0.1",
        "198.19.1.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "239.255.255.255",
        "240.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "::127.0.0.1",
        "::ffff:127.0.0.1",
        "::ffff:8.8.8.8",
        "64:ff9b::7f00:1",
        "64:ff9b:1::1",
        "100::1",
        "100:0:0:1::1",
        "2001::1",
        "2001:2::1",
        "2001:db8::1",
        "2002:7f00:1::",
        "3fff::1",
        "5f00::1",
        "fc00::1",
        "fdff::1",
        "fe80::1",
        "fec0::1",
        "ff02::1",
    ] {
        assert!(!is_public_address(address.parse().unwrap()), "{address}");
    }
    for address in [
        "8.8.8.8",
        "1.1.1.1",
        "93.184.216.34",
        "100.63.255.255",
        "100.128.0.1",
        "172.15.1.1",
        "172.32.1.1",
        "2001:4860:4860::8888",
        "2606:4700:4700::1111",
    ] {
        assert!(is_public_address(address.parse().unwrap()), "{address}");
    }
}

#[tokio::test]
async fn dangerous_url_spellings_fail_before_any_http() {
    for url in [
        "file:///etc/passwd",
        "http://localhost/",
        "http://LOCALHOST./",
        "http://x.localhost/",
        "http://127.1/",
        "http://2130706433/",
        "http://0x7f000001/",
        "http://0177.0.0.1/",
        "http://[::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://[fe80::1%25en0]/",
        "http://169.254.169.254/",
        "http://u:secret@8.8.8.8/",
        "http://@8.8.8.8/",
        "http://8.8.8.8:0/",
        "http://8.8.8.8/\nsecret",
    ] {
        let err = fetch(url, FetchOptions::default(), CancellationToken::new())
            .await
            .unwrap_err();
        assert!(!err.to_string().contains("secret"), "{url}");
        assert!(
            !err.to_string().contains("HTTP request failed"),
            "{url}: {err}"
        );
    }
}

#[tokio::test]
async fn precancel_and_invalid_bounds_do_not_resolve_or_fetch() {
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        fetch(
            "https://unresolved.invalid/",
            FetchOptions::default(),
            token.clone()
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
    assert!(
        validate_url("https://unresolved.invalid/", token)
            .await
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    let options = FetchOptions {
        max_bytes: usize::MAX,
        ..Default::default()
    };
    assert!(
        fetch(
            "https://unresolved.invalid/",
            options,
            CancellationToken::new()
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("limits")
    );
}
