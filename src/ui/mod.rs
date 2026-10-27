mod containers;
mod detail;
mod files;
mod hub;
mod images;
mod pulls;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::Stylize,
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph, Row, Table, Tabs, Wrap},
};

use crate::app::{
    App, Tab,
    detail::{DetailTab, Loadable},
    exec::{ExecView, PromptState},
    logs::strip_ansi,
    terminal::TerminalState,
};

pub mod theme {
    use ratatui::style::{Color, Modifier, Style};

    pub fn header() -> Style {
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    }
    pub fn selected() -> Style {
        Style::new()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    }
    pub fn active_tab() -> Style {
        Style::new()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    }
    pub fn dim() -> Style {
        Style::new().fg(Color::DarkGray)
    }
    pub fn error() -> Style {
        Style::new().fg(Color::Red)
    }
    pub fn key() -> Style {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }
}

pub fn render(app: &App, frame: &mut Frame) {
    let [tabs_area, body, status] = split_screen(frame.area());

    render_tabs(app, frame, tabs_area);
    match &app.terminal {
        Some(term) => {
            let [main, pane] = split_body(body);
            render_body(app, frame, main);
            render_terminal(term, frame, pane);
        }
        None => render_body(app, frame, body),
    }
    render_status(app, frame, status);

    if app.pulls.visible {
        pulls::render(app, frame);
    }
    if let Some(view) = &app.exec_view {
        render_exec_view(frame, view);
    }
    if let Some(prompt) = &app.prompt {
        render_prompt(frame, status, prompt);
    }
    if let Some(confirm) = &app.confirm {
        render_confirm(frame, &confirm.message);
    }
    if app.show_help {
        render_help(frame);
    }
}

fn render_prompt(frame: &mut Frame, area: Rect, prompt: &PromptState) {
    let label = format!(" {} $ ", prompt.container_name);
    let width = area
        .width
        .saturating_sub(label.chars().count() as u16)
        .max(1) as usize;
    let scroll = prompt.input.visual_scroll(width);
    let value: String = prompt.input.value().chars().skip(scroll).collect();
    let line = Line::from(vec![
        Span::styled(label.clone(), theme::key()),
        Span::raw(value),
    ]);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(line), area);
    let x = area.x + label.chars().count() as u16 + (prompt.input.visual_cursor() - scroll) as u16;
    frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), area.y));
}

fn render_exec_view(frame: &mut Frame, view: &ExecView) {
    let full = frame.area();
    let area = centered(
        full,
        full.width.saturating_sub(8),
        full.height.saturating_sub(4),
    );
    let (status, text): (String, Text) = match &view.result {
        Loadable::Loading => (
            "running…".into(),
            Line::styled("running…", theme::dim()).into(),
        ),
        Loadable::Failed(e) => (
            "failed".into(),
            Line::styled(e.clone(), theme::error()).into(),
        ),
        Loadable::Loaded(out) => {
            let code = out.exit_code.map_or("?".to_string(), |c| c.to_string());
            let body = if out.output.is_empty() {
                Line::styled("(no output)", theme::dim()).into()
            } else {
                Text::raw(strip_ansi(&out.output))
            };
            (format!("exit {code}"), body)
        }
    };
    let title = format!(" {} $ {} — {status} ", view.container_name, view.command);
    let block = Block::bordered()
        .title(title)
        .title_bottom(Line::styled(" Esc close · : run another ", theme::dim()));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).scroll((view.scroll, 0)).block(block),
        area,
    );
}

fn render_confirm(frame: &mut Frame, message: &str) {
    let width = (message.chars().count() as u16 + 6).clamp(30, 70);
    let area = centered(frame.area(), width, 5);
    let text = vec![
        Line::raw(message.to_string()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("y", theme::key()),
            Span::raw(" yes   "),
            Span::styled("n", theme::key()),
            Span::raw(" no"),
        ]),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" Confirm ")
                .border_style(theme::error()),
        ),
        area,
    );
}

fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let titles = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| format!("[{}] {}", i + 1, t.title()));
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .highlight_style(theme::active_tab());
    frame.render_widget(tabs, area);
}

fn render_body(app: &App, frame: &mut Frame, area: Rect) {
    match app.tab {
        Tab::Hub => hub::render(app, frame, area),
        Tab::Images => images::render(app, frame, area),
        Tab::Containers => match &app.detail {
            Some(d) => detail::render(app, d, frame, area),
            None => containers::render(app, frame, area),
        },
    }
}

