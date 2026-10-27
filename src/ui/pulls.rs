use ratatui::{
    Frame,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};

use super::{centered, theme};
use crate::{
    app::App,
    format::{human_bytes, progress_bar},
};

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let area = centered(
        area,
        area.width.saturating_sub(8).min(100),
        area.height.saturating_sub(4),
    );
    let block = Block::bordered().title(" Pulls — c clear finished · Esc close ");
    let bar_width = (area.width as usize).saturating_sub(30).clamp(10, 40);

    let mut lines: Vec<Line> = Vec::new();
    if app.pulls.items.is_empty() {
        lines.push(Line::styled(
            "No pulls yet. Press p on a Hub result or tag.",
            theme::dim(),
        ));
    }
    for (image, pull) in app.pulls.items.iter().rev() {
        let (state, style) = match (&pull.error, pull.done) {
            (Some(_), _) => ("failed".to_string(), theme::error()),
            (None, true) => ("done".to_string(), theme::header()),
            (None, false) => (format!("{:>3.0}%", pull.progress() * 100.0), theme::key()),
        };
        lines.push(Line::from(vec![
            Span::styled(image.clone(), theme::header()),
            Span::raw("  "),
            Span::raw(progress_bar(
                if pull.done && pull.error.is_none() {
                    1.0
                } else {
                    pull.progress()
                },
                bar_width,
            )),
            Span::raw(" "),
            Span::styled(state, style),
        ]));
        if let Some(e) = &pull.error {
            lines.push(Line::styled(format!("  {e}"), theme::error()));
        }
        if !pull.done {
            for (id, layer) in &pull.layers {
                let size = if layer.total > 0 && !layer.is_complete() {
                    format!(
                        "  {} / {}",
                        human_bytes(layer.current),
                        human_bytes(layer.total)
                    )
                } else {
                    String::new()
                };
                lines.push(Line::styled(
                    format!("  {:<12} {}{size}", id, layer.status),
                    theme::dim(),
                ));
            }
            if !pull.status.is_empty() {
                lines.push(Line::styled(format!("  {}", pull.status), theme::dim()));
            }
        }
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

#[cfg(test)]
mod tests {
    use crate::{
        app::App,
        docker::pull::{PullEvent, PullState},
        ui::test_util::render_to_string,
    };

    fn ev(id: &str, status: &str, progress: Option<(u64, u64)>) -> PullEvent {
        PullEvent {
            id: Some(id.into()),
            status: status.into(),
            current: progress.map(|p| p.0),
            total: progress.map(|p| p.1),
        }
    }

    #[test]
    fn renders_pull_panel() {
        let mut app = App::new();
        app.pulls.visible = true;
        let mut done = PullState::default();
        done.finish(Ok(()));
        app.pulls.items.insert("alpine:3.20".into(), done);
        let mut failed = PullState::default();
        failed.finish(Err("manifest unknown".into()));
        app.pulls.items.insert("nope:1".into(), failed);
        let mut active = PullState::default();
        active.apply(ev("a1b2c3d4e5f6", "Already exists", None));
        active.apply(ev(
            "0f9e8d7c6b5a",
            "Downloading",
            Some((3_000_000, 6_000_000)),
        ));
        app.pulls.items.insert("nginx:latest".into(), active);
        insta::assert_snapshot!(render_to_string(&app, 80, 14));
    }
}
