use crossterm::event::{Event, KeyCode, KeyEvent};
use tui_input::{Input, backend::crossterm::EventHandler};

use super::{App, detail::Loadable};
use crate::{
    docker::{exec::ExecOutput, models::Container},
    msg::Command,
};

/// The `:` prompt for running a one-off command in a container.
#[derive(Debug)]
pub struct PromptState {
    pub input: Input,
    pub container_id: String,
    pub container_name: String,
}

/// Output popup of a one-off command.
#[derive(Debug)]
pub struct ExecView {
    pub container_name: String,
    pub command: String,
    pub result: Loadable<ExecOutput>,
    pub scroll: u16,
}

impl App {
    /// The container that `x` and `:` act on: the open detail view's, or the
    /// selected one in the list.
    pub(super) fn target_container(&self) -> Option<&Container> {
        match &self.detail {
            Some(d) => Some(&d.container),
            None => self.containers.selected(),
        }
    }

    /// The target container if it is running; toasts why not otherwise.
    pub(super) fn running_target(&mut self) -> Option<Container> {
        let c = self.target_container()?.clone();
        if !c.state.is_running() {
            self.toast(format!("{} is not running", c.name), true);
            return None;
        }
        Some(c)
    }

    pub(super) fn open_shell(&mut self) -> Vec<Command> {
        match self.running_target() {
            Some(c) => vec![Command::DetectShell(c.id)],
            None => vec![],
        }
    }

    pub(super) fn open_prompt(&mut self) -> Vec<Command> {
        if let Some(c) = self.running_target() {
            self.prompt = Some(PromptState {
                input: Input::default(),
                container_id: c.id,
                container_name: c.name,
            });
        }
        vec![]
    }

    pub(super) fn on_prompt_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let Some(prompt) = self.prompt.as_mut() else {
            return vec![];
        };
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => {
                let command = prompt.input.value().trim().to_string();
                if command.is_empty() {
                    return vec![];
                }
                let prompt = self.prompt.take().expect("prompt is open");
                self.exec_view = Some(ExecView {
                    container_name: prompt.container_name,
                    command: command.clone(),
                    result: Loadable::Loading,
                    scroll: 0,
                });
                return vec![Command::Exec {
                    id: prompt.container_id,
                    command,
                }];
            }
            _ => {
                prompt.input.handle_event(&Event::Key(key));
            }
        }
        vec![]
    }

    pub(super) fn on_exec_view_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let Some(view) = self.exec_view.as_mut() else {
            return vec![];
        };
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => self.exec_view = None,
            KeyCode::Down | KeyCode::Char('j') => view.scroll = view.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => view.scroll = view.scroll.saturating_sub(1),
            KeyCode::PageDown => view.scroll = view.scroll.saturating_add(20),
            KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(20),
            // Run another command in the same container.
            KeyCode::Char(':') => {
                self.exec_view = None;
                return self.open_prompt();
            }
            _ => {}
        }
        vec![]
    }

    pub(super) fn on_shell_detected(
        &mut self,
        id: String,
        result: Result<String, String>,
    ) -> Vec<Command> {
        let embedded = self.pending_terminal.take().is_some_and(|p| p == id);
        match result {
            Ok(shell) if embedded => self.start_terminal(id, shell),
            Ok(shell) => vec![Command::RunShell { id, shell }],
            Err(e) => {
                self.toast(format!("Cannot open shell: {e}"), true);
                vec![]
            }
        }
    }

    pub(super) fn on_exec_done(&mut self, command: String, result: Result<ExecOutput, String>) {
        if let Some(view) = self.exec_view.as_mut().filter(|v| v.command == command) {
            view.result = result.into();
        }
    }
}
