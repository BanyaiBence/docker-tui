use crate::support::*;
use crossterm::event::KeyCode;
use docker_tui::{app::App, msg::Command};

use docker_tui::app::confirm::*;
use docker_tui::msg::Msg;

fn app_with_confirm() -> App {
    let mut app = App::new();
    app.confirm = Some(Confirm {
        message: "Sure?".into(),
        command: Command::LoadImages,
    });
    app
}

#[test]
fn y_runs_command() {
    let mut app = app_with_confirm();
    assert_eq!(
        app.update(key(KeyCode::Char('y'))),
        vec![Command::LoadImages]
    );
    assert!(app.confirm.is_none());
}

#[test]
fn enter_runs_command() {
    let mut app = app_with_confirm();
    assert_eq!(app.update(key(KeyCode::Enter)), vec![Command::LoadImages]);
}

#[test]
fn n_and_esc_cancel() {
    for code in [KeyCode::Char('n'), KeyCode::Esc] {
        let mut app = app_with_confirm();
        assert!(app.update(key(code)).is_empty());
        assert!(app.confirm.is_none());
    }
}

#[test]
fn other_keys_are_swallowed() {
    let mut app = app_with_confirm();
    app.update(key(KeyCode::Char('q')));
    assert!(!app.should_quit);
    assert!(app.confirm.is_some());
}

#[test]
fn toast_expires_after_ticks() {
    let mut app = App::new();
    app.toast("hello", false);
    for _ in 0..TOAST_TICKS - 1 {
        app.update(Msg::Tick);
    }
    assert!(app.toast.is_some());
    app.update(Msg::Tick);
    assert!(app.toast.is_none());
}
