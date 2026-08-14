use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers as TermwizModifiers};

use crate::interaction::{Key, KeyStroke, MediaKey, ModifierKey, Modifiers};

pub(crate) fn map_input(event: InputEvent) -> Option<crate::terminal::TerminalEvent> {
    match event {
        InputEvent::Key(key) => key_stroke(key).map(crate::terminal::TerminalEvent::Key),
        InputEvent::Paste(text) => Some(crate::terminal::TerminalEvent::Paste(text)),
        InputEvent::Resized { .. } => Some(crate::terminal::TerminalEvent::Resize),
        InputEvent::Mouse(_) | InputEvent::PixelMouse(_) | InputEvent::Wake => None,
    }
}

pub(crate) fn key_stroke(event: KeyEvent) -> Option<KeyStroke> {
    let key = match event.key {
        // CSI-u encodes shifted CR/LF as characters (for example, CSI 13;2u).
        // Keep the framework's canonical Enter shape used by modifyOtherKeys.
        KeyCode::Char('\n' | '\r') if event.modifiers.contains(TermwizModifiers::SHIFT) => {
            Key::Enter
        }
        KeyCode::Char(character) => Key::Char(character),
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Home | KeyCode::KeyPadHome => Key::Home,
        KeyCode::End | KeyCode::KeyPadEnd => Key::End,
        KeyCode::PageUp | KeyCode::KeyPadPageUp => Key::PageUp,
        KeyCode::PageDown | KeyCode::KeyPadPageDown => Key::PageDown,
        KeyCode::LeftArrow | KeyCode::ApplicationLeftArrow => Key::Left,
        KeyCode::RightArrow | KeyCode::ApplicationRightArrow => Key::Right,
        KeyCode::UpArrow | KeyCode::ApplicationUpArrow => Key::Up,
        KeyCode::DownArrow | KeyCode::ApplicationDownArrow => Key::Down,
        KeyCode::Function(number) => Key::Function(number),
        KeyCode::CapsLock => Key::CapsLock,
        KeyCode::ScrollLock => Key::ScrollLock,
        KeyCode::NumLock => Key::NumLock,
        KeyCode::Print | KeyCode::PrintScreen => Key::PrintScreen,
        KeyCode::Pause => Key::Pause,
        KeyCode::Menu | KeyCode::LeftMenu | KeyCode::RightMenu | KeyCode::Applications => Key::Menu,
        KeyCode::MediaNextTrack => Key::Media(MediaKey::TrackNext),
        KeyCode::MediaPrevTrack => Key::Media(MediaKey::TrackPrevious),
        KeyCode::MediaStop => Key::Media(MediaKey::Stop),
        KeyCode::MediaPlayPause => Key::Media(MediaKey::PlayPause),
        KeyCode::VolumeMute => Key::Media(MediaKey::MuteVolume),
        KeyCode::VolumeDown => Key::Media(MediaKey::LowerVolume),
        KeyCode::VolumeUp => Key::Media(MediaKey::RaiseVolume),
        KeyCode::KeyPadBegin => Key::KeypadBegin,
        KeyCode::Numpad0 => Key::Char('0'),
        KeyCode::Numpad1 => Key::Char('1'),
        KeyCode::Numpad2 => Key::Char('2'),
        KeyCode::Numpad3 => Key::Char('3'),
        KeyCode::Numpad4 => Key::Char('4'),
        KeyCode::Numpad5 => Key::Char('5'),
        KeyCode::Numpad6 => Key::Char('6'),
        KeyCode::Numpad7 => Key::Char('7'),
        KeyCode::Numpad8 => Key::Char('8'),
        KeyCode::Numpad9 => Key::Char('9'),
        KeyCode::Multiply => Key::Char('*'),
        KeyCode::Add => Key::Char('+'),
        KeyCode::Separator => Key::Char(','),
        KeyCode::Subtract => Key::Char('-'),
        KeyCode::Decimal => Key::Char('.'),
        KeyCode::Divide => Key::Char('/'),
        KeyCode::Shift => Key::Modifier(ModifierKey::LeftShift),
        KeyCode::LeftShift => Key::Modifier(ModifierKey::LeftShift),
        KeyCode::RightShift => Key::Modifier(ModifierKey::RightShift),
        KeyCode::Control | KeyCode::LeftControl => Key::Modifier(ModifierKey::LeftControl),
        KeyCode::RightControl => Key::Modifier(ModifierKey::RightControl),
        KeyCode::Alt | KeyCode::LeftAlt => Key::Modifier(ModifierKey::LeftAlt),
        KeyCode::RightAlt => Key::Modifier(ModifierKey::RightAlt),
        KeyCode::Hyper => Key::Modifier(ModifierKey::LeftHyper),
        KeyCode::Super | KeyCode::LeftWindows | KeyCode::RightWindows => {
            Key::Modifier(ModifierKey::LeftSuper)
        }
        KeyCode::Meta => Key::Modifier(ModifierKey::LeftMeta),
        KeyCode::Cancel | KeyCode::Clear => Key::Null,
        _ => return None,
    };
    Some(KeyStroke::with_modifiers(key, modifiers(event.modifiers)))
}

