//! Folding the `docker pull` progress stream into per-layer state.

use bollard::models::CreateImageInfo;
use indexmap::IndexMap;

/// One message from the pull stream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullEvent {
    /// Layer id; `None` for image-level messages.
    pub id: Option<String>,
    pub status: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

impl From<CreateImageInfo> for PullEvent {
    fn from(info: CreateImageInfo) -> Self {
        let detail = info.progress_detail.unwrap_or_default();
        PullEvent {
            id: info.id,
            status: info.status.unwrap_or_default(),
            current: detail.current.map(|c| c.max(0) as u64),
            total: detail.total.map(|t| t.max(0) as u64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayerProgress {
    pub status: String,
    pub current: u64,
    pub total: u64,
}

impl LayerProgress {
    pub fn is_complete(&self) -> bool {
        matches!(self.status.as_str(), "Pull complete" | "Already exists")
    }

    /// Whether the layer's bytes have all arrived (it may still be extracting).
    pub fn is_downloaded(&self) -> bool {
        self.is_complete()
            || matches!(
                self.status.as_str(),
                "Download complete" | "Verifying Checksum" | "Extracting"
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullState {
    pub layers: IndexMap<String, LayerProgress>,
    /// Latest image-level status line, e.g. `Digest: sha256:…`.
    pub status: String,
    pub done: bool,
    pub error: Option<String>,
}

impl PullState {
    pub fn apply(&mut self, event: PullEvent) {
        let id = match event.id {
            // The first message carries the tag as its id.
            Some(id) if !event.status.starts_with("Pulling from") => id,
            _ => {
                self.status = event.status;
                return;
            }
        };
        let layer = self.layers.entry(id).or_default();
        if event.status == "Downloading" {
            layer.current = event.current.unwrap_or(layer.current);
            layer.total = event.total.unwrap_or(layer.total);
        }
        layer.status = event.status;
    }

    /// Overall download progress in `0.0..=1.0`.
    pub fn progress(&self) -> f64 {
        if self.layers.is_empty() {
            return 0.0;
        }
        if self.layers.values().all(LayerProgress::is_complete) {
            return 1.0;
        }
        let (current, total) = self.layers.values().fold((0, 0), |(c, t), l| {
            let done = if l.is_downloaded() {
                l.total
            } else {
                l.current
            };
            (c + done, t + l.total)
        });
        if total == 0 {
            0.0
        } else {
            current as f64 / total as f64
        }
    }

    pub fn finish(&mut self, result: Result<(), String>) {
        self.done = true;
        self.error = result.err();
    }
}
