use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Block, Cell, Row, Table, TableState},
};

use super::{empty_or_error, theme};
use crate::{
    app::App,
    format::{human_bytes, relative_time},
};

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let state = &app.images;
    let mut title = format!(" Images ({}) ", state.items.len());
    if state.loading && state.items.is_empty() {
        title.push_str("loading… ");
    }
    let block = Block::bordered().title(title);

    if state.items.is_empty() {
        empty_or_error(frame, area, block, state.error.as_deref(), "No images");
        return;
    }

    let rows = state.items.iter().map(|i| {
        Row::new([
            Cell::from(i.display_name().to_string()),
            Cell::from(i.short_id().to_string()).style(theme::dim()),
            Cell::from(human_bytes(i.size)),
            Cell::from(relative_time(app.now, i.created)),
            Cell::from(if i.containers > 0 {
                i.containers.to_string()
            } else {
                String::new()
            }),
        ])
    });
    let header =
        Row::new(["REPOSITORY:TAG", "ID", "SIZE", "CREATED", "USED BY"]).style(theme::header());
    let table = Table::new(
        rows,
        [
            Constraint::Fill(3),
            Constraint::Length(12),
            Constraint::Length(9),
            Constraint::Length(16),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(theme::selected());
    let mut ts = TableState::default().with_selected(Some(state.selection.index));
    frame.render_stateful_widget(table, area, &mut ts);
}
