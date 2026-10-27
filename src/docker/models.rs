//! Application-level Docker models, decoupled from bollard's generated types.

use bollard::models::{ContainerSummary, ContainerSummaryStateEnum, ImageSummary, PortSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Restarting,
    Exited,
    Dead,
    Other,
}

impl ContainerState {
    pub fn is_running(self) -> bool {
        matches!(self, ContainerState::Running | ContainerState::Restarting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Remove,
}

impl ContainerAction {
    /// Past-tense verb for status messages.
    pub fn done_label(self) -> &'static str {
        match self {
            ContainerAction::Start => "Started",
            ContainerAction::Stop => "Stopped",
            ContainerAction::Restart => "Restarted",
            ContainerAction::Remove => "Removed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// Full container id.
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    /// Docker's human status, e.g. `"Up 3 hours"`.
    pub status: String,
    pub ports: String,
    pub created: i64,
}

impl Container {
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(12)]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Id without the `sha256:` prefix.
    pub id: String,
    pub repo_tags: Vec<String>,
    pub size: u64,
    pub created: i64,
    pub containers: i64,
}

impl Image {
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(12)]
    }

    /// First tag, or `<none>:<none>` for dangling images.
    pub fn display_name(&self) -> &str {
        self.repo_tags
            .first()
            .map(String::as_str)
            .unwrap_or("<none>:<none>")
    }
}

/// `0.0.0.0:8080->80/tcp, 443/tcp` (deduplicated, sorted by private port).
pub fn format_ports(ports: &[PortSummary]) -> String {
    let mut sorted: Vec<&PortSummary> = ports.iter().collect();
    // IPv4 bindings sort first so they win deduplication against `::`.
    sorted.sort_by_key(|p| (p.private_port, p.public_port, p.ip.as_deref() == Some("::")));
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for p in sorted {
        let proto = p.typ.map(|t| t.to_string()).unwrap_or_else(|| "tcp".into());
        let key = (p.private_port, p.public_port, proto.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(match p.public_port {
            Some(public) => {
                let ip = p.ip.as_deref().unwrap_or("0.0.0.0");
                format!("{ip}:{public}->{}/{proto}", p.private_port)
            }
            None => format!("{}/{proto}", p.private_port),
        });
    }
    out.join(", ")
}

impl From<ContainerSummary> for Container {
    fn from(c: ContainerSummary) -> Self {
        let state = match c.state {
            Some(ContainerSummaryStateEnum::CREATED) => ContainerState::Created,
            Some(ContainerSummaryStateEnum::RUNNING) => ContainerState::Running,
            Some(ContainerSummaryStateEnum::PAUSED) => ContainerState::Paused,
            Some(ContainerSummaryStateEnum::RESTARTING) => ContainerState::Restarting,
            Some(ContainerSummaryStateEnum::EXITED) => ContainerState::Exited,
            Some(ContainerSummaryStateEnum::DEAD) => ContainerState::Dead,
            _ => ContainerState::Other,
        };
        let name = c
            .names
            .and_then(|n| n.into_iter().next())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();
        Container {
            id: c.id.unwrap_or_default(),
            name,
            image: c.image.unwrap_or_default(),
            state,
            status: c.status.unwrap_or_default(),
            ports: format_ports(c.ports.as_deref().unwrap_or_default()),
            created: c.created.unwrap_or_default(),
        }
    }
}

impl From<ImageSummary> for Image {
    fn from(i: ImageSummary) -> Self {
        let repo_tags = i
            .repo_tags
            .into_iter()
            .filter(|t| t != "<none>:<none>")
            .collect();
        Image {
            id: i.id.trim_start_matches("sha256:").to_string(),
            repo_tags,
            size: i.size.max(0) as u64,
            created: i.created,
            containers: i.containers,
        }
    }
}
