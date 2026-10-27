//! Executes [`Command`]s on background tasks and reports back via [`Msg`]s.

use std::sync::{Arc, Mutex};

use futures::{StreamExt, stream::BoxStream};
use tokio::{
    io::AsyncWriteExt,
    sync::mpsc::{self, UnboundedSender},
};
use tokio_util::sync::CancellationToken;

use crate::{
    docker::DockerApi,
    hub::HubApi,
    msg::{Command, Msg},
};

pub struct Runtime {
    docker: Arc<dyn DockerApi>,
    hub: Arc<dyn HubApi>,
    tx: UnboundedSender<Msg>,
    /// Cancels the streams belonging to the current detail view.
    streams: CancellationToken,
    /// Keystrokes for the embedded shell; dropping it closes the shell's stdin.
    terminal_input: Option<UnboundedSender<Vec<u8>>>,
    /// Exec id of the embedded shell, once attached.
    terminal_exec: Arc<Mutex<Option<String>>>,
}

impl Runtime {
    pub fn new(docker: Arc<dyn DockerApi>, hub: Arc<dyn HubApi>, tx: UnboundedSender<Msg>) -> Self {
        Self {
            docker,
            hub,
            tx,
            streams: CancellationToken::new(),
            terminal_input: None,
            terminal_exec: Arc::default(),
        }
    }

    pub fn execute(&mut self, command: Command) {
        let docker = self.docker.clone();
        match command {
            Command::LoadContainers => {
                self.spawn(async move { Msg::ContainersLoaded(docker.list_containers().await) })
            }
            Command::LoadImages => {
                self.spawn(async move { Msg::ImagesLoaded(docker.list_images().await) })
            }
            Command::InspectContainer(id) => self.spawn(async move {
                let result = docker.inspect_container(&id).await;
                Msg::Inspected { id, result }
            }),
            Command::Container { id, name, action } => self.spawn(async move {
                let result = docker.container_action(&id, action).await;
                Msg::ActionDone {
                    label: format!("{} {name}", action.done_label()),
                    result,
                }
            }),
            Command::RemoveImage { id, name } => self.spawn(async move {
                let result = docker.remove_image(&id).await;
                Msg::ActionDone {
                    label: format!("Removed image {name}"),
                    result,
                }
            }),
            Command::StreamLogs(id) => {
                let stream = docker.logs(&id);
                let end_id = id.clone();
                self.forward(
                    stream,
                    self.streams.clone(),
                    move |text| Msg::LogChunk {
                        id: id.clone(),
                        text,
                    },
                    move |error| Some(Msg::LogsEnded { id: end_id, error }),
                );
            }
            Command::StreamStats(id) => {
                let stream = docker.stats(&id);
                self.forward(
                    stream,
                    self.streams.clone(),
                    move |sample| Msg::Stats {
                        id: id.clone(),
                        sample,
                    },
                    |_| None,
                );
            }
            Command::HubSearch { query, page } => {
                let hub = self.hub.clone();
                self.spawn(async move {
                    let result = hub.search(&query, page).await;
                    Msg::HubSearched {
                        query,
                        page,
                        result,
                    }
                })
            }
            Command::HubTags { repo, page } => {
                let hub = self.hub.clone();
                self.spawn(async move {
                    let result = hub.tags(&repo, page).await;
                    Msg::HubTagsLoaded {
                        repo: repo.name,
                        page,
                        result,
                    }
                })
            }
            Command::Pull(image) => {
                let stream = docker.pull(&image);
                let end_image = image.clone();
                // Pulls outlive the detail view, so they get their own token.
                self.forward(
                    stream,
                    CancellationToken::new(),
                    move |event| Msg::PullProgress {
                        image: image.clone(),
                        event,
                    },
                    move |error| {
                        let result = error.map_or(Ok(()), Err);
                        Some(Msg::PullDone {
                            image: end_image,
                            result,
                        })
                    },
                );
            }
            Command::ListDir { id, path } => self.spawn(async move {
                let result = docker.list_dir(&id, &path).await;
                Msg::DirListed { id, path, result }
            }),
            Command::ReadFile { id, path } => self.spawn(async move {
                let result = docker.read_file(&id, &path).await;
                Msg::FileRead { id, path, result }
            }),
            Command::Diff(id) => self.spawn(async move {
                let result = docker.diff(&id).await;
                Msg::DiffLoaded { id, result }
            }),
            Command::DetectShell(id) => self.spawn(async move {
                let result = docker.detect_shell(&id).await;
                Msg::ShellDetected { id, result }
            }),
            Command::Exec { id, command } => self.spawn(async move {
                let result = docker.exec(&id, &command).await;
                Msg::ExecDone { command, result }
            }),
            Command::StartTerminal {
                id,
                shell,
                rows,
                cols,
            } => self.start_terminal(id, shell, rows, cols),
            Command::TerminalInput(bytes) => {
                if let Some(input) = &self.terminal_input {
                    let _ = input.send(bytes);
                }
            }
            Command::TerminalResize { rows, cols } => {
                let exec_id = self.terminal_exec.lock().unwrap().clone();
                if let Some(exec_id) = exec_id {
                    tokio::spawn(async move {
                        if let Err(e) = docker.resize_exec(&exec_id, rows, cols).await {
                            tracing::warn!("resizing exec {exec_id}: {e}");
                        }
                    });
                }
            }
            Command::RunShell { .. } => {
                tracing::warn!("RunShell must be handled by the main loop");
            }
            Command::StopStreams => {
                self.streams.cancel();
                self.streams = CancellationToken::new();
            }
        }
    }

