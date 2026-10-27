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

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{
        app::{Tab, key},
        docker::models::ContainerState,
        msg::Msg,
        runtime::mock::container,
    };

    fn app() -> App {
        let mut app = App::new();
        app.update(Msg::ContainersLoaded(Ok(vec![
            container("c1", "web", ContainerState::Running),
            container("c2", "db", ContainerState::Exited),
        ])));
        app
    }

    #[test]
    fn x_detects_shell_of_selected_container() {
        let mut app = app();
        assert_eq!(
            app.update(key(KeyCode::Char('x'))),
            vec![Command::DetectShell("c1".into())]
        );
    }

    #[test]
    fn x_works_from_detail_view() {
        let mut app = app();
        app.update(key(KeyCode::Enter));
        assert_eq!(
            app.update(key(KeyCode::Char('x'))),
            vec![Command::DetectShell("c1".into())]
        );
    }

    #[test]
    fn x_on_stopped_container_explains() {
        let mut app = app();
        app.update(key(KeyCode::Char('j')));
        assert!(app.update(key(KeyCode::Char('x'))).is_empty());
        assert!(app.toast.as_ref().unwrap().error);
    }

    #[test]
    fn detected_shell_is_run() {
        let mut app = app();
        let cmds = app.update(Msg::ShellDetected {
            id: "c1".into(),
            result: Ok("/bin/bash".into()),
        });
        assert_eq!(
            cmds,
            vec![Command::RunShell {
                id: "c1".into(),
                shell: "/bin/bash".into()
            }]
        );
    }

    #[test]
    fn shell_detection_failure_toasts() {
        let mut app = app();
        assert!(
            app.update(Msg::ShellDetected {
                id: "c1".into(),
                result: Err("no shell".into())
            })
            .is_empty()
        );
        assert!(app.toast.as_ref().unwrap().text.contains("no shell"));
    }

    #[test]
    fn shell_exit_error_toasts_and_refreshes() {
        let mut app = app();
        let cmds = app.update(Msg::ShellExited(Err("docker CLI not found".into())));
        assert_eq!(cmds, vec![Command::LoadContainers]);
        assert!(app.toast.as_ref().unwrap().error);
    }

    #[test]
    fn colon_prompt_runs_command() {
        let mut app = app();
        app.update(key(KeyCode::Char(':')));
        assert_eq!(app.prompt.as_ref().unwrap().container_name, "web");
        for c in "ls q".chars() {
            app.update(key(KeyCode::Char(c)));
        }
        assert!(!app.should_quit, "typing goes to the prompt");
        let cmds = app.update(key(KeyCode::Enter));
        assert_eq!(
            cmds,
            vec![Command::Exec {
                id: "c1".into(),
                command: "ls q".into()
            }]
        );
        assert!(app.prompt.is_none());
        assert_eq!(app.exec_view.as_ref().unwrap().result, Loadable::Loading);
    }

    #[test]
    fn empty_prompt_does_nothing_and_esc_cancels() {
        let mut app = app();
        app.update(key(KeyCode::Char(':')));
        assert!(app.update(key(KeyCode::Enter)).is_empty());
        app.update(key(KeyCode::Esc));
        assert!(app.prompt.is_none());
        assert!(app.exec_view.is_none());
    }

    #[test]
    fn exec_output_is_shown_and_closable() {
        let mut app = app();
        app.update(key(KeyCode::Char(':')));
        app.update(key(KeyCode::Char('w')));
        app.update(key(KeyCode::Enter));
        app.update(Msg::ExecDone {
            command: "w".into(),
            result: Ok(ExecOutput {
                output: "hi".into(),
                exit_code: Some(0),
            }),
        });
        assert!(matches!(
            app.exec_view.as_ref().unwrap().result,
            Loadable::Loaded(_)
        ));
        app.update(key(KeyCode::Char('1')));
        assert_eq!(app.tab, Tab::Containers, "popup is modal");
        app.update(key(KeyCode::Esc));
        assert!(app.exec_view.is_none());
    }

    #[test]
    fn prompt_not_available_on_other_tabs() {
        let mut app = app();
        app.tab = Tab::Images;
        app.update(key(KeyCode::Char(':')));
        assert!(app.prompt.is_none());
    }
}
