use bollard::models::CreateImageInfo;
use bollard::models::ProgressDetail;

use docker_tui::docker::pull::*;

fn ev(id: Option<&str>, status: &str, progress: Option<(u64, u64)>) -> PullEvent {
    PullEvent {
        id: id.map(String::from),
        status: status.into(),
        current: progress.map(|p| p.0),
        total: progress.map(|p| p.1),
    }
}

#[test]
fn event_from_create_image_info() {
    let info = CreateImageInfo {
        id: Some("abc".into()),
        status: Some("Downloading".into()),
        progress_detail: Some(ProgressDetail {
            current: Some(10),
            total: Some(100),
        }),
        ..Default::default()
    };
    assert_eq!(
        PullEvent::from(info),
        ev(Some("abc"), "Downloading", Some((10, 100)))
    );
}

#[test]
fn image_level_messages_update_status() {
    let mut s = PullState::default();
    s.apply(ev(Some("latest"), "Pulling from library/nginx", None));
    s.apply(ev(None, "Digest: sha256:abc", None));
    assert_eq!(s.status, "Digest: sha256:abc");
    assert!(
        s.layers.is_empty(),
        "tag id of first message is not a layer"
    );
}

#[test]
fn tracks_layers_in_order_with_progress() {
    let mut s = PullState::default();
    s.apply(ev(Some("l1"), "Pulling fs layer", None));
    s.apply(ev(Some("l2"), "Pulling fs layer", None));
    s.apply(ev(Some("l1"), "Downloading", Some((50, 100))));
    s.apply(ev(Some("l2"), "Downloading", Some((0, 300))));
    assert_eq!(s.layers.keys().collect::<Vec<_>>(), ["l1", "l2"]);
    assert_eq!(s.layers["l1"].current, 50);
    assert!((s.progress() - 50.0 / 400.0).abs() < 1e-9);
}

#[test]
fn extraction_does_not_reset_download_progress() {
    let mut s = PullState::default();
    s.apply(ev(Some("l1"), "Downloading", Some((100, 100))));
    s.apply(ev(Some("l1"), "Download complete", None));
    s.apply(ev(Some("l1"), "Extracting", Some((10, 100))));
    assert!((s.progress() - 1.0).abs() < 1e-9);
    assert_eq!(s.layers["l1"].status, "Extracting");
}

#[test]
fn complete_layers_count_as_full_progress() {
    let mut s = PullState::default();
    s.apply(ev(Some("l1"), "Already exists", None));
    s.apply(ev(Some("l2"), "Pull complete", None));
    assert!(s.layers.values().all(LayerProgress::is_complete));
    assert_eq!(s.progress(), 1.0);
}

#[test]
fn no_layers_means_no_progress() {
    assert_eq!(PullState::default().progress(), 0.0);
}