    /// Forward `stream` items as messages until it ends, fails, or `token`
    /// is cancelled. `on_end` gets the error, if any.
    fn forward<T: Send + 'static>(
        &self,
        mut stream: BoxStream<'static, Result<T, String>>,
        token: CancellationToken,
        on_item: impl Fn(T) -> Msg + Send + 'static,
        on_end: impl FnOnce(Option<String>) -> Option<Msg> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let error = loop {
                let item = tokio::select! {
                    _ = token.cancelled() => return,
                    item = stream.next() => item,
                };
                match item {
                    Some(Ok(v)) => {
                        if tx.send(on_item(v)).is_err() {
                            return;
                        }
                    }
                    Some(Err(e)) => break Some(e),
                    None => break None,
                }
            };
            if let Some(msg) = on_end(error) {
                let _ = tx.send(msg);
            }
        });
    }

    fn start_terminal(&mut self, id: String, shell: String, rows: u16, cols: u16) {
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        // Replacing the sender closes any previous session's stdin.
        self.terminal_input = Some(input_tx);
        let docker = self.docker.clone();
        let exec_slot = self.terminal_exec.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let session = match docker.attach_shell(&id, &shell, rows, cols).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Msg::TerminalClosed(Some(e)));
                    return;
                }
            };
            *exec_slot.lock().unwrap() = Some(session.exec_id.clone());

            let mut input = session.input;
            tokio::spawn(async move {
                while let Some(bytes) = input_rx.recv().await {
                    if input.write_all(&bytes).await.is_err() || input.flush().await.is_err() {
                        break;
                    }
                }
                let _ = input.shutdown().await;
            });

            let mut output = session.output;
            let error = loop {
                match output.next().await {
                    Some(Ok(bytes)) => {
                        if tx.send(Msg::TerminalOutput(bytes)).is_err() {
                            return;
                        }
                    }
                    Some(Err(e)) => break Some(e),
                    None => break None,
                }
            };
            let _ = tx.send(Msg::TerminalClosed(error));
        });
    }

    /// Run `fut` in the background and post its message.
    fn spawn(&self, fut: impl Future<Output = Msg> + Send + 'static) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(fut.await);
        });
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use async_trait::async_trait;
    use futures::stream::{self, BoxStream, StreamExt};

    use crate::docker::{
        DirListing, DockerApi, Result,
        exec::{ExecOutput, ExecSession},
        fs::{ChangeKind, FileKind, FilePreview, FsChange, FsEntry},
        models::{Container, ContainerAction, ContainerState, Image},
        pull::PullEvent,
        stats::StatsSample,
    };

    pub fn container(id: &str, name: &str, state: ContainerState) -> Container {
        Container {
            id: id.into(),
            name: name.into(),
            image: "alpine:3.20".into(),
            state,
            status: match state {
                ContainerState::Running => "Up 2 hours".into(),
                _ => "Exited (0) 3 days ago".into(),
            },
            ports: String::new(),
            created: 0,
        }
    }

    pub fn image(id: &str, tag: &str) -> Image {
        Image {
            id: id.into(),
            repo_tags: vec![tag.into()],
            size: 7_800_000,
            created: 0,
            containers: 0,
        }
    }

    #[derive(Default)]
    pub struct MockDocker {
        pub containers: Vec<Container>,
        pub images: Vec<Image>,
        pub fail: bool,
        pub log_chunks: Vec<String>,
        pub stats: Vec<StatsSample>,
        /// When set, streams never end; the flag is raised once they are dropped.
        pub endless_streams: Option<Arc<AtomicBool>>,
        pub calls: Mutex<Vec<String>>,
        pub pull_events: Vec<PullEvent>,
        pub terminal_output: Vec<Vec<u8>>,
        /// Keeps the session open after its output until the input closes.
        pub terminal_waits_for_input: bool,
        /// The far end of the last session's stdin.
        pub terminal_peer: Mutex<Option<tokio::io::DuplexStream>>,
    }

    /// Raises its flag when dropped, i.e. when the owning stream is cancelled.
    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    impl MockDocker {
        fn stream<T: Send + 'static>(&self, items: Vec<T>) -> BoxStream<'static, Result<T>> {
            let items = stream::iter(items.into_iter().map(Ok));
            match &self.endless_streams {
                Some(flag) => {
                    let guard = DropFlag(flag.clone());
                    items
                        .chain(stream::pending())
                        .map(move |x| {
                            let _ = &guard;
                            x
                        })
                        .boxed()
                }
                None => items.boxed(),
            }
        }

        fn record(&self, call: String) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl MockDocker {
        fn check(&self) -> Result<()> {
            if self.fail {
                Err("boom".into())
            } else {
                Ok(())
            }
        }
    }

    #[async_trait]
    impl DockerApi for MockDocker {
        async fn list_containers(&self) -> Result<Vec<Container>> {
            self.check()?;
            Ok(self.containers.clone())
        }

        async fn list_images(&self) -> Result<Vec<Image>> {
            self.check()?;
            Ok(self.images.clone())
        }

        async fn inspect_container(&self, id: &str) -> Result<String> {
            self.check()?;
            Ok(format!("{{\"Id\": \"{id}\"}}"))
        }

        async fn container_action(&self, id: &str, action: ContainerAction) -> Result<()> {
            self.check()?;
            self.record(format!("{action:?} {id}"));
            Ok(())
        }

        async fn remove_image(&self, id: &str) -> Result<()> {
            self.check()?;
            self.record(format!("rmi {id}"));
            Ok(())
        }

        fn logs(&self, _id: &str) -> BoxStream<'static, Result<String>> {
            self.stream(self.log_chunks.clone())
        }

        fn stats(&self, _id: &str) -> BoxStream<'static, Result<StatsSample>> {
            self.stream(self.stats.clone())
        }

        fn pull(&self, _image: &str) -> BoxStream<'static, Result<PullEvent>> {
            stream::iter(self.pull_events.clone().into_iter().map(Ok)).boxed()
        }

        async fn list_dir(&self, _id: &str, path: &str) -> Result<DirListing> {
            self.check()?;
            let entry = FsEntry {
                name: format!("in{}", path.replace('/', "_")),
                kind: FileKind::File,
                size: 1,
                mode: "-rw-r--r--".into(),
                modified: String::new(),
                link_target: None,
            };
            Ok(DirListing {
                entries: vec![entry],
                ..Default::default()
            })
        }

        async fn read_file(&self, _id: &str, path: &str) -> Result<FilePreview> {
            self.check()?;
            Ok(FilePreview {
                content: format!("contents of {path}"),
                binary: false,
                size: 1,
                truncated: false,
            })
        }

        async fn diff(&self, _id: &str) -> Result<Vec<FsChange>> {
            self.check()?;
            Ok(vec![FsChange {
                path: "/tmp/x".into(),
                kind: ChangeKind::Added,
            }])
        }

        async fn detect_shell(&self, _id: &str) -> Result<String> {
            self.check()?;
            Ok("/bin/sh".into())
        }

        async fn exec(&self, _id: &str, command: &str) -> Result<ExecOutput> {
            self.check()?;
            Ok(ExecOutput {
                output: format!("ran {command}"),
                exit_code: Some(0),
            })
        }

        async fn attach_shell(
            &self,
            id: &str,
            shell: &str,
            rows: u16,
            cols: u16,
        ) -> Result<ExecSession> {
            self.check()?;
            self.record(format!("attach {id} {shell} {rows}x{cols}"));
            let (input, peer) = tokio::io::duplex(1024);
            let output = stream::iter(self.terminal_output.clone().into_iter().map(Ok));
            let output = if self.terminal_waits_for_input {
                output.chain(stream::pending()).boxed()
            } else {
                output.boxed()
            };
            *self.terminal_peer.lock().unwrap() = Some(peer);
            Ok(ExecSession {
                exec_id: "exec1".into(),
                output,
                input: Box::pin(input),
            })
        }

        async fn resize_exec(&self, exec_id: &str, rows: u16, cols: u16) -> Result<()> {
            self.record(format!("resize {exec_id} {rows}x{cols}"));
            Ok(())
        }
    }
}

