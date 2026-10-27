use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use crate::msg::Command;

/// A yes/no question guarding a destructive command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub message: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub until_tick: u64,
}

/// How long toasts stay visible.
pub const TOAST_TICKS: u64 = 16;

impl App {
    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            until_tick: self.ticks + TOAST_TICKS,
        });
    }

    pub(super) fn on_confirm_key(&mut self, key: KeyEvent) -> Vec<Command> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self
                .confirm
                .take()
                .map(|c| vec![c.command])
                .unwrap_or_default(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
                vec![]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{app::key, msg::Msg};

    fn app_with_confirm() -> App {
        let mut app = App::new();
        app.confirm = Some(Confirm {
            message: "Sure?".into(),
            command: Command::LoadImages,
        });
        app
    }

    #[test]
    fn y_runs_command() {
        let mut app = app_with_confirm();
        assert_eq!(
            app.update(key(KeyCode::Char('y'))),
            vec![Command::LoadImages]
        );
        assert!(app.confirm.is_none());
    }

    #[test]
    fn enter_runs_command() {
        let mut app = app_with_confirm();
        assert_eq!(app.update(key(KeyCode::Enter)), vec![Command::LoadImages]);
    }

    #[test]
    fn n_and_esc_cancel() {
        for code in [KeyCode::Char('n'), KeyCode::Esc] {
            let mut app = app_with_confirm();
            assert!(app.update(key(code)).is_empty());
            assert!(app.confirm.is_none());
        }
    }

    #[test]
    fn other_keys_are_swallowed() {
        let mut app = app_with_confirm();
        app.update(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
        assert!(app.confirm.is_some());
    }

    #[test]
    fn toast_expires_after_ticks() {
        let mut app = App::new();
        app.toast("hello", false);
        for _ in 0..TOAST_TICKS - 1 {
            app.update(Msg::Tick);
        }
        assert!(app.toast.is_some());
        app.update(Msg::Tick);
        assert!(app.toast.is_none());
    }
}
