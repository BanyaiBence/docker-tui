use crate::support::*;
use crossterm::event::KeyCode;

use docker_tui::{
    app::{App, Tab},
    msg::{Command, Msg},
};

fn app_with_images() -> App {
    let mut app = App::new();
    app.tab = Tab::Images;
    app.update(Msg::ImagesLoaded(Ok(vec![
        image("i1", "alpine:3.20"),
        image("i2", "nginx:latest"),
    ])));
    app
}

#[test]
fn loaded_images_are_navigable() {
    let mut app = app_with_images();
    assert_eq!(app.images.selected().unwrap().display_name(), "alpine:3.20");
    app.update(key(KeyCode::Char('j')));
    assert_eq!(
        app.images.selected().unwrap().display_name(),
        "nginx:latest"
    );
}

#[test]
fn r_refreshes_images() {
    let mut app = app_with_images();
    assert_eq!(
        app.update(key(KeyCode::Char('r'))),
        vec![Command::LoadImages]
    );
    assert!(app.images.loading);
}

#[test]
fn d_asks_before_removing_image() {
    let mut app = app_with_images();
    app.update(key(KeyCode::Char('j')));
    assert!(app.update(key(KeyCode::Char('d'))).is_empty());
    let confirm = app.confirm.clone().unwrap();
    assert!(confirm.message.contains("nginx:latest"));
    assert_eq!(
        confirm.command,
        Command::RemoveImage {
            id: "i2".into(),
            name: "nginx:latest".into()
        }
    );
}

#[test]
fn load_error_is_kept() {
    let mut app = app_with_images();
    app.update(Msg::ImagesLoaded(Err("nope".into())));
    assert_eq!(app.images.error.as_deref(), Some("nope"));
    assert_eq!(app.images.items.len(), 2);
}
