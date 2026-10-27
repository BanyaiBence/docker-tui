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
