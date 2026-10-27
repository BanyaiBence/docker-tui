use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, TableState},
};

use super::theme;
use crate::{
    app::{
        detail::Loadable,
        files::{ChangesState, FilesState, PreviewState},
    },
    docker::fs::{ChangeKind, FileKind, FsEntry},
    format::human_bytes,
};

pub fn render(files: &FilesState, frame: &mut Frame, area: Rect) {
    if let Some(preview) = &files.preview {
        render_preview(preview, frame, area);
    } else if let Some(changes) = &files.changes {
        render_changes(changes, frame, area);
    } else {
        render_listing(files, frame, area);
    }
}

fn status_line(message: &str, style: Style) -> Paragraph<'_> {
    Paragraph::new(Line::styled(message, style))
}

fn render_listing(files: &FilesState, frame: &mut Frame, area: Rect) {
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let mut spans = vec![Span::styled(files.path.clone(), theme::header())];
    let listing = match &files.listing {
        None | Some(Loadable::Loading) => {
            frame.render_widget(Paragraph::new(Line::from(spans)), header);
            frame.render_widget(status_line("loading…", theme::dim()), body);
            return;
        }
        Some(Loadable::Failed(e)) => {
            frame.render_widget(Paragraph::new(Line::from(spans)), header);
            frame.render_widget(status_line(e, theme::error()), body);
            return;
        }
        Some(Loadable::Loaded(listing)) => listing,
    };
    spans.push(Span::styled(
        format!("  {} entries", listing.entries.len()),
        theme::dim(),
    ));
    if listing.via_archive {
        spans.push(Span::styled("  (via archive)", theme::dim()));
    }
    if listing.partial {
        spans.push(Span::styled(
            "  (partial: archive too large)",
            theme::error(),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), header);

    if listing.entries.is_empty() {
        frame.render_widget(status_line("empty directory", theme::dim()), body);
        return;
    }
    let rows = listing.entries.iter().map(|e| {
        Row::new([
            Cell::from(e.mode.clone()).style(theme::dim()),
            Cell::from(
                Line::from(if e.kind == FileKind::Dir {
                    String::new()
                } else {
                    human_bytes(e.size)
                })
                .right_aligned(),
            ),
            Cell::from(e.modified.clone()).style(theme::dim()),
            Cell::from(entry_name(e)),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(11),
            Constraint::Length(9),
            Constraint::Length(16),
            Constraint::Fill(1),
        ],
    )
    .row_highlight_style(theme::selected());
    let mut ts = TableState::default().with_selected(Some(files.selection.index));
    frame.render_stateful_widget(table, body, &mut ts);
}

fn entry_name(e: &FsEntry) -> Line<'static> {
    match e.kind {
        FileKind::Dir => Line::styled(
            format!("{}/", e.name),
            Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ),
        FileKind::Symlink => Line::from(vec![
            Span::styled(e.name.clone(), Style::new().fg(Color::Cyan)),
            Span::styled(
                e.link_target
                    .as_ref()
                    .map(|t| format!(" -> {t}"))
                    .unwrap_or_default(),
                theme::dim(),
            ),
        ]),
        FileKind::Other => Line::styled(e.name.clone(), Style::new().fg(Color::Yellow)),
        FileKind::File => Line::raw(e.name.clone()),
    }
}

fn render_preview(preview: &PreviewState, frame: &mut Frame, area: Rect) {
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let mut spans = vec![Span::styled(preview.path.clone(), theme::header())];
    match &preview.content {
        Loadable::Loading => frame.render_widget(status_line("loading…", theme::dim()), body),
        Loadable::Failed(e) => frame.render_widget(status_line(e, theme::error()), body),
        Loadable::Loaded(p) => {
            spans.push(Span::styled(
                format!("  {}", human_bytes(p.size)),
                theme::dim(),
            ));
            if p.binary {
                spans.push(Span::styled("  [binary]", theme::dim()));
            }
            if p.truncated {
                spans.push(Span::styled("  [showing start of file]", theme::dim()));
            }
            frame.render_widget(
                Paragraph::new(p.content.as_str()).scroll((preview.scroll, 0)),
                body,
            );
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), header);
}

fn render_changes(changes: &ChangesState, frame: &mut Frame, area: Rect) {
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Changes since the image (docker diff)",
            theme::header(),
        )),
        header,
    );
    let items = match &changes.items {
        Loadable::Loading => {
            return frame.render_widget(status_line("loading…", theme::dim()), body);
        }
        Loadable::Failed(e) => return frame.render_widget(status_line(e, theme::error()), body),
        Loadable::Loaded(items) if items.is_empty() => {
            return frame.render_widget(status_line("no changes", theme::dim()), body);
        }
        Loadable::Loaded(items) => items,
    };
    let rows = items.iter().map(|c| {
        let (sym, color) = match c.kind {
            ChangeKind::Added => ("A", Color::Green),
            ChangeKind::Modified => ("C", Color::Yellow),
            ChangeKind::Deleted => ("D", Color::Red),
        };
        Row::new([
            Cell::from(Span::styled(
                sym,
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            )),
            Cell::from(c.path.clone()),
        ])
    });
    let table = Table::new(rows, [Constraint::Length(2), Constraint::Fill(1)])
        .row_highlight_style(theme::selected());
    let mut ts = TableState::default().with_selected(Some(changes.selection.index));
    frame.render_stateful_widget(table, body, &mut ts);
}
