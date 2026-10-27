use crate::support::*;
use crossterm::event::KeyCode;
use docker_tui::{
    docker::pull::{PullEvent, PullState},
    msg::Command,
};

use docker_tui::{
    app::{App, Tab},
    hub::models::SearchPage,
    msg::Msg,
};

/// Hub results with "nginx" selected, ready to press `p`.
fn hub_app() -> App {
    let mut app = App::new();
    app.tab = Tab::Hub;
    app.hub.query = "nginx".into();
    app.update(Msg::HubSearched {
        query: "nginx".into(),
        page: 1,
        result: Ok(SearchPage {
            count: 1,
            results: vec![repo("nginx")],
            has_next: false,
        }),
    });
    app
}

#[test]
fn pulling_registers_state_and_opens_panel() {
    let mut app = hub_app();
    assert_eq!(
        app.update(key(KeyCode::Char('p'))),
        vec![Command::Pull("nginx:latest".into())]
    );
    assert!(app.pulls.items.contains_key("nginx:latest"));
    assert!(app.pulls.visible);
    assert_eq!(app.pulls.active(), 1);
}

#[test]
fn duplicate_pull_is_ignored() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('p')));
    app.update(key(KeyCode::Esc));
    assert!(app.update(key(KeyCode::Char('p'))).is_empty());
    assert!(app.toast.as_ref().unwrap().text.contains("already"));
}

#[test]
fn progress_is_applied() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('p')));
    app.update(Msg::PullProgress {
        image: "nginx:latest".into(),
        event: PullEvent {
            id: Some("l1".into()),
            status: "Downloading".into(),
            current: Some(1),
            total: Some(2),
        },
    });
    assert_eq!(app.pulls.items["nginx:latest"].layers["l1"].current, 1);
}

#[test]
fn finished_pull_toasts_and_refreshes_images() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('p')));
    let cmds = app.update(Msg::PullDone {
        image: "nginx:latest".into(),
        result: Ok(()),
    });
    assert_eq!(cmds, vec![Command::LoadImages]);
    assert!(app.pulls.items["nginx:latest"].done);
    assert_eq!(app.pulls.active(), 0);
    assert_eq!(app.toast.as_ref().unwrap().text, "Pulled nginx:latest");
}

#[test]
fn failed_pull_shows_error() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('p')));
    app.update(Msg::PullDone {
        image: "nginx:latest".into(),
        result: Err("manifest unknown".into()),
    });
    assert!(app.toast.as_ref().unwrap().error);
    assert_eq!(
        app.pulls.items["nginx:latest"].error.as_deref(),
        Some("manifest unknown")
    );
}

#[test]
fn panel_toggles_with_shift_p_and_closes_with_esc() {
    let mut app = App::new();
    app.update(key(KeyCode::Char('P')));
    assert!(app.pulls.visible);
    app.update(key(KeyCode::Char('1')));
    assert_eq!(app.tab, Tab::Containers, "panel is modal");
    app.update(key(KeyCode::Esc));
    assert!(!app.pulls.visible);
}

#[test]
fn c_clears_finished_pulls() {
    let mut app = hub_app();
    app.update(key(KeyCode::Char('p')));
    app.pulls.items.insert(
        "old:1".into(),
        PullState {
            done: true,
            ..Default::default()
        },
    );
    app.update(key(KeyCode::Char('c')));
    assert_eq!(app.pulls.items.keys().collect::<Vec<_>>(), ["nginx:latest"]);
}
