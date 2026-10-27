use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexMap;

use super::App;
use crate::{
    docker::pull::{PullEvent, PullState},
    msg::Command,
};

#[derive(Debug, Default)]
pub struct PullsState {
    /// Pulls by image reference, in start order.
    pub items: IndexMap<String, PullState>,
    pub visible: bool,
}

impl PullsState {
    pub fn active(&self) -> usize {
        self.items.values().filter(|p| !p.done).count()
    }
}

impl App {
    /// Begin pulling `image` unless it is already being pulled.
    pub(super) fn start_pull(&mut self, image: String) -> Vec<Command> {
        if self.pulls.items.get(&image).is_some_and(|p| !p.done) {
            self.toast(format!("{image} is already being pulled"), false);
            return vec![];
        }
        // Re-pulling moves the entry to the end.
        self.pulls.items.shift_remove(&image);
        self.pulls.items.insert(image.clone(), PullState::default());
        self.pulls.visible = true;
        self.toast(format!("Pulling {image}"), false);
        vec![Command::Pull(image)]
    }

    pub(super) fn on_pull_progress(&mut self, image: String, event: PullEvent) {
        if let Some(pull) = self.pulls.items.get_mut(&image) {
            pull.apply(event);
        }
    }

    pub(super) fn on_pull_done(
        &mut self,
        image: String,
        result: Result<(), String>,
    ) -> Vec<Command> {
        match &result {
            Ok(()) => self.toast(format!("Pulled {image}"), false),
            Err(e) => self.toast(format!("Pulling {image} failed: {e}"), true),
        }
        if let Some(pull) = self.pulls.items.get_mut(&image) {
            pull.finish(result);
        }
        self.images.refresh()
    }

    pub(super) fn on_pulls_key(&mut self, key: KeyEvent) -> Vec<Command> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('P') | KeyCode::Char('q') => self.pulls.visible = false,
            KeyCode::Char('c') => self.pulls.items.retain(|_, p| !p.done),
            _ => {}
        }
        vec![]
    }
}
