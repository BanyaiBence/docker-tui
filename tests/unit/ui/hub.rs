use crate::support::*;
use crossterm::event::KeyCode;

use docker_tui::{
    app::{App, Tab},
    hub::models::{SearchPage, TagPage},
    msg::Msg,
};

fn app_with_results() -> App {
    let mut app = App::new();
    app.tab = Tab::Hub;
    app.update(key(KeyCode::Char('/')));
    for c in "nginx".chars() {
        app.update(key(KeyCode::Char(c)));
    }
    app.update(key(KeyCode::Enter));
    let mut nginx = repo("nginx");
    nginx.stars = 21_410;
    nginx.pulls = 13_508_033_679;
    nginx.description = "Official build of Nginx.".into();
    app.update(Msg::HubSearched {
        query: "nginx".into(),
        page: 1,
        result: Ok(SearchPage {
            count: 295_417,
            results: vec![nginx, repo("bitnami/nginx")],
            has_next: true,
        }),
    });
    app
}

#[test]
fn renders_empty_hub() {
    let mut app = App::new();
    app.tab = Tab::Hub;
    insta::assert_snapshot!(render_to_string(&app, 70, 9));
}

#[test]
fn renders_search_results() {
    insta::assert_snapshot!(render_to_string(&app_with_results(), 90, 10));
}

#[test]
fn renders_tags() {
    let mut app = app_with_results();
    app.update(key(KeyCode::Enter));
    let mut latest = tag("latest");
    latest.size = 5_740_963;
    latest.last_updated = Some("2026-10-24T19:52:22.389337Z".into());
    latest.architectures = vec!["amd64".into(), "arm64".into()];
    app.update(Msg::HubTagsLoaded {
        repo: "nginx".into(),
        page: 1,
        result: Ok(TagPage {
            count: 2,
            results: vec![latest, tag("1.27")],
            has_next: false,
        }),
    });
    insta::assert_snapshot!(render_to_string(&app, 80, 10));
}
