use docker_tui::msg::{Command, Msg};
use std::sync::Arc;

use tokio::sync::mpsc;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::support::mock::*;
use docker_tui::docker::{
    models::{ContainerAction, ContainerState},
    pull::PullEvent,
    stats::StatsSample,
};
use docker_tui::runtime::*;

fn runtime(mock: MockDocker) -> (Runtime, mpsc::UnboundedReceiver<Msg>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Runtime::new(
            Arc::new(mock),
            Arc::new(crate::support::mock_hub::MockHub),
            tx,
        ),
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
    let repo = docker_tui::hub::models::Repo {
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
    let mut rt = Runtime::new(
        mock.clone(),
        Arc::new(crate::support::mock_hub::MockHub),
        tx,
    );
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
    let mut rt = Runtime::new(
        mock.clone(),
        Arc::new(crate::support::mock_hub::MockHub),
        tx,
    );
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
