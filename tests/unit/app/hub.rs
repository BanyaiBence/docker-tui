use crate::support::*;
use crossterm::event::KeyCode;
use docker_tui::{
    app::App,
    hub::models::{SearchPage, TagPage},
    msg::Command,
};

use docker_tui::{app::Tab, msg::Msg};

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.update(key(KeyCode::Char(c)));
    }
}

fn hub_app() -> App {
    let mut app = App::new();
    app.tab = Tab::Hub;
    app
}

/// Hub tab with a submitted search for "nginx" and two results.
fn app_with_results(has_next: bool) -> App {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('/')));
    type_text(&mut app, "nginx");
    app.update(key(KeyCode::Enter));
    app.update(Msg::HubSearched {
        query: "nginx".into(),
        page: 1,
        result: Ok(SearchPage {
            count: 3,
            results: vec![repo("nginx"), repo("bitnami/nginx")],
            has_next,
        }),
    });
    app
}

#[test]
fn slash_starts_editing_and_typing_fills_input() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('/')));
    assert!(app.hub.editing);
    type_text(&mut app, "q1");
    assert_eq!(app.hub.input.value(), "q1");
    assert!(!app.should_quit, "q is text while editing");
    assert_eq!(app.tab, Tab::Hub, "1 is text while editing");
}

#[test]
fn enter_submits_search() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('/')));
    type_text(&mut app, "redis");
    let cmds = app.update(key(KeyCode::Enter));
    assert_eq!(
        cmds,
        vec![Command::HubSearch {
            query: "redis".into(),
            page: 1
        }]
    );
    assert!(!app.hub.editing);
    assert!(app.hub.loading);
    assert_eq!(app.hub.query, "redis");
}

#[test]
fn enter_on_blank_query_does_nothing() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('/')));
    type_text(&mut app, "  ");
    assert!(app.update(key(KeyCode::Enter)).is_empty());
}

#[test]
fn esc_stops_editing_without_searching() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('/')));
    type_text(&mut app, "x");
    assert!(app.update(key(KeyCode::Esc)).is_empty());
    assert!(!app.hub.editing);
}

#[test]
fn results_are_stored() {
    let app = app_with_results(false);
    assert_eq!(app.hub.results.len(), 2);
    assert_eq!(app.hub.count, 3);
    assert!(!app.hub.loading);
    assert_eq!(app.hub.selected().unwrap().name, "nginx");
}

#[test]
fn stale_results_are_ignored() {
    let mut app = app_with_results(false);
    app.update(Msg::HubSearched {
        query: "old".into(),
        page: 1,
        result: Ok(SearchPage {
            count: 1,
            results: vec![repo("old")],
            has_next: false,
        }),
    });
    assert_eq!(app.hub.results.len(), 2);
}

#[test]
fn search_error_is_shown() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('/')));
    type_text(&mut app, "x");
    app.update(key(KeyCode::Enter));
    app.update(Msg::HubSearched {
        query: "x".into(),
        page: 1,
        result: Err("rate limit".into()),
    });
    assert_eq!(app.hub.error.as_deref(), Some("rate limit"));
    assert!(!app.hub.loading);
}

#[test]
fn reaching_the_end_loads_next_page() {
    let mut app = app_with_results(true);
    let cmds = app.update(key(KeyCode::Char('j')));
    assert_eq!(
        cmds,
        vec![Command::HubSearch {
            query: "nginx".into(),
            page: 2
        }]
    );
    // no duplicate request while loading
    assert!(app.update(key(KeyCode::Char('j'))).is_empty());
    app.update(Msg::HubSearched {
        query: "nginx".into(),
        page: 2,
        result: Ok(SearchPage {
            count: 3,
            results: vec![repo("x/nginx")],
            has_next: false,
        }),
    });
    assert_eq!(app.hub.results.len(), 3);
    assert!(!app.hub.has_next);
}

#[test]
fn no_next_page_when_last() {
    let mut app = app_with_results(false);
    assert!(app.update(key(KeyCode::Char('j'))).is_empty());
}

#[test]
fn enter_opens_tags() {
    let mut app = app_with_results(false);
    let cmds = app.update(key(KeyCode::Enter));
    assert_eq!(
        cmds,
        vec![Command::HubTags {
            repo: repo("nginx"),
            page: 1
        }]
    );
    let tags = app.hub.tags.as_ref().unwrap();
    assert_eq!(tags.repo.name, "nginx");
    assert!(tags.loading);
}

fn app_with_tags() -> App {
    let mut app = app_with_results(false);
    app.update(key(KeyCode::Enter));
    app.update(Msg::HubTagsLoaded {
        repo: "nginx".into(),
        page: 1,
        result: Ok(TagPage {
            count: 2,
            results: vec![tag("latest"), tag("1.27")],
            has_next: false,
        }),
    });
    app
}

#[test]
fn tags_are_stored_and_stale_ignored() {
    let mut app = app_with_tags();
    app.update(Msg::HubTagsLoaded {
        repo: "other".into(),
        page: 1,
        result: Ok(TagPage {
            count: 1,
            results: vec![tag("x")],
            has_next: false,
        }),
    });
    let tags = app.hub.tags.as_ref().unwrap();
    assert_eq!(tags.items.len(), 2);
    assert!(!tags.loading);
}

#[test]
fn enter_on_tag_pulls_it() {
    let mut app = app_with_tags();
    app.update(key(KeyCode::Char('j')));
    assert_eq!(
        app.update(key(KeyCode::Enter)),
        vec![Command::Pull("nginx:1.27".into())]
    );
}

#[test]
fn esc_closes_tags() {
    let mut app = app_with_tags();
    app.update(key(KeyCode::Esc));
    assert!(app.hub.tags.is_none());
}

#[test]
fn p_pulls_latest_from_results() {
    let mut app = app_with_results(false);
    app.update(key(KeyCode::Char('j')));
    assert_eq!(
        app.update(key(KeyCode::Char('p'))),
        vec![Command::Pull("bitnami/nginx:latest".into())]
    );
}

#[test]
fn slash_from_results_edits_again() {
    let mut app = app_with_results(false);
    app.update(key(KeyCode::Char('/')));
    assert!(app.hub.editing);
    assert_eq!(app.hub.input.value(), "nginx");
}
