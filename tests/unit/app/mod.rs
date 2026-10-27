use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use docker_tui::msg::{Command, Msg};

use crate::support::key;
mod confirm;
mod containers;
mod detail;
mod exec;
mod files;
mod hub;
mod images;
mod list;
mod logs;
mod pulls;
mod terminal;

use docker_tui::app::*;

#[test]
fn starts_on_containers_tab() {
    assert_eq!(App::new().tab, Tab::Containers);
}

#[test]
fn init_loads_lists() {
    let mut app = App::new();
    assert_eq!(
        app.init(),
        vec![Command::LoadContainers, Command::LoadImages]
    );
    assert!(app.containers.loading && app.images.loading);
}

#[test]
fn periodic_refresh_on_ticks() {
    let mut app = App::new();
    let mut cmds = vec![];
    for _ in 0..REFRESH_TICKS {
        cmds = app.update(Msg::Tick);
    }
    assert_eq!(cmds, vec![Command::LoadContainers, Command::LoadImages]);
}

#[test]
fn refresh_interval_is_configurable() {
    let mut app = App::new();
    app.refresh_ticks = 2;
    assert!(app.update(Msg::Tick).is_empty());
    assert_eq!(
        app.update(Msg::Tick),
        vec![Command::LoadContainers, Command::LoadImages]
    );
}

#[test]
fn periodic_refresh_skips_lists_still_loading() {
    let mut app = App::new();
    app.containers.loading = true;
    let mut cmds = vec![];
    for _ in 0..REFRESH_TICKS {
        cmds = app.update(Msg::Tick);
    }
    assert_eq!(cmds, vec![Command::LoadImages]);
}

#[test]
fn q_quits() {
    let mut app = App::new();
    app.update(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn ctrl_c_quits() {
    let mut app = App::new();
    app.update(Msg::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.should_quit);
}

#[test]
fn number_keys_select_tabs() {
    let mut app = App::new();
    app.update(key(KeyCode::Char('1')));
    assert_eq!(app.tab, Tab::Hub);
    app.update(key(KeyCode::Char('2')));
    assert_eq!(app.tab, Tab::Images);
    app.update(key(KeyCode::Char('3')));
    assert_eq!(app.tab, Tab::Containers);
}

#[test]
fn tab_and_backtab_cycle() {
    let mut app = App::new();
    app.update(key(KeyCode::Tab));
    assert_eq!(app.tab, Tab::Hub);
    app.update(key(KeyCode::BackTab));
    assert_eq!(app.tab, Tab::Containers);
    app.update(key(KeyCode::BackTab));
    assert_eq!(app.tab, Tab::Images);
}

#[test]
fn question_mark_toggles_help_and_esc_closes_it() {
    let mut app = App::new();
    app.update(key(KeyCode::Char('?')));
    assert!(app.show_help);
    app.update(key(KeyCode::Esc));
    assert!(!app.show_help);
}

#[test]
fn help_swallows_other_keys() {
    let mut app = App::new();
    app.update(key(KeyCode::Char('?')));
    app.update(key(KeyCode::Char('1')));
    assert_eq!(app.tab, Tab::Containers);
    // q still closes help rather than quitting
    app.update(key(KeyCode::Char('q')));
    assert!(!app.show_help);
    assert!(!app.should_quit);
}
