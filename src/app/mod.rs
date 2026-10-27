use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub mod confirm;
pub mod containers;
pub mod detail;
pub mod exec;
pub mod files;
pub mod hub;
pub mod images;
pub mod list;
pub mod logs;
pub mod pulls;
pub mod terminal;

use confirm::{Confirm, Toast};
use containers::ContainersState;
use detail::DetailState;
use exec::{ExecView, PromptState};
use hub::HubState;
use images::ImagesState;
use pulls::PullsState;
use terminal::TerminalState;

use crate::msg::{Command, Msg};

/// Default refresh interval in ticks (250ms each).
const REFRESH_TICKS: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    Hub,
    Images,
    #[default]
    Containers,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Hub, Tab::Images, Tab::Containers];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Hub => "Hub",
            Tab::Images => "Images",
            Tab::Containers => "Containers",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    pub fn next(self) -> Tab {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Tab {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Default)]
pub struct App {
    pub tab: Tab,
    pub show_help: bool,
    pub should_quit: bool,
    /// Current unix time, refreshed on ticks; used for relative timestamps.
    pub now: i64,
    pub ticks: u64,
    pub containers: ContainersState,
    pub images: ImagesState,
    pub detail: Option<DetailState>,
    pub confirm: Option<Confirm>,
    pub toast: Option<Toast>,
    pub hub: HubState,
    pub pulls: PullsState,
    pub prompt: Option<PromptState>,
    pub exec_view: Option<ExecView>,
    pub terminal: Option<TerminalState>,
    /// Container whose shell is being detected for the embedded pane.
    pub pending_terminal: Option<String>,
    /// Terminal size in columns and rows.
    pub screen: (u16, u16),
    /// Refresh lists every this many ticks.
    pub refresh_ticks: u64,
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

impl App {
    pub fn new() -> Self {
        Self {
            now: unix_now(),
            screen: (80, 24),
            refresh_ticks: REFRESH_TICKS,
            ..Self::default()
        }
    }

    /// Commands to run once at startup.
    pub fn init(&mut self) -> Vec<Command> {
        let mut cmds = self.containers.refresh();
        cmds.extend(self.images.refresh());
        cmds
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Command> {
        match msg {
            Msg::Key(key) => self.on_key(key),
            Msg::Resize(w, h) => {
                self.screen = (w, h);
                self.resize_terminal()
            }
            Msg::TerminalOutput(bytes) => {
                self.on_terminal_output(&bytes);
                vec![]
            }
            Msg::TerminalClosed(error) => {
                self.on_terminal_closed(error);
                vec![]
            }
            Msg::Tick => self.on_tick(),
            Msg::ContainersLoaded(result) => {
                self.containers.set_items(result);
                if let Some(d) = self.detail.as_mut()
                    && let Some(c) = self
                        .containers
                        .items
                        .iter()
                        .find(|c| c.id == d.container.id)
                {
                    d.container = c.clone();
                }
                vec![]
            }
            Msg::ImagesLoaded(result) => {
                self.images.set_items(result);
                vec![]
            }
            Msg::Inspected { id, result } => {
                self.on_inspected(id, result);
                vec![]
            }
            Msg::ActionDone { label, result } => {
                match result {
                    Ok(()) => self.toast(label, false),
                    Err(e) => self.toast(format!("{label} failed: {e}"), true),
                }
                let mut cmds = self.containers.refresh();
                cmds.extend(self.images.refresh());
                cmds
            }
            Msg::LogChunk { id, text } => {
                if let Some(d) = self.detail_for(&id) {
                    d.logs.push(&text);
                }
                vec![]
            }
            Msg::LogsEnded { id, error } => {
                if let (Some(d), Some(e)) = (self.detail_for(&id), error) {
                    d.logs.push_line(&format!("[log stream error: {e}]"));
                }
                vec![]
            }
            Msg::Stats { id, sample } => {
                if let Some(d) = self.detail_for(&id) {
                    d.push_stats(sample);
                }
                vec![]
            }
            Msg::HubSearched {
                query,
                page,
                result,
            } => {
                self.on_hub_searched(query, page, result);
                vec![]
            }
            Msg::HubTagsLoaded { repo, page, result } => {
                self.on_hub_tags(repo, page, result);
                vec![]
            }
            Msg::PullProgress { image, event } => {
                self.on_pull_progress(image, event);
                vec![]
            }
            Msg::PullDone { image, result } => self.on_pull_done(image, result),
            Msg::DirListed { id, path, result } => {
                if let Some(d) = self.detail_for(&id) {
                    d.files.on_listed(&path, result);
                }
                vec![]
            }
            Msg::FileRead { id, path, result } => {
                if let Some(d) = self.detail_for(&id) {
                    d.files.on_file_read(&path, result);
                }
                vec![]
            }
            Msg::DiffLoaded { id, result } => {
                if let Some(d) = self.detail_for(&id) {
                    d.files.on_diff(result);
                }
                vec![]
            }
            Msg::ShellDetected { id, result } => self.on_shell_detected(id, result),
            Msg::ShellExited(result) => {
                match result {
                    Ok(()) => self.toast("Shell closed", false),
                    Err(e) => self.toast(format!("Shell failed: {e}"), true),
                }
                self.containers.refresh()
            }
            Msg::ExecDone { command, result } => {
                self.on_exec_done(command, result);
                vec![]
            }
        }
    }

    /// The open detail view, if it shows container `id`.
    fn detail_for(&mut self, id: &str) -> Option<&mut DetailState> {
        self.detail.as_mut().filter(|d| d.container.id == id)
    }

    fn on_tick(&mut self) -> Vec<Command> {
        self.now = unix_now();
        self.ticks += 1;
        if self
            .toast
            .as_ref()
            .is_some_and(|t| self.ticks >= t.until_tick)
        {
            self.toast = None;
        }
        let mut cmds = vec![];
        if self.ticks.is_multiple_of(self.refresh_ticks.max(1)) {
            if !self.containers.loading {
                cmds.extend(self.containers.refresh());
            }
            if !self.images.loading {
                cmds.extend(self.images.refresh());
            }
        }
        cmds
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if self.terminal.as_ref().is_some_and(|t| t.focused) {
            return self.on_terminal_key(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return vec![];
        }
        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.show_help = false;
            }
            return vec![];
        }
        if self.confirm.is_some() {
            return self.on_confirm_key(key);
        }
        if self.prompt.is_some() {
            return self.on_prompt_key(key);
        }
        if self.exec_view.is_some() {
            return self.on_exec_view_key(key);
        }
        if self.pulls.visible {
            return self.on_pulls_key(key);
        }
        if self.tab == Tab::Hub && self.hub.editing {
            return self.on_hub_editing_key(key);
        }
        if self.on_global_key(key) {
            return vec![];
        }
        match self.tab {
            Tab::Hub => self.on_hub_key(key),
            Tab::Images => self.on_images_key(key),
            Tab::Containers if self.detail.is_some() => self.on_detail_key(key),
            Tab::Containers => self.on_containers_key(key),
        }
    }

