use crossterm::event::{KeyCode, KeyEvent};

use super::{App, confirm::Confirm, list::Selection};
use crate::{docker::models::Image, msg::Command};

#[derive(Debug, Default)]
pub struct ImagesState {
    pub items: Vec<Image>,
    pub selection: Selection,
    pub loading: bool,
    pub error: Option<String>,
}

impl ImagesState {
    pub fn selected(&self) -> Option<&Image> {
        self.items.get(self.selection.index)
    }

    pub fn set_items(&mut self, result: Result<Vec<Image>, String>) {
        self.loading = false;
        match result {
            Ok(items) => {
                self.items = items;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        self.selection.clamp(self.items.len());
    }

    pub fn refresh(&mut self) -> Vec<Command> {
        self.loading = true;
        vec![Command::LoadImages]
    }
}

impl App {
    pub(super) fn on_images_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let state = &mut self.images;
        if state.selection.on_key(key.code, state.items.len()) {
            return vec![];
        }
        match key.code {
            KeyCode::Char('r') => state.refresh(),
            KeyCode::Char('d') => {
                if let Some(i) = state.selected() {
                    let name = i.display_name().to_string();
                    self.confirm = Some(Confirm {
                        message: format!("Remove image {name}?"),
                        command: Command::RemoveImage {
                            id: i.id.clone(),
                            name,
                        },
                    });
                }
                vec![]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use crate::{
        app::{App, Tab, key},
        msg::{Command, Msg},
        runtime::mock::image,
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
}
