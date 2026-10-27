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
