mod containers;
mod detail;
mod files;
mod hub;
mod images;
mod pulls;

use crate::support::*;
use docker_tui::app::{App, Tab};

#[test]
fn renders_tab_bar_with_selected_tab() {
    let mut app = App::new();
    app.tab = Tab::Images;
    insta::assert_snapshot!(render_to_string(&app, 60, 8));
}

#[test]
fn renders_confirm_dialog() {
    let mut app = App::new();
    app.confirm = Some(docker_tui::app::confirm::Confirm {
        message: "Remove container web?".into(),
        command: docker_tui::msg::Command::LoadImages,
    });
    insta::assert_snapshot!(render_to_string(&app, 60, 10));
}

#[test]
fn renders_toast_in_status_bar() {
    let mut app = App::new();
    app.toast("Stopped web", false);
    insta::assert_snapshot!(render_to_string(&app, 60, 5));
}

#[test]
fn renders_command_prompt() {
    use crossterm::event::KeyCode;

    use docker_tui::{docker::models::ContainerState, msg::Msg};

    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    app.update(key(KeyCode::Char(':')));
    for c in "ls /etc".chars() {
        app.update(key(KeyCode::Char(c)));
    }
    insta::assert_snapshot!(render_to_string(&app, 60, 6));
}

#[test]
fn renders_exec_output() {
    use crossterm::event::KeyCode;

    use docker_tui::{
        docker::{exec::ExecOutput, models::ContainerState},
        msg::Msg,
    };

    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    app.update(key(KeyCode::Char(':')));
    app.update(key(KeyCode::Char('w')));
    app.update(key(KeyCode::Enter));
    app.update(Msg::ExecDone {
        command: "w".into(),
        result: Ok(ExecOutput {
            output: "\x1b[1mhello\x1b[0m\nworld\n".into(),
            exit_code: Some(0),
        }),
    });
    insta::assert_snapshot!(render_to_string(&app, 60, 10));
}

#[test]
fn renders_terminal_pane() {
    use crossterm::event::KeyCode;

    use docker_tui::{docker::models::ContainerState, msg::Msg};

    let mut app = App::new();
    app.update(Msg::Resize(80, 10));
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    app.update(key(KeyCode::Char('X')));
    app.update(Msg::ShellDetected {
        id: "c1".into(),
        result: Ok("/bin/sh".into()),
    });
    app.update(Msg::TerminalOutput(b"/ # ls\r\nbin  etc\r\n/ # ".to_vec()));
    insta::assert_snapshot!(render_to_string(&app, 80, 10));
}

#[test]
fn renders_help_popup() {
    let mut app = App::new();
    app.show_help = true;
    insta::assert_snapshot!(render_to_string(&app, 80, 30));
}
