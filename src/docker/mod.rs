pub mod exec;
pub mod fs;
pub mod models;
pub mod pull;
pub mod stats;

use async_trait::async_trait;
use bollard::{
    Docker,
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    query_parameters::{
        ContainerArchiveInfoOptionsBuilder, CreateImageOptionsBuilder,
        DownloadFromContainerOptionsBuilder, ListContainersOptionsBuilder,
        ListImagesOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
        RemoveImageOptionsBuilder, ResizeExecOptionsBuilder, RestartContainerOptions,
        StartContainerOptions, StatsOptionsBuilder, StopContainerOptions,
    },
};

use exec::{ExecOutput, ExecSession};
use fs::{FilePreview, FsChange, FsEntry};
use futures::{StreamExt, stream::BoxStream};
use models::{Container, ContainerAction, Image};
use pull::PullEvent;
use stats::StatsSample;

pub type Result<T> = std::result::Result<T, String>;

/// The subset of the Docker Engine API the app uses. Mocked in tests.
#[async_trait]
pub trait DockerApi: Send + Sync {
    async fn list_containers(&self) -> Result<Vec<Container>>;
    async fn list_images(&self) -> Result<Vec<Image>>;
    /// `docker inspect` output as pretty-printed JSON.
    async fn inspect_container(&self, id: &str) -> Result<String>;
    async fn container_action(&self, id: &str, action: ContainerAction) -> Result<()>;
    async fn remove_image(&self, id: &str) -> Result<()>;
    /// Follow stdout+stderr, starting with the last `log_tail` lines.
    fn logs(&self, id: &str) -> BoxStream<'static, Result<String>>;
    fn stats(&self, id: &str) -> BoxStream<'static, Result<StatsSample>>;
    /// Pull an image reference, streaming progress.
    fn pull(&self, image: &str) -> BoxStream<'static, Result<PullEvent>>;
    /// List a directory: `ls` via exec when possible, else the archive endpoint.
    async fn list_dir(&self, id: &str, path: &str) -> Result<DirListing>;
    async fn read_file(&self, id: &str, path: &str) -> Result<FilePreview>;
    /// `docker diff`: files changed relative to the image.
    async fn diff(&self, id: &str) -> Result<Vec<FsChange>>;
    /// The best interactive shell available in the container.
    async fn detect_shell(&self, id: &str) -> Result<String>;
    /// Run a command line non-interactively (via `sh -c` when available).
    async fn exec(&self, id: &str, command: &str) -> Result<ExecOutput>;
    /// Start `shell` attached to a PTY of `rows`×`cols`.
    async fn attach_shell(
        &self,
        id: &str,
        shell: &str,
        rows: u16,
        cols: u16,
    ) -> Result<ExecSession>;
    async fn resize_exec(&self, exec_id: &str, rows: u16, cols: u16) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirListing {
    pub entries: Vec<FsEntry>,
    /// Listed from the archive endpoint because `ls` could not be used.
    pub via_archive: bool,
    /// The archive was too large to read fully, so entries may be missing.
    pub partial: bool,
}

/// Split `name[:tag]` into name and tag, defaulting to `latest` so that a
/// pull never fetches every tag of a repository.
pub fn split_image_ref(image: &str) -> (&str, &str) {
    // The tag parameter also accepts a digest.
    if let Some((name, digest)) = image.split_once('@') {
        return (name, digest);
    }
    let last_segment = image.rsplit('/').next().unwrap_or(image);
    match last_segment.rsplit_once(':') {
        Some((_, tag)) => (&image[..image.len() - tag.len() - 1], tag),
        None => (image, "latest"),
    }
}

pub struct BollardDocker {
    docker: Docker,
    /// Shell tried before the built-in candidates.
    preferred_shell: Option<String>,
    log_tail: u32,
}

impl BollardDocker {
    pub fn connect(host: Option<&str>) -> Result<Self> {
        let docker = match host {
            Some(h) if h.starts_with("unix://") || h.starts_with('/') => {
                Docker::connect_with_unix(h, 120, bollard::API_DEFAULT_VERSION)
            }
            Some(h) => Docker::connect_with_http(h, 120, bollard::API_DEFAULT_VERSION),
            None => Docker::connect_with_defaults(),
        }
        .map_err(|e| e.to_string())?;
        Ok(Self {
            docker,
            preferred_shell: None,
            log_tail: 500,
        })
    }

    pub fn with_shell(mut self, shell: Option<String>) -> Self {
        self.preferred_shell = shell;
        self
    }

    pub fn with_log_tail(mut self, lines: u32) -> Self {
        self.log_tail = lines;
        self
    }

