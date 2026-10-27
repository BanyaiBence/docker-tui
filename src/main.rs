use std::{collections::VecDeque, sync::Arc, time::Duration};

use clap::Parser;
use color_eyre::{Result, eyre::eyre};
use crossterm::{
    event::EventStream,
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use docker_tui::{
    app::App,
    config::Config,
    docker::BollardDocker,
    event,
    hub::ReqwestHub,
    msg::{Command, Msg},
    runtime::Runtime,
    ui,
};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

/// Browse Docker Hub, manage images and explore containers from the terminal.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Docker daemon address (defaults to DOCKER_HOST or the local socket).
    #[arg(short = 'H', long)]
    host: Option<String>,
    /// Config file (defaults to ~/.config/docker-tui/config.toml).
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,
}

const TICK_MS: u64 = 250;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Args::parse();
    let _log_guard = init_logging();

    let config_path = args.config.or_else(Config::default_path);
    let config = match &config_path {
        Some(path) => Config::load(path).map_err(|e| eyre!("invalid config: {e}"))?,
        None => Config::default(),
    };
    let host = config.resolve_host(args.host, std::env::var("DOCKER_HOST").ok());

    let docker = BollardDocker::connect(host.as_deref())
        .map_err(|e| eyre!(e))?
        .with_shell(config.shell.clone())
        .with_log_tail(config.log_tail);
    if let Err(e) = docker.ping().await {
        let hint = if e.contains("ermission denied") {
            "\nHint: add your user to the `docker` group or run with access to the socket."
        } else {
            ""
        };
        return Err(eyre!("cannot reach the Docker daemon: {e}{hint}"));
    }

    // `ratatui::init` installs a panic hook that restores the terminal first.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, Arc::new(docker), host, &config).await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut DefaultTerminal,
    docker: Arc<BollardDocker>,
    host: Option<String>,
    config: &Config,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let mut runtime = Runtime::new(docker, Arc::new(ReqwestHub::new()), tx);
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    // Don't burst missed ticks after the shell blocked the loop.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut app = App::new();
    app.refresh_ticks = config.refresh_ticks(TICK_MS);
    let mut pending: VecDeque<Command> = app.init().into();
    let size = terminal.size()?;
    pending.extend(app.update(Msg::Resize(size.width, size.height)));

    loop {
        while let Some(cmd) = pending.pop_front() {
            match cmd {
                Command::RunShell { id, shell } => {
                    // The event stream's reader thread would steal the shell's input.
                    drop(events);
                    let result = run_shell(terminal, host.as_deref(), &id, &shell);
                    events = EventStream::new();
                    pending.extend(app.update(Msg::ShellExited(result)));
                }
                cmd => runtime.execute(cmd),
            }
        }
        terminal.draw(|f| ui::render(&app, f))?;

        let msg = tokio::select! {
            Some(Ok(ev)) = events.next() => match event::translate(ev) {
                Some(msg) => msg,
                None => continue,
            },
            Some(msg) = rx.recv() => msg,
            _ = tick.tick() => Msg::Tick,
        };

        pending.extend(app.update(msg));
        if app.should_quit {
            return Ok(());
        }
    }
}

/// Hand the terminal to `docker exec -it` until the shell exits.
fn run_shell(
    terminal: &mut DefaultTerminal,
    host: Option<&str>,
    id: &str,
    shell: &str,
) -> Result<(), String> {
    ratatui::restore();
    println!("Attached to {id} ({shell}). Exit the shell to return to docker-tui.");
    let mut cmd = std::process::Command::new("docker");
    if let Some(h) = host {
        cmd.args(["-H", h]);
    }
    let status = cmd.args(["exec", "-it", id, shell]).status();

    let resumed = enable_raw_mode()
        .and_then(|_| execute!(std::io::stdout(), EnterAlternateScreen))
        .and_then(|_| terminal.clear());
    if let Err(e) = resumed {
        return Err(format!("could not restore the terminal: {e}"));
    }

    match status {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("the docker CLI was not found in PATH".into())
        }
        Err(e) => Err(e.to_string()),
        // 125-127 are `docker exec` failures; other codes come from the shell's last command.
        Ok(s) if matches!(s.code(), Some(125..=127)) => Err(format!("docker exec failed ({s})")),
        Ok(_) => Ok(()),
    }
}

/// Log to a file in the platform data dir; stdout belongs to the TUI.
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let dirs = directories::ProjectDirs::from("", "", "docker-tui")?;
    let log_dir = dirs.data_local_dir();
    std::fs::create_dir_all(log_dir).ok()?;
    let appender = tracing_appender::rolling::never(log_dir, "docker-tui.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("DOCKER_TUI_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    Some(guard)
}
