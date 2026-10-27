use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Paragraph, Row, Table, TableState},
};

use super::{empty_or_error, theme};
use crate::{
    app::{App, hub::TagsState},
    format::{compact_number, human_bytes},
};

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let [search_area, body] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
    render_search(app, frame, search_area);
    match &app.hub.tags {
        Some(tags) => render_tags(tags, frame, body),
        None => render_results(app, frame, body),
    }
}

fn render_search(app: &App, frame: &mut Frame, area: Rect) {
    let hub = &app.hub;
    let border = if hub.editing {
        Style::new().fg(Color::Cyan)
    } else {
        theme::dim()
    };
    let block = Block::bordered()
        .title(" Search Docker Hub ")
        .border_style(border);
    let inner = block.inner(area);
    let width = inner.width.max(1) as usize;
    let scroll = hub.input.visual_scroll(width);
    let text = if hub.input.value().is_empty() && !hub.editing {
        Line::styled("press / to search", theme::dim())
    } else {
        Line::raw(hub.input.value())
    };
    frame.render_widget(
        Paragraph::new(text).scroll((0, scroll as u16)).block(block),
        area,
    );
    if hub.editing {
        let x = hub.input.visual_cursor().saturating_sub(scroll) as u16;
        frame.set_cursor_position(Position::new(inner.x + x, inner.y));
    }
}

fn render_results(app: &App, frame: &mut Frame, area: Rect) {
    let hub = &app.hub;
    let mut title = if hub.query.is_empty() {
        " Results ".to_string()
    } else {
        format!(
            " Results for \"{}\" ({} of {}) ",
            hub.query,
            hub.results.len(),
            compact_number(hub.count)
        )
    };
    if hub.loading {
        title.push_str("loading… ");
    }
    let block = Block::bordered().title(title);
    if hub.results.is_empty() {
        let empty = if hub.query.is_empty() {
            "Search for images to pull"
        } else {
            "No results"
        };
        empty_or_error(frame, area, block, hub.error.as_deref(), empty);
        return;
    }
    let rows = hub.results.iter().map(|r| {
        let mut name = vec![Span::raw(r.name.clone())];
        if r.official {
            name.push(Span::styled(" official", Style::new().fg(Color::Green)));
        }
        Row::new([
            Cell::from(Line::from(name)),
            Cell::from(Line::from(format!("★ {}", compact_number(r.stars))).right_aligned()),
            Cell::from(Line::from(compact_number(r.pulls)).right_aligned()),
            Cell::from(r.description.clone()).style(theme::dim()),
        ])
    });
    let header = Row::new(["NAME", "STARS", "PULLS", "DESCRIPTION"]).style(theme::header());
    let table = Table::new(
        rows,
        [
            Constraint::Fill(2),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Fill(3),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(theme::selected());
    let mut ts = TableState::default().with_selected(Some(hub.selection.index));
    frame.render_stateful_widget(table, area, &mut ts);
}

fn render_tags(tags: &TagsState, frame: &mut Frame, area: Rect) {
    let mut title = format!(" Tags of {} ", tags.repo.name);
    if tags.loading {
        title.push_str("loading… ");
    }
    let block = Block::bordered().title(title);
    if tags.items.is_empty() {
        let empty = if tags.loading { "" } else { "No tags" };
        empty_or_error(frame, area, block, tags.error.as_deref(), empty);
        return;
    }
    let rows = tags.items.iter().map(|t| {
        Row::new([
            Cell::from(t.name.clone()),
            Cell::from(Line::from(human_bytes(t.size)).right_aligned()),
            Cell::from(
                t.last_updated
                    .as_deref()
                    .map(|d| d.get(..10).unwrap_or(d))
                    .unwrap_or("")
                    .to_string(),
            ),
            Cell::from(t.architectures.join(", ")).style(theme::dim()),
        ])
    });
    let header = Row::new(["TAG", "SIZE", "UPDATED", "PLATFORMS"]).style(theme::header());
    let table = Table::new(
        rows,
        [
            Constraint::Fill(2),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Fill(2),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(theme::selected());
    let mut ts = TableState::default().with_selected(Some(tags.selection.index));
    frame.render_stateful_widget(table, area, &mut ts);
}
