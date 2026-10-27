use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};

use docker_tui::docker::{
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
