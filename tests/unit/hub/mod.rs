use docker_tui::hub::models::Repo;
mod models;

use docker_tui::hub::*;

#[test]
fn search_url_encodes_query() {
    assert_eq!(
        search_url("my app&x", 2),
        "https://hub.docker.com/v2/search/repositories/?query=my%20app%26x&page=2&page_size=25"
    );
}

#[test]
fn tags_url_uses_namespace() {
    let repo = Repo {
        name: "nginx".into(),
        description: String::new(),
        stars: 0,
        pulls: 0,
        official: true,
    };
    assert_eq!(
        tags_url(&repo, 1),
        "https://hub.docker.com/v2/repositories/library/nginx/tags?page=1&page_size=25&ordering=last_updated"
    );
}
