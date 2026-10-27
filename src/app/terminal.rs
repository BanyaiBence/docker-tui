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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::{
        app::{Tab, key},
        docker::models::ContainerState,
        msg::Msg,
        runtime::mock::container,
        ui::terminal_area,
    };

    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    fn bytes(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
        key_to_bytes(&k(code, mods), false).unwrap()
    }

    #[test]
    fn encodes_text_and_control_keys() {
        assert_eq!(bytes(KeyCode::Char('a'), KeyModifiers::NONE), b"a");
        assert_eq!(bytes(KeyCode::Char('A'), KeyModifiers::SHIFT), b"A");
        assert_eq!(
            bytes(KeyCode::Char('é'), KeyModifiers::NONE),
            "é".as_bytes()
        );
        assert_eq!(bytes(KeyCode::Char('c'), KeyModifiers::CONTROL), [3]);
        assert_eq!(bytes(KeyCode::Char('d'), KeyModifiers::CONTROL), [4]);
        assert_eq!(bytes(KeyCode::Char('x'), KeyModifiers::ALT), b"\x1bx");
        assert_eq!(bytes(KeyCode::Enter, KeyModifiers::NONE), b"\r");
        assert_eq!(bytes(KeyCode::Backspace, KeyModifiers::NONE), [0x7f]);
        assert_eq!(bytes(KeyCode::Tab, KeyModifiers::NONE), b"\t");
        assert_eq!(bytes(KeyCode::BackTab, KeyModifiers::SHIFT), b"\x1b[Z");
        assert_eq!(bytes(KeyCode::Esc, KeyModifiers::NONE), b"\x1b");
    }

    #[test]
    fn encodes_navigation_keys() {
        assert_eq!(bytes(KeyCode::Up, KeyModifiers::NONE), b"\x1b[A");
        assert_eq!(bytes(KeyCode::Left, KeyModifiers::NONE), b"\x1b[D");
        assert_eq!(bytes(KeyCode::Home, KeyModifiers::NONE), b"\x1b[H");
        assert_eq!(bytes(KeyCode::End, KeyModifiers::NONE), b"\x1b[F");
        assert_eq!(bytes(KeyCode::PageUp, KeyModifiers::NONE), b"\x1b[5~");
        assert_eq!(bytes(KeyCode::Delete, KeyModifiers::NONE), b"\x1b[3~");
        assert_eq!(bytes(KeyCode::F(1), KeyModifiers::NONE), b"\x1bOP");
        assert_eq!(bytes(KeyCode::F(5), KeyModifiers::NONE), b"\x1b[15~");
        assert_eq!(bytes(KeyCode::F(12), KeyModifiers::NONE), b"\x1b[24~");
    }

    #[test]
    fn application_cursor_mode_changes_arrows() {
        let up = key_to_bytes(&k(KeyCode::Up, KeyModifiers::NONE), true).unwrap();
        assert_eq!(up, b"\x1bOA");
    }

    #[test]
    fn release_chord_is_ctrl_backslash_or_ctrl_4() {
        assert!(is_release_chord(&k(
            KeyCode::Char('\\'),
            KeyModifiers::CONTROL
        )));
        assert!(is_release_chord(&k(
            KeyCode::Char('4'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_release_chord(&k(
            KeyCode::Char('\\'),
            KeyModifiers::NONE
        )));
    }

    fn app() -> App {
        let mut app = App::new();
        app.update(Msg::Resize(100, 30));
        app.update(Msg::ContainersLoaded(Ok(vec![
            container("c1", "web", ContainerState::Running),
            container("c2", "db", ContainerState::Exited),
        ])));
        app
    }

    fn app_with_terminal() -> App {
        let mut app = app();
        app.update(key(KeyCode::Char('X')));
        app.update(Msg::ShellDetected {
            id: "c1".into(),
            result: Ok("/bin/sh".into()),
        });
        app
    }

    fn pane_size(w: u16, h: u16) -> (u16, u16) {
        let area = terminal_area(ratatui::layout::Rect::new(0, 0, w, h));
        (area.height, area.width)
    }

    #[test]
    fn shift_x_detects_shell_then_starts_embedded_session() {
        let mut app = app();
        assert_eq!(
            app.update(key(KeyCode::Char('X'))),
            vec![Command::DetectShell("c1".into())]
        );
        let cmds = app.update(Msg::ShellDetected {
            id: "c1".into(),
            result: Ok("/bin/sh".into()),
        });
        let (rows, cols) = pane_size(100, 30);
        assert_eq!(
            cmds,
            vec![Command::StartTerminal {
                id: "c1".into(),
                shell: "/bin/sh".into(),
                rows,
                cols
            }]
        );
        let term = app.terminal.as_ref().unwrap();
        assert!(term.focused);
        assert_eq!(term.parser.screen().size(), (rows, cols));
    }

    #[test]
    fn plain_x_still_runs_external_shell() {
        let mut app = app();
        app.update(key(KeyCode::Char('x')));
        let cmds = app.update(Msg::ShellDetected {
            id: "c1".into(),
            result: Ok("/bin/sh".into()),
        });
        assert!(matches!(cmds[0], Command::RunShell { .. }));
        assert!(app.terminal.is_none());
    }

    #[test]
    fn focused_pane_forwards_all_keys() {
        let mut app = app_with_terminal();
        assert_eq!(
            app.update(key(KeyCode::Char('q'))),
            vec![Command::TerminalInput(b"q".to_vec())]
        );
        assert!(!app.should_quit);
        let ctrl_c = Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(app.update(ctrl_c), vec![Command::TerminalInput(vec![3])]);
        assert!(!app.should_quit);
        app.update(key(KeyCode::Char('2')));
        assert_eq!(app.tab, Tab::Containers);
    }

    #[test]
    fn release_chord_returns_focus_and_x_refocuses() {
        let mut app = app_with_terminal();
        let release = Msg::Key(KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL));
        assert!(app.update(release).is_empty());
        assert!(!app.terminal.as_ref().unwrap().focused);
        app.update(key(KeyCode::Char('j')));
        assert_eq!(
            app.containers.selection.index, 1,
            "keys drive the app again"
        );
        assert!(
            app.update(key(KeyCode::Char('X'))).is_empty(),
            "no new session"
        );
        assert!(app.terminal.as_ref().unwrap().focused);
    }

    #[test]
    fn output_is_fed_to_the_emulator() {
        let mut app = app_with_terminal();
        app.update(Msg::TerminalOutput(
            b"hello \x1b[1mworld\x1b[0m\r\n$ ".to_vec(),
        ));
        let contents = app.terminal.as_ref().unwrap().parser.screen().contents();
        assert!(contents.starts_with("hello world\n$"));
    }

    #[test]
    fn closing_removes_pane() {
        let mut app = app_with_terminal();
        app.update(Msg::TerminalClosed(None));
        assert!(app.terminal.is_none());
        app.update(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn closing_with_error_toasts() {
        let mut app = app_with_terminal();
        app.update(Msg::TerminalClosed(Some("attach failed".into())));
        assert!(app.toast.as_ref().unwrap().error);
    }

    #[test]
    fn resize_resizes_emulator_and_pty() {
        let mut app = app_with_terminal();
        let cmds = app.update(Msg::Resize(120, 40));
        let (rows, cols) = pane_size(120, 40);
        assert_eq!(cmds, vec![Command::TerminalResize { rows, cols }]);
        assert_eq!(
            app.terminal.as_ref().unwrap().parser.screen().size(),
            (rows, cols)
        );
        assert!(
            app.update(Msg::Resize(120, 40)).is_empty(),
            "unchanged size sends nothing"
        );
    }

    #[test]
    fn embedded_shell_needs_running_container() {
        let mut app = app();
        app.update(key(KeyCode::Char('j')));
        assert!(app.update(key(KeyCode::Char('X'))).is_empty());
    }
}
