use crate::support::*;
use crossterm::event::KeyCode;

use docker_tui::app::detail::{DetailTab, Loadable, STATS_HISTORY};
use docker_tui::{
    app::App,
    docker::{models::ContainerState, stats::StatsSample},
    msg::{Command, Msg},
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
fn detail_starts_loading_inspect() {
    let app = app_with_detail();
    assert_eq!(app.detail.as_ref().unwrap().inspect, Loadable::Loading);
}

#[test]
fn inspect_result_is_stored() {
    let mut app = app_with_detail();
    app.update(Msg::Inspected {
        id: "c1".into(),
        result: Ok("{}".into()),
    });
    assert_eq!(
        app.detail.as_ref().unwrap().inspect,
        Loadable::Loaded("{}".into())
    );
}

#[test]
fn inspect_result_for_other_container_is_ignored() {
    let mut app = app_with_detail();
    app.update(Msg::Inspected {
        id: "zzz".into(),
        result: Ok("{}".into()),
    });
    assert_eq!(app.detail.as_ref().unwrap().inspect, Loadable::Loading);
}

#[test]
fn opening_running_container_streams_logs_and_stats() {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    assert_eq!(
        app.update(key(KeyCode::Enter)),
        vec![
            Command::InspectContainer("c1".into()),
            Command::StreamLogs("c1".into()),
            Command::StreamStats("c1".into()),
        ]
    );
}

#[test]
fn opening_stopped_container_skips_stats() {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Exited,
    )])));
    assert_eq!(
        app.update(key(KeyCode::Enter)),
        vec![
            Command::InspectContainer("c1".into()),
            Command::StreamLogs("c1".into())
        ]
    );
}

#[test]
fn esc_closes_detail_and_stops_streams() {
    let mut app = app_with_detail();
    assert_eq!(app.update(key(KeyCode::Esc)), vec![Command::StopStreams]);
    assert!(app.detail.is_none());
}

#[test]
fn h_l_switch_detail_tabs() {
    let mut app = app_with_detail();
    let tab = |app: &App| app.detail.as_ref().unwrap().tab;
    assert_eq!(tab(&app), DetailTab::Info);
    app.update(key(KeyCode::Char('l')));
    assert_eq!(tab(&app), DetailTab::Logs);
    app.update(key(KeyCode::Right));
    assert_eq!(tab(&app), DetailTab::Stats);
    app.update(key(KeyCode::Char('l')));
    assert_eq!(tab(&app), DetailTab::Files);
    app.update(key(KeyCode::Char('l')));
    assert_eq!(tab(&app), DetailTab::Info);
    app.update(key(KeyCode::Char('h')));
    assert_eq!(tab(&app), DetailTab::Files);
}

#[test]
fn entering_files_tab_lists_root() {
    let mut app = app_with_detail();
    assert_eq!(
        app.update(key(KeyCode::Left)),
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/".into()
        }]
    );
}

#[test]
fn files_tab_routes_keys_and_results() {
    use docker_tui::docker::fs::FileKind;
    let mut app = app_with_detail();
    app.update(key(KeyCode::Left));
    app.update(Msg::DirListed {
        id: "c1".into(),
        path: "/".into(),
        result: Ok(listing(vec![entry("etc", FileKind::Dir)])),
    });
    assert_eq!(
        app.update(key(KeyCode::Enter)),
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/etc".into()
        }]
    );
    // Backspace goes up a directory instead of closing the view
    app.update(key(KeyCode::Backspace));
    assert!(app.detail.is_some());
    // Esc without overlay still closes the detail view
    assert_eq!(app.update(key(KeyCode::Esc)), vec![Command::StopStreams]);
}

#[test]
fn log_chunks_for_open_container_are_buffered() {
    let mut app = app_with_detail();
    app.update(Msg::LogChunk {
        id: "c1".into(),
        text: "hello\n".into(),
    });
    app.update(Msg::LogChunk {
        id: "other".into(),
        text: "nope\n".into(),
    });
    assert_eq!(app.detail.as_ref().unwrap().logs.lines, ["hello"]);
}

#[test]
fn log_stream_error_is_shown_inline() {
    let mut app = app_with_detail();
    app.update(Msg::LogsEnded {
        id: "c1".into(),
        error: Some("gone".into()),
    });
    let logs = &app.detail.as_ref().unwrap().logs;
    assert!(logs.lines.last().unwrap().contains("gone"));
}

#[test]
fn logs_tab_scrolls_and_follows() {
    let mut app = app_with_detail();
    app.update(key(KeyCode::Char('l')));
    app.update(Msg::LogChunk {
        id: "c1".into(),
        text: "a\nb\nc\n".into(),
    });
    app.update(key(KeyCode::Char('k')));
    assert_eq!(app.detail.as_ref().unwrap().logs.scroll_from_bottom, 1);
    app.update(key(KeyCode::Char('G')));
    assert!(app.detail.as_ref().unwrap().logs.following());
}

#[test]
fn stats_samples_are_kept_with_bounded_history() {
    let mut app = app_with_detail();
    for i in 0..STATS_HISTORY + 5 {
        let sample = StatsSample {
            cpu_percent: i as f64,
            ..Default::default()
        };
        app.update(Msg::Stats {
            id: "c1".into(),
            sample,
        });
    }
    let stats = &app.detail.as_ref().unwrap().stats;
    assert_eq!(stats.len(), STATS_HISTORY);
    assert_eq!(
        stats.back().unwrap().cpu_percent,
        (STATS_HISTORY + 4) as f64
    );
}

#[test]
fn j_k_scroll_detail() {
    let mut app = app_with_detail();
    app.update(key(KeyCode::Char('j')));
    app.update(key(KeyCode::Char('j')));
    assert_eq!(app.detail.as_ref().unwrap().scroll, 2);
    app.update(key(KeyCode::Char('k')));
    app.update(key(KeyCode::Char('k')));
    app.update(key(KeyCode::Char('k')));
    assert_eq!(app.detail.as_ref().unwrap().scroll, 0);
}

#[test]
fn refreshed_list_updates_open_container() {
    let mut app = app_with_detail();
    let mut updated = container("c1", "web", ContainerState::Exited);
    updated.status = "Exited (0) 1 second ago".into();
    app.update(Msg::ContainersLoaded(Ok(vec![updated])));
    assert_eq!(
        app.detail.as_ref().unwrap().container.state,
        ContainerState::Exited
    );
    assert_eq!(
        app.detail.as_ref().unwrap().container.status,
        "Exited (0) 1 second ago"
    );
}

#[test]
fn detail_does_not_swallow_quit() {
    let mut app = app_with_detail();
    app.update(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}