/// Key hints for the current context, shown in the status bar.
fn hints(app: &App) -> &'static [(&'static str, &'static str)] {
    match app.tab {
        Tab::Containers => match &app.detail {
            Some(d) if d.tab == DetailTab::Files => match (&d.files.preview, &d.files.changes) {
                (Some(_), _) => &[("Esc", "close"), ("j/k", "scroll")],
                (None, Some(_)) => &[("Esc", "close"), ("Enter", "view file")],
                (None, None) => &[
                    ("Enter", "open"),
                    ("Bksp", "up"),
                    ("D", "changes"),
                    ("x", "shell"),
                    (":", "run"),
                    ("Esc", "back"),
                ],
            },
            Some(_) => &[
                ("Esc", "back"),
                ("h/l", "tabs"),
                ("j/k", "scroll"),
                ("G", "follow"),
                ("x", "shell"),
                (":", "run"),
            ],
            None => &[
                ("Enter", "details"),
                ("x/X", "shell/pane"),
                (":", "run"),
                ("s", "start/stop"),
                ("R", "restart"),
                ("d", "remove"),
                ("a", "all/running"),
            ],
        },
        Tab::Images => &[("d", "remove"), ("r", "refresh")],
        Tab::Hub if app.hub.editing => &[("Enter", "search"), ("Esc", "cancel")],
        Tab::Hub if app.hub.tags.is_some() => {
            &[("Enter/p", "pull tag"), ("Esc", "back"), ("P", "pulls")]
        }
        Tab::Hub => &[
            ("/", "search"),
            ("Enter", "tags"),
            ("p", "pull latest"),
            ("P", "pulls"),
        ],
    }
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    if app.terminal.as_ref().is_some_and(|t| t.focused) && app.toast.is_none() {
        let line = Line::from(vec![
            Span::styled(" shell has focus — ", theme::dim()),
            Span::styled("Ctrl-\\", theme::key()),
            Span::styled(" to return to docker-tui", theme::dim()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    if let Some(toast) = &app.toast {
        let style = if toast.error {
            theme::error()
        } else {
            theme::header()
        };
        frame.render_widget(
            Paragraph::new(Line::styled(format!(" {}", toast.text), style)),
            area,
        );
        return;
    }
    let mut spans = vec![Span::raw(" ")];
    let active = app.pulls.active();
    if active > 0 {
        spans.push(Span::styled(
            format!("⇣ {active} pulling "),
            theme::header(),
        ));
    }
    for (k, d) in hints(app).iter().chain(&[("?", "help"), ("q", "quit")]) {
        spans.push(Span::styled(*k, theme::key()));
        spans.push(Span::styled(format!(" {d}  "), theme::dim()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render an empty list placeholder, or the load error if there is one.
fn empty_or_error(frame: &mut Frame, area: Rect, block: Block, error: Option<&str>, empty: &str) {
    let text = match error {
        Some(e) => Line::styled(format!("Error: {e}"), theme::error()),
        None => Line::styled(empty.to_string(), theme::dim()),
    };
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }).block(block),
        area,
    );
}

const HELP: &[(&str, &str)] = &[
    ("1 2 3 / Tab", "switch tabs"),
    ("↑↓ / j k", "move selection"),
    ("g / G", "first / last"),
    ("Enter", "open details / tags"),
    ("/", "hub: search"),
    ("p", "hub: pull image / tag"),
    ("P", "show pulls"),
    ("Esc", "back"),
    ("h / l", "detail: switch Info/Logs/Stats/Files"),
    ("x", "open a shell in the container"),
    ("X", "open a shell in a side pane"),
    ("Ctrl-\\", "pane: give focus back to the app"),
    (":", "run a command in the container"),
    ("Bksp", "files: parent directory"),
    ("D", "files: changes since image"),
    ("G / f", "logs: follow tail"),
    ("s", "start / stop container"),
    ("R", "restart container"),
    ("d", "remove container / image"),
    ("a", "containers: all / running"),
    ("r", "refresh"),
    ("?", "toggle help"),
    ("q / Ctrl-c", "quit"),
];

fn render_help(frame: &mut Frame) {
    let area = centered(frame.area(), 60, HELP.len() as u16 + 2);
    let rows = HELP
        .iter()
        .map(|(k, d)| Row::new([k.bold().into(), Line::from(*d)]));
    let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(10)])
        .block(Block::bordered().title(" Help — Esc to close "));
    frame.render_widget(Clear, area);
    frame.render_widget(table, area);
}

/// Inner area of the embedded shell pane for a terminal of size `full`.
/// Shared with the app so the PTY matches what is drawn.
pub fn terminal_area(full: Rect) -> Rect {
    let [_, body, _] = split_screen(full);
    let [_, pane] = split_body(body);
    Block::bordered().inner(pane)
}

/// Tab bar, body and status bar.
fn split_screen(area: Rect) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area)
}

/// The body split into the main view and the shell pane.
fn split_body(body: Rect) -> [Rect; 2] {
    Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(body)
}

fn render_terminal(term: &TerminalState, frame: &mut Frame, area: Rect) {
    let (border, hint) = if term.focused {
        (theme::key(), " Ctrl-\\ release focus ")
    } else {
        (theme::dim(), " X focus ")
    };
    let block = Block::bordered()
        .border_style(border)
        .title(format!(" shell: {} ", term.container_name))
        .title_bottom(Line::styled(hint, theme::dim()));
    let mut cursor = tui_term::widget::Cursor::default();
    if !term.focused {
        cursor.hide();
    }
    frame.render_widget(
        tui_term::widget::PseudoTerminal::new(term.parser.screen())
            .block(block)
            .cursor(cursor),
        area,
    );
}

/// A rectangle of at most `width`×`height` centered inside `area`.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
