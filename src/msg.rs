use crossterm::event::KeyEvent;

use crate::{
    docker::{
        DirListing,
        exec::ExecOutput,
        fs::{FilePreview, FsChange},
        models::{Container, ContainerAction, Image},
        pull::PullEvent,
        stats::StatsSample,
    },
    hub::models::{Repo, SearchPage, TagPage},
};

/// Everything that can happen to the application: terminal input, timer ticks
/// and results of background work.
#[derive(Debug, Clone)]
pub enum Msg {
    Key(KeyEvent),
    Tick,
    /// New terminal size in columns and rows.
    Resize(u16, u16),
    ContainersLoaded(Result<Vec<Container>, String>),
    ImagesLoaded(Result<Vec<Image>, String>),
    Inspected {
        id: String,
        result: Result<String, String>,
    },
    /// A container or image action finished; `label` describes it for the user.
    ActionDone {
        label: String,
        result: Result<(), String>,
    },
    LogChunk {
        id: String,
        text: String,
    },
    LogsEnded {
        id: String,
        error: Option<String>,
    },
    Stats {
        id: String,
        sample: StatsSample,
    },
    HubSearched {
        query: String,
        page: u32,
        result: Result<SearchPage, String>,
    },
    HubTagsLoaded {
        /// Repository name the tags belong to.
        repo: String,
        page: u32,
        result: Result<TagPage, String>,
    },
    PullProgress {
        image: String,
        event: PullEvent,
    },
    PullDone {
        image: String,
        result: Result<(), String>,
    },
    DirListed {
        id: String,
        path: String,
        result: Result<DirListing, String>,
    },
    FileRead {
        id: String,
        path: String,
        result: Result<FilePreview, String>,
    },
    DiffLoaded {
        id: String,
        result: Result<Vec<FsChange>, String>,
    },
    ShellDetected {
        id: String,
        result: Result<String, String>,
    },
    /// The interactive shell returned control to the TUI.
    ShellExited(Result<(), String>),
    ExecDone {
        command: String,
        result: Result<ExecOutput, String>,
    },
    TerminalOutput(Vec<u8>),
    /// The embedded shell session ended, with an error if it failed.
    TerminalClosed(Option<String>),
}

/// Side effects requested by [`crate::app::App::update`], executed by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    LoadContainers,
    LoadImages,
    InspectContainer(String),
    Container {
        id: String,
        name: String,
        action: ContainerAction,
    },
    RemoveImage {
        id: String,
        name: String,
    },
    /// Follow a container's logs until [`Command::StopStreams`].
    StreamLogs(String),
    /// Follow a container's resource usage until [`Command::StopStreams`].
    StreamStats(String),
    StopStreams,
    HubSearch {
        query: String,
        page: u32,
    },
    HubTags {
        repo: Repo,
        page: u32,
    },
    /// Pull an image reference such as `nginx:latest`; not cancelled by `StopStreams`.
    Pull(String),
    ListDir {
        id: String,
        path: String,
    },
    ReadFile {
        id: String,
        path: String,
    },
    Diff(String),
    DetectShell(String),
    /// Suspend the TUI and attach to an interactive shell. Handled by the
    /// main loop, which owns the terminal.
    RunShell {
        id: String,
        shell: String,
    },
    Exec {
        id: String,
        command: String,
    },
    /// Start an embedded shell session with a PTY of the given size.
    StartTerminal {
        id: String,
        shell: String,
        rows: u16,
        cols: u16,
    },
    TerminalInput(Vec<u8>),
    TerminalResize {
        rows: u16,
        cols: u16,
    },
}
