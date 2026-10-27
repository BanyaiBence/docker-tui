use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Sparkline, Tabs},
};

use super::{containers::state_symbol, files, theme};
use crate::{
    app::{
        App,
        detail::{DetailState, DetailTab, Loadable},
    },
    format::human_bytes,
};

pub fn render(app: &App, detail: &DetailState, frame: &mut Frame, area: Rect) {
    let _ = app;
    let c = &detail.container;
    let mut title = state_symbol(c.state);
    title.spans.insert(0, Span::raw(" "));
    title.spans.push(Span::raw(format!(" {} ", c.name)));
    title.spans.push(Span::styled(
        format!("{} · {} ", c.image, c.status),
        theme::dim(),
    ));
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [tabs_area, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let tabs = Tabs::new(DetailTab::ALL.iter().map(|t| t.title()))
        .select(DetailTab::ALL.iter().position(|t| *t == detail.tab))
        .highlight_style(theme::active_tab());
    frame.render_widget(tabs, tabs_area);

    match detail.tab {
        DetailTab::Info => render_info(detail, frame, body),
        DetailTab::Logs => render_logs(detail, frame, body),
        DetailTab::Stats => render_stats(detail, frame, body),
        DetailTab::Files => files::render(&detail.files, frame, body),
    }
}

fn render_logs(detail: &DetailState, frame: &mut Frame, area: Rect) {
    let logs = &detail.logs;
    if logs.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("no output yet", theme::dim())),
            area,
        );
        return;
    }
    let [text_area, footer] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let lines: Vec<Line> = logs
        .window(text_area.height as usize)
        .into_iter()
        .map(Line::raw)
        .collect();
    frame.render_widget(Paragraph::new(lines), text_area);
    let status = if logs.following() {
        Line::styled("── following ──", theme::dim())
    } else {
        Line::styled(
            format!(
                "── paused, {} lines below (G to follow) ──",
                logs.scroll_from_bottom
            ),
            theme::header(),
        )
    };
    frame.render_widget(Paragraph::new(status), footer);
}

fn render_stats(detail: &DetailState, frame: &mut Frame, area: Rect) {
    let Some(last) = detail.stats.back() else {
        let msg = if detail.container.state.is_running() {
            "waiting for stats…"
        } else {
            "container is not running"
        };
        frame.render_widget(Paragraph::new(Line::styled(msg, theme::dim())), area);
        return;
    };
    let [cpu_label, cpu_area, mem_label, mem_area, net_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Sparklines are integer-valued, so scale percentages up for resolution.
    let cpu: Vec<u64> = detail
        .stats
        .iter()
        .map(|s| (s.cpu_percent * 100.0) as u64)
        .collect();
    let mem: Vec<u64> = detail.stats.iter().map(|s| s.mem_used).collect();
    let tail = |v: &[u64], w: u16| v[v.len().saturating_sub(w as usize)..].to_vec();

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("CPU ", theme::header()),
            Span::raw(format!("{:.1}%", last.cpu_percent)),
        ])),
        cpu_label,
    );
    frame.render_widget(
        Sparkline::default()
            .data(tail(&cpu, cpu_area.width))
            .style(Style::new().fg(Color::Green)),
        cpu_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("MEM ", theme::header()),
            Span::raw(format!(
                "{} / {} ({:.1}%)",
                human_bytes(last.mem_used),
                human_bytes(last.mem_limit),
                last.mem_percent()
            )),
        ])),
        mem_label,
    );
    frame.render_widget(
        Sparkline::default()
            .data(tail(&mem, mem_area.width))
            .style(Style::new().fg(Color::Magenta)),
        mem_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("NET ", theme::header()),
            Span::raw(format!(
                "rx {}  tx {}",
                human_bytes(last.net_rx),
                human_bytes(last.net_tx)
            )),
        ])),
        net_area,
    );
}

fn render_info(detail: &DetailState, frame: &mut Frame, area: Rect) {
    let text: ratatui::text::Text = match &detail.inspect {
        Loadable::Loading => Line::styled("loading…", theme::dim()).into(),
        Loadable::Failed(e) => Line::styled(e.clone(), theme::error()).into(),
        Loadable::Loaded(json) => json.as_str().into(),
    };
    frame.render_widget(Paragraph::new(text).scroll((detail.scroll, 0)), area);
}
