use docker_tui::app::logs::*;

#[test]
fn splits_chunks_into_lines_and_keeps_partial() {
    let mut b = LogBuffer::default();
    b.push("one\ntw");
    b.push("o\nthree");
    assert_eq!(b.lines, ["one", "two"]);
    assert_eq!(b.all_lines().collect::<Vec<_>>(), ["one", "two", "three"]);
    assert_eq!(b.len(), 3);
}

#[test]
fn strips_ansi_and_carriage_returns() {
    assert_eq!(strip_ansi("\x1b[32mINFO\x1b[0m ok\r"), "INFO ok");
    assert_eq!(strip_ansi("\x1b]0;title\x07text"), "text");
}

#[test]
fn caps_line_count() {
    let mut b = LogBuffer::default();
    for i in 0..MAX_LINES + 10 {
        b.push(&format!("{i}\n"));
    }
    assert_eq!(b.lines.len(), MAX_LINES);
    assert_eq!(b.lines[0], "10");
}

#[test]
fn window_follows_tail_by_default() {
    let mut b = LogBuffer::default();
    b.push("a\nb\nc\nd\n");
    assert_eq!(b.window(2), ["c", "d"]);
    assert_eq!(b.window(10), ["a", "b", "c", "d"]);
}

#[test]
fn scrolling_up_is_bounded_and_shifts_window() {
    let mut b = LogBuffer::default();
    b.push("a\nb\nc\nd\n");
    b.scroll_up(1);
    assert!(!b.following());
    assert_eq!(b.window(2), ["b", "c"]);
    b.scroll_up(100);
    assert_eq!(b.window(2), ["a", "b"]);
    b.scroll_down(100);
    assert!(b.following());
}

#[test]
fn scroll_position_is_kept_while_new_lines_arrive() {
    let mut b = LogBuffer::default();
    b.push("a\nb\nc\n");
    b.scroll_up(1);
    let before = b.window(1);
    assert_eq!(before, ["b"]);
    b.push("d\n");
    assert_eq!(b.window(1), ["b"]);
}
