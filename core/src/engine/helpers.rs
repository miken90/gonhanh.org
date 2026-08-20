//! Small free-function helpers for the Vietnamese input engine (moved out of mod.rs).
//! pub(super) so mod.rs's on_key pipeline (the only caller) can still use them.

use crate::data::keys;

/// Convert break key to its character representation
/// Handles both shifted and unshifted break characters for shortcut matching.
/// Examples: MINUS → '-', Shift+DOT → '>', Shift+MINUS → '_'
pub(super) fn break_key_to_char(key: u16, shift: bool) -> Option<char> {
    if shift {
        // Shifted break characters
        match key {
            keys::N1 => Some('!'),
            keys::N2 => Some('@'),
            keys::N3 => Some('#'),
            keys::N4 => Some('$'),
            keys::N5 => Some('%'),
            keys::N6 => Some('^'),
            keys::N7 => Some('&'),
            keys::N8 => Some('*'),
            keys::N9 => Some('('),
            keys::N0 => Some(')'),
            keys::MINUS => Some('_'),
            keys::EQUAL => Some('+'),
            keys::SEMICOLON => Some(':'),
            keys::QUOTE => Some('"'),
            keys::COMMA => Some('<'),
            keys::DOT => Some('>'),
            keys::SLASH => Some('?'),
            keys::BACKSLASH => Some('|'),
            keys::LBRACKET => Some('{'),
            keys::RBRACKET => Some('}'),
            keys::BACKQUOTE => Some('~'),
            _ => None,
        }
    } else {
        // Unshifted break characters
        match key {
            keys::MINUS => Some('-'),
            keys::EQUAL => Some('='),
            keys::SEMICOLON => Some(';'),
            keys::QUOTE => Some('\''),
            keys::COMMA => Some(','),
            keys::DOT => Some('.'),
            keys::SLASH => Some('/'),
            keys::BACKSLASH => Some('\\'),
            keys::LBRACKET => Some('['),
            keys::RBRACKET => Some(']'),
            keys::BACKQUOTE => Some('`'),
            _ => None,
        }
    }
}
