use docker_tui::hub::models::*;

const SEARCH: &str = include_str!("../../fixtures/hub_search.json");
const TAGS: &str = include_str!("../../fixtures/hub_tags.json");

fn repo(name: &str, official: bool) -> Repo {
    Repo {
        name: name.into(),
        description: String::new(),
        stars: 0,
        pulls: 0,
        official,
    }
}

#[test]
fn parses_search_results() {
    let page = parse_search(SEARCH).unwrap();
    assert!(page.count > 1000);
    assert!(page.has_next);
    assert_eq!(page.results.len(), 3);
    let nginx = &page.results[0];
    assert_eq!(nginx.name, "nginx");
    assert!(nginx.official);
    assert!(nginx.stars > 0 && nginx.pulls > 0);
    assert_eq!(nginx.description, "Official build of Nginx.");
    assert_eq!(page.results[1].name, "nginx/nginx-ingress");
}

#[test]
fn parses_tags() {
    let page = parse_tags(TAGS).unwrap();
    assert!(page.has_next);
    assert_eq!(page.results.len(), 3);
    let t = &page.results[0];
    assert_eq!(t.name, "stable-alpine3.24-slim");
    assert_eq!(t.size, 5_740_963);
    assert_eq!(t.architectures, ["amd64", "ppc64le"]);
    assert!(t.last_updated.as_deref().unwrap().starts_with("2026-"));
}

#[test]
fn last_page_has_no_next() {
    let page = parse_search(r#"{"count": 1, "next": "", "results": []}"#).unwrap();
    assert!(!page.has_next);
    let page = parse_tags(r#"{"count": 1, "next": null, "results": []}"#).unwrap();
    assert!(!page.has_next);
}

#[test]
fn tolerates_missing_and_null_fields() {
    let page =
        parse_search(r#"{"results": [{"repo_name": "x", "short_description": null}]}"#).unwrap();
    assert_eq!(page.results[0].name, "x");
    let page = parse_tags(r#"{"results": [{"name": "latest", "images": null}]}"#).unwrap();
    assert_eq!(page.results[0].architectures, Vec::<String>::new());
}

#[test]
fn invalid_json_is_an_error() {
    assert!(parse_search("<html>").is_err());
}

#[test]
fn architectures_are_deduplicated_and_skip_unknown() {
    let json = r#"{"results": [{"name": "t", "images": [
        {"architecture": "amd64"}, {"architecture": "amd64"}, {"architecture": "unknown"}, {"architecture": "arm64"}
    ]}]}"#;
    assert_eq!(
        parse_tags(json).unwrap().results[0].architectures,
        ["amd64", "arm64"]
    );
}

#[test]
fn official_repos_live_in_library() {
    assert_eq!(
        repo("nginx", true).namespace_and_name(),
        ("library", "nginx")
    );
    assert_eq!(
        repo("bitnami/redis", false).namespace_and_name(),
        ("bitnami", "redis")
    );
}

#[test]
fn image_ref_includes_tag() {
    assert_eq!(repo("nginx", true).image_ref("1.27"), "nginx:1.27");
    assert_eq!(
        repo("bitnami/redis", false).image_ref("latest"),
        "bitnami/redis:latest"
    );
}
