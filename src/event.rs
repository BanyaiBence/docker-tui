use crossterm::event::{Event, KeyEventKind};

use crate::msg::Msg;

/// Translate a raw terminal event into an application message, dropping the
/// ones we don't care about (key releases, mouse, focus).
pub fn translate(event: Event) -> Option<Msg> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => Some(Msg::Key(key)),
        Event::Resize(w, h) => Some(Msg::Resize(w, h)),
        _ => None,
    }
}
