use crate::support::*;
use crossterm::event::KeyCode;
use docker_tui::{
    app::{App, detail::Loadable},
    docker::exec::ExecOutput,
    msg::Command,
};

use docker_tui::{app::Tab, docker::models::ContainerState, msg::Msg};

fn app() -> App {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![
        container("c1", "web", ContainerState::Running),
        container("c2", "db", ContainerState::Exited),
    ])));
    app
}

#[test]
fn x_detects_shell_of_selected_container() {
    let mut app = app();
    assert_eq!(
        app.update(key(KeyCode::Char('x'))),
        vec![Command::DetectShell("c1".into())]
    );
}

#[test]
fn x_works_from_detail_view() {
    let mut app = app();
    app.update(key(KeyCode::Enter));
    assert_eq!(
        app.update(key(KeyCode::Char('x'))),
        vec![Command::DetectShell("c1".into())]
    );
}

#[test]
fn x_on_stopped_container_explains() {
    let mut app = app();
    app.update(key(KeyCode::Char('j')));
    assert!(app.update(key(KeyCode::Char('x'))).is_empty());
    assert!(app.toast.as_ref().unwrap().error);
}

#[test]
fn detected_shell_is_run() {
    let mut app = app();
    let cmds = app.update(Msg::ShellDetected {
        id: "c1".into(),
        result: Ok("/bin/bash".into()),
    });
    assert_eq!(
        cmds,
        vec![Command::RunShell {
            id: "c1".into(),
            shell: "/bin/bash".into()
        }]
    );
}

#[test]
fn shell_detection_failure_toasts() {
    let mut app = app();
    assert!(
        app.update(Msg::ShellDetected {
            id: "c1".into(),
            result: Err("no shell".into())
        })
        .is_empty()
    );
    assert!(app.toast.as_ref().unwrap().text.contains("no shell"));
}

#[test]
fn shell_exit_error_toasts_and_refreshes() {
    let mut app = app();
    let cmds = app.update(Msg::ShellExited(Err("docker CLI not found".into())));
    assert_eq!(cmds, vec![Command::LoadContainers]);
    assert!(app.toast.as_ref().unwrap().error);
}

#[test]
fn colon_prompt_runs_command() {
    let mut app = app();
    app.update(key(KeyCode::Char(':')));
    assert_eq!(app.prompt.as_ref().unwrap().container_name, "web");
    for c in "ls q".chars() {
        app.update(key(KeyCode::Char(c)));
    }
    assert!(!app.should_quit, "typing goes to the prompt");
    let cmds = app.update(key(KeyCode::Enter));
    assert_eq!(
        cmds,
        vec![Command::Exec {
            id: "c1".into(),
            command: "ls q".into()
        }]
    );
    assert!(app.prompt.is_none());
    assert_eq!(app.exec_view.as_ref().unwrap().result, Loadable::Loading);
}

#[test]
fn empty_prompt_does_nothing_and_esc_cancels() {
    let mut app = app();
    app.update(key(KeyCode::Char(':')));
    assert!(app.update(key(KeyCode::Enter)).is_empty());
    app.update(key(KeyCode::Esc));
    assert!(app.prompt.is_none());
    assert!(app.exec_view.is_none());
}

#[test]
fn exec_output_is_shown_and_closable() {
    let mut app = app();
    app.update(key(KeyCode::Char(':')));
    app.update(key(KeyCode::Char('w')));
    app.update(key(KeyCode::Enter));
    app.update(Msg::ExecDone {
        command: "w".into(),
        result: Ok(ExecOutput {
            output: "hi".into(),
            exit_code: Some(0),
        }),
    });
    assert!(matches!(
        app.exec_view.as_ref().unwrap().result,
        Loadable::Loaded(_)
    ));
    app.update(key(KeyCode::Char('1')));
    assert_eq!(app.tab, Tab::Containers, "popup is modal");
    app.update(key(KeyCode::Esc));
    assert!(app.exec_view.is_none());
}

#[test]
fn prompt_not_available_on_other_tabs() {
    let mut app = app();
    app.tab = Tab::Images;
    app.update(key(KeyCode::Char(':')));
    assert!(app.prompt.is_none());
}
