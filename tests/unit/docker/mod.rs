mod fs;
mod models;
mod pull;
mod stats;

use docker_tui::docker::split_image_ref;

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
