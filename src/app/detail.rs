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

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::{DetailTab, Loadable, STATS_HISTORY};
    use crate::{
        app::{App, key},
        docker::{models::ContainerState, stats::StatsSample},
        msg::{Command, Msg},
        runtime::mock::container,
    };

    fn app_with_detail() -> App {
        let mut app = App::new();
        app.update(Msg::ContainersLoaded(Ok(vec![container(
            "c1",
            "web",
            ContainerState::Running,
        )])));
        app.update(key(KeyCode::Enter));
        app
    }

    #[test]
    fn detail_starts_loading_inspect() {
        let app = app_with_detail();
        assert_eq!(app.detail.as_ref().unwrap().inspect, Loadable::Loading);
    }

    #[test]
    fn inspect_result_is_stored() {
        let mut app = app_with_detail();
        app.update(Msg::Inspected {
            id: "c1".into(),
            result: Ok("{}".into()),
        });
        assert_eq!(
            app.detail.as_ref().unwrap().inspect,
            Loadable::Loaded("{}".into())
        );
    }

    #[test]
    fn inspect_result_for_other_container_is_ignored() {
        let mut app = app_with_detail();
        app.update(Msg::Inspected {
            id: "zzz".into(),
            result: Ok("{}".into()),
        });
        assert_eq!(app.detail.as_ref().unwrap().inspect, Loadable::Loading);
    }

    #[test]
    fn opening_running_container_streams_logs_and_stats() {
        let mut app = App::new();
        app.update(Msg::ContainersLoaded(Ok(vec![container(
            "c1",
            "web",
            ContainerState::Running,
        )])));
        assert_eq!(
            app.update(key(KeyCode::Enter)),
            vec![
                Command::InspectContainer("c1".into()),
                Command::StreamLogs("c1".into()),
                Command::StreamStats("c1".into()),
            ]
        );
    }

    #[test]
    fn opening_stopped_container_skips_stats() {
        let mut app = App::new();
        app.update(Msg::ContainersLoaded(Ok(vec![container(
            "c1",
            "web",
            ContainerState::Exited,
        )])));
        assert_eq!(
            app.update(key(KeyCode::Enter)),
            vec![
                Command::InspectContainer("c1".into()),
                Command::StreamLogs("c1".into())
            ]
        );
    }

    #[test]
    fn esc_closes_detail_and_stops_streams() {
        let mut app = app_with_detail();
        assert_eq!(app.update(key(KeyCode::Esc)), vec![Command::StopStreams]);
        assert!(app.detail.is_none());
    }

    #[test]
    fn h_l_switch_detail_tabs() {
        let mut app = app_with_detail();
        let tab = |app: &App| app.detail.as_ref().unwrap().tab;
        assert_eq!(tab(&app), DetailTab::Info);
        app.update(key(KeyCode::Char('l')));
        assert_eq!(tab(&app), DetailTab::Logs);
        app.update(key(KeyCode::Right));
        assert_eq!(tab(&app), DetailTab::Stats);
        app.update(key(KeyCode::Char('l')));
        assert_eq!(tab(&app), DetailTab::Files);
        app.update(key(KeyCode::Char('l')));
        assert_eq!(tab(&app), DetailTab::Info);
        app.update(key(KeyCode::Char('h')));
        assert_eq!(tab(&app), DetailTab::Files);
    }

    #[test]
    fn entering_files_tab_lists_root() {
        let mut app = app_with_detail();
        assert_eq!(
            app.update(key(KeyCode::Left)),
            vec![Command::ListDir {
                id: "c1".into(),
                path: "/".into()
            }]
        );
    }

    #[test]
    fn files_tab_routes_keys_and_results() {
        use crate::{
            app::files::tests::{entry, listing},
            docker::fs::FileKind,
        };
        let mut app = app_with_detail();
        app.update(key(KeyCode::Left));
        app.update(Msg::DirListed {
            id: "c1".into(),
            path: "/".into(),
            result: Ok(listing(vec![entry("etc", FileKind::Dir)])),
        });
        assert_eq!(
            app.update(key(KeyCode::Enter)),
            vec![Command::ListDir {
                id: "c1".into(),
                path: "/etc".into()
            }]
        );
        // Backspace goes up a directory instead of closing the view
        app.update(key(KeyCode::Backspace));
        assert!(app.detail.is_some());
        // Esc without overlay still closes the detail view
        assert_eq!(app.update(key(KeyCode::Esc)), vec![Command::StopStreams]);
    }

    #[test]
    fn log_chunks_for_open_container_are_buffered() {
        let mut app = app_with_detail();
        app.update(Msg::LogChunk {
            id: "c1".into(),
            text: "hello\n".into(),
        });
        app.update(Msg::LogChunk {
            id: "other".into(),
            text: "nope\n".into(),
        });
        assert_eq!(app.detail.as_ref().unwrap().logs.lines, ["hello"]);
    }

    #[test]
    fn log_stream_error_is_shown_inline() {
        let mut app = app_with_detail();
        app.update(Msg::LogsEnded {
            id: "c1".into(),
            error: Some("gone".into()),
        });
        let logs = &app.detail.as_ref().unwrap().logs;
        assert!(logs.lines.last().unwrap().contains("gone"));
    }

    #[test]
    fn logs_tab_scrolls_and_follows() {
        let mut app = app_with_detail();
        app.update(key(KeyCode::Char('l')));
        app.update(Msg::LogChunk {
            id: "c1".into(),
            text: "a\nb\nc\n".into(),
        });
        app.update(key(KeyCode::Char('k')));
        assert_eq!(app.detail.as_ref().unwrap().logs.scroll_from_bottom, 1);
        app.update(key(KeyCode::Char('G')));
        assert!(app.detail.as_ref().unwrap().logs.following());
    }

    #[test]
    fn stats_samples_are_kept_with_bounded_history() {
        let mut app = app_with_detail();
        for i in 0..STATS_HISTORY + 5 {
            let sample = StatsSample {
                cpu_percent: i as f64,
                ..Default::default()
            };
            app.update(Msg::Stats {
                id: "c1".into(),
                sample,
            });
        }
        let stats = &app.detail.as_ref().unwrap().stats;
        assert_eq!(stats.len(), STATS_HISTORY);
        assert_eq!(
            stats.back().unwrap().cpu_percent,
            (STATS_HISTORY + 4) as f64
        );
    }

    #[test]
    fn j_k_scroll_detail() {
        let mut app = app_with_detail();
        app.update(key(KeyCode::Char('j')));
        app.update(key(KeyCode::Char('j')));
        assert_eq!(app.detail.as_ref().unwrap().scroll, 2);
        app.update(key(KeyCode::Char('k')));
        app.update(key(KeyCode::Char('k')));
        app.update(key(KeyCode::Char('k')));
        assert_eq!(app.detail.as_ref().unwrap().scroll, 0);
    }

    #[test]
    fn refreshed_list_updates_open_container() {
        let mut app = app_with_detail();
        let mut updated = container("c1", "web", ContainerState::Exited);
        updated.status = "Exited (0) 1 second ago".into();
        app.update(Msg::ContainersLoaded(Ok(vec![updated])));
        assert_eq!(
            app.detail.as_ref().unwrap().container.state,
            ContainerState::Exited
        );
        assert_eq!(
            app.detail.as_ref().unwrap().container.status,
            "Exited (0) 1 second ago"
        );
    }

    #[test]
    fn detail_does_not_swallow_quit() {
        let mut app = app_with_detail();
        app.update(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }
}
