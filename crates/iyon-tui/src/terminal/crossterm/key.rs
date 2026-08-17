use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MediaKeyCode, ModifierKeyCode,
};

use crate::interaction::{Key, KeyStroke, MediaKey, ModifierKey, Modifiers};

pub(crate) fn key_stroke(event: KeyEvent) -> Option<KeyStroke> {
    if matches!(event.kind, KeyEventKind::Release) {
        return None;
    }

    let mut modifiers = modifiers(event.modifiers);
    let key = match event.code {
        KeyCode::BackTab => {
            modifiers |= Modifiers::SHIFT;
            Key::Tab
        }
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::F(number) => Key::Function(number),
        KeyCode::Char(character) => Key::Char(character),
        KeyCode::Null => Key::Null,
        KeyCode::Esc => Key::Escape,
        KeyCode::CapsLock => Key::CapsLock,
        KeyCode::ScrollLock => Key::ScrollLock,
        KeyCode::NumLock => Key::NumLock,
        KeyCode::PrintScreen => Key::PrintScreen,
        KeyCode::Pause => Key::Pause,
        KeyCode::Menu => Key::Menu,
        KeyCode::KeypadBegin => Key::KeypadBegin,
        KeyCode::Media(key) => Key::Media(media_key(key)),
        KeyCode::Modifier(key) => Key::Modifier(modifier_key(key)),
    };
    Some(KeyStroke::with_modifiers(key, modifiers))
}

fn modifiers(value: KeyModifiers) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if value.contains(KeyModifiers::SHIFT) {
        modifiers |= Modifiers::SHIFT;
    }
    if value.contains(KeyModifiers::CONTROL) {
        modifiers |= Modifiers::CONTROL;
    }
    if value.contains(KeyModifiers::ALT) {
        modifiers |= Modifiers::ALT;
    }
    if value.contains(KeyModifiers::SUPER) {
        modifiers |= Modifiers::SUPER;
    }
    if value.contains(KeyModifiers::HYPER) {
        modifiers |= Modifiers::HYPER;
    }
    if value.contains(KeyModifiers::META) {
        modifiers |= Modifiers::META;
    }
    modifiers
}

fn media_key(key: MediaKeyCode) -> MediaKey {
    match key {
        MediaKeyCode::Play => MediaKey::Play,
        MediaKeyCode::Pause => MediaKey::Pause,
        MediaKeyCode::PlayPause => MediaKey::PlayPause,
        MediaKeyCode::Reverse => MediaKey::Reverse,
        MediaKeyCode::Stop => MediaKey::Stop,
        MediaKeyCode::FastForward => MediaKey::FastForward,
        MediaKeyCode::Rewind => MediaKey::Rewind,
        MediaKeyCode::TrackNext => MediaKey::TrackNext,
        MediaKeyCode::TrackPrevious => MediaKey::TrackPrevious,
        MediaKeyCode::Record => MediaKey::Record,
        MediaKeyCode::LowerVolume => MediaKey::LowerVolume,
        MediaKeyCode::RaiseVolume => MediaKey::RaiseVolume,
        MediaKeyCode::MuteVolume => MediaKey::MuteVolume,
    }
}

fn modifier_key(key: ModifierKeyCode) -> ModifierKey {
    match key {
        ModifierKeyCode::LeftShift => ModifierKey::LeftShift,
        ModifierKeyCode::LeftControl => ModifierKey::LeftControl,
        ModifierKeyCode::LeftAlt => ModifierKey::LeftAlt,
        ModifierKeyCode::LeftSuper => ModifierKey::LeftSuper,
        ModifierKeyCode::LeftHyper => ModifierKey::LeftHyper,
        ModifierKeyCode::LeftMeta => ModifierKey::LeftMeta,
        ModifierKeyCode::RightShift => ModifierKey::RightShift,
        ModifierKeyCode::RightControl => ModifierKey::RightControl,
        ModifierKeyCode::RightAlt => ModifierKey::RightAlt,
        ModifierKeyCode::RightSuper => ModifierKey::RightSuper,
        ModifierKeyCode::RightHyper => ModifierKey::RightHyper,
        ModifierKeyCode::RightMeta => ModifierKey::RightMeta,
        ModifierKeyCode::IsoLevel3Shift => ModifierKey::IsoLevel3Shift,
        ModifierKeyCode::IsoLevel5Shift => ModifierKey::IsoLevel5Shift,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyStroke {
        key_stroke(KeyEvent::new(code, modifiers)).expect("key maps")
    }

    #[test]
    fn backtab_is_canonicalized_and_release_is_dropped() {
        let event = KeyEvent {
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        let stroke = key_stroke(event).expect("press becomes stroke");
        assert_eq!(stroke.key(), Key::Tab);
        assert!(stroke.modifiers().contains(Modifiers::SHIFT));
        assert!(stroke.modifiers().contains(Modifiers::CONTROL));
        assert_eq!(
            key_stroke(KeyEvent {
                kind: KeyEventKind::Release,
                ..event
            }),
            None
        );
        assert_eq!(
            key_stroke(KeyEvent {
                kind: KeyEventKind::Repeat,
                ..event
            }),
            Some(stroke)
        );
    }

    #[test]
    fn all_crossterm_modifiers_are_preserved() {
        let modifiers = KeyModifiers::SHIFT
            | KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META;
        let stroke = key(KeyCode::Char('x'), modifiers);
        for modifier in [
            Modifiers::SHIFT,
            Modifiers::CONTROL,
            Modifiers::ALT,
            Modifiers::SUPER,
            Modifiers::HYPER,
            Modifiers::META,
        ] {
            assert!(stroke.modifiers().contains(modifier));
        }
    }

    #[test]
    fn enter_and_shift_enter_remain_distinct() {
        assert_eq!(
            key(KeyCode::Enter, KeyModifiers::NONE),
            KeyStroke::new(Key::Enter)
        );
        assert_eq!(
            key(KeyCode::Enter, KeyModifiers::SHIFT),
            KeyStroke::with_modifiers(Key::Enter, Modifiers::SHIFT)
        );
    }

    #[test]
    fn crossterm_char_events_are_not_invented_as_enter() {
        assert_eq!(
            key(KeyCode::Char('\n'), KeyModifiers::NONE),
            KeyStroke::new(Key::Char('\n'))
        );
        assert_eq!(
            key(KeyCode::Char('\r'), KeyModifiers::NONE),
            KeyStroke::new(Key::Char('\r'))
        );
        assert_eq!(
            key(KeyCode::Char('j'), KeyModifiers::CONTROL),
            KeyStroke::with_modifiers(Key::Char('j'), Modifiers::CONTROL)
        );
    }

    #[test]
    fn ctrl_c_etx_is_preserved_as_a_native_character_stroke() {
        assert_eq!(
            key(KeyCode::Char('\u{0003}'), KeyModifiers::NONE),
            KeyStroke::new(Key::Char('\u{0003}'))
        );
    }
}
