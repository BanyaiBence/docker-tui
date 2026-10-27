use crate::support::*;
use crossterm::event::KeyCode;

use docker_tui::{
    app::{App, Tab},
    docker::models::{ContainerAction, ContainerState},
    msg::{Command, Msg},
};

fn app_with_containers() -> App {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![
        container("c1", "web", ContainerState::Running),
        container("c2", "db", ContainerState::Exited),
        container("c3", "cache", ContainerState::Running),
    ])));
    app
}

#[test]
fn loaded_containers_are_stored() {
    let app = app_with_containers();
    assert_eq!(app.containers.items.len(), 3);
    assert!(!app.containers.loading);
    assert_eq!(app.containers.selected().unwrap().name, "web");
}

#[test]
fn load_error_is_kept_and_items_preserved() {
    let mut app = app_with_containers();
    app.update(Msg::ContainersLoaded(Err("permission denied".into())));
    assert_eq!(app.containers.error.as_deref(), Some("permission denied"));
    assert_eq!(app.containers.items.len(), 3);
}

#[test]
fn successful_load_clears_error() {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Err("x".into())));
    app.update(Msg::ContainersLoaded(Ok(vec![])));
    assert!(app.containers.error.is_none());
}

#[test]
fn j_k_move_selection() {
    let mut app = app_with_containers();
    app.update(key(KeyCode::Char('j')));
    assert_eq!(app.containers.selected().unwrap().name, "db");
    app.update(key(KeyCode::Down));
    app.update(key(KeyCode::Down));
    assert_eq!(app.containers.selected().unwrap().name, "cache");
    app.update(key(KeyCode::Char('k')));
    assert_eq!(app.containers.selected().unwrap().name, "db");
    app.update(key(KeyCode::Char('g')));
    assert_eq!(app.containers.selected().unwrap().name, "web");
    app.update(key(KeyCode::Char('G')));
    assert_eq!(app.containers.selected().unwrap().name, "cache");
}

#[test]
fn a_toggles_stopped_containers() {
    let mut app = app_with_containers();
    app.update(key(KeyCode::Char('G')));
    app.update(key(KeyCode::Char('a')));
    assert!(!app.containers.show_all);
    let names: Vec<_> = app
        .containers
        .visible()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["web", "cache"]);
    assert_eq!(app.containers.selected().unwrap().name, "cache");
}

#[test]
fn selection_is_clamped_when_list_shrinks() {
    let mut app = app_with_containers();
    app.update(key(KeyCode::Char('G')));
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    assert_eq!(app.containers.selected().unwrap().name, "web");
}

#[test]
fn r_refreshes() {
    let mut app = app_with_containers();
    assert_eq!(
        app.update(key(KeyCode::Char('r'))),
        vec![Command::LoadContainers]
    );
    assert!(app.containers.loading);
}

#[test]
fn enter_opens_detail_and_inspects() {
    let mut app = app_with_containers();
    app.update(key(KeyCode::Char('j')));
    let cmds = app.update(key(KeyCode::Enter));
    assert_eq!(cmds[0], Command::InspectContainer("c2".into()));
    assert_eq!(app.detail.as_ref().unwrap().container.name, "db");
}

fn action(id: &str, name: &str, action: ContainerAction) -> Command {
    Command::Container {
        id: id.into(),
        name: name.into(),
        action,
    }
}

#[test]
fn s_stops_running_and_starts_stopped() {
    let mut app = app_with_containers();
    assert_eq!(
        app.update(key(KeyCode::Char('s'))),
        vec![action("c1", "web", ContainerAction::Stop)]
    );
    app.update(key(KeyCode::Char('j')));
    assert_eq!(
        app.update(key(KeyCode::Char('s'))),
        vec![action("c2", "db", ContainerAction::Start)]
    );
}

#[test]
fn shift_r_restarts() {
    let mut app = app_with_containers();
    assert_eq!(
        app.update(key(KeyCode::Char('R'))),
        vec![action("c1", "web", ContainerAction::Restart)]
    );
}

#[test]
fn d_asks_before_removing() {
    let mut app = app_with_containers();
    assert!(app.update(key(KeyCode::Char('d'))).is_empty());
    let confirm = app.confirm.clone().unwrap();
    assert!(confirm.message.contains("web"));
    assert_eq!(
        confirm.command,
        action("c1", "web", ContainerAction::Remove)
    );
    assert_eq!(
        app.update(key(KeyCode::Char('y'))),
        vec![action("c1", "web", ContainerAction::Remove)]
    );
}

#[test]
fn finished_action_toasts_and_refreshes() {
    let mut app = app_with_containers();
    let cmds = app.update(Msg::ActionDone {
        label: "Stopped web".into(),
        result: Ok(()),
    });
    assert_eq!(cmds, vec![Command::LoadContainers, Command::LoadImages]);
    let toast = app.toast.clone().unwrap();
    assert_eq!((toast.text.as_str(), toast.error), ("Stopped web", false));
}

#[test]
fn failed_action_shows_error_toast() {
    let mut app = app_with_containers();
    app.update(Msg::ActionDone {
        label: "Stopped web".into(),
        result: Err("no such container".into()),
    });
    let toast = app.toast.clone().unwrap();
    assert!(toast.error);
    assert!(toast.text.contains("no such container"));
}

#[test]
fn enter_on_empty_list_does_nothing() {
    let mut app = App::new();
    assert!(app.update(key(KeyCode::Enter)).is_empty());
    assert!(app.detail.is_none());
}

#[test]
fn container_keys_only_apply_on_containers_tab() {
    let mut app = app_with_containers();
    app.tab = Tab::Images;
    app.update(key(KeyCode::Char('j')));
    assert_eq!(app.containers.selection.index, 0);
}
