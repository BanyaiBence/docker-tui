use crate::support::*;
use crossterm::event::KeyCode;

use docker_tui::{
    app::App,
    docker::{models::ContainerState, stats::StatsSample},
    msg::Msg,
};

fn app_with_detail() -> App {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    app.update(key(KeyCode::Enter));
    app
}

#[test]
fn renders_loading_inspect() {
    insta::assert_snapshot!(render_to_string(&app_with_detail(), 70, 8));
}

#[test]
fn renders_inspect_json_scrolled() {
    let mut app = app_with_detail();
    app.update(Msg::Inspected {
        id: "c1".into(),
        result: Ok("{\n  \"Id\": \"c1\",\n  \"Name\": \"/web\"\n}".into()),
    });
    app.update(key(KeyCode::Char('j')));
    insta::assert_snapshot!(render_to_string(&app, 70, 8));
}

#[test]
fn renders_logs_following() {
    let mut app = app_with_detail();
    app.update(key(KeyCode::Char('l')));
    app.update(Msg::LogChunk {
        id: "c1".into(),
        text: "one\ntwo\nthree\nfour\n".into(),
    });
    insta::assert_snapshot!(render_to_string(&app, 70, 9));
}

#[test]
fn renders_logs_paused() {
    let mut app = app_with_detail();
    app.update(key(KeyCode::Char('l')));
    app.update(Msg::LogChunk {
        id: "c1".into(),
        text: "one\ntwo\nthree\nfour\n".into(),
    });
    app.update(key(KeyCode::Char('k')));
    insta::assert_snapshot!(render_to_string(&app, 70, 9));
}

#[test]
fn renders_stats() {
    let mut app = app_with_detail();
    app.update(key(KeyCode::Char('l')));
    app.update(key(KeyCode::Char('l')));
    for i in 0..20u64 {
        let sample = StatsSample {
            cpu_percent: (i % 5) as f64 * 10.0,
            mem_used: 50_000_000 + i * 1_000_000,
            mem_limit: 1_000_000_000,
            net_rx: 1_500,
            net_tx: 900,
        };
        app.update(Msg::Stats {
            id: "c1".into(),
            sample,
        });
    }
    insta::assert_snapshot!(render_to_string(&app, 60, 14));
}
