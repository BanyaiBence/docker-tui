use crossterm::event::{Event, KeyCode, KeyEvent};
use tui_input::{Input, backend::crossterm::EventHandler};

use super::{App, list::Selection};
use crate::{
    hub::models::{Repo, SearchPage, Tag, TagPage},
    msg::Command,
};

#[derive(Debug, Default)]
pub struct HubState {
    pub input: Input,
    pub editing: bool,
    /// The query the current results belong to.
    pub query: String,
    pub results: Vec<Repo>,
    pub count: u64,
    pub page: u32,
    pub has_next: bool,
    pub selection: Selection,
    pub loading: bool,
    pub error: Option<String>,
    pub tags: Option<TagsState>,
}

/// Tag list of one repository, shown on top of the search results.
#[derive(Debug)]
pub struct TagsState {
    pub repo: Repo,
    pub items: Vec<Tag>,
    pub page: u32,
    pub has_next: bool,
    pub selection: Selection,
    pub loading: bool,
    pub error: Option<String>,
}

impl HubState {
    pub fn selected(&self) -> Option<&Repo> {
        self.results.get(self.selection.index)
    }
}

impl HubState {
    fn submit(&mut self) -> Vec<Command> {
        let query = self.input.value().trim().to_string();
        if query.is_empty() {
            return vec![];
        }
        *self = HubState {
            input: std::mem::take(&mut self.input),
            query: query.clone(),
            page: 1,
            loading: true,
            ..HubState::default()
        };
        vec![Command::HubSearch { query, page: 1 }]
    }

    /// Request the next page once the cursor reaches the last loaded result.
    fn maybe_load_more(&mut self) -> Vec<Command> {
        if self.has_next && !self.loading && self.selection.index + 1 >= self.results.len() {
            self.loading = true;
            return vec![Command::HubSearch {
                query: self.query.clone(),
                page: self.page + 1,
            }];
        }
        vec![]
    }
}

impl TagsState {
    fn new(repo: Repo) -> Self {
        Self {
            repo,
            items: Vec::new(),
            page: 1,
            has_next: false,
            selection: Selection::default(),
            loading: true,
            error: None,
        }
    }

    fn selected_ref(&self) -> Option<String> {
        self.items
            .get(self.selection.index)
            .map(|t| self.repo.image_ref(&t.name))
    }

    fn maybe_load_more(&mut self) -> Vec<Command> {
        if self.has_next && !self.loading && self.selection.index + 1 >= self.items.len() {
            self.loading = true;
            return vec![Command::HubTags {
                repo: self.repo.clone(),
                page: self.page + 1,
            }];
        }
        vec![]
    }
}

impl App {
    pub(super) fn on_hub_editing_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let hub = &mut self.hub;
        match key.code {
            KeyCode::Enter => {
                let cmds = hub.submit();
                hub.editing = cmds.is_empty();
                cmds
            }
            KeyCode::Esc => {
                hub.editing = false;
                vec![]
            }
            _ => {
                hub.input.handle_event(&Event::Key(key));
                vec![]
            }
        }
    }

    pub(super) fn on_hub_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if let Some(tags) = self.hub.tags.as_mut() {
            if tags.selection.on_key(key.code, tags.items.len()) {
                return tags.maybe_load_more();
            }
            return match key.code {
                KeyCode::Esc | KeyCode::Backspace => {
                    self.hub.tags = None;
                    vec![]
                }
                KeyCode::Enter | KeyCode::Char('p') => match tags.selected_ref() {
                    Some(image) => self.start_pull(image),
                    None => vec![],
                },
                _ => vec![],
            };
        }

        let hub = &mut self.hub;
        if hub.selection.on_key(key.code, hub.results.len()) {
            return hub.maybe_load_more();
        }
        match key.code {
            KeyCode::Char('/') | KeyCode::Char('i') => {
                hub.editing = true;
                vec![]
            }
            KeyCode::Enter => match hub.selected().cloned() {
                Some(repo) => {
                    hub.tags = Some(TagsState::new(repo.clone()));
                    vec![Command::HubTags { repo, page: 1 }]
                }
                None => vec![],
            },
            KeyCode::Char('p') => match hub.selected().map(|r| r.image_ref("latest")) {
                Some(image) => self.start_pull(image),
                None => vec![],
            },
            _ => vec![],
        }
    }

    pub(super) fn on_hub_searched(
        &mut self,
        query: String,
        page: u32,
        result: Result<SearchPage, String>,
    ) {
        let hub = &mut self.hub;
        if query != hub.query {
            return;
        }
        hub.loading = false;
        match result {
            Ok(p) => {
                if page <= 1 {
                    hub.results = p.results;
                } else {
                    hub.results.extend(p.results);
                }
                hub.count = p.count;
                hub.has_next = p.has_next;
                hub.page = page;
                hub.error = None;
            }
            Err(e) => hub.error = Some(e),
        }
        hub.selection.clamp(hub.results.len());
    }

    pub(super) fn on_hub_tags(&mut self, repo: String, page: u32, result: Result<TagPage, String>) {
        let Some(tags) = self.hub.tags.as_mut().filter(|t| t.repo.name == repo) else {
            return;
        };
        tags.loading = false;
        match result {
            Ok(p) => {
                if page <= 1 {
                    tags.items = p.results;
                } else {
                    tags.items.extend(p.results);
                }
                tags.has_next = p.has_next;
                tags.page = page;
                tags.error = None;
            }
            Err(e) => tags.error = Some(e),
        }
        tags.selection.clamp(tags.items.len());
    }
}
