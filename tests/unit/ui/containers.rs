use crate::support::*;
use docker_tui::{app::App, docker::models::ContainerState, msg::Msg};

fn fixture() -> App {
    let mut app = App::new();
    let mut web = container("0123456789abcdef", "web", ContainerState::Running);
    web.image = "nginx:latest".into();
    web.ports = "0.0.0.0:8080->80/tcp".into();
    app.update(Msg::ContainersLoaded(Ok(vec![
        web,
        container("fedcba9876543210", "db", ContainerState::Exited),
    ])));
    app
}

#[test]
fn renders_container_table() {
    insta::assert_snapshot!(render_to_string(&fixture(), 110, 8));
}

#[test]
fn renders_running_only_filter() {
    let mut app = fixture();
    app.containers.show_all = false;
    insta::assert_snapshot!(render_to_string(&app, 110, 7));
}

#[test]
fn renders_error_when_empty() {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Err(
        "permission denied while connecting".into(),
    )));
    insta::assert_snapshot!(render_to_string(&app, 60, 7));
}
