//! Shared test helpers: mocks for the Docker and Hub APIs, model builders,
//! key events and headless rendering.

pub mod mock;
pub mod mock_hub;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use docker_tui::{
    app::App,
    docker::{
        DirListing,
        fs::{FileKind, FsEntry},
    },
    hub::models::{Repo, Tag},
    msg::Msg,
};
pub use mock::{container, image};
use ratatui::{Terminal, backend::TestBackend};

/// A key press without modifiers.
pub fn key(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Render `app` into a `width`×`height` buffer, as text for snapshots.
pub fn render_to_string(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| docker_tui::ui::render(app, f)).unwrap();
    terminal.backend().to_string()
}

/// A Hub repository; names without a namespace are official images.
pub fn repo(name: &str) -> Repo {
    Repo {
        name: name.into(),
        description: String::new(),
        stars: 1,
        pulls: 2,
        official: !name.contains('/'),
    }
}

/// A small single-platform tag.
pub fn tag(name: &str) -> Tag {
    Tag {
        name: name.into(),
        size: 1_000,
        last_updated: None,
        architectures: vec!["amd64".into()],
    }
}

/// A directory entry with `ls`-style mode for its kind.
pub fn entry(name: &str, kind: FileKind) -> FsEntry {
    FsEntry {
        name: name.into(),
        kind,
        size: 10,
        mode: if kind == FileKind::Dir {
            "drwxr-xr-x".into()
        } else {
            "-rw-r--r--".into()
        },
        modified: "Oct 27 22:28".into(),
        link_target: None,
    }
}

/// A plain `ls` listing of `entries`.
pub fn listing(entries: Vec<FsEntry>) -> DirListing {
    DirListing {
        entries,
        ..Default::default()
    }
}
