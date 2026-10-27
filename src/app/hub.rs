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

#[cfg(test)]
pub(crate) mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{
        app::{Tab, key},
        msg::Msg,
    };

    pub fn repo(name: &str) -> Repo {
        Repo {
            name: name.into(),
            description: String::new(),
            stars: 1,
            pulls: 2,
            official: !name.contains('/'),
        }
    }

    pub fn tag(name: &str) -> Tag {
        Tag {
            name: name.into(),
            size: 1_000,
            last_updated: None,
            architectures: vec!["amd64".into()],
        }
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.update(key(KeyCode::Char(c)));
        }
    }

    fn hub_app() -> App {
        let mut app = App::new();
        app.tab = Tab::Hub;
        app
    }

    /// Hub tab with a submitted search for "nginx" and two results.
    fn app_with_results(has_next: bool) -> App {
        let mut app = hub_app();
        app.update(key(KeyCode::Char('/')));
        type_text(&mut app, "nginx");
        app.update(key(KeyCode::Enter));
        app.update(Msg::HubSearched {
            query: "nginx".into(),
            page: 1,
            result: Ok(SearchPage {
                count: 3,
                results: vec![repo("nginx"), repo("bitnami/nginx")],
                has_next,
            }),
        });
        app
    }

    #[test]
    fn slash_starts_editing_and_typing_fills_input() {
        let mut app = hub_app();
        app.update(key(KeyCode::Char('/')));
        assert!(app.hub.editing);
        type_text(&mut app, "q1");
        assert_eq!(app.hub.input.value(), "q1");
        assert!(!app.should_quit, "q is text while editing");
        assert_eq!(app.tab, Tab::Hub, "1 is text while editing");
    }

    #[test]
    fn enter_submits_search() {
        let mut app = hub_app();
        app.update(key(KeyCode::Char('/')));
        type_text(&mut app, "redis");
        let cmds = app.update(key(KeyCode::Enter));
        assert_eq!(
            cmds,
            vec![Command::HubSearch {
                query: "redis".into(),
                page: 1
            }]
        );
        assert!(!app.hub.editing);
        assert!(app.hub.loading);
        assert_eq!(app.hub.query, "redis");
    }

    #[test]
    fn enter_on_blank_query_does_nothing() {
        let mut app = hub_app();
        app.update(key(KeyCode::Char('/')));
        type_text(&mut app, "  ");
        assert!(app.update(key(KeyCode::Enter)).is_empty());
    }

    #[test]
    fn esc_stops_editing_without_searching() {
        let mut app = hub_app();
        app.update(key(KeyCode::Char('/')));
        type_text(&mut app, "x");
        assert!(app.update(key(KeyCode::Esc)).is_empty());
        assert!(!app.hub.editing);
    }

    #[test]
    fn results_are_stored() {
        let app = app_with_results(false);
        assert_eq!(app.hub.results.len(), 2);
        assert_eq!(app.hub.count, 3);
        assert!(!app.hub.loading);
        assert_eq!(app.hub.selected().unwrap().name, "nginx");
    }

    #[test]
    fn stale_results_are_ignored() {
        let mut app = app_with_results(false);
        app.update(Msg::HubSearched {
            query: "old".into(),
            page: 1,
            result: Ok(SearchPage {
                count: 1,
                results: vec![repo("old")],
                has_next: false,
            }),
        });
        assert_eq!(app.hub.results.len(), 2);
    }

    #[test]
    fn search_error_is_shown() {
        let mut app = hub_app();
        app.update(key(KeyCode::Char('/')));
        type_text(&mut app, "x");
        app.update(key(KeyCode::Enter));
        app.update(Msg::HubSearched {
            query: "x".into(),
            page: 1,
            result: Err("rate limit".into()),
        });
        assert_eq!(app.hub.error.as_deref(), Some("rate limit"));
        assert!(!app.hub.loading);
    }

    #[test]
    fn reaching_the_end_loads_next_page() {
        let mut app = app_with_results(true);
        let cmds = app.update(key(KeyCode::Char('j')));
        assert_eq!(
            cmds,
            vec![Command::HubSearch {
                query: "nginx".into(),
                page: 2
            }]
        );
        // no duplicate request while loading
        assert!(app.update(key(KeyCode::Char('j'))).is_empty());
        app.update(Msg::HubSearched {
            query: "nginx".into(),
            page: 2,
            result: Ok(SearchPage {
                count: 3,
                results: vec![repo("x/nginx")],
                has_next: false,
            }),
        });
        assert_eq!(app.hub.results.len(), 3);
        assert!(!app.hub.has_next);
    }

    #[test]
    fn no_next_page_when_last() {
        let mut app = app_with_results(false);
        assert!(app.update(key(KeyCode::Char('j'))).is_empty());
    }

    #[test]
    fn enter_opens_tags() {
        let mut app = app_with_results(false);
        let cmds = app.update(key(KeyCode::Enter));
        assert_eq!(
            cmds,
            vec![Command::HubTags {
                repo: repo("nginx"),
                page: 1
            }]
        );
        let tags = app.hub.tags.as_ref().unwrap();
        assert_eq!(tags.repo.name, "nginx");
        assert!(tags.loading);
    }

    fn app_with_tags() -> App {
        let mut app = app_with_results(false);
        app.update(key(KeyCode::Enter));
        app.update(Msg::HubTagsLoaded {
            repo: "nginx".into(),
            page: 1,
            result: Ok(TagPage {
                count: 2,
                results: vec![tag("latest"), tag("1.27")],
                has_next: false,
            }),
        });
        app
    }

    #[test]
    fn tags_are_stored_and_stale_ignored() {
        let mut app = app_with_tags();
        app.update(Msg::HubTagsLoaded {
            repo: "other".into(),
            page: 1,
            result: Ok(TagPage {
                count: 1,
                results: vec![tag("x")],
                has_next: false,
            }),
        });
        let tags = app.hub.tags.as_ref().unwrap();
        assert_eq!(tags.items.len(), 2);
        assert!(!tags.loading);
    }

    #[test]
    fn enter_on_tag_pulls_it() {
        let mut app = app_with_tags();
        app.update(key(KeyCode::Char('j')));
        assert_eq!(
            app.update(key(KeyCode::Enter)),
            vec![Command::Pull("nginx:1.27".into())]
        );
    }

    #[test]
    fn esc_closes_tags() {
        let mut app = app_with_tags();
        app.update(key(KeyCode::Esc));
        assert!(app.hub.tags.is_none());
    }

    #[test]
    fn p_pulls_latest_from_results() {
        let mut app = app_with_results(false);
        app.update(key(KeyCode::Char('j')));
        assert_eq!(
            app.update(key(KeyCode::Char('p'))),
            vec![Command::Pull("bitnami/nginx:latest".into())]
        );
    }

    #[test]
    fn slash_from_results_edits_again() {
        let mut app = app_with_results(false);
        app.update(key(KeyCode::Char('/')));
        assert!(app.hub.editing);
        assert_eq!(app.hub.input.value(), "nginx");
    }
}
