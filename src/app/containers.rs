use crossterm::event::{KeyCode, KeyEvent};

use super::{App, confirm::Confirm, list::Selection};
use crate::{
    docker::models::{Container, ContainerAction},
    msg::Command,
};

#[derive(Debug)]
pub struct ContainersState {
    pub items: Vec<Container>,
    pub selection: Selection,
    /// Show stopped containers too.
    pub show_all: bool,
    pub loading: bool,
    pub error: Option<String>,
}

impl Default for ContainersState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selection: Selection::default(),
            show_all: true,
            loading: false,
            error: None,
        }
    }
}

impl ContainersState {
    pub fn visible(&self) -> Vec<&Container> {
        self.items
            .iter()
            .filter(|c| self.show_all || c.state.is_running())
            .collect()
    }

    pub fn selected(&self) -> Option<&Container> {
        self.visible().get(self.selection.index).copied()
    }

    pub fn set_items(&mut self, result: Result<Vec<Container>, String>) {
        self.loading = false;
        match result {
            Ok(items) => {
                self.items = items;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        self.selection.clamp(self.visible().len());
    }

    pub fn refresh(&mut self) -> Vec<Command> {
        self.loading = true;
        vec![Command::LoadContainers]
    }
}

impl App {
    pub(super) fn on_containers_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let state = &mut self.containers;
        if state.selection.on_key(key.code, state.visible().len()) {
            return vec![];
        }
        match key.code {
            KeyCode::Char('a') => {
                state.show_all = !state.show_all;
                state.selection.clamp(state.visible().len());
                vec![]
            }
            KeyCode::Char('r') => state.refresh(),
            KeyCode::Enter => match state.selected().cloned() {
                Some(c) => self.open_detail(c),
                None => vec![],
            },
            KeyCode::Char('s') => self.with_selected(|c| {
                let action = if c.state.is_running() {
                    ContainerAction::Stop
                } else {
                    ContainerAction::Start
                };
                action_command(c, action)
            }),
            KeyCode::Char('R') => {
                self.with_selected(|c| action_command(c, ContainerAction::Restart))
            }
            KeyCode::Char('x') => self.open_shell(),
            KeyCode::Char('X') => self.open_terminal(),
            KeyCode::Char(':') => self.open_prompt(),
            KeyCode::Char('d') => {
                if let Some(c) = self.containers.selected() {
                    let warning = if c.state.is_running() {
                        " It is running and will be killed."
                    } else {
                        ""
                    };
                    self.confirm = Some(Confirm {
                        message: format!("Remove container {}?{warning}", c.name),
                        command: action_command(c, ContainerAction::Remove),
                    });
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn with_selected(&self, f: impl FnOnce(&Container) -> Command) -> Vec<Command> {
        self.containers.selected().map(f).into_iter().collect()
    }
}

fn action_command(c: &Container, action: ContainerAction) -> Command {
    Command::Container {
        id: c.id.clone(),
        name: c.name.clone(),
        action,
    }
}
