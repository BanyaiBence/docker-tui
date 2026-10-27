use crossterm::event::{KeyCode, KeyEvent};

use std::collections::VecDeque;

use super::{App, files::FilesState, logs::LogBuffer};
use crate::{
    docker::{models::Container, stats::StatsSample},
    msg::Command,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loadable<T> {
    Loading,
    Loaded(T),
    Failed(String),
}

impl<T> From<Result<T, String>> for Loadable<T> {
    fn from(r: Result<T, String>) -> Self {
        match r {
            Ok(v) => Loadable::Loaded(v),
            Err(e) => Loadable::Failed(e),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    Info,
    Logs,
    Stats,
    Files,
}

impl DetailTab {
    pub const ALL: [DetailTab; 4] = [
        DetailTab::Info,
        DetailTab::Logs,
        DetailTab::Stats,
        DetailTab::Files,
    ];

    pub fn title(self) -> &'static str {
        match self {
            DetailTab::Info => "Info",
            DetailTab::Logs => "Logs",
            DetailTab::Stats => "Stats",
            DetailTab::Files => "Files",
        }
    }
}

/// Number of stats samples kept for sparklines.
pub const STATS_HISTORY: usize = 120;

/// Drill-down view of a single container.
#[derive(Debug)]
pub struct DetailState {
    pub container: Container,
    pub tab: DetailTab,
    pub inspect: Loadable<String>,
    pub scroll: u16,
    pub logs: LogBuffer,
    pub stats: VecDeque<StatsSample>,
    pub files: FilesState,
}

impl DetailState {
    pub fn push_stats(&mut self, sample: StatsSample) {
        self.stats.push_back(sample);
        while self.stats.len() > STATS_HISTORY {
            self.stats.pop_front();
        }
    }

    fn switch_tab(&mut self, delta: isize) {
        let n = DetailTab::ALL.len() as isize;
        let i = DetailTab::ALL
            .iter()
            .position(|t| *t == self.tab)
            .unwrap_or(0) as isize;
        self.tab = DetailTab::ALL[(i + delta).rem_euclid(n) as usize];
    }

    pub fn new(container: Container) -> Self {
        Self {
            container,
            tab: DetailTab::default(),
            inspect: Loadable::Loading,
            scroll: 0,
            logs: LogBuffer::default(),
            stats: VecDeque::new(),
            files: FilesState::default(),
        }
    }
}

impl App {
    /// Open the detail view for `container`.
    pub(super) fn open_detail(&mut self, container: Container) -> Vec<Command> {
        let id = container.id.clone();
        let running = container.state.is_running();
        self.detail = Some(DetailState::new(container));
        let mut cmds = vec![
            Command::InspectContainer(id.clone()),
            Command::StreamLogs(id.clone()),
        ];
        if running {
            cmds.push(Command::StreamStats(id));
        }
        cmds
    }

    pub(super) fn on_detail_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let Some(detail) = self.detail.as_mut() else {
            return vec![];
        };
        let id = detail.container.id.clone();
        if detail.tab == DetailTab::Files {
            let files = &mut detail.files;
            let handled_here = files.has_overlay() || !matches!(key.code, KeyCode::Esc);
            if handled_here
                && !matches!(
                    key.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Char('h' | 'l' | 'x' | 'X' | ':')
                )
            {
                return files.on_key(&id, key.code);
            }
        }
        match key.code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.detail = None;
                return vec![Command::StopStreams];
            }
            KeyCode::Right | KeyCode::Char('l') => detail.switch_tab(1),
            KeyCode::Left | KeyCode::Char('h') => detail.switch_tab(-1),
            KeyCode::Char('x') => return self.open_shell(),
            KeyCode::Char('X') => return self.open_terminal(),
            KeyCode::Char(':') => return self.open_prompt(),
            code => match detail.tab {
                DetailTab::Info => scroll_info(detail, code),
                DetailTab::Logs => scroll_logs(detail, code),
                DetailTab::Stats | DetailTab::Files => {}
            },
        }
        // Entering the Files tab loads the root directory on first visit.
        match self.detail.as_mut() {
            Some(d) if d.tab == DetailTab::Files => d.files.ensure_loaded(&id),
            _ => vec![],
        }
    }
}

fn scroll_info(detail: &mut DetailState, code: KeyCode) {
    match code {
        KeyCode::Down | KeyCode::Char('j') => detail.scroll = detail.scroll.saturating_add(1),
        KeyCode::Up | KeyCode::Char('k') => detail.scroll = detail.scroll.saturating_sub(1),
        KeyCode::PageDown => detail.scroll = detail.scroll.saturating_add(20),
        KeyCode::PageUp => detail.scroll = detail.scroll.saturating_sub(20),
        KeyCode::Home | KeyCode::Char('g') => detail.scroll = 0,
        _ => {}
    }
}

fn scroll_logs(detail: &mut DetailState, code: KeyCode) {
    let logs = &mut detail.logs;
    match code {
        KeyCode::Up | KeyCode::Char('k') => logs.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => logs.scroll_down(1),
        KeyCode::PageUp => logs.scroll_up(20),
        KeyCode::PageDown => logs.scroll_down(20),
        KeyCode::Home | KeyCode::Char('g') => logs.scroll_up(usize::MAX / 2),
        KeyCode::End | KeyCode::Char('G') | KeyCode::Char('f') => logs.follow(),
        _ => {}
    }
}

impl App {
    pub(super) fn on_inspected(&mut self, id: String, result: Result<String, String>) {
        if let Some(detail) = self.detail.as_mut().filter(|d| d.container.id == id) {
            detail.inspect = result.into();
        }
    }
}
