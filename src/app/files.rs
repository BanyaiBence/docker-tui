use crossterm::event::KeyCode;

use super::{detail::Loadable, list::Selection};
use crate::{
    docker::{
        DirListing,
        fs::{self, ChangeKind, FileKind, FilePreview, FsChange},
    },
    msg::Command,
};

/// File browser state for one container.
#[derive(Debug)]
pub struct FilesState {
    pub path: String,
    /// `None` until the Files tab is first opened.
    pub listing: Option<Loadable<DirListing>>,
    pub selection: Selection,
    /// Entry to select once the listing arrives (the dir we came up from).
    focus_name: Option<String>,
    pub preview: Option<PreviewState>,
    /// `docker diff` view, shown instead of the listing when open.
    pub changes: Option<ChangesState>,
}

#[derive(Debug)]
pub struct PreviewState {
    pub path: String,
    pub content: Loadable<FilePreview>,
    pub scroll: u16,
}

#[derive(Debug)]
pub struct ChangesState {
    pub items: Loadable<Vec<FsChange>>,
    pub selection: Selection,
}

impl Default for FilesState {
    fn default() -> Self {
        Self {
            path: "/".into(),
            listing: None,
            selection: Selection::default(),
            focus_name: None,
            preview: None,
            changes: None,
        }
    }
}

impl FilesState {
    /// Load the root directory the first time the tab is shown.
    pub fn ensure_loaded(&mut self, id: &str) -> Vec<Command> {
        if self.listing.is_some() {
            return vec![];
        }
        self.load(id, self.path.clone())
    }

    fn load(&mut self, id: &str, path: String) -> Vec<Command> {
        self.path = path.clone();
        self.listing = Some(Loadable::Loading);
        vec![Command::ListDir {
            id: id.to_string(),
            path,
        }]
    }

    fn open_preview(&mut self, id: &str, path: String) -> Vec<Command> {
        self.preview = Some(PreviewState {
            path: path.clone(),
            content: Loadable::Loading,
            scroll: 0,
        });
        vec![Command::ReadFile {
            id: id.to_string(),
            path,
        }]
    }

    pub fn on_key(&mut self, id: &str, code: KeyCode) -> Vec<Command> {
        if let Some(preview) = self.preview.as_mut() {
            match code {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => self.preview = None,
                KeyCode::Down | KeyCode::Char('j') => {
                    preview.scroll = preview.scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    preview.scroll = preview.scroll.saturating_sub(1)
                }
                KeyCode::PageDown => preview.scroll = preview.scroll.saturating_add(20),
                KeyCode::PageUp => preview.scroll = preview.scroll.saturating_sub(20),
                KeyCode::Home | KeyCode::Char('g') => preview.scroll = 0,
                _ => {}
            }
            return vec![];
        }

        if let Some(changes) = self.changes.as_mut() {
            let items: &[FsChange] = match &changes.items {
                Loadable::Loaded(items) => items,
                _ => &[],
            };
            if changes.selection.on_key(code, items.len()) {
                return vec![];
            }
            return match code {
                KeyCode::Esc | KeyCode::Char('D') => {
                    self.changes = None;
                    vec![]
                }
                KeyCode::Enter => match items.get(changes.selection.index) {
                    Some(c) if c.kind != ChangeKind::Deleted => {
                        let path = c.path.clone();
                        self.open_preview(id, path)
                    }
                    _ => vec![],
                },
                _ => vec![],
            };
        }

        let entries = match &self.listing {
            Some(Loadable::Loaded(l)) => l.entries.as_slice(),
            _ => &[],
        };
        if self.selection.on_key(code, entries.len()) {
            return vec![];
        }
        match code {
            KeyCode::Enter => match entries.get(self.selection.index) {
                Some(e) => {
                    let path = fs::join(&self.path, &e.name);
                    if matches!(e.kind, FileKind::Dir | FileKind::Symlink) {
                        self.selection = Selection::default();
                        self.load(id, path)
                    } else {
                        self.open_preview(id, path)
                    }
                }
                None => vec![],
            },
            KeyCode::Backspace | KeyCode::Char('-') if self.path != "/" => {
                let name = self.path.rsplit('/').next().unwrap_or_default().to_string();
                self.focus_name = Some(name);
                let parent = fs::parent(&self.path);
                self.load(id, parent)
            }
            KeyCode::Char('r') => self.load(id, self.path.clone()),
            KeyCode::Char('D') => {
                self.changes = Some(ChangesState {
                    items: Loadable::Loading,
                    selection: Selection::default(),
                });
                vec![Command::Diff(id.to_string())]
            }
            _ => vec![],
        }
    }

    /// Whether Esc should be handled here (closing a preview or the changes
    /// view) rather than closing the detail view.
    pub fn has_overlay(&self) -> bool {
        self.preview.is_some() || self.changes.is_some()
    }

    pub fn on_listed(&mut self, path: &str, result: Result<DirListing, String>) {
        if path != self.path {
            return;
        }
        let focus = self.focus_name.take();
        if let Ok(listing) = &result {
            let found = focus.and_then(|name| listing.entries.iter().position(|e| e.name == name));
            self.selection.index = found.unwrap_or(0);
        }
        self.listing = Some(result.into());
    }

    pub fn on_file_read(&mut self, path: &str, result: Result<FilePreview, String>) {
        if let Some(preview) = self.preview.as_mut().filter(|p| p.path == path) {
            preview.content = result.into();
        }
    }

    pub fn on_diff(&mut self, result: Result<Vec<FsChange>, String>) {
        if let Some(changes) = self.changes.as_mut() {
            changes.items = result.into();
            changes.selection = Selection::default();
        }
    }
}