#[cfg(test)]
pub(crate) mod mock_hub {
    use async_trait::async_trait;

    use crate::hub::{
        HubApi,
        models::{Repo, SearchPage, Tag, TagPage},
    };

    #[derive(Default)]
    pub struct MockHub;

    #[async_trait]
    impl HubApi for MockHub {
        async fn search(&self, query: &str, page: u32) -> Result<SearchPage, String> {
            if query == "fail" {
                return Err("rate limit".into());
            }
            let repo = Repo {
                name: format!("{query}-{page}"),
                description: String::new(),
                stars: 0,
                pulls: 0,
                official: true,
            };
            Ok(SearchPage {
                count: 1,
                results: vec![repo],
                has_next: false,
            })
        }

        async fn tags(&self, repo: &Repo, _page: u32) -> Result<TagPage, String> {
            let tag = Tag {
                name: format!("{}-tag", repo.name),
                size: 0,
                last_updated: None,
                architectures: vec![],
            };
            Ok(TagPage {
                count: 1,
                results: vec![tag],
                has_next: false,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;

    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{mock::*, *};
    use crate::docker::{
        models::{ContainerAction, ContainerState},
        pull::PullEvent,
        stats::StatsSample,
    };

    fn runtime(mock: MockDocker) -> (Runtime, mpsc::UnboundedReceiver<Msg>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Runtime::new(Arc::new(mock), Arc::new(super::mock_hub::MockHub), tx),
            rx,
        )
    }

    #[tokio::test]
    async fn hub_search_posts_page() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::HubSearch {
            query: "nginx".into(),
            page: 2,
        });
        match rx.recv().await.unwrap() {
            Msg::HubSearched {
                query,
                page,
                result: Ok(p),
            } => {
                assert_eq!((query.as_str(), page), ("nginx", 2));
                assert_eq!(p.results[0].name, "nginx-2");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn hub_search_error_is_posted() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::HubSearch {
            query: "fail".into(),
            page: 1,
        });
        assert!(matches!(
            rx.recv().await.unwrap(),
            Msg::HubSearched { result: Err(_), .. }
        ));
    }

    #[tokio::test]
    async fn hub_tags_posts_repo_name() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        let repo = crate::hub::models::Repo {
            name: "nginx".into(),
            description: String::new(),
            stars: 0,
            pulls: 0,
            official: true,
        };
        rt.execute(Command::HubTags { repo, page: 1 });
        match rx.recv().await.unwrap() {
            Msg::HubTagsLoaded {
                repo,
                page: 1,
                result: Ok(p),
            } => {
                assert_eq!(repo, "nginx");
                assert_eq!(p.results[0].name, "nginx-tag");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn pull_posts_progress_then_done_and_survives_stop_streams() {
        let event = PullEvent {
            id: Some("l1".into()),
            status: "Downloading".into(),
            current: Some(1),
            total: Some(2),
        };
        let (mut rt, mut rx) = runtime(MockDocker {
            pull_events: vec![event.clone()],
            ..Default::default()
        });
        rt.execute(Command::Pull("nginx:latest".into()));
        rt.execute(Command::StopStreams);
        assert!(
            matches!(rx.recv().await.unwrap(), Msg::PullProgress { image, event: e } if image == "nginx:latest" && e == event)
        );
        assert!(matches!(
            rx.recv().await.unwrap(),
            Msg::PullDone { result: Ok(()), .. }
        ));
    }

    #[tokio::test]
    async fn load_containers_posts_result() {
        let (mut rt, mut rx) = runtime(MockDocker {
            containers: vec![container("c1", "web", ContainerState::Running)],
            ..Default::default()
        });
        rt.execute(Command::LoadContainers);
        match rx.recv().await.unwrap() {
            Msg::ContainersLoaded(Ok(list)) => assert_eq!(list[0].name, "web"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn load_images_posts_result() {
        let (mut rt, mut rx) = runtime(MockDocker {
            images: vec![image("i1", "alpine:3.20")],
            ..Default::default()
        });
        rt.execute(Command::LoadImages);
        assert!(matches!(rx.recv().await.unwrap(), Msg::ImagesLoaded(Ok(l)) if l.len() == 1));
    }

    #[tokio::test]
    async fn errors_are_posted_as_strings() {
        let (mut rt, mut rx) = runtime(MockDocker {
            fail: true,
            ..Default::default()
        });
        rt.execute(Command::LoadContainers);
        assert!(matches!(rx.recv().await.unwrap(), Msg::ContainersLoaded(Err(e)) if e == "boom"));
    }

    #[tokio::test]
    async fn container_action_reports_label() {
        let mock = Arc::new(MockDocker::default());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut rt = Runtime::new(mock.clone(), Arc::new(super::mock_hub::MockHub), tx);
        rt.execute(Command::Container {
            id: "c1".into(),
            name: "web".into(),
            action: ContainerAction::Stop,
        });
        match rx.recv().await.unwrap() {
            Msg::ActionDone { label, result } => {
                assert_eq!(label, "Stopped web");
                assert!(result.is_ok());
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(*mock.calls.lock().unwrap(), ["Stop c1"]);
    }

    #[tokio::test]
    async fn remove_image_reports_label() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::RemoveImage {
            id: "i1".into(),
            name: "nginx:latest".into(),
        });
        assert!(matches!(
            rx.recv().await.unwrap(),
            Msg::ActionDone { label, result: Ok(()) } if label == "Removed image nginx:latest"
        ));
    }

    #[tokio::test]
    async fn log_stream_posts_chunks_then_end() {
        let (mut rt, mut rx) = runtime(MockDocker {
            log_chunks: vec!["a\n".into(), "b\n".into()],
            ..Default::default()
        });
        rt.execute(Command::StreamLogs("c1".into()));
        assert!(matches!(rx.recv().await.unwrap(), Msg::LogChunk { text, .. } if text == "a\n"));
        assert!(matches!(rx.recv().await.unwrap(), Msg::LogChunk { text, .. } if text == "b\n"));
        assert!(matches!(
            rx.recv().await.unwrap(),
            Msg::LogsEnded { error: None, .. }
        ));
    }

    #[tokio::test]
    async fn stats_stream_posts_samples() {
        let sample = StatsSample {
            cpu_percent: 12.5,
            ..Default::default()
        };
        let (mut rt, mut rx) = runtime(MockDocker {
            stats: vec![sample],
            ..Default::default()
        });
        rt.execute(Command::StreamStats("c1".into()));
        assert!(
            matches!(rx.recv().await.unwrap(), Msg::Stats { id, sample: s } if id == "c1" && s == sample)
        );
    }

    #[tokio::test]
    async fn stop_streams_cancels_running_streams() {
        let dropped = Arc::new(AtomicBool::new(false));
        let (mut rt, mut rx) = runtime(MockDocker {
            log_chunks: vec!["a\n".into()],
            endless_streams: Some(dropped.clone()),
            ..Default::default()
        });
        rt.execute(Command::StreamLogs("c1".into()));
        rx.recv().await.unwrap();
        rt.execute(Command::StopStreams);
        for _ in 0..100 {
            if dropped.load(Ordering::SeqCst) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("stream was not cancelled");
    }

    #[tokio::test]
    async fn list_dir_posts_listing_for_path() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::ListDir {
            id: "c1".into(),
            path: "/etc".into(),
        });
        match rx.recv().await.unwrap() {
            Msg::DirListed {
                id,
                path,
                result: Ok(l),
            } => {
                assert_eq!((id.as_str(), path.as_str()), ("c1", "/etc"));
                assert_eq!(l.entries[0].name, "in_etc");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_file_and_diff_post_results() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::ReadFile {
            id: "c1".into(),
            path: "/etc/hosts".into(),
        });
        assert!(
            matches!(rx.recv().await.unwrap(), Msg::FileRead { path, result: Ok(p), .. } if path == "/etc/hosts" && p.content.contains("hosts"))
        );
        rt.execute(Command::Diff("c1".into()));
        assert!(
            matches!(rx.recv().await.unwrap(), Msg::DiffLoaded { result: Ok(c), .. } if c.len() == 1)
        );
    }

    #[tokio::test]
    async fn detect_shell_and_exec_post_results() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::DetectShell("c1".into()));
        assert!(
            matches!(rx.recv().await.unwrap(), Msg::ShellDetected { result: Ok(s), .. } if s == "/bin/sh")
        );
        rt.execute(Command::Exec {
            id: "c1".into(),
            command: "ls".into(),
        });
        assert!(
            matches!(rx.recv().await.unwrap(), Msg::ExecDone { command, result: Ok(o) } if command == "ls" && o.output == "ran ls")
        );
    }

    #[tokio::test]
    async fn terminal_session_streams_output_then_closes() {
        let (mut rt, mut rx) = runtime(MockDocker {
            terminal_output: vec![b"hi".to_vec()],
            ..Default::default()
        });
        rt.execute(Command::StartTerminal {
            id: "c1".into(),
            shell: "/bin/sh".into(),
            rows: 24,
            cols: 80,
        });
        assert!(matches!(rx.recv().await.unwrap(), Msg::TerminalOutput(b) if b == b"hi"));
        assert!(matches!(
            rx.recv().await.unwrap(),
            Msg::TerminalClosed(None)
        ));
    }

    #[tokio::test]
    async fn terminal_input_and_resize_reach_the_session() {
        use tokio::io::AsyncReadExt;

        let mock = Arc::new(MockDocker {
            terminal_output: vec![b"$ ".to_vec()],
            terminal_waits_for_input: true,
            ..Default::default()
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut rt = Runtime::new(mock.clone(), Arc::new(super::mock_hub::MockHub), tx);
        rt.execute(Command::StartTerminal {
            id: "c1".into(),
            shell: "/bin/sh".into(),
            rows: 24,
            cols: 80,
        });
        rx.recv().await.unwrap(); // session is up once output flows

        rt.execute(Command::TerminalInput(b"ls\r".to_vec()));
        let mut peer = mock.terminal_peer.lock().unwrap().take().unwrap();
        let mut buf = [0u8; 3];
        tokio::time::timeout(std::time::Duration::from_secs(1), peer.read_exact(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf, b"ls\r");

        rt.execute(Command::TerminalResize {
            rows: 30,
            cols: 100,
        });
        for _ in 0..100 {
            if mock
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|c| c == "resize exec1 30x100")
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("resize not forwarded: {:?}", mock.calls.lock().unwrap());
    }

    #[tokio::test]
    async fn failed_attach_closes_with_error() {
        let (mut rt, mut rx) = runtime(MockDocker {
            fail: true,
            ..Default::default()
        });
        rt.execute(Command::StartTerminal {
            id: "c1".into(),
            shell: "/bin/sh".into(),
            rows: 24,
            cols: 80,
        });
        assert!(matches!(rx.recv().await.unwrap(), Msg::TerminalClosed(Some(e)) if e == "boom"));
    }

    #[tokio::test]
    async fn inspect_posts_id_and_json() {
        let (mut rt, mut rx) = runtime(MockDocker::default());
        rt.execute(Command::InspectContainer("abc".into()));
        match rx.recv().await.unwrap() {
            Msg::Inspected {
                id,
                result: Ok(json),
            } => {
                assert_eq!(id, "abc");
                assert!(json.contains("abc"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
