//! Integration tests against the real Docker daemon.
//!
//! Run with `cargo test -- --ignored`. They only touch containers named
//! `dtui-test-*` that they create themselves.

use std::time::Duration;

use bollard::{
    Docker,
    models::ContainerCreateBody,
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
    },
};
use docker_tui::docker::{
    BollardDocker, DockerApi,
    models::{ContainerAction, ContainerState},
};
use futures::StreamExt;

pub const IMAGE: &str = "alpine:3.20";

/// A throwaway container, force-removed on drop.
pub struct TestContainer {
    pub id: String,
    pub name: String,
}

impl TestContainer {
    pub async fn start(name: &str, cmd: &[&str]) -> Self {
        let docker = Docker::connect_with_defaults().unwrap();
        if docker.inspect_image(IMAGE).await.is_err() {
            let opts = CreateImageOptionsBuilder::default()
                .from_image(IMAGE)
                .build();
            let mut pull = docker.create_image(Some(opts), None, None);
            while let Some(r) = pull.next().await {
                r.unwrap();
            }
        }
        let name = format!("dtui-test-{name}-{}", std::process::id());
        let opts = CreateContainerOptionsBuilder::default().name(&name).build();
        let body = ContainerCreateBody {
            image: Some(IMAGE.into()),
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            ..Default::default()
        };
        let id = docker.create_container(Some(opts), body).await.unwrap().id;
        docker.start_container(&id, None).await.unwrap();
        TestContainer { id, name }
    }
}

impl Drop for TestContainer {
    fn drop(&mut self) {
        let id = self.id.clone();
        // Drop can't be async; clean up on a fresh runtime thread.
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let docker = Docker::connect_with_defaults().unwrap();
                let opts = RemoveContainerOptionsBuilder::default().force(true).build();
                let _ = docker.remove_container(&id, Some(opts)).await;
            })
        })
        .join()
        .unwrap();
    }
}