fn modifiers(value: TermwizModifiers) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if value.contains(TermwizModifiers::SHIFT) {
        modifiers |= Modifiers::SHIFT;
    }
    if value.contains(TermwizModifiers::CTRL) {
        modifiers |= Modifiers::CONTROL;
    }
    if value.contains(TermwizModifiers::ALT) {
        modifiers |= Modifiers::ALT;
    }
    if value.contains(TermwizModifiers::SUPER) {
        modifiers |= Modifiers::SUPER;
    }
    modifiers
}

#[cfg(test)]
mod tests {
    use super::*;
    use termwiz::input::InputParser;

    fn parsed_key(sequence: &[u8]) -> KeyEvent {
        let mut parser = InputParser::new();
        let events = parser.parse_as_vec(sequence, false);
        assert_eq!(events.len(), 1);
        match events
            .into_iter()
            .next()
            .expect("parser returned one event")
        {
            InputEvent::Key(event) => event,
            event => panic!("expected key event, got {event:?}"),
        }
    }

    fn mapped_key(event: KeyEvent) -> KeyStroke {
        match map_input(InputEvent::Key(event)) {
            Some(crate::terminal::TerminalEvent::Key(stroke)) => stroke,
            event => panic!("expected mapped key event, got {event:?}"),
        }
    }

    #[test]
    fn modify_other_keys_shift_enter_csi_27_2_13_tilde_decodes_to_enter_with_shift() {
        let event = parsed_key(b"\x1b[27;2;13~");
        assert_eq!(
            event,
            KeyEvent {
                key: KeyCode::Enter,
                modifiers: TermwizModifiers::SHIFT,
            }
        );
        assert_eq!(
            mapped_key(event),
            KeyStroke::with_modifiers(Key::Enter, Modifiers::SHIFT)
        );
    }

    #[test]
    fn csi_u_shifted_cr_and_lf_decode_to_enter_with_shift() {
        for (sequence, character) in [(&b"\x1b[13;2u"[..], '\r'), (&b"\x1b[10;2u"[..], '\n')] {
            let event = parsed_key(sequence);
            assert_eq!(
                event,
                KeyEvent {
                    key: KeyCode::Char(character),
                    modifiers: TermwizModifiers::SHIFT,
                }
            );
            assert_eq!(
                mapped_key(event),
                KeyStroke::with_modifiers(Key::Enter, Modifiers::SHIFT)
            );
        }
    }

    #[test]
    fn shift_tab_keeps_the_framework_canonical_shape() {
        let stroke = key_stroke(KeyEvent {
            key: KeyCode::Tab,
            modifiers: TermwizModifiers::SHIFT,
        })
        .expect("key maps");
        assert_eq!(stroke.key(), Key::Tab);
        assert!(stroke.modifiers().contains(Modifiers::SHIFT));
    }

    #[test]
    fn keypad_digits_become_text_input() {
        let stroke = key_stroke(KeyEvent {
            key: KeyCode::Numpad7,
            modifiers: TermwizModifiers::NONE,
        })
        .expect("key maps");
        assert_eq!(stroke.key(), Key::Char('7'));
    }
}
