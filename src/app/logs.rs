//! Bounded, line-oriented log buffer fed by arbitrary byte chunks.

pub const MAX_LINES: usize = 5_000;

#[derive(Debug, Default)]
pub struct LogBuffer {
    pub lines: Vec<String>,
    /// Trailing text not yet terminated by a newline.
    pending: String,
    /// Lines scrolled up from the bottom; 0 means following the tail.
    pub scroll_from_bottom: usize,
}

impl LogBuffer {
    pub fn push(&mut self, chunk: &str) {
        let before = self.len();
        let text = strip_ansi(chunk);
        let mut parts = text.split('\n');
        self.pending.push_str(parts.next().unwrap_or_default());
        for part in parts {
            let line = std::mem::replace(&mut self.pending, part.to_string());
            self.lines.push(line);
        }
        if self.lines.len() > MAX_LINES {
            self.lines.drain(..self.lines.len() - MAX_LINES);
        }
        // Keep a scrolled-up view anchored on the same lines.
        if !self.following() {
            let added = self.len().saturating_sub(before);
            self.scroll_from_bottom =
                (self.scroll_from_bottom + added).min(self.len().saturating_sub(1));
        }
    }

    /// Append a complete line, terminating any partial one first.
    pub fn push_line(&mut self, line: &str) {
        if !self.pending.is_empty() {
            self.push("\n");
        }
        self.push(&format!("{line}\n"));
    }

    /// All complete lines plus the in-progress one, if any.
    pub fn all_lines(&self) -> impl Iterator<Item = &str> {
        self.lines
            .iter()
            .map(String::as_str)
            .chain((!self.pending.is_empty()).then_some(self.pending.as_str()))
    }

    pub fn len(&self) -> usize {
        self.lines.len() + usize::from(!self.pending.is_empty())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn following(&self) -> bool {
        self.scroll_from_bottom == 0
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_from_bottom = (self.scroll_from_bottom + n).min(self.len().saturating_sub(1));
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_from_bottom = self.scroll_from_bottom.saturating_sub(n);
    }

    pub fn follow(&mut self) {
        self.scroll_from_bottom = 0;
    }

    /// The window of lines to show in a view `height` lines tall.
    pub fn window(&self, height: usize) -> Vec<&str> {
        let all: Vec<&str> = self.all_lines().collect();
        // Never scroll past the point where the view would be partly empty.
        let end = all
            .len()
            .saturating_sub(self.scroll_from_bottom)
            .max(height.min(all.len()));
        let start = end.saturating_sub(height);
        all[start..end].to_vec()
    }
}

/// Remove ANSI escape sequences and carriage returns.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: parameters until a final byte in @..~
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: until BEL or ESC \\
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}
