use docker_tui::format::*;

#[test]
fn human_bytes_picks_unit() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(999), "999 B");
    assert_eq!(human_bytes(1_500), "1.5 KB");
    assert_eq!(human_bytes(7_800_000), "7.8 MB");
    assert_eq!(human_bytes(1_230_000_000), "1.23 GB");
    assert_eq!(human_bytes(187_000_000), "187 MB");
}

#[test]
fn relative_time_buckets() {
    let now = 1_000_000;
    assert_eq!(relative_time(now, now - 5), "just now");
    assert_eq!(relative_time(now, now - 60), "1 minute ago");
    assert_eq!(relative_time(now, now - 150), "2 minutes ago");
    assert_eq!(relative_time(now, now - 3 * 3600), "3 hours ago");
    assert_eq!(relative_time(now, now - 86_400), "1 day ago");
    assert_eq!(relative_time(now, now - 20 * 86_400), "2 weeks ago");
    assert_eq!(relative_time(now, now - 65 * 86_400), "2 months ago");
    assert_eq!(relative_time(now, now - 800 * 86_400), "2 years ago");
}

#[test]
fn relative_time_in_future_is_just_now() {
    assert_eq!(relative_time(100, 200), "just now");
}

#[test]
fn compact_numbers() {
    assert_eq!(compact_number(0), "0");
    assert_eq!(compact_number(999), "999");
    assert_eq!(compact_number(1_000), "1K");
    assert_eq!(compact_number(21_410), "21.4K");
    assert_eq!(compact_number(1_088_201_159), "1.1B");
    assert_eq!(compact_number(13_508_033_679), "13.5B");
    assert_eq!(compact_number(2_500_000), "2.5M");
}

#[test]
fn progress_bars() {
    assert_eq!(progress_bar(0.0, 4), "░░░░");
    assert_eq!(progress_bar(0.5, 4), "██░░");
    assert_eq!(progress_bar(1.0, 4), "████");
    assert_eq!(progress_bar(7.0, 2), "██");
    assert_eq!(progress_bar(-1.0, 2), "░░");
}

#[test]
fn truncate_adds_ellipsis() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 6), "hello…");
    assert_eq!(truncate("héllo", 3), "hé…");
}