    pub async fn ping(&self) -> Result<()> {
        self.docker
            .ping()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn is_running(&self, id: &str) -> Result<bool> {
        let info = self
            .docker
            .inspect_container(id, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(info.state.and_then(|s| s.running).unwrap_or(false))
    }

    /// Run `cmd` and collect its output (capped) and exit code.
    async fn exec_capture(&self, id: &str, cmd: &[&str]) -> Result<ExecOutput> {
        let config = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };
        let exec = self
            .docker
            .create_exec(id, config)
            .await
            .map_err(|e| e.to_string())?;
        let mut output = Vec::new();
        if let StartExecResults::Attached {
            output: mut stream, ..
        } = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| e.to_string())?
        {
            while let Some(chunk) = stream.next().await {
                output.extend_from_slice(chunk.map_err(|e| e.to_string())?.as_ref());
                if output.len() > EXEC_OUTPUT_CAP {
                    output.truncate(EXEC_OUTPUT_CAP);
                    break;
                }
            }
        }
        let inspect = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(ExecOutput {
            output: String::from_utf8_lossy(&output).into_owned(),
            exit_code: inspect.exit_code,
        })
    }

    /// Download `path` as a tar archive, stopping after `cap` bytes.
    /// Returns the bytes and whether the download was cut short.
    async fn download(&self, id: &str, path: &str, cap: usize) -> Result<(Vec<u8>, bool)> {
        let opts = DownloadFromContainerOptionsBuilder::default()
            .path(path)
            .build();
        let mut stream = self.docker.download_from_container(id, Some(opts));
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
            if bytes.len() >= cap {
                return Ok((bytes, true));
            }
        }
        Ok((bytes, false))
    }
}

/// Largest archive read when listing a directory without `ls`.
const LIST_ARCHIVE_CAP: usize = 16 * 1024 * 1024;
/// Bytes of a file shown in the preview.
pub const PREVIEW_BYTES: usize = 256 * 1024;
const EXEC_OUTPUT_CAP: usize = 1024 * 1024;

