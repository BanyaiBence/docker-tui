use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Row, Table, TableState},
};

use super::{empty_or_error, theme};
use crate::{app::App, docker::models::ContainerState};

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let state = &app.containers;
    let visible = state.visible();
    let filter = if state.show_all { "all" } else { "running" };
    let mut title = format!(" Containers ({}, {filter}) ", visible.len());
    if state.loading && state.items.is_empty() {
        title.push_str("loading… ");
    }
    let block = Block::bordered().title(title);

    if visible.is_empty() {
        empty_or_error(frame, area, block, state.error.as_deref(), "No containers");
        return;
    }

    let rows = visible.iter().map(|c| {
        Row::new([
            Cell::from(state_symbol(c.state)),
            Cell::from(c.name.clone()),
            Cell::from(c.image.clone()),
            Cell::from(c.status.clone()),
            Cell::from(c.ports.clone()),
            Cell::from(c.short_id().to_string()).style(theme::dim()),
        ])
    });
    let header = Row::new(["", "NAME", "IMAGE", "STATUS", "PORTS", "ID"]).style(theme::header());
    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Fill(2),
            Constraint::Fill(2),
            Constraint::Fill(2),
            Constraint::Fill(2),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(theme::selected());
    let mut ts = TableState::default().with_selected(Some(state.selection.index));
    frame.render_stateful_widget(table, area, &mut ts);
}

pub fn state_symbol(state: ContainerState) -> Line<'static> {
    let (sym, color) = match state {
        ContainerState::Running => ("●", Color::Green),
        ContainerState::Paused => ("◐", Color::Yellow),
        ContainerState::Restarting => ("↻", Color::Yellow),
        ContainerState::Dead => ("✖", Color::Red),
        _ => ("○", Color::DarkGray),
    };
    Line::from(Span::styled(
        sym,
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    ))
}

#[cfg(test)]
mod tests {
    use crate::{
        app::App, docker::models::ContainerState, msg::Msg, runtime::mock::container,
        ui::test_util::render_to_string,
    };

    fn fixture() -> App {
        let mut app = App::new();
        let mut web = container("0123456789abcdef", "web", ContainerState::Running);
        web.image = "nginx:latest".into();
        web.ports = "0.0.0.0:8080->80/tcp".into();
        app.update(Msg::ContainersLoaded(Ok(vec![
            web,
            container("fedcba9876543210", "db", ContainerState::Exited),
        ])));
        app
    }

    #[test]
    fn renders_container_table() {
        insta::assert_snapshot!(render_to_string(&fixture(), 110, 8));
    }

    #[test]
    fn renders_running_only_filter() {
        let mut app = fixture();
        app.containers.show_all = false;
        insta::assert_snapshot!(render_to_string(&app, 110, 7));
    }

    #[test]
    fn renders_error_when_empty() {
        let mut app = App::new();
        app.update(Msg::ContainersLoaded(Err(
            "permission denied while connecting".into(),
        )));
        insta::assert_snapshot!(render_to_string(&app, 60, 7));
    }
}
