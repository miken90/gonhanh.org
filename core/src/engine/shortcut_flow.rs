//! Word-boundary shortcut trigger logic for the Vietnamese input engine
//! (moved out of mod.rs).

use super::Engine;
use super::Result;

impl Engine {
    /// Try word boundary shortcuts (triggered by space, punctuation, etc.)
    /// The `trigger_char` is appended to the output (space for space, punctuation for punctuation)
    pub(super) fn try_word_boundary_shortcut_with_char(&mut self, trigger_char: char) -> Result {
        // Issue #107: Allow shortcuts with special char prefix (like "#fne")
        // If shortcut_prefix is set, we still try to match even with empty buffer
        if self.buf.is_empty() && self.shortcut_prefix.is_empty() {
            return Result::none();
        }

        // Don't trigger shortcut if word has non-letter prefix (like "149k")
        // But DO allow shortcut_prefix (like "#fne") - that's intentional
        if self.has_non_letter_prefix {
            return Result::none();
        }

        // Build full trigger string including shortcut_prefix if present
        let full_trigger = if self.shortcut_prefix.is_empty() {
            self.buf.to_full_string()
        } else {
            format!("{}{}", self.shortcut_prefix, self.buf.to_full_string())
        };

        let input_method = self.current_input_method();

        // Check for word boundary shortcut match
        // For SPACE: append to output (space is "consumed" via Result::forward later)
        // For punctuation: pass None - don't append, platform layer types it normally
        // (This matches auto-restore behavior which also doesn't append break char)
        let key_char = if trigger_char == ' ' {
            Some(' ')
        } else {
            None // Punctuation: don't append, let platform type it
        };
        if let Some(m) =
            self.shortcuts
                .try_match_for_method(&full_trigger, key_char, true, input_method)
        {
            let output: Vec<char> = m.output.chars().collect();
            // backspace_count = trigger.len() which already includes prefix (e.g., "#fne" = 4)
            return Result::send(m.backspace_count as u8, &output);
        }

        Result::none()
    }

    /// Try word boundary shortcuts (triggered by space)
    pub(super) fn try_word_boundary_shortcut(&mut self) -> Result {
        self.try_word_boundary_shortcut_with_char(' ')
    }
}
