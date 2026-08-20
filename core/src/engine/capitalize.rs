//! Auto-capitalize helpers for the Vietnamese input engine (moved out of mod.rs).
//! pub(super) so mod.rs's on_key pipeline (the only caller) can still use them.

use super::Engine;
use crate::data::keys;

impl Engine {
    /// Set whether to enable auto-capitalize after sentence-ending punctuation
    pub fn set_auto_capitalize(&mut self, enabled: bool) {
        self.auto_capitalize = enabled;
        if !enabled {
            self.pending_capitalize = false;
            self.saw_sentence_ending = false;
        }
    }
}

/// Check if key is sentence-ending punctuation (. ! ?) but NOT Enter
/// Issue #185: Only set pending_capitalize after punctuation + space
#[inline]
pub(super) fn is_sentence_ending_punctuation(key: u16, shift: bool) -> bool {
    key == keys::DOT
        || (shift && key == keys::N1) // !
        || (shift && key == keys::SLASH) // ?
}

/// Check if a break key should reset pending_capitalize
/// Neutral keys like quotes, parentheses, arrows should NOT reset (preserve pending)
/// Word-breaking keys like comma should reset
#[inline]
pub(super) fn should_reset_pending_capitalize(key: u16, shift: bool) -> bool {
    // These neutral characters/keys should NOT reset pending_capitalize:
    // - Quotes: ' " (QUOTE with/without shift)
    // - Parentheses: ( ) (Shift+9, Shift+0)
    // - Brackets: [ ] { } (LBRACKET, RBRACKET with/without shift)
    // - Arrow keys: navigation shouldn't reset pending state
    // - Tab, ESC: navigation/cancel shouldn't reset pending state
    let is_neutral = key == keys::QUOTE
        || key == keys::LBRACKET
        || key == keys::RBRACKET
        || (shift && key == keys::N9)  // (
        || (shift && key == keys::N0)  // )
        || key == keys::LEFT
        || key == keys::RIGHT
        || key == keys::UP
        || key == keys::DOWN
        || key == keys::TAB
        || key == keys::ESC;

    // Reset for all other break keys (comma, semicolon, etc.)
    !is_neutral
}
