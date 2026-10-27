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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

    use super::*;

    fn key_event(kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn key_presses_and_repeats_become_key_msgs() {
        assert!(matches!(
            translate(key_event(KeyEventKind::Press)),
            Some(Msg::Key(_))
        ));
        assert!(matches!(
            translate(key_event(KeyEventKind::Repeat)),
            Some(Msg::Key(_))
        ));
    }

    #[test]
    fn key_releases_are_dropped() {
        assert!(translate(key_event(KeyEventKind::Release)).is_none());
    }

    #[test]
    fn resize_becomes_resize_msg() {
        assert!(matches!(
            translate(Event::Resize(80, 24)),
            Some(Msg::Resize(80, 24))
        ));
    }

    #[test]
    fn focus_events_are_dropped() {
        assert!(translate(Event::FocusGained).is_none());
    }
}
