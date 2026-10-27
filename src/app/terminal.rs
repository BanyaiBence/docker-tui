//! Embedded shell pane: a vt100 screen fed by an exec session.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;
use crate::msg::Command;

/// Lines of scrollback kept by the emulator.
const SCROLLBACK: usize = 1_000;

pub struct TerminalState {
    pub container_id: String,
    pub container_name: String,
    pub parser: vt100::Parser,
    /// Whether keys go to the shell.
    pub focused: bool,
}

impl std::fmt::Debug for TerminalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalState")
            .field("container_id", &self.container_id)
            .field("focused", &self.focused)
            .finish_non_exhaustive()
    }
}

/// Whether `key` is the chord that releases focus from the shell pane.
/// Legacy terminals report Ctrl-\\ as Ctrl-4.
pub fn is_release_chord(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('\\') | KeyCode::Char('4'))
}

/// Bytes a terminal would send for `key`. `app_cursor` selects the
/// application cursor key mode (DECCKM) used by full-screen programs.
pub fn key_to_bytes(key: &KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let csi = |s: &str| format!("\x1b[{s}").into_bytes();
    let cursor = |c: char| {
        if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let mut bytes = match key.code {
        KeyCode::Char(c) if ctrl => match c.to_ascii_lowercase() {
            c @ 'a'..='z' => vec![c as u8 - b'a' + 1],
            ' ' | '@' | '2' => vec![0],
            '[' | '3' => vec![0x1b],
            '\\' | '4' => vec![0x1c],
            ']' | '5' => vec![0x1d],
            '^' | '6' => vec![0x1e],
            '_' | '/' | '7' => vec![0x1f],
            c => c.to_string().into_bytes(),
        },
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => csi("Z"),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => cursor('A'),
        KeyCode::Down => cursor('B'),
        KeyCode::Right => cursor('C'),
        KeyCode::Left => cursor('D'),
        KeyCode::Home => cursor('H'),
        KeyCode::End => cursor('F'),
        KeyCode::Insert => csi("2~"),
        KeyCode::Delete => csi("3~"),
        KeyCode::PageUp => csi("5~"),
        KeyCode::PageDown => csi("6~"),
        KeyCode::F(n @ 1..=4) => format!("\x1bO{}", (b'P' + n - 1) as char).into_bytes(),
        KeyCode::F(n) => {
            let code = match n {
                5 => 15,
                6..=10 => n + 11,
                11 | 12 => n + 12,
                _ => return None,
            };
            csi(&format!("{code}~"))
        }
        _ => return None,
    };
    if alt {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

impl App {
    /// `X`: focus the open shell pane, or start one for the target container.
    pub(super) fn open_terminal(&mut self) -> Vec<Command> {
        if let Some(t) = self.terminal.as_mut() {
            t.focused = true;
            return vec![];
        }
        match self.running_target() {
            Some(c) => {
                self.pending_terminal = Some(c.id.clone());
                vec![Command::DetectShell(c.id)]
            }
            None => vec![],
        }
    }

    pub(super) fn on_terminal_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let Some(t) = self.terminal.as_mut() else {
            return vec![];
        };
        if is_release_chord(&key) {
            t.focused = false;
            return vec![];
        }
        key_to_bytes(&key, t.parser.screen().application_cursor())
            .map(Command::TerminalInput)
            .into_iter()
            .collect()
    }

    /// Rows and columns of the shell pane at the current screen size.
    fn pane_size(&self) -> (u16, u16) {
        let area = crate::ui::terminal_area(ratatui::layout::Rect::new(
            0,
            0,
            self.screen.0,
            self.screen.1,
        ));
        (area.height.max(1), area.width.max(1))
    }

    /// The shell for an embedded session was found; start it.
    pub(super) fn start_terminal(&mut self, id: String, shell: String) -> Vec<Command> {
        let (rows, cols) = self.pane_size();
        let container_name = self
            .containers
            .items
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| id.clone());
        self.terminal = Some(TerminalState {
            container_id: id.clone(),
            container_name,
            parser: vt100::Parser::new(rows, cols, SCROLLBACK),
            focused: true,
        });
        vec![Command::StartTerminal {
            id,
            shell,
            rows,
            cols,
        }]
    }

    pub(super) fn on_terminal_output(&mut self, bytes: &[u8]) {
        if let Some(t) = self.terminal.as_mut() {
            t.parser.process(bytes);
        }
    }

    pub(super) fn on_terminal_closed(&mut self, error: Option<String>) {
        if self.terminal.take().is_none() {
            return;
        }
        match error {
            Some(e) => self.toast(format!("Shell session failed: {e}"), true),
            None => self.toast("Shell session ended", false),
        }
    }

    /// Keep the emulator and the remote PTY the size of the pane.
    pub(super) fn resize_terminal(&mut self) -> Vec<Command> {
        let (rows, cols) = self.pane_size();
        match self.terminal.as_mut() {
            Some(t) if t.parser.screen().size() != (rows, cols) => {
                t.parser.screen_mut().set_size(rows, cols);
                vec![Command::TerminalResize { rows, cols }]
            }
            _ => vec![],
        }
    }
}
