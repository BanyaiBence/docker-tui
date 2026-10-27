use docker_tui::app::list::*;

#[test]
fn move_by_is_bounded() {
    let mut s = Selection::default();
    s.move_by(-1, 3);
    assert_eq!(s.index, 0);
    s.move_by(1, 3);
    s.move_by(5, 3);
    assert_eq!(s.index, 2);
    s.move_by(-1, 3);
    assert_eq!(s.index, 1);
}

#[test]
fn move_on_empty_list_stays_at_zero() {
    let mut s = Selection::default();
    s.move_by(1, 0);
    assert_eq!(s.index, 0);
}

#[test]
fn clamp_pulls_cursor_back() {
    let mut s = Selection { index: 5 };
    s.clamp(3);
    assert_eq!(s.index, 2);
    s.clamp(0);
    assert_eq!(s.index, 0);
}
