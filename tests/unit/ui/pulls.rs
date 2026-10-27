use crate::support::*;
use docker_tui::{
    app::App,
    docker::pull::{PullEvent, PullState},
};

fn ev(id: &str, status: &str, progress: Option<(u64, u64)>) -> PullEvent {
    PullEvent {
        id: Some(id.into()),
        status: status.into(),
        current: progress.map(|p| p.0),
        total: progress.map(|p| p.1),
    }
}

#[test]
fn renders_pull_panel() {
    let mut app = App::new();
    app.pulls.visible = true;
    let mut done = PullState::default();
    done.finish(Ok(()));
    app.pulls.items.insert("alpine:3.20".into(), done);
    let mut failed = PullState::default();
    failed.finish(Err("manifest unknown".into()));
    app.pulls.items.insert("nope:1".into(), failed);
    let mut active = PullState::default();
    active.apply(ev("a1b2c3d4e5f6", "Already exists", None));
    active.apply(ev(
        "0f9e8d7c6b5a",
        "Downloading",
        Some((3_000_000, 6_000_000)),
    ));
    app.pulls.items.insert("nginx:latest".into(), active);
    insta::assert_snapshot!(render_to_string(&app, 80, 14));
}
