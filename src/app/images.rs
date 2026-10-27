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
