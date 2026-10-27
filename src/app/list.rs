/// Cursor over a list whose length changes underneath it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    pub index: usize,
}

impl Selection {
    pub fn move_by(&mut self, delta: isize, len: usize) {
        let max = len.saturating_sub(1) as isize;
        self.index = (self.index as isize + delta).clamp(0, max) as usize;
    }

    pub fn first(&mut self) {
        self.index = 0;
    }

    pub fn last(&mut self, len: usize) {
        self.index = len.saturating_sub(1);
    }

    /// Keep the cursor in range after the list changed length.
    pub fn clamp(&mut self, len: usize) {
        self.index = self.index.min(len.saturating_sub(1));
    }

    /// Handle the common list navigation keys. Returns whether `key` was used.
    pub fn on_key(&mut self, code: crossterm::event::KeyCode, len: usize) -> bool {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1, len),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1, len),
            KeyCode::PageDown => self.move_by(10, len),
            KeyCode::PageUp => self.move_by(-10, len),
            KeyCode::Home | KeyCode::Char('g') => self.first(),
            KeyCode::End | KeyCode::Char('G') => self.last(len),
            _ => return false,
        }
        true
    }
}