#[async_trait]
impl DockerApi for BollardDocker {
    async fn list_containers(&self) -> Result<Vec<Container>> {
        let opts = ListContainersOptionsBuilder::default().all(true).build();
        let mut list: Vec<Container> = self
            .docker
            .list_containers(Some(opts))
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(Container::from)
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.created));
        Ok(list)
    }

    async fn list_images(&self) -> Result<Vec<Image>> {
        let opts = ListImagesOptionsBuilder::default().build();
        let mut list: Vec<Image> = self
            .docker
            .list_images(Some(opts))
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(Image::from)
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.created));
        Ok(list)
    }

    async fn inspect_container(&self, id: &str) -> Result<String> {
        let info = self
            .docker
            .inspect_container(id, None)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&info).map_err(|e| e.to_string())
    }

    async fn container_action(&self, id: &str, action: ContainerAction) -> Result<()> {
        let d = &self.docker;
        match action {
            ContainerAction::Start => d.start_container(id, None::<StartContainerOptions>).await,
            ContainerAction::Stop => d.stop_container(id, None::<StopContainerOptions>).await,
            ContainerAction::Restart => {
                d.restart_container(id, None::<RestartContainerOptions>)
                    .await
            }
            ContainerAction::Remove => {
                let opts = RemoveContainerOptionsBuilder::default().force(true).build();
                d.remove_container(id, Some(opts)).await
            }
        }
        .map_err(|e| e.to_string())
    }

    async fn remove_image(&self, id: &str) -> Result<()> {
        let opts = RemoveImageOptionsBuilder::default().build();
        self.docker
            .remove_image(id, Some(opts), None)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn logs(&self, id: &str) -> BoxStream<'static, Result<String>> {
        let opts = LogsOptionsBuilder::default()
            .follow(true)
            .stdout(true)
            .stderr(true)
            .tail(&self.log_tail.to_string())
            .build();
        self.docker
            .logs(id, Some(opts))
            .map(|r| r.map(|out| out.to_string()).map_err(|e| e.to_string()))
            .boxed()
    }

    fn stats(&self, id: &str) -> BoxStream<'static, Result<StatsSample>> {
        let opts = StatsOptionsBuilder::default().stream(true).build();
        self.docker
            .stats(id, Some(opts))
            .map(|r| r.map(|s| stats::compute(&s)).map_err(|e| e.to_string()))
            .boxed()
    }

    fn pull(&self, image: &str) -> BoxStream<'static, Result<PullEvent>> {
        let (name, tag) = split_image_ref(image);
        let opts = CreateImageOptionsBuilder::default()
            .from_image(name)
            .tag(tag)
            .build();
        self.docker
            .create_image(Some(opts), None, None)
            .map(|r| r.map(PullEvent::from).map_err(|e| e.to_string()))
            .boxed()
    }

    async fn list_dir(&self, id: &str, path: &str) -> Result<DirListing> {
        if self.is_running(id).await? {
            // Trailing slash makes `ls` follow symlinked directories.
            let target = if path.ends_with('/') {
                path.to_string()
            } else {
                format!("{path}/")
            };
            let out = self.exec_capture(id, &["ls", "-la", &target]).await?;
            if out.exit_code == Some(0) {
                return Ok(DirListing {
                    entries: fs::parse_ls(&out.output),
                    ..Default::default()
                });
            }
            if !exec::is_missing_executable(&out) {
                return Err(out.output.trim().to_string());
            }
        }
        let (bytes, partial) = self.download(id, path, LIST_ARCHIVE_CAP).await?;
        Ok(DirListing {
            entries: fs::list_tar(&bytes)?,
            via_archive: true,
            partial,
        })
    }

    async fn read_file(&self, id: &str, path: &str) -> Result<FilePreview> {
        // The tar header plus the previewed bytes; the rest is not needed.
        let (bytes, _) = self.download(id, path, PREVIEW_BYTES + 64 * 1024).await?;
        fs::preview_tar(&bytes, PREVIEW_BYTES)
    }

    async fn diff(&self, id: &str) -> Result<Vec<FsChange>> {
        let changes = self
            .docker
            .container_changes(id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(changes
            .unwrap_or_default()
            .into_iter()
            .map(FsChange::from)
            .collect())
    }

    async fn detect_shell(&self, id: &str) -> Result<String> {
        // Stat through the archive endpoint: works even without `test` or `which`.
        let preferred = self.preferred_shell.as_deref().into_iter();
        for candidate in preferred.chain(exec::SHELL_CANDIDATES.iter().copied()) {
            let opts = ContainerArchiveInfoOptionsBuilder::default()
                .path(candidate)
                .build();
            if self
                .docker
                .get_container_archive_info(id, Some(opts))
                .await
                .is_ok()
            {
                return Ok(candidate.to_string());
            }
        }
        Err("no shell found in container".into())
    }

    async fn exec(&self, id: &str, command: &str) -> Result<ExecOutput> {
        let out = self.exec_capture(id, &["sh", "-c", command]).await?;
        if !exec::is_missing_executable(&out) {
            return Ok(out);
        }
        // No shell: run the words directly.
        let words: Vec<&str> = command.split_whitespace().collect();
        self.exec_capture(id, &words).await
    }

    async fn attach_shell(
        &self,
        id: &str,
        shell: &str,
        rows: u16,
        cols: u16,
    ) -> Result<ExecSession> {
        let config = CreateExecOptions {
            cmd: Some(vec![shell.to_string()]),
            env: Some(vec!["TERM=xterm-256color".to_string()]),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true),
            ..Default::default()
        };
        let exec = self
            .docker
            .create_exec(id, config)
            .await
            .map_err(|e| e.to_string())?;
        let opts = StartExecOptions {
            detach: false,
            tty: true,
            output_capacity: None,
        };
        match self
            .docker
            .start_exec(&exec.id, Some(opts))
            .await
            .map_err(|e| e.to_string())?
        {
            StartExecResults::Attached { output, input } => {
                // The PTY exists once the exec has started.
                self.resize_exec(&exec.id, rows, cols).await?;
                let output = output
                    .map(|r| {
                        r.map(|o| o.into_bytes().to_vec())
                            .map_err(|e| e.to_string())
                    })
                    .boxed();
                Ok(ExecSession {
                    exec_id: exec.id,
                    output,
                    input,
                })
            }
            StartExecResults::Detached => Err("exec started detached".into()),
        }
    }

    async fn resize_exec(&self, exec_id: &str, rows: u16, cols: u16) -> Result<()> {
        let opts = ResizeExecOptionsBuilder::default()
            .h(rows.into())
            .w(cols.into())
            .build();
        self.docker
            .resize_exec(exec_id, opts)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::split_image_ref;

    #[test]
    fn splits_image_refs() {
        assert_eq!(split_image_ref("nginx:1.27"), ("nginx", "1.27"));
        assert_eq!(split_image_ref("nginx"), ("nginx", "latest"));
        assert_eq!(
            split_image_ref("localhost:5000/app"),
            ("localhost:5000/app", "latest")
        );
        assert_eq!(
            split_image_ref("localhost:5000/app:v2"),
            ("localhost:5000/app", "v2")
        );
        assert_eq!(split_image_ref("nginx@sha256:abc"), ("nginx", "sha256:abc"));
    }
}
