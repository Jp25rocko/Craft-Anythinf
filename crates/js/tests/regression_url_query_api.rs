//! GraphQL and tRPC endpoints are read from a URL's path. The host was taken to end at the first
//! `/`, so with no path a slash in the query or fragment started one: `https://example.test?next=/graphql`
//! read as a GraphQL endpoint and `https://example.test#next=/trpc/user.list` as a tRPC procedure.

use craft_anything_js::api_ops::{UrlApi, url_api};
use craft_anything_js::{Limits, ParseMode, analyze};

#[test]
fn a_query_or_fragment_is_not_the_path() {
    for url in [
        "https://example.test?next=/graphql",
        "https://example.test/?next=/graphql",
        "https://example.test#next=/trpc/user.list",
        "https://example.test?return=/api/trpc",
        "http://localhost:3000?redirect=/gql",
        "https://login.example.test?continue=https://api.example.test/graphql",
    ] {
        assert_eq!(url_api(url), None, "{url}");
    }
}

/// Paths still count, with or without a query or fragment after them.
#[test]
fn paths_still_count() {
    assert_eq!(url_api("https://api.example.test/graphql"), Some(UrlApi::Graphql));
    assert_eq!(url_api("https://api.example.test/graphql?next=/home"), Some(UrlApi::Graphql));
    assert_eq!(url_api("https://api.example.test/v1/gql#top"), Some(UrlApi::Graphql));
    assert_eq!(url_api("http://localhost:3000/api/trpc/user.list?batch=1"), Some(UrlApi::Trpc { procedures: vec![Some("user.list".into())] }));
    assert_eq!(url_api("/graphql?next=/x"), Some(UrlApi::Graphql));
}

/// End to end: a login redirect is not an API operation; a real endpoint still is.
#[test]
fn analysis_reads_only_the_path() {
    let src = "fetch('https://auth.example.test?next=/graphql');\nfetch('https://auth.example.test#next=/trpc/user.list');\n";
    let a = analyze("app.js", src, &Limits::default());
    assert_eq!(a.facts.mode, ParseMode::Parsed);
    assert!(a.facts.api_operations.is_empty(), "{:?}", a.facts.api_operations);
    let a = analyze("app.js", "fetch('https://api.example.test/graphql?next=/home');\n", &Limits::default());
    assert!(a.facts.api_operations.iter().any(|o| o.protocol == "graphql"), "{:?}", a.facts.api_operations);
}
