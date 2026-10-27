//! Human-friendly formatting helpers shared by the views.

/// `1536` → `"1.5 KB"`; uses 1000-based units like the docker CLI.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    // Three significant digits, without trailing zeros.
    let decimals = if value < 10.0 {
        2
    } else if value < 100.0 {
        1
    } else {
        0
    };
    let mut num = format!("{value:.decimals$}");
    if num.contains('.') {
        num = num.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    format!("{num} {}", UNITS[unit])
}

/// Time between `then` and `now` (unix seconds) as e.g. `"3 hours ago"`.
pub fn relative_time(now: i64, then: i64) -> String {
    const MIN: i64 = 60;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    let diff = now - then;
    let (n, unit) = match diff {
        d if d < MIN => return "just now".into(),
        d if d < HOUR => (d / MIN, "minute"),
        d if d < DAY => (d / HOUR, "hour"),
        d if d < 14 * DAY => (d / DAY, "day"),
        d if d < 60 * DAY => (d / (7 * DAY), "week"),
        d if d < 365 * DAY => (d / (30 * DAY), "month"),
        d => (d / (365 * DAY), "year"),
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

/// Truncate to at most `max` chars, appending `…` when cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `21410` → `"21.4K"`, `13508033679` → `"13.5B"`.
pub fn compact_number(n: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "K")];
    for (size, suffix) in UNITS {
        if n >= size {
            let value = format!("{:.1}", n as f64 / size as f64);
            let value = value.trim_end_matches(".0");
            return format!("{value}{suffix}");
        }
    }
    n.to_string()
}

/// A text progress bar `width` cells wide, e.g. `"████░░░░"`.
pub fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = ((fraction.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
