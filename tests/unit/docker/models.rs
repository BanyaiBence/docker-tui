use bollard::models::PortSummaryTypeEnum;
use bollard::models::{ContainerSummary, ContainerSummaryStateEnum, ImageSummary, PortSummary};

use docker_tui::docker::models::*;

fn port(private: u16, public: Option<u16>, ip: Option<&str>) -> PortSummary {
    PortSummary {
        ip: ip.map(String::from),
        private_port: private,
        public_port: public,
        typ: Some(PortSummaryTypeEnum::TCP),
    }
}

#[test]
fn container_from_summary() {
    let summary = ContainerSummary {
        id: Some("0123456789abcdef0123".into()),
        names: Some(vec!["/web".into()]),
        image: Some("nginx:latest".into()),
        state: Some(ContainerSummaryStateEnum::RUNNING),
        status: Some("Up 3 hours".into()),
        created: Some(1_700_000_000),
        ports: Some(vec![port(80, Some(8080), Some("0.0.0.0"))]),
        ..Default::default()
    };
    let c = Container::from(summary);
    assert_eq!(c.name, "web");
    assert_eq!(c.short_id(), "0123456789ab");
    assert_eq!(c.image, "nginx:latest");
    assert_eq!(c.state, ContainerState::Running);
    assert_eq!(c.status, "Up 3 hours");
    assert_eq!(c.ports, "0.0.0.0:8080->80/tcp");
    assert_eq!(c.created, 1_700_000_000);
}

#[test]
fn container_from_sparse_summary() {
    let c = Container::from(ContainerSummary {
        state: Some(ContainerSummaryStateEnum::EXITED),
        ..Default::default()
    });
    assert_eq!(c.name, "");
    assert_eq!(c.state, ContainerState::Exited);
    assert_eq!(c.ports, "");
}

#[test]
fn ports_dedup_ipv4_ipv6_and_sort() {
    let ports = [
        port(443, None, None),
        port(80, Some(8080), Some("::")),
        port(80, Some(8080), Some("0.0.0.0")),
    ];
    assert_eq!(format_ports(&ports), "0.0.0.0:8080->80/tcp, 443/tcp");
}

#[test]
fn image_from_summary() {
    let img = Image::from(ImageSummary {
        id: "sha256:abcdef0123456789".into(),
        repo_tags: vec!["alpine:3.20".into()],
        size: 7_800_000,
        created: 42,
        containers: 2,
        ..Default::default()
    });
    assert_eq!(img.id, "abcdef0123456789");
    assert_eq!(img.short_id(), "abcdef012345");
    assert_eq!(img.display_name(), "alpine:3.20");
    assert_eq!(img.size, 7_800_000);
}

#[test]
fn dangling_image_name() {
    let img = Image::from(ImageSummary {
        id: "sha256:ff".into(),
        ..Default::default()
    });
    assert_eq!(img.display_name(), "<none>:<none>");
}