    /// Keys that work anywhere no modal or text input has focus.
    /// Returns whether the key was handled.
    fn on_global_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('P') => self.pulls.visible = true,
            KeyCode::Char('1') => self.tab = Tab::Hub,
            KeyCode::Char('2') => self.tab = Tab::Images,
            KeyCode::Char('3') => self.tab = Tab::Containers,
            KeyCode::Tab => self.tab = self.tab.next(),
            KeyCode::BackTab => self.tab = self.tab.prev(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
pub(crate) fn key(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_containers_tab() {
        assert_eq!(App::new().tab, Tab::Containers);
    }

    #[test]
    fn init_loads_lists() {
        let mut app = App::new();
        assert_eq!(
            app.init(),
            vec![Command::LoadContainers, Command::LoadImages]
        );
        assert!(app.containers.loading && app.images.loading);
    }

    #[test]
    fn periodic_refresh_on_ticks() {
        let mut app = App::new();
        let mut cmds = vec![];
        for _ in 0..REFRESH_TICKS {
            cmds = app.update(Msg::Tick);
        }
        assert_eq!(cmds, vec![Command::LoadContainers, Command::LoadImages]);
    }

    #[test]
    fn refresh_interval_is_configurable() {
        let mut app = App::new();
        app.refresh_ticks = 2;
        assert!(app.update(Msg::Tick).is_empty());
        assert_eq!(
            app.update(Msg::Tick),
            vec![Command::LoadContainers, Command::LoadImages]
        );
    }

    #[test]
    fn periodic_refresh_skips_lists_still_loading() {
        let mut app = App::new();
        app.containers.loading = true;
        let mut cmds = vec![];
        for _ in 0..REFRESH_TICKS {
            cmds = app.update(Msg::Tick);
        }
        assert_eq!(cmds, vec![Command::LoadImages]);
    }

    #[test]
    fn q_quits() {
        let mut app = App::new();
        app.update(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        app.update(Msg::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(app.should_quit);
    }

    #[test]
    fn number_keys_select_tabs() {
        let mut app = App::new();
        app.update(key(KeyCode::Char('1')));
        assert_eq!(app.tab, Tab::Hub);
        app.update(key(KeyCode::Char('2')));
        assert_eq!(app.tab, Tab::Images);
        app.update(key(KeyCode::Char('3')));
        assert_eq!(app.tab, Tab::Containers);
    }

    #[test]
    fn tab_and_backtab_cycle() {
        let mut app = App::new();
        app.update(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Hub);
        app.update(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Containers);
        app.update(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Images);
    }

    #[test]
    fn question_mark_toggles_help_and_esc_closes_it() {
        let mut app = App::new();
        app.update(key(KeyCode::Char('?')));
        assert!(app.show_help);
        app.update(key(KeyCode::Esc));
        assert!(!app.show_help);
    }

    #[test]
    fn help_swallows_other_keys() {
        let mut app = App::new();
        app.update(key(KeyCode::Char('?')));
        app.update(key(KeyCode::Char('1')));
        assert_eq!(app.tab, Tab::Containers);
        // q still closes help rather than quitting
        app.update(key(KeyCode::Char('q')));
        assert!(!app.show_help);
        assert!(!app.should_quit);
    }
}