fn api() -> BollardDocker {
    BollardDocker::connect(None).unwrap()
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn lists_inspects_and_streams_a_container() {
    let c = TestContainer::start(
        "stream",
        &["sh", "-c", "for i in 1 2 3; do echo line$i; done; sleep 60"],
    )
    .await;
    let api = api();

    let listed = api.list_containers().await.unwrap();
    let mine = listed
        .iter()
        .find(|x| x.id == c.id)
        .expect("container listed");
    assert_eq!(mine.name, c.name);
    assert_eq!(mine.state, ContainerState::Running);

    let json = api.inspect_container(&c.id).await.unwrap();
    assert!(json.contains(&c.id));

    let mut logs = api.logs(&c.id);
    let mut text = String::new();
    while !text.contains("line3") {
        let chunk = tokio::time::timeout(Duration::from_secs(10), logs.next())
            .await
            .expect("log output in time")
            .unwrap()
            .unwrap();
        text.push_str(&chunk);
    }

    let mut stats = api.stats(&c.id);
    let sample = tokio::time::timeout(Duration::from_secs(10), stats.next())
        .await
        .expect("stats in time")
        .unwrap()
        .unwrap();
    assert!(sample.mem_limit > 0);
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn stops_starts_and_removes_a_container() {
    let c = TestContainer::start("lifecycle", &["sleep", "300"]).await;
    let api = api();
    let state = |list: Vec<docker_tui::docker::models::Container>| {
        list.into_iter().find(|x| x.id == c.id).map(|x| x.state)
    };

    api.container_action(&c.id, ContainerAction::Stop)
        .await
        .unwrap();
    assert_eq!(
        state(api.list_containers().await.unwrap()),
        Some(ContainerState::Exited)
    );

    api.container_action(&c.id, ContainerAction::Start)
        .await
        .unwrap();
    assert_eq!(
        state(api.list_containers().await.unwrap()),
        Some(ContainerState::Running)
    );

    api.container_action(&c.id, ContainerAction::Remove)
        .await
        .unwrap();
    assert_eq!(state(api.list_containers().await.unwrap()), None);
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn lists_directories_with_ls_in_running_container() {
    let c = TestContainer::start("ls", &["sleep", "300"]).await;
    let listing = api().list_dir(&c.id, "/etc").await.unwrap();
    assert!(!listing.via_archive);
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"passwd"), "{names:?}");
    assert!(names.contains(&"apk"), "{names:?}");
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn lists_directories_of_stopped_container_via_archive() {
    let c = TestContainer::start("archive", &["true"]).await;
    let api = api();
    // wait for `true` to exit
    for _ in 0..50 {
        let list = api.list_containers().await.unwrap();
        if list.iter().any(|x| x.id == c.id && !x.state.is_running()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let listing = api.list_dir(&c.id, "/etc").await.unwrap();
    assert!(listing.via_archive);
    assert!(listing.entries.iter().any(|e| e.name == "passwd"));
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn listing_a_missing_directory_fails() {
    let c = TestContainer::start("missing", &["sleep", "300"]).await;
    assert!(api().list_dir(&c.id, "/does/not/exist").await.is_err());
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn reads_files_and_diffs() {
    let c = TestContainer::start(
        "files",
        &["sh", "-c", "echo hi > /tmp/created && sleep 300"],
    )
    .await;
    let api = api();
    let preview = api.read_file(&c.id, "/etc/alpine-release").await.unwrap();
    assert!(preview.content.starts_with("3.20"));
    assert!(!preview.binary);

    let mut changes = vec![];
    for _ in 0..50 {
        changes = api.diff(&c.id).await.unwrap();
        if changes.iter().any(|ch| ch.path == "/tmp/created") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let created = changes
        .iter()
        .find(|ch| ch.path == "/tmp/created")
        .expect("change listed");
    assert_eq!(created.kind, docker_tui::docker::fs::ChangeKind::Added);
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn detects_shell_and_runs_commands() {
    let c = TestContainer::start("exec", &["sleep", "300"]).await;
    let api = api();
    assert_eq!(api.detect_shell(&c.id).await.unwrap(), "/bin/sh");
    let out = api.exec(&c.id, "echo hello; exit 3").await.unwrap();
    assert_eq!(out.output, "hello\n");
    assert_eq!(out.exit_code, Some(3));
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn embedded_shell_session_round_trip() {
    use tokio::io::AsyncWriteExt;

    let c = TestContainer::start("tty", &["sleep", "300"]).await;
    let api = api();
    let mut session = api.attach_shell(&c.id, "/bin/sh", 24, 80).await.unwrap();
    api.resize_exec(&session.exec_id, 30, 100).await.unwrap();
    session
        .input
        .write_all(b"stty size; echo MARK$((40+2))\r")
        .await
        .unwrap();
    session.input.flush().await.unwrap();
    let mut text = String::new();
    while !text.contains("MARK42") {
        let chunk = tokio::time::timeout(Duration::from_secs(10), session.output.next())
            .await
            .expect("output in time")
            .expect("stream open")
            .unwrap();
        text.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(text.contains("30 100"), "{text}");
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn preferred_shell_wins_when_present() {
    let c = TestContainer::start("prefshell", &["sleep", "300"]).await;
    let api = BollardDocker::connect(None)
        .unwrap()
        .with_shell(Some("/bin/ash".into()));
    assert_eq!(api.detect_shell(&c.id).await.unwrap(), "/bin/ash");
    let api = BollardDocker::connect(None)
        .unwrap()
        .with_shell(Some("/nope/zsh".into()));
    assert_eq!(api.detect_shell(&c.id).await.unwrap(), "/bin/sh");
}

#[tokio::test]
#[ignore = "needs a Docker daemon"]
async fn log_tail_limits_initial_lines() {
    let c = TestContainer::start("tail", &["sh", "-c", "seq 1 50; sleep 300"]).await;
    let api = BollardDocker::connect(None).unwrap().with_log_tail(5);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut logs = api.logs(&c.id);
    let mut text = String::new();
    while !text.contains("50\n") {
        let chunk = tokio::time::timeout(Duration::from_secs(10), logs.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        text.push_str(&chunk);
    }
    assert_eq!(text, "46\n47\n48\n49\n50\n");
}
