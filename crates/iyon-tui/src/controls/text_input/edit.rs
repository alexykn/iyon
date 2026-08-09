//! Text normalization and the legacy-compatible shell-editor word model.

pub(super) const WORD_SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";

pub(super) fn canonicalize(input: &str, multiline: bool) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(normalized.len());
    for character in normalized.chars() {
        match character {
            '\t' => output.push_str("    "),
            '\n' if multiline => output.push('\n'),
            '\n' => output.push(' '),
            character if is_discarded_control(character) => {}
            character => output.push(character),
        }
    }
    output
}

fn is_discarded_control(character: char) -> bool {
    character.is_control() && character != '\n'
}

pub(super) fn is_separator(character: char) -> bool {
    character.is_whitespace() || WORD_SEPARATORS.contains(character)
}
