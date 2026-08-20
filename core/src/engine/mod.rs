//! Vietnamese IME Engine
//!
//! Core engine for Vietnamese input method processing.
//! Uses pattern-based transformation with validation-first approach.
//!
//! ## Architecture
//!
//! 1. **Validation First**: Check if buffer is valid Vietnamese before transforming
//! 2. **Pattern-Based**: Scan entire buffer for patterns instead of case-by-case
//! 3. **Shortcut Support**: User-defined abbreviations with priority
//! 4. **Longest-Match-First**: For diacritic placement

mod auto_restore;
mod capitalize;
pub mod buffer;
mod helpers;
pub mod shortcut;
mod shortcut_flow;
pub mod syllable;
pub mod transform;
pub mod validation;

use crate::data::{
    chars::{self, mark, tone},
    constants, english_dict, keys,
    vowel::{Phonology, Vowel},
};
use crate::input::{self, ToneType};
use crate::utils;
use buffer::{Buffer, Char, MAX};
use capitalize::{is_sentence_ending_punctuation, should_reset_pending_capitalize};
use helpers::break_key_to_char;
use shortcut::{InputMethod, ShortcutTable};
use validation::{
    is_foreign_word_pattern, is_valid, is_valid_for_transform_with_foreign, is_valid_with_foreign,
    is_valid_with_tones,
};

/// Engine action result
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    None = 0,
    Send = 1,
    Restore = 2,
}

/// Result for FFI
#[repr(C)]
pub struct Result {
    pub chars: [u32; MAX],
    pub action: u8,
    pub backspace: u8,
    pub count: u8,
    /// Flags byte:
    /// - bit 0 (0x01): key_consumed - if set, the trigger key should NOT be passed through
    ///   Used for shortcuts where the trigger key is part of the replacement
    pub flags: u8,
}

/// Flag: key was consumed by shortcut, don't pass through
pub const FLAG_KEY_CONSUMED: u8 = 0x01;

impl Result {
    pub fn none() -> Self {
        Self {
            chars: [0; MAX],
            action: Action::None as u8,
            backspace: 0,
            count: 0,
            flags: 0,
        }
    }

    pub fn send(backspace: u8, chars: &[char]) -> Self {
        // Cap at u8::MAX (255) to prevent count overflow — MAX is 256 but count is u8
        let n = chars.len().min(u8::MAX as usize);
        let mut result = Self {
            chars: [0; MAX],
            action: Action::Send as u8,
            backspace,
            count: n as u8,
            flags: 0,
        };
        for (i, &c) in chars.iter().take(n).enumerate() {
            result.chars[i] = c as u32;
        }
        result
    }

    /// Send with key_consumed flag set (shortcut consumed the trigger key)
    pub fn send_consumed(backspace: u8, chars: &[char]) -> Self {
        let mut result = Self::send(backspace, chars);
        result.flags = FLAG_KEY_CONSUMED;
        result
    }

    /// Check if key was consumed (should not be passed through)
    pub fn key_consumed(&self) -> bool {
        self.flags & FLAG_KEY_CONSUMED != 0
    }
}

/// Transform type for revert tracking
#[derive(Clone, Copy, Debug, PartialEq)]
enum Transform {
    Mark(u16, u8),
    Tone(u16, u8),
    Stroke(u16),
    /// Short-pattern stroke (d + vowel + d → đ + vowel)
    /// This is revertible if next character creates invalid Vietnamese
    ShortPatternStroke,
    /// Delayed circumflex (same-vowel trigger: aa → â, ee → ê, oo → ô)
    /// Stores the consumed vowel key for potential revert
    /// This is revertible if next consonant creates invalid pattern (like "expect" → "ễpct")
    DelayedCircumflex(u16),
    /// W as vowel ư (for revert: ww → w)
    WAsVowel,
    /// W shortcut was explicitly skipped (prevent re-transformation)
    WShortcutSkipped,
    /// Bracket as vowel: ] → ư, [ → ơ (Issue #159)
    BracketAsVowel,
}

/// Word history ring buffer capacity (stores last N committed words)
const HISTORY_CAPACITY: usize = 10;

/// Ring buffer for word history (stack-allocated, O(1) push/pop)
///
/// Used for backspace-after-space feature: when user presses backspace
/// immediately after committing a word with space, restore the previous
/// buffer state to allow editing.
struct WordHistory {
    data: [Buffer; HISTORY_CAPACITY],
    head: usize,
    len: usize,
}

impl WordHistory {
    fn new() -> Self {
        Self {
            data: std::array::from_fn(|_| Buffer::new()),
            head: 0,
            len: 0,
        }
    }

    /// Push buffer to history (overwrites oldest if full)
    fn push(&mut self, buf: Buffer) {
        self.data[self.head] = buf;
        self.head = (self.head + 1) % HISTORY_CAPACITY;
        if self.len < HISTORY_CAPACITY {
            self.len += 1;
        }
    }

    /// Pop most recent buffer from history
    fn pop(&mut self) -> Option<Buffer> {
        if self.len == 0 {
            return None;
        }
        self.head = (self.head + HISTORY_CAPACITY - 1) % HISTORY_CAPACITY;
        self.len -= 1;
        Some(self.data[self.head].clone())
    }

    fn clear(&mut self) {
        self.len = 0;
        self.head = 0;
    }
}

/// Main Vietnamese IME engine
pub struct Engine {
    buf: Buffer,
    method: u8,
    enabled: bool,
    last_transform: Option<Transform>,
    shortcuts: ShortcutTable,
    /// Raw keystroke history for ESC restore (key, caps, shift)
    raw_input: Vec<(u16, bool, bool)>,
    /// True if current word has non-letter characters before letters
    /// Used to prevent false shortcut matches (e.g., "149k" should not match "k")
    has_non_letter_prefix: bool,
    /// Skip w→ư shortcut in Telex mode (user preference)
    /// When true, typing 'w' stays as 'w' instead of converting to 'ư'
    /// Horn modifier (try_tone) still works: "ow" → "ơ", "uw" → "ư"
    skip_w_shortcut: bool,
    /// Enable bracket shortcuts: ] → ư, [ → ơ (Issue #159)
    bracket_shortcut: bool,
    /// Enable ESC key to restore raw ASCII (undo Vietnamese transforms)
    /// When false, ESC key is passed through without restoration
    esc_restore_enabled: bool,
    /// Enable free tone placement (skip validation)
    /// When true, allows placing diacritics anywhere without spelling validation
    free_tone_enabled: bool,
    /// Use modern orthography for tone placement (hoà vs hòa)
    /// When true: oà, uý (tone on second vowel)
    /// When false: òa, úy (tone on first vowel - traditional)
    modern_tone: bool,
    /// Enable English auto-restore (experimental)
    /// When true, automatically restores English words that were transformed
    /// e.g., "tẽt" → "text", "ễpct" → "expect"
    english_auto_restore: bool,
    /// Word history for backspace-after-space feature
    word_history: WordHistory,
    /// Number of spaces typed after committing a word (for backspace tracking)
    /// When this reaches 0 on backspace, we restore the committed word
    spaces_after_commit: u8,
    /// Pending breve position: position of 'a' that has deferred breve
    /// Breve on 'a' in open syllables (like "raw") is invalid Vietnamese
    /// We defer applying breve until a valid final consonant is typed
    pending_breve_pos: Option<usize>,
    /// Issue #133: Pending horn position on 'u' in "uơ" pattern
    /// When "uo" + 'w' is typed at end of syllable, only 'o' gets horn initially.
    /// If a final consonant/vowel is added, also apply horn to 'u'.
    /// Examples: "huow" → "huơ" (stays), "duow" + "c" → "dược" (u gets horn)
    pending_u_horn_pos: Option<usize>,
    /// Tracks if stroke was reverted in current word (ddd → dd)
    /// When true, subsequent 'd' keys are treated as normal letters, not stroke triggers
    /// This prevents "ddddd" from oscillating between đ and dd states
    stroke_reverted: bool,
    /// Tracks if a mark was reverted in current word
    /// Used by auto-restore to detect words like "issue", "bass" that need restoration
    had_mark_revert: bool,
    /// Pending pop from raw_input after mark revert
    /// When true, the NEXT consonant key will trigger a pop to remove the consumed modifier
    /// This differentiates: "tesst" → "test" (consonant after) vs "issue" → "issue" (vowel after)
    pending_mark_revert_pop: bool,
    /// Tracks if ANY Vietnamese transform was ever applied during this word
    /// (marks, tones, or stroke). Used to prevent false auto-restore for words
    /// with numbers/symbols that never had Vietnamese transforms applied.
    /// Example: "nhatkha1407@gmail.com" has no transforms, so shouldn't restore.
    had_any_transform: bool,
    /// Tracks if circumflex was applied from V+C+V pattern by vowel trigger (not mark key)
    /// Example: "toto" → "tôt" (second 'o' triggers circumflex on first 'o')
    /// Used for auto-restore: if no mark follows, restore on space (e.g., "toto " → "toto ")
    had_vowel_triggered_circumflex: bool,
    /// Tracks if circumflex was REVERTED by third vowel (aa→â, aaa→aa)
    /// Example: "dataa" → "dât" (after 4th key), typing 5th 'a' reverts to "data"
    /// Used in build_raw_chars to collapse double vowel at end for restore
    had_circumflex_revert: bool,
    /// Issue #211: Tracks which vowel key triggered circumflex revert (extended vowel mode)
    /// When set, subsequent same-key vowels append raw instead of re-transforming
    /// Example: aaa→aa (reverted_circumflex_key=A), aaaa→aaa (skip transform, append raw)
    reverted_circumflex_key: Option<u16>,
    /// Tracks if ANY Telex transform was applied (tone, mark, or stroke)
    /// Used for whitelist-based auto-restore to English words
    had_telex_transform: bool,
    /// Stores raw_input string when telex double pattern is detected (BEFORE modification)
    /// For stroke revert (ddd→dd), raw_input is modified to remove one 'd', but we need
    /// the original for whitelist lookup (e.g., "daddy" not "dady")
    telex_double_raw: Option<String>,
    /// Stores length of raw_input at time telex_double_raw was stored
    /// Used to append subsequent chars typed after revert
    telex_double_raw_len: usize,
    /// Issue #107: Special character prefix for shortcut matching
    /// When a shifted symbol (like #, @, $) is typed first, store it here
    /// so shortcuts like "#fne" can match even though # is normally a break char
    /// Extended: Now accumulates multiple break chars for shortcuts like "->" → "→"
    shortcut_prefix: String,
    /// Buffer was just restored from DELETE - clear on next letter input
    /// This prevents typing after restore from appending to old buffer
    restored_pending_clear: bool,
    /// Restored word was pure ASCII (no Vietnamese chars) - clear on ANY letter
    /// For Vietnamese restored words, only clear on consonant (allow mark/tone edits)
    restored_is_ascii: bool,
    /// Auto-capitalize first letter after sentence-ending punctuation
    /// Triggers: . ! ? Enter → next letter becomes uppercase
    auto_capitalize: bool,
    /// Pending capitalize state: set after sentence-ending punctuation + space
    pending_capitalize: bool,
    /// Tracks if auto-capitalize was just used on the current word
    /// Used to restore pending_capitalize when user deletes the capitalized letter
    auto_capitalize_used: bool,
    /// Tracks if we just saw sentence-ending punctuation (. ! ?)
    /// Only set pending_capitalize when space/Enter follows
    /// Issue #185: don't capitalize immediately after punctuation (e.g., google.com)
    saw_sentence_ending: bool,
    /// Allow foreign consonants (z, w, j, f) as valid initial consonants
    /// When true, these letters are accepted as Vietnamese consonants for loanwords
    allow_foreign_consonants: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            buf: Buffer::new(),
            method: 0,
            enabled: true,
            last_transform: None,
            shortcuts: ShortcutTable::with_defaults(),
            raw_input: Vec::with_capacity(64),
            has_non_letter_prefix: false,
            skip_w_shortcut: false,
            bracket_shortcut: false,    // Default: OFF (Issue #159)
            esc_restore_enabled: false, // Default: OFF (user request)
            free_tone_enabled: false,
            modern_tone: true,           // Default: modern style (hoà, thuý)
            english_auto_restore: false, // Default: OFF (experimental feature)
            word_history: WordHistory::new(),
            spaces_after_commit: 0,
            pending_breve_pos: None,
            pending_u_horn_pos: None,
            stroke_reverted: false,
            had_mark_revert: false,
            pending_mark_revert_pop: false,
            had_any_transform: false,
            had_vowel_triggered_circumflex: false,
            had_circumflex_revert: false,
            reverted_circumflex_key: None,
            had_telex_transform: false,
            telex_double_raw: None,
            telex_double_raw_len: 0,
            shortcut_prefix: String::new(),
            restored_pending_clear: false,
            restored_is_ascii: false,
            auto_capitalize: false, // Default: OFF
            pending_capitalize: false,
            auto_capitalize_used: false,
            saw_sentence_ending: false,
            allow_foreign_consonants: false, // Default: OFF
        }
    }

    pub fn set_method(&mut self, method: u8) {
        self.method = method;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.buf.clear();
            self.word_history.clear();
            self.spaces_after_commit = 0;
        }
    }

    /// Set whether to skip w→ư shortcut in Telex mode
    pub fn set_skip_w_shortcut(&mut self, skip: bool) {
        self.skip_w_shortcut = skip;
    }

    /// Set whether bracket shortcuts are enabled: ] → ư, [ → ơ (Issue #159)
    pub fn set_bracket_shortcut(&mut self, enabled: bool) {
        self.bracket_shortcut = enabled;
    }

    /// Set whether ESC key restores raw ASCII
    pub fn set_esc_restore(&mut self, enabled: bool) {
        self.esc_restore_enabled = enabled;
    }

    /// Set whether to enable free tone placement (skip validation)
    pub fn set_free_tone(&mut self, enabled: bool) {
        self.free_tone_enabled = enabled;
    }

    /// Set whether to use modern orthography for tone placement
    pub fn set_modern_tone(&mut self, modern: bool) {
        self.modern_tone = modern;
    }

    /// Set whether to enable English auto-restore (experimental)
    pub fn set_english_auto_restore(&mut self, enabled: bool) {
        self.english_auto_restore = enabled;
    }

    /// Set whether to allow foreign consonants (z, w, j, f) as valid initials
    pub fn set_allow_foreign_consonants(&mut self, enabled: bool) {
        self.allow_foreign_consonants = enabled;
    }

    /// Get whether foreign consonants are allowed
    pub fn allow_foreign_consonants(&self) -> bool {
        self.allow_foreign_consonants
    }

    pub fn shortcuts(&self) -> &ShortcutTable {
        &self.shortcuts
    }

    pub fn shortcuts_mut(&mut self) -> &mut ShortcutTable {
        &mut self.shortcuts
    }

    /// Debug: get buffer length
    pub fn debug_buffer_len(&self) -> usize {
        self.buf.len()
    }

    /// Debug: get raw_input length (alias for raw_input_len)
    pub fn debug_raw_input_len(&self) -> usize {
        self.raw_input.len()
    }

    /// Debug: check had_any_transform flag
    pub fn debug_had_any_transform(&self) -> bool {
        self.had_any_transform
    }

    /// Debug: get buffer content as string
    pub fn debug_buffer_string(&self) -> String {
        self.buf.to_full_string()
    }

    /// Debug: dump full buffer state
    pub fn debug_buffer_state(&self) -> String {
        let mut result = String::new();
        for (i, c) in self.buf.iter().enumerate() {
            result.push_str(&format!(
                "[{}] key={} tone={} mark={} stroke={}\n",
                i, c.key, c.tone, c.mark, c.stroke
            ));
        }
        result
    }

    /// Debug: check had_mark_revert flag
    pub fn debug_had_mark_revert(&self) -> bool {
        self.had_mark_revert
    }

    /// Debug: dump raw_input
    pub fn debug_raw_input(&self) -> String {
        self.raw_input
            .iter()
            .map(|(k, c, s)| format!("({},{},{})", k, c, s))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get current input method as InputMethod enum
    fn current_input_method(&self) -> InputMethod {
        match self.method {
            0 => InputMethod::Telex,
            1 => InputMethod::Vni,
            _ => InputMethod::All,
        }
    }

    /// Handle key event - main entry point
    ///
    /// # Arguments
    /// * `key` - macOS virtual keycode
    /// * `caps` - true if Caps Lock is active (for uppercase letters)
    /// * `ctrl` - true if Cmd/Ctrl/Alt is pressed (bypasses IME)
    pub fn on_key(&mut self, key: u16, caps: bool, ctrl: bool) -> Result {
        self.on_key_ext(key, caps, ctrl, false)
    }

    /// Handle key event with actual Unicode character for shortcuts.
    ///
    /// Used for Option-modified keys on macOS where the keycode doesn't change
    /// but the character is different (e.g., Option+V produces √).
    ///
    /// # Arguments
    /// * `key` - macOS virtual keycode
    /// * `caps` - true if Caps Lock is active
    /// * `ctrl` - true if Cmd/Ctrl is pressed (bypasses IME)
    /// * `shift` - true if Shift is pressed
    /// * `ch` - The actual Unicode character. If Some, uses this for shortcut matching.
    ///
    /// # Issue #275
    /// This enables shortcuts with special characters like √√ → ✅
    /// When typing ≈ç√√, we need to find √√ as a suffix match.
    pub fn on_key_with_char(
        &mut self,
        key: u16,
        caps: bool,
        ctrl: bool,
        shift: bool,
        ch: Option<char>,
    ) -> Result {
        // No character provided → fall back to normal processing
        let Some(ch) = ch else {
            return self.on_key_ext(key, caps, ctrl, shift);
        };

        // Issue #363: When ctrl=true but ch is provided (Option+key on macOS),
        // skip Vietnamese transforms but still accumulate for shortcut matching.
        // Platform passes ctrl=true to bypass Telex/VNI, but shortcuts like √√→✅
        // need the character to be accumulated in shortcut_prefix.
        if ctrl {
            self.buf.clear();
            self.raw_input.clear();
            self.word_history.clear();
            self.spaces_after_commit = 0;
            // Fall through to shortcut accumulation below
        }

        // Accumulate character for suffix matching
        self.shortcut_prefix.push(ch);

        // Try suffix matches (longest first) using char_indices to avoid allocations
        let input_method = self.current_input_method();
        for (idx, _) in self.shortcut_prefix.char_indices() {
            let suffix = &self.shortcut_prefix[idx..];
            if let Some(m) = self.shortcuts.try_match_for_method(
                suffix,
                None,
                false, // immediate, not word boundary
                input_method,
            ) {
                let output: Vec<char> = m.output.chars().collect();
                let backspace_count = (m.backspace_count as u8).saturating_sub(1);
                self.shortcut_prefix.clear();
                return Result::send_consumed(backspace_count, &output);
            }
        }

        // No match yet, let the character pass through
        Result::none()
    }

    /// Check if key+shift combo is a raw mode prefix character
    /// Raw prefixes: @ # : /
    #[allow(dead_code)] // TEMP DISABLED
    fn is_raw_prefix(key: u16, shift: bool) -> bool {
        // / doesn't need shift
        if key == keys::SLASH && !shift {
            return true;
        }
        // @ # : need shift
        if !shift {
            return false;
        }
        matches!(
            key,
            keys::N2              // @ = Shift+2
                | keys::N3        // # = Shift+3
                | keys::SEMICOLON // : = Shift+;
        )
    }

    /// Handle key event with extended parameters
    ///
    /// # Arguments
    /// * `key` - macOS virtual keycode
    /// * `caps` - true if Caps Lock is active (for uppercase letters)
    /// * `ctrl` - true if Cmd/Ctrl/Alt is pressed (bypasses IME)
    /// * `shift` - true if Shift key is pressed (for symbols like @, #, $)
    pub fn on_key_ext(&mut self, key: u16, caps: bool, ctrl: bool, shift: bool) -> Result {
        // Issue #129: Process shortcuts even when IME is disabled
        // Only bypass completely for Ctrl/Cmd modifier keys
        if ctrl {
            self.clear();
            self.word_history.clear();
            self.spaces_after_commit = 0;
            return Result::none();
        }

        // When IME is disabled, process shortcuts but skip Vietnamese transforms
        // This allows both word shortcuts (btw → by the way) and symbol shortcuts (-> → →)
        if !self.enabled {
            // Clear Vietnamese state
            self.buf.clear();
            self.raw_input.clear();
            self.word_history.clear();
            self.spaces_after_commit = 0;

            // Word boundary keys (Space, Enter): check for word shortcuts
            if key == keys::SPACE || key == keys::RETURN || key == keys::ENTER {
                if !self.shortcut_prefix.is_empty() {
                    let input_method = self.current_input_method();
                    if let Some(m) = self.shortcuts.try_match_for_method(
                        &self.shortcut_prefix,
                        None,
                        true, // is_word_boundary = true for word shortcuts
                        input_method,
                    ) {
                        let output: Vec<char> = m.output.chars().collect();
                        let backspace_count = m.backspace_count as u8;
                        self.shortcut_prefix.clear();
                        // For Space, include space in output; for Enter, don't
                        if key == keys::SPACE {
                            let mut output_with_space = output;
                            output_with_space.push(' ');
                            return Result::send(backspace_count, &output_with_space);
                        } else {
                            return Result::send(backspace_count, &output);
                        }
                    }
                }
                self.shortcut_prefix.clear();
                return Result::none();
            }

            // Backspace: pop the last accumulated char so the prefix stays in sync
            // with the on-screen text. Without this, correcting a typo (e.g.
            // "#hcn" -> Backspace -> "#hcm") falls through to the unknown-key
            // branch below which clears the whole prefix, so the shortcut never
            // matches on the following Space.
            if key == keys::DELETE {
                self.shortcut_prefix.pop();
                return Result::none();
            }

            // Break keys (punctuation): check for immediate shortcuts like "->"
            if keys::is_break_ext(key, shift) {
                if let Some(ch) = break_key_to_char(key, shift) {
                    // Word shortcuts must also fire on punctuation, not only on
                    // Space/Enter. Match the accumulated prefix BEFORE the break
                    // char is appended (e.g. "#hcm" then ","). key_char=None so the
                    // break char is not appended — the platform types it after the
                    // replacement. Mirrors the enabled-mode word-boundary behavior.
                    if !self.shortcut_prefix.is_empty() {
                        let input_method = self.current_input_method();
                        if let Some(m) = self.shortcuts.try_match_for_method(
                            &self.shortcut_prefix,
                            None,
                            true, // word boundary
                            input_method,
                        ) {
                            let output: Vec<char> = m.output.chars().collect();
                            let backspace_count = m.backspace_count as u8;
                            self.shortcut_prefix.clear();
                            return Result::send(backspace_count, &output);
                        }
                    }

                    self.shortcut_prefix.push(ch);

                    let input_method = self.current_input_method();
                    if let Some(m) = self.shortcuts.try_match_for_method(
                        &self.shortcut_prefix,
                        None,
                        false,
                        input_method,
                    ) {
                        let output: Vec<char> = m.output.chars().collect();
                        let backspace_count = (m.backspace_count as u8).saturating_sub(1);
                        self.shortcut_prefix.clear();
                        return Result::send_consumed(backspace_count, &output);
                    }
                    return Result::none();
                }
                // Break key without char mapping (Tab, arrows, etc.) - clear and pass through
                self.shortcut_prefix.clear();
                return Result::none();
            }

            // Letter and number keys: accumulate for word shortcuts (e.g., "btw", "f1", "a1")
            if let Some(ch) = utils::key_to_char(key, caps) {
                self.shortcut_prefix.push(ch);
                return Result::none();
            }

            // Unknown keys: clear shortcut prefix and pass through
            self.shortcut_prefix.clear();
            return Result::none();
        }

        // Check for word boundary shortcuts ONLY on SPACE
        // Also auto-restore invalid Vietnamese to raw English
        if key == keys::SPACE {
            // Handle pending mark revert pop on space (end of word)
            // When telex_double_raw is set, we use it directly for restore, no pop needed.
            // The telex_double_raw contains the exact original input before any modification.
            // Examples:
            //   "nurses" → telex_double_raw="nurses", use directly for restore
            //   "simss" → telex_double_raw="simss", use directly for restore (ss→sims via whitelist)
            //   "taxxi" → telex_double_raw="taxx", buffer "taxi" kept (clean, no marks)
            if self.pending_mark_revert_pop {
                self.pending_mark_revert_pop = false;
                // telex_double_raw is always set when pending_mark_revert_pop is true
                // (both set in revert_mark). Don't modify raw_input here - use
                // telex_double_raw for restore which has the correct original chars.
            }

            // First check for shortcut
            let shortcut_result = self.try_word_boundary_shortcut();
            if shortcut_result.action != 0 {
                self.clear();
                return shortcut_result;
            }

            // Auto-restore: if buffer has transforms but is invalid Vietnamese,
            // restore to raw English (like ESC but triggered by space)
            let restore_result = self.try_auto_restore_on_space();

            // If auto-restore happened, repopulate buffer with plain chars from raw_input
            // This ensures word_history stores the correct restored word (not transformed)
            // Example: "restore" → buffer was "rếtore" (6 chars), raw_input has 7 keys
            // After this, buffer has "restore" (7 chars) for correct history
            if restore_result.action != 0 {
                self.buf.clear();
                for &(key, caps, _) in &self.raw_input {
                    self.buf.push(Char::new(key, caps));
                }
            }

            // Push buffer to history before clearing (for backspace-after-space feature)
            if !self.buf.is_empty() {
                self.word_history.push(self.buf.clone());
                self.spaces_after_commit = 1; // First space after word
            } else if self.spaces_after_commit > 0 {
                // Additional space after commit - increment counter
                self.spaces_after_commit = self.spaces_after_commit.saturating_add(1);
            }
            self.auto_capitalize_used = false; // Reset on word commit

            // Issue #185: Set pending_capitalize on space AFTER sentence-ending punctuation
            // This ensures "google.com" doesn't capitalize, but "ok. ban" does
            if self.auto_capitalize && self.saw_sentence_ending {
                self.pending_capitalize = true;
                // Keep saw_sentence_ending for multiple spaces (e.g., "ok.  ban")
            }

            self.clear();
            return restore_result;
        }

        // ESC key: restore to raw ASCII (undo all Vietnamese transforms)
        // Only if esc_restore is enabled by user
        if key == keys::ESC {
            let result = if self.esc_restore_enabled {
                self.restore_to_raw()
            } else {
                Result::none()
            };
            self.clear();
            self.word_history.clear();
            self.spaces_after_commit = 0;
            return result;
        }

        // Issue #159: In Telex mode, `]` → ư and `[` → ơ
        // caps affects revert: ]] → ], uppercase (Shift/CapsLock) → }
        if self.method == 0 && (key == keys::RBRACKET || key == keys::LBRACKET) {
            if let Some(result) = self.try_bracket_as_vowel(key, caps) {
                return result;
            }
        }

        // Other break keys (punctuation, arrows, etc.)
        // Also trigger auto-restore for invalid Vietnamese before clearing
        // Use is_break_ext to handle shifted symbols like @, !, #, etc.
        if keys::is_break_ext(key, shift) {
            // Issue #107 + Bug #11: When buffer is empty AND we're at true start of input
            // (no word history), accumulate break chars for shortcuts.
            // This allows shortcuts like "#fne", "->", "=>" to work.
            // BUT: if there's word history (user just typed "du "), break chars should
            // clear history as before, not accumulate.
            let at_true_start =
                self.buf.is_empty() && self.word_history.len == 0 && self.spaces_after_commit == 0;

            // Also continue accumulating if we already started a prefix
            let continuing_prefix = self.buf.is_empty() && !self.shortcut_prefix.is_empty();

            if at_true_start || continuing_prefix {
                // Track additional break chars for backspace-after-break restore
                // When user types multiple break chars after a word (e.g., "duow;;"),
                // each break char should count as one "space" for restore purposes.
                // Without this, only the first break increments sac, so "duow;;" + <<
                // only needs 1 backspace to restore, and the 2nd backspace deletes
                // from the restored buffer instead of undoing the 2nd break char.
                if continuing_prefix && self.spaces_after_commit > 0 {
                    self.spaces_after_commit = self.spaces_after_commit.saturating_add(1);
                }

                // Reset has_non_letter_prefix when starting a new shortcut at true start
                // This ensures shortcuts like "->" work after DELETE cleared the buffer
                if at_true_start {
                    self.has_non_letter_prefix = false;
                }

                // Try to get the character for this break key
                if let Some(ch) = break_key_to_char(key, shift) {
                    self.shortcut_prefix.push(ch);

                    // Check for immediate shortcut match
                    let input_method = self.current_input_method();
                    if let Some(m) = self.shortcuts.try_match_for_method(
                        &self.shortcut_prefix,
                        None,
                        false,
                        input_method,
                    ) {
                        // Found a match! Send the replacement with key_consumed flag
                        // Note: backspace_count - 1 because current key hasn't been typed yet
                        // Example: "->" trigger has backspace_count=2, but only '-' is on screen
                        let output: Vec<char> = m.output.chars().collect();
                        let backspace_count = (m.backspace_count as u8).saturating_sub(1);
                        self.shortcut_prefix.clear();
                        return Result::send_consumed(backspace_count, &output);
                    }

                    // Issue #185: Only set saw_sentence_ending for punctuation (not Enter)
                    // pending_capitalize will be set when space follows
                    if self.auto_capitalize && is_sentence_ending_punctuation(key, shift) {
                        self.saw_sentence_ending = true;
                    } else if self.auto_capitalize && (key == keys::RETURN || key == keys::ENTER) {
                        // Enter = newline = immediate capitalize (no space needed)
                        self.pending_capitalize = true;
                        self.saw_sentence_ending = false;
                    }
                    return Result::none(); // Let the char pass through, keep accumulating
                }
            }

            // Issue #185: Only set saw_sentence_ending for punctuation (not Enter)
            // pending_capitalize will be set when space follows
            if self.auto_capitalize && is_sentence_ending_punctuation(key, shift) {
                self.saw_sentence_ending = true;
            } else if self.auto_capitalize && (key == keys::RETURN || key == keys::ENTER) {
                // Enter = newline = immediate capitalize (no space needed)
                self.pending_capitalize = true;
                self.saw_sentence_ending = false;
            } else if self.auto_capitalize && should_reset_pending_capitalize(key, shift) {
                // Reset pending for word-breaking keys (comma, semicolon, etc.)
                // But preserve pending for neutral keys (quotes, parentheses, brackets)
                self.pending_capitalize = false;
                self.saw_sentence_ending = false;
            }
            self.auto_capitalize_used = false; // Reset on word boundary

            // Issue #167: Check for word boundary shortcuts on punctuation and ENTER
            // Example: "ko." → "không." or "ko<Enter>" → "không<Enter>"
            // ENTER doesn't have a printable char, so check it separately
            let trigger_char = if key == keys::RETURN || key == keys::ENTER {
                Some('\n') // ENTER: use newline as trigger (won't be appended)
            } else {
                break_key_to_char(key, shift)
            };
            if let Some(ch) = trigger_char {
                let shortcut_result = self.try_word_boundary_shortcut_with_char(ch);
                if shortcut_result.action != 0 {
                    self.clear();
                    self.word_history.clear();
                    self.spaces_after_commit = 0;
                    return shortcut_result;
                }
            }

            let restore_result = self.try_auto_restore_on_break();

            // Push buffer to history before clearing (like SPACE handler)
            // This enables backspace-after-break to restore the word
            // Example: "ddu." → backspace → "đu" restored → "f" → "đù"
            if !self.buf.is_empty() {
                // If auto-restore happened, repopulate buffer with plain chars first
                if restore_result.action != 0 {
                    self.buf.clear();
                    for &(key, caps, _) in &self.raw_input {
                        self.buf.push(Char::new(key, caps));
                    }
                }
                self.word_history.push(self.buf.clone());
                self.spaces_after_commit = 1; // Break char counts as 1 space for restore
            } else if self.spaces_after_commit > 0 && break_key_to_char(key, shift).is_some() {
                // Buffer is empty but we recently committed a word (via space or break),
                // AND this break key produces a visible character (punctuation like ; , .).
                // Increment counter so backspace can undo all separators before restoring.
                // Navigation keys (TAB, RETURN, arrows) still clear history since they
                // indicate the user has moved away from the word.
                self.spaces_after_commit = self.spaces_after_commit.saturating_add(1);
            } else {
                self.word_history.clear();
                self.spaces_after_commit = 0;
            }

            self.clear();

            // Issue #130: After clearing buffer, store break char as potential shortcut prefix
            // This allows shortcuts like "->" to work after "abc->" (where "-" clears "abc")
            // Example: type "→abc->" should produce "→abc→"
            if let Some(ch) = break_key_to_char(key, shift) {
                self.shortcut_prefix.push(ch);
            }

            return restore_result;
        }

        if key == keys::DELETE {
            // Backspace-after-space feature: restore previous word when all spaces deleted
            // Track spaces typed after commit, restore word when counter reaches 0
            if self.spaces_after_commit > 0 && self.buf.is_empty() {
                self.spaces_after_commit -= 1;
                if self.spaces_after_commit == 0 {
                    // All spaces deleted - restore the word buffer
                    if let Some(restored_buf) = self.word_history.pop() {
                        // Restore raw_input from buffer (for ESC restore to work)
                        self.restore_raw_input_from_buffer(&restored_buf);
                        self.buf = restored_buf;
                        // Re-detect pending_u_horn_pos for "uơ" pattern at end of buffer
                        // This state is lost on clear() but needed for correct horn placement
                        // Example: "duơ" restored → type "c" → should become "dươc"
                        self.re_detect_pending_u_horn();
                        // Rebuild last_transform so a repeated modifier key still toggles
                        // the diacritic off, exactly as it would before the word was
                        // committed. Example: "sow" → "sơ" → Space → Backspace → "w"
                        // must revert to "sow", not absorb the "w".
                        self.re_detect_last_transform();
                        // Mark that buffer was restored - if user types new letter,
                        // clear buffer first (they want fresh word, not append)
                        self.restored_pending_clear = true;
                    }
                }
                // Delete one space
                return Result::send(1, &[]);
            }
            // DON'T reset spaces_after_commit here!
            // User might delete all new input and want to restore previous word.
            // Reset only happens on: break keys, ESC, ctrl, or new commit.

            // If buffer is already empty, user is deleting content from previous word
            // that we don't track. Mark this to prevent false shortcut matches.
            // e.g., "đa" + SPACE + backspace×2 + "a" should NOT match shortcut "a"
            if self.buf.is_empty() {
                self.has_non_letter_prefix = true;
            }

            // Issue: When deleting a char with mark/tone, we need to pop both the base char
            // AND the modifier key from raw_input. Example: "per" → buf=["p","ẻ"], raw=[(P),(E),(R)]
            // When backspace removes "ẻ", we must pop both R (modifier) and E (base) from raw_input.
            // Check if char being deleted has a mark before popping.
            let char_has_mark = self.buf.last().is_some_and(|c| c.mark != 0);
            // Also check for circumflex tone (from double vowel or delayed circumflex pattern)
            let char_has_circumflex = self.buf.last().is_some_and(|c| c.tone == tone::CIRCUMFLEX);

            self.buf.pop();
            self.raw_input.pop();

            // If char had a mark, pop the modifier's base vowel too
            // This ensures raw_input stays in sync with buffer
            if char_has_mark && !self.raw_input.is_empty() {
                self.raw_input.pop();
            }

            // Issue: When Vietnamese mark is repositioned (e.g., "us" → "ú", then "use" → "ue" + mark on e),
            // the deleted char absorbed the mark from previous char. After backspace, if remaining
            // buffer char has NO mark but raw_input has mark keys (s/f/r/x/j) for it, those are stale.
            // Example: "user" → buf=[u,ẻ] with mark moved from u to e, raw=[u,s,e,r]
            // After backspace: buf=[u mark=0], raw=[u,s] - but 's' is stale!
            // Fix: if remaining char has no mark but raw_input's last entry is a mark key, pop it.
            // IMPORTANT: Only run this if there are active transforms (had_any_transform).
            // After auto-restore, buffer has plain chars and raw_input has legitimate letters
            // like 'f' that would be incorrectly treated as stale mark keys.
            if self.had_any_transform && !self.buf.is_empty() && !self.raw_input.is_empty() {
                let remaining_has_no_mark = self.buf.last().is_some_and(|c| c.mark == 0);
                if remaining_has_no_mark && self.raw_input.len() >= 2 {
                    let (last_key, _, _) = self.raw_input[self.raw_input.len() - 1];
                    let mark_keys = [keys::S, keys::F, keys::R, keys::X, keys::J];
                    if mark_keys.contains(&last_key) {
                        // Stale mark key - pop it
                        self.raw_input.pop();
                    }
                }
            }

            // Issue: When char with circumflex is deleted, the stale vowel key that triggered
            // circumflex may still be in raw_input. This happens with delayed circumflex (ata→ât).
            // Example: "data" → buf=[d,â,t], raw=[d,a,t,a]. After <<: buf=[d], raw=[d,a] - 'a' is stale!
            // Fix: if deleted char had circumflex AND remaining char has no tone, pop the vowel.
            if char_has_circumflex && !self.buf.is_empty() && !self.raw_input.is_empty() {
                let remaining_has_no_tone = self.buf.last().is_some_and(|c| c.tone == 0);
                if remaining_has_no_tone && self.raw_input.len() >= 2 {
                    let (last_key, _, _) = self.raw_input[self.raw_input.len() - 1];
                    // Circumflex vowels are a, e, o
                    if matches!(last_key, keys::A | keys::E | keys::O) {
                        // Stale vowel from circumflex pattern - pop it
                        self.raw_input.pop();
                    }
                }
            }

            // When buffer becomes empty after pop, clear raw_input completely
            // This handles edge cases where modifiers may still be left over
            if self.buf.is_empty() {
                self.raw_input.clear();
            }
            self.last_transform = None;
            // Reset stroke_reverted on backspace so user can re-trigger stroke
            // e.g., "ddddd" → "dddd", then backspace×3 → "d", then "d" → "đ"
            self.stroke_reverted = false;
            // Issue #217: Reset reverted_circumflex_key on backspace so user can re-trigger circumflex
            // e.g., "eee" → "ee", then backspace×2 → "", type "phe" → "phê" (not "phee")
            self.reverted_circumflex_key = None;
            // Reset had_circumflex_revert on backspace so user can type circumflex in new words
            // e.g., "meee" → "mee", then backspace×3 → "", type "phee" → "phê" (not "phee")
            self.had_circumflex_revert = false;
            // Only reset restored_pending_clear when buffer is empty
            // (user finished deleting restored word completely)
            // If buffer still has chars, user might think they cleared everything
            // but actually didn't - let them start fresh on next letter input
            if self.buf.is_empty() {
                // Chain-restore: when a restored buffer is fully deleted via continuous
                // backspaces and word_history has more entries, enable restoring the
                // previous word on the next backspace. The `restored_pending_clear` flag
                // ensures this only happens in continuous backspace sequences — if the
                // user typed any letter (which clears the flag), the chain breaks.
                // Example: "dươc vẫn " → bs restores "vẫn" → bs×3 deletes it →
                //          bs restores "dươc" → "j" applies mark → "được"
                if self.restored_pending_clear && self.word_history.len > 0 {
                    self.spaces_after_commit = 1;
                }
                self.restored_pending_clear = false;
                // Restore pending_capitalize if user deleted the auto-capitalized letter
                // This allows: ". B" → delete B → ". " → type again → auto-capitalizes
                if self.auto_capitalize_used {
                    self.pending_capitalize = true;
                    self.auto_capitalize_used = false;
                }
            }
            return Result::none();
        }

        // After DELETE restore, determine if user wants to:
        // 1. Continue editing restored word (add tone/mark) - mark keys, tone keys
        // 2. Start fresh word - regular letters (not mark/tone keys)
        // This allows "cha" + restore + "f" → "chà" (f is mark key)
        // But "cha" + restore + "m" → "m..." (m is consonant, start fresh)
        // For pure ASCII restored words (like "shortcuts"), also clear on vowels
        // unless they're mark/tone keys (allow "ban" + restore + "s" → "bán")
        if self.restored_pending_clear && keys::is_letter(key) {
            let m = input::get(self.method);
            let is_modifier =
                m.mark(key).is_some() || m.tone(key).is_some() || m.remove(key) || m.stroke(key);
            // Clear buffer when letter is NOT a modifier (mark/tone/remove):
            // - Vietnamese restored: clear on consonant (vowels may add diacritics)
            // - ASCII restored: clear on any non-modifier letter (consonant OR vowel)
            let should_clear = if self.restored_is_ascii {
                // Pure ASCII: clear on any letter except modifier keys
                !is_modifier
            } else {
                // Vietnamese: clear only on consonant that's not a modifier
                keys::is_consonant(key) && !is_modifier
            };
            if should_clear && self.pending_u_horn_pos.is_none() {
                self.clear();
            }
            // Reset flags regardless - user is now actively typing
            self.restored_pending_clear = false;
            self.restored_is_ascii = false;
        }

        // Issue #212: Reset has_non_letter_prefix when user starts typing letter into empty buffer
        // This allows shortcuts to work after: expand → delete all → retype
        // e.g., "ko" → "không " → backspace×6 → "ko" → should expand again
        if self.buf.is_empty() && keys::is_letter(key) && self.has_non_letter_prefix {
            self.has_non_letter_prefix = false;
        }

        // Auto-capitalize: force uppercase for first letter after sentence-ending punctuation
        let was_auto_capitalized = self.pending_capitalize && keys::is_letter(key) && !caps;
        let effective_caps = if self.pending_capitalize && keys::is_letter(key) {
            self.pending_capitalize = false;
            self.saw_sentence_ending = false; // Reset after capitalizing
            self.auto_capitalize_used = true; // Track that we used auto-capitalize
            true // Force uppercase
        } else {
            // Reset pending on number (e.g., "1.5" should not capitalize "5")
            if self.pending_capitalize && keys::is_number(key) {
                self.pending_capitalize = false;
                self.saw_sentence_ending = false;
                self.auto_capitalize_used = false; // Number after punctuation, reset
            }
            // Issue #185: Reset saw_sentence_ending when letter is typed without space
            // e.g., "google.com" - 'c' typed after '.' without space, don't capitalize
            if self.saw_sentence_ending && keys::is_letter(key) {
                self.saw_sentence_ending = false;
            }
            caps
        };

        // Record raw keystroke for ESC restore (letters and numbers only)
        if keys::is_letter(key) || keys::is_number(key) {
            self.raw_input.push((key, effective_caps, shift));
        }

        let result = self.process(key, effective_caps, shift);

        // If auto-capitalize triggered for first letter of a new word and process returned none,
        // we need to send the uppercase character since the original key was lowercase
        if was_auto_capitalized && result.action == Action::None as u8 && self.buf.len() == 1 {
            if let Some(ch) = crate::utils::key_to_char(key, true) {
                return Result::send(0, &[ch]);
            }
        }

        result
    }

    /// Main processing pipeline - pattern-based
    fn process(&mut self, key: u16, caps: bool, shift: bool) -> Result {
        let m = input::get(self.method);

        // Handle pending mark revert pop: if previous key was a mark revert,
        // reset the flag. When telex_double_raw is set, we use it directly for
        // restore, so no need to modify raw_input here.
        // For vowel (issue) vs consonant (test) patterns, the whitelist and
        // restore logic will handle them correctly using telex_double_raw.
        if self.pending_mark_revert_pop && keys::is_letter(key) {
            self.pending_mark_revert_pop = false;
            // telex_double_raw is always set when pending_mark_revert_pop is true
            // (both set in revert_mark). Don't modify raw_input here.
        }

        // Revert short-pattern stroke when new letter creates invalid Vietnamese
        // This handles: "ded" → "đe" (stroke applied), then 'i' → "dedi" (invalid, revert)
        // IMPORTANT: This check must happen BEFORE any modifiers (tone, mark, etc.)
        // because the modifier key (like 'e' for circumflex) would transform the
        // buffer before we can check validity.
        //
        // We check validity using raw_input (not self.buf) because:
        // - self.buf = [đ, e] after stroke (2 chars)
        // - raw_input = [d, e, d, e] with new 'e' (4 chars - the actual full input)
        // Checking [D, E, D, E] correctly identifies "dede" as invalid.
        //
        // Skip revert for:
        // - Mark keys (s, f, r, x, j) - confirm Vietnamese intent
        // - Tone keys (a, e, o, w) that can apply to buffer - allows fast typing
        //   e.g., "dod" → "đo" + 'o' → "đô" (user typed d-o-d-o fast, intended "ddoo")
        // - Stroke keys ('d') - handled separately in try_stroke for proper revert behavior
        //   e.g., "dadd" → "dad" (d reverts stroke and adds itself, not "dadd")
        let is_mark_key = m.mark(key).is_some();
        let is_tone_key = m.tone(key).is_some();
        let is_stroke_key = m.stroke(key);

        if keys::is_letter(key)
            && !is_mark_key
            && !is_tone_key
            && !is_stroke_key
            && matches!(self.last_transform, Some(Transform::ShortPatternStroke))
        {
            // Build buffer_keys from raw_input (which already includes current key)
            let raw_keys: Vec<u16> = self.raw_input.iter().map(|&(k, _, _)| k).collect();

            // Also check if the buffer (with stroke) + new key would be valid Vietnamese
            // This handles delayed stroke patterns like "dadu" → "đau":
            // - raw_input = [d, a, d, u] (invalid as "dadu")
            // - But buffer + key = [đ, a] + [u] = "đau" (valid)
            // If buffer + key is valid, don't revert the stroke
            let mut buf_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
            buf_keys.push(key);

            // EXCEPTION: Vietnamese triple-o words (đoòng, etc.)
            // These have literal double-o which fails standard validation,
            // but they are valid Vietnamese words when completing the pattern
            // Example: buffer [đ,o,o] + 'n' → "đoon" - part of đoòng pattern
            let is_triple_o_word = self.is_vietnamese_triple_o_word();

            if !is_valid(&raw_keys) && !is_valid(&buf_keys) && !is_triple_o_word {
                // Invalid pattern - revert stroke and rebuild from raw_input
                if let Some(raw_chars) = self.build_raw_chars() {
                    // Calculate backspace: screen shows buffer content (e.g., "đe")
                    let backspace = self.buf.len() as u8;

                    // Rebuild buffer from raw_input (plain chars, no stroke)
                    self.buf.clear();
                    for &(k, c, _) in &self.raw_input {
                        self.buf.push(Char::new(k, c));
                    }
                    self.last_transform = None;

                    return Result::send(backspace, &raw_chars);
                }
            }
        }

        // Revert delayed circumflex if adding consonant creates invalid pattern
        // Example: "expect" → e + x(ngã) + p + e(circumflex) → "ễp" + c → invalid "pc" pattern
        // When "pc" or "pct" appears after vowel, it's clearly not Vietnamese → revert
        // Skip this check for mark keys (s, f, r, x, j) - they confirm Vietnamese intent
        // Skip this check for stroke keys (d) - they trigger đ transformation
        // Skip this check for tone keys (w, a, e, o in Telex) - they apply tone modifiers
        // Issue: "hojpow" was incorrectly reverting because 'w' was treated as consonant
        // creating invalid "pw" final, but 'w' is a horn modifier that should switch ộ → ợ
        let is_mark_key = m.mark(key).is_some();
        let is_stroke_key = m.stroke(key);
        let is_tone_key = m.tone(key).is_some();
        if keys::is_consonant(key)
            && !is_mark_key
            && !is_stroke_key
            && !is_tone_key
            && matches!(self.last_transform, Some(Transform::DelayedCircumflex(_)))
        {
            // Check consonants after the vowel that got circumflex
            // Find the vowel with circumflex
            let circumflex_pos = self.buf.iter().position(|c| c.tone == tone::CIRCUMFLEX);

            if let Some(vowel_pos) = circumflex_pos {
                // Get consonants after the circumflex vowel (excluding current key being added)
                let consonants_after: Vec<u16> = (vowel_pos + 1..self.buf.len())
                    .filter_map(|i| {
                        self.buf.get(i).and_then(|c| {
                            if keys::is_consonant(c.key) {
                                Some(c.key)
                            } else {
                                None
                            }
                        })
                    })
                    .collect();

                // Check if adding current consonant would create invalid pattern
                // Valid Vietnamese finals: single (c,m,n,p,t) or pairs (ch,ng,nh)
                // If we already have 1+ consonants and adding more → invalid
                let would_be_invalid = match consonants_after.len() {
                    0 => false, // First consonant after vowel - could be valid
                    1 => {
                        // Second consonant: only valid if forms ch/ng/nh
                        let pair = [consonants_after[0], key];
                        !constants::VALID_FINALS_2.contains(&pair)
                    }
                    _ => true, // 3+ consonants is always invalid
                };

                if would_be_invalid {
                    // Invalid pattern detected - revert circumflex but keep existing marks
                    // This handles "expect" → e + x(ngã) + p + e(circumflex) → "ẽp" + c
                    // Result should be "ẽpec" (keep ngã, remove circumflex, add 'c')
                    //
                    // The delayed circumflex consumed the trigger vowel (it wasn't added to buffer).
                    // So we need to restore from raw_input but preserve the mark on the vowel.

                    // Save the mark value before restoring
                    let mark_val = self.buf.get(vowel_pos).map(|c| c.mark).unwrap_or(0);

                    // Calculate backspace: clear current displayed buffer
                    let backspace = self.buf.len() as u8;

                    // Rebuild buffer from raw_input (plain chars with trigger vowel)
                    self.buf.clear();
                    for &(k, c, _) in &self.raw_input {
                        self.buf.push(Char::new(k, c));
                    }

                    // Reapply the mark to the vowel at the same position
                    // Note: vowel_pos is still valid since raw_input has all chars
                    if mark_val > 0 {
                        if let Some(c) = self.buf.get_mut(vowel_pos) {
                            c.mark = mark_val;
                        }
                    }

                    self.last_transform = None;
                    self.had_vowel_triggered_circumflex = false;
                    // Keep had_any_transform if mark was preserved
                    if mark_val == 0 {
                        self.had_any_transform = false;
                    }

                    // Build output chars (with mark if any)
                    let output: Vec<char> = self
                        .buf
                        .iter()
                        .filter_map(|c| {
                            if c.mark > 0 {
                                chars::to_char(c.key, c.caps, tone::NONE, c.mark)
                            } else {
                                utils::key_to_char(c.key, c.caps)
                            }
                        })
                        .collect();

                    return Result::send(backspace, &output);
                }
            }
        }

        // In VNI mode, if Shift is pressed with a number key, skip all modifiers
        // User wants the symbol (@ for Shift+2, # for Shift+3, etc.), not VNI marks
        let skip_vni_modifiers = self.method == 1 && shift && keys::is_number(key);

        // B1 fix: a VNI digit (1-9, 0) is a stroke/tone/mark/remove modifier only
        // when it continues an in-progress syllable, i.e. the buffer's last char
        // is a letter. If the buffer is empty or already ends in a digit/symbol
        // (plain numeric input like "abc123", or a stale buffer left over from an
        // untracked key such as an arrow/nav press), there is nothing markable to
        // attach to - the digit must pass through as a literal instead of hunting
        // an earlier vowel in the buffer. Intentional modifiers like "a1" (a+sắc)
        // or "o6" (o+circumflex) are unaffected since the buffer ends in the
        // letter that was just typed.
        let vni_digit_non_markable = self.method == 1
            && keys::is_number(key)
            && !self.buf.last().is_some_and(|c| keys::is_letter(c.key));

        // Skip modifiers after circumflex revert (ooo→oo, eee→ee, aaa→aa)
        // Example: "booo" → "boo" (revert), then "s" → "boos" (not "boós")
        // Example: "seee" → "see" (revert), then "m" → "seem" (not "seém")
        // This applies to ALL subsequent keys until word ends (space/break clears flag)
        // EXCEPTION: Vietnamese triple-o words with valid tones (s=sắc, f=huyền)
        // Triple-o words only use sắc or huyền tones, NOT ngã (x), hỏi (r), nặng (j)
        let is_valid_triple_o_tone =
            (key == keys::S || key == keys::F) && self.is_vietnamese_triple_o_word();
        let skip_after_revert = self.had_circumflex_revert && !is_valid_triple_o_tone;

        // Check modifiers by scanning buffer for patterns

        // 1. Stroke modifier (d → đ)
        if !skip_vni_modifiers && !vni_digit_non_markable && m.stroke(key) {
            if let Some(result) = self.try_stroke(key, caps) {
                return result;
            }
        }

        // 2. Tone modifier (circumflex, horn, breve)
        if !skip_vni_modifiers && !vni_digit_non_markable && !skip_after_revert {
            if let Some(tone_type) = m.tone(key) {
                let targets = m.tone_targets(key);
                if let Some(result) = self.try_tone(key, caps, tone_type, targets) {
                    return result;
                }
            }
        }

        // 3. Mark modifier
        if !skip_vni_modifiers && !vni_digit_non_markable && !skip_after_revert {
            if let Some(mark_val) = m.mark(key) {
                if let Some(result) = self.try_mark(key, caps, mark_val) {
                    return result;
                }
            }
        }

        // 4. Remove modifier
        // Only consume key if there's something to remove; otherwise fall through to normal letter
        // This allows shortcuts like "zz" to work when buffer has no marks/tones to remove
        if !skip_vni_modifiers && !vni_digit_non_markable && m.remove(key) {
            if let Some(result) = self.try_remove() {
                return result;
            }
        }

        // 5. In Telex: "w" as vowel "ư" when valid Vietnamese context
        // Examples: "w" → "ư", "nhw" → "như", but "kw" → "kw" (invalid)
        if self.method == 0 && key == keys::W {
            if let Some(result) = self.try_w_as_vowel(caps) {
                return result;
            }
        }

        // Not a modifier - normal letter
        self.handle_normal_letter(key, caps)
    }

    /// Try "w" as vowel "ư" in Telex mode
    ///
    /// Rules:
    /// - "w" alone → "ư"
    /// - "nhw" → "như" (valid consonant + ư)
    /// - "kw" → "kw" (invalid, k cannot precede ư)
    /// - "ww" → revert to "w" (shortcut skipped)
    /// - "www" → "ww" (subsequent w just adds normally)
    fn try_w_as_vowel(&mut self, caps: bool) -> Option<Result> {
        // Issue #44: If breve is pending (deferred due to open syllable),
        // don't convert w→ư. Let w be added as regular letter.
        // Example: "aw" → breve deferred → should stay "aw", not become "aư"
        if self.pending_breve_pos.is_some() {
            return None;
        }

        // If user disabled w→ư shortcut, skip w→ư conversion entirely
        // Horn modifier (try_tone) still works: "ow" → "ơ", "uw" → "ư"
        if self.skip_w_shortcut {
            return None;
        }

        // If shortcut was previously skipped, don't try again
        if matches!(self.last_transform, Some(Transform::WShortcutSkipped)) {
            return None;
        }

        // If we already have a complete ươ compound, swallow the second 'w'
        // This handles "dduwowcj" where the second 'w' should be no-op
        // Use send(0, []) to intercept and consume the key without output
        if self.has_complete_uo_compound() {
            return Some(Result::send(0, &[]));
        }

        // Check revert: ww → w (skip shortcut)
        // Preserve original case: Ww → W, wW → w
        if let Some(Transform::WAsVowel) = self.last_transform {
            self.last_transform = Some(Transform::WShortcutSkipped);
            // Track ww pattern for whitelist-based restore
            self.had_telex_transform = true;
            // Store raw_input BEFORE modification for whitelist lookup
            self.telex_double_raw = Some(self.get_raw_input_string_preserve_case());
            // Get original case from buffer before popping
            let original_caps = self.buf.last().map(|c| c.caps).unwrap_or(caps);
            self.buf.pop();
            self.buf.push(Char::new(keys::W, original_caps));
            // Fix raw_input: "ww" typed → raw has [w,w] but buffer is "w"
            // Remove the shortcut-triggering 'w' from raw_input so restore works correctly
            // raw_input: [a, w, w] → [a, w] (remove first 'w' that triggered shortcut)
            // This ensures "awwait" → "await" not "awwait" on auto-restore
            if self.raw_input.len() >= 2 {
                let current = self.raw_input.pop(); // current 'w' (just added)
                self.raw_input.pop(); // shortcut-trigger 'w' (consumed, discard)
                if let Some(c) = current {
                    self.raw_input.push(c);
                }
            }
            // Store length AFTER modification
            self.telex_double_raw_len = self.raw_input.len();
            let w = if original_caps { 'W' } else { 'w' };
            return Some(Result::send(1, &[w]));
        }

        // Try adding U (ư base) to buffer and validate
        self.buf.push(Char::new(keys::U, caps));

        // Set horn tone to make it ư
        if let Some(c) = self.buf.get_mut(self.buf.len() - 1) {
            c.tone = tone::HORN;
        }

        // Validate: is this valid Vietnamese?
        // Use is_valid_with_tones to check modifier requirements (e.g., E+U needs circumflex)
        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
        let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
        if is_valid_with_tones(&buffer_keys, &buffer_tones) {
            self.last_transform = Some(Transform::WAsVowel);
            self.had_any_transform = true;

            // W shortcut adds ư without replacing anything on screen
            // (the raw 'w' key was never output, so no backspace needed)
            let vowel_char = chars::to_char(keys::U, caps, tone::HORN, 0).unwrap();
            return Some(Result::send(0, &[vowel_char]));
        }

        // Invalid - remove the U we added
        self.buf.pop();
        None
    }

    /// Try to apply stroke transformation by scanning buffer
    ///
    /// Issue #51: In Telex mode, only apply stroke when the new 'd' is ADJACENT to
    /// an existing 'd'. According to Vietnamese Telex docs (Section 9.2.2), "dd" → "đ"
    /// should only work when the two 'd's are consecutive. For words like "deadline",
    /// the 'd's are separated by "ea", so stroke should NOT apply.
    ///
    /// In VNI mode, '9' is always an intentional stroke command (not a letter), so
    /// delayed stroke is allowed (e.g., "duong9" → "đuong").
    fn try_stroke(&mut self, key: u16, caps: bool) -> Option<Result> {
        // If stroke was already reverted in this word (ddd → dd), skip further stroke attempts
        // This prevents "ddddd" from oscillating and ensures subsequent 'd's are just letters
        if self.stroke_reverted && key == keys::D {
            return None;
        }

        // Check for stroke revert first: ddd → dd
        // If last transform was stroke and same key pressed again, revert the stroke
        if let Some(Transform::Stroke(last_key)) = self.last_transform {
            if last_key == key {
                // Find the stroked 'd' to revert
                if let Some(pos) = self.buf.iter().position(|c| c.key == keys::D && c.stroke) {
                    // Revert: un-stroke the 'd'
                    if let Some(c) = self.buf.get_mut(pos) {
                        c.stroke = false;
                    }
                    // Add another 'd' as normal char (preserve caps state)
                    self.buf.push(Char::new(key, caps));
                    self.last_transform = None;
                    // Mark that stroke was reverted - subsequent 'd' keys will be normal letters
                    self.stroke_reverted = true;
                    // Track dd pattern for whitelist-based restore
                    self.had_telex_transform = true;
                    // Store raw_input BEFORE modification for whitelist lookup
                    // For "daddy": raw_input = [d,a,d,d] → store "dadd"
                    self.telex_double_raw = Some(self.get_raw_input_string_preserve_case());
                    // Fix raw_input: "ddd" typed → raw has [d,d,d] but buffer is "dd"
                    // Remove the stroke-triggering 'd' from raw_input so restore works correctly
                    // raw_input: [d, d, d] → [d, d] (remove middle 'd' that triggered stroke)
                    // This ensures "didd" → "did" not "didd" on auto-restore
                    if self.raw_input.len() >= 2 {
                        let current = self.raw_input.pop(); // current 'd' (just added)
                        self.raw_input.pop(); // stroke-trigger 'd' (consumed, discard)
                        if let Some(c) = current {
                            self.raw_input.push(c);
                        }
                    }
                    // Store length AFTER modification - for "daddy": [d,a,d] → len=3
                    // Subsequent chars (y) start at position 3
                    self.telex_double_raw_len = self.raw_input.len();
                    // Use rebuild_from_after_insert because the new 'd' was just pushed
                    // and hasn't been displayed on screen yet
                    return Some(self.rebuild_from_after_insert(pos));
                }
            }
        }

        // Check for short-pattern stroke revert: dadd → dad
        // If last transform was short-pattern stroke and 'd' is pressed again, revert the stroke
        // This is similar to the ddd → dd revert above, but for delayed stroke patterns
        if let Some(Transform::ShortPatternStroke) = self.last_transform {
            if key == keys::D {
                // Find the stroked 'd' to revert
                if let Some(pos) = self.buf.iter().position(|c| c.key == keys::D && c.stroke) {
                    // Revert: un-stroke the 'd'
                    if let Some(c) = self.buf.get_mut(pos) {
                        c.stroke = false;
                    }
                    // Add another 'd' as normal char (preserve caps state)
                    self.buf.push(Char::new(key, caps));
                    self.last_transform = None;
                    // Mark that stroke was reverted - subsequent 'd' keys will be normal letters
                    self.stroke_reverted = true;
                    // Track dd pattern for whitelist-based restore
                    self.had_telex_transform = true;
                    // Store raw_input BEFORE modification for whitelist lookup
                    self.telex_double_raw = Some(self.get_raw_input_string_preserve_case());
                    // Fix raw_input same as above
                    if self.raw_input.len() >= 2 {
                        let current = self.raw_input.pop();
                        self.raw_input.pop();
                        if let Some(c) = current {
                            self.raw_input.push(c);
                        }
                    }
                    // Store length AFTER modification
                    self.telex_double_raw_len = self.raw_input.len();
                    // Use rebuild_from_after_insert because the new 'd' was just pushed
                    // and hasn't been displayed on screen yet
                    return Some(self.rebuild_from_after_insert(pos));
                }
            }
        }

        // Collect buffer keys once for all validations
        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
        let has_vowel = buffer_keys.iter().any(|&k| keys::is_vowel(k));

        // Check for circumflex trigger pattern: D + V1 + C + V2 + V2 (same vowel at end)
        // Example: "duoto" = [D, U, O, T, O] → after circumflex becomes "duôt" = [D, U, Ô, T]
        // This pattern specifically allows stroke on initial 'd' even when vowel pattern looks invalid
        // Conditions:
        // 1. Initial must be 'd' (this is try_stroke, so we're checking for stroke)
        // 2. Last two vowels must be same (circumflex trigger)
        // 3. There must be a non-extending final (t, m, p) between them
        let has_circumflex_trigger_pattern = {
            let first_is_d = buffer_keys.first() == Some(&keys::D);
            // After circumflex revert (e.g., "dât" → "data"), buffer has [D,A,T,A]
            // which falsely matches D + V1 + C + V2 pattern. The duplicate vowels are
            // from the revert, not intentional Vietnamese input, so skip detection.
            let after_revert = self.had_circumflex_revert;

            if first_is_d && !after_revert {
                let vowel_positions: Vec<(usize, u16)> = buffer_keys
                    .iter()
                    .enumerate()
                    .filter(|(_, &k)| keys::is_vowel(k))
                    .map(|(i, &k)| (i, k))
                    .collect();

                // Check if last two same vowels are separated by non-extending final (t, m, p)
                if vowel_positions.len() >= 2 {
                    let (pos1, key1) = vowel_positions[vowel_positions.len() - 2];
                    let (pos2, key2) = vowel_positions[vowel_positions.len() - 1];

                    // Must be same vowel that can take circumflex
                    let same_circumflex_vowel =
                        key1 == key2 && matches!(key1, keys::A | keys::E | keys::O);

                    // Check consonants between the vowels
                    let consonants_between: Vec<u16> = (pos1 + 1..pos2)
                        .filter_map(|j| buffer_keys.get(j).copied())
                        .filter(|&k| !keys::is_vowel(k))
                        .collect();

                    // Must have exactly one non-extending final consonant (t, m, p)
                    let has_non_extending_final = consonants_between.len() == 1
                        && matches!(consonants_between[0], keys::T | keys::M | keys::P);

                    same_circumflex_vowel && has_non_extending_final
                } else {
                    false
                }
            } else {
                false
            }
        };

        // Find position of un-stroked 'd' to apply stroke
        // Also track if this is a short pattern stroke (revertible)
        let (pos, is_short_pattern_stroke) = if self.method == 0 {
            // Telex: First try adjacent 'd' (last char is un-stroked d)
            let last_pos = self.buf.len().checked_sub(1)?;
            let last_char = self.buf.get(last_pos)?;

            if last_char.key == keys::D && !last_char.stroke {
                // Adjacent stroke: "dd" → "đ" (not a short pattern)
                (last_pos, false)
            } else {
                // Delayed stroke: check if initial 'd' can be stroked
                // Only allow if: first char is 'd', has vowel, and forms valid Vietnamese
                let first_char = self.buf.get(0)?;
                if first_char.key != keys::D || first_char.stroke {
                    return None;
                }

                // Must have at least one vowel for delayed stroke
                if !has_vowel {
                    return None;
                }

                // Must form valid Vietnamese (including vowel pattern) for delayed stroke
                // Use is_valid() instead of is_valid_for_transform() to check vowel patterns
                // This prevents "dea" + "d" → "đea" (invalid "ea" diphthong)
                // BUT: Allow circumflex trigger patterns even if they look invalid now
                // ALSO: Allow Vietnamese triple-o words (đoòng) which have literal double-o
                if !has_circumflex_trigger_pattern
                    && !is_valid_with_foreign(&buffer_keys, self.allow_foreign_consonants)
                    && !self.is_vietnamese_triple_o_word()
                {
                    return None;
                }

                // For open syllables (d + vowel only), defer stroke to try_mark
                // UNLESS:
                // - A mark is already applied (confirms Vietnamese intent)
                // - The triggering key is 'd' AND buffer is vowels-only after initial 'd'
                //   This allows "did" → "đi", "dod" → "đo", "duod" → "đuo", etc.
                // This prevents "de" + "d" → "đe" while allowing:
                // - "dods" → "đó" (mark key triggers stroke)
                // - "dojd" → "đọ" (mark already present, stroke applies immediately)
                // - "did" → "đi" (d triggers stroke on short open syllable)
                // - "duod" → "đuo" (d triggers stroke on diphthong open syllable)
                let syllable = syllable::parse(&buffer_keys);
                let has_mark_applied = self.buf.iter().any(|c| c.mark > 0);
                // Allow 'd' to trigger immediate stroke on open syllables with d + vowels only
                // Examples: "di" (len 2), "duo" (len 3), "dua" (len 3), "duoi" (len 4)
                let is_d_vowels_only_pattern = key == keys::D
                    && self.buf.len() >= 2
                    && self.buf.iter().skip(1).all(|c| keys::is_vowel(c.key));
                // For circumflex trigger pattern (duoto → đuôt), we should allow stroke
                // even if syllable.final_c is empty, because the pattern will become valid
                // after circumflex is applied
                if syllable.final_c.is_empty()
                    && !has_mark_applied
                    && !is_d_vowels_only_pattern
                    && !has_circumflex_trigger_pattern
                {
                    // Open syllable without mark, not d+vowels pattern, not circumflex trigger
                    return None;
                }

                // Track if this is a short pattern stroke (can be reverted later)
                // Only revertible if no mark applied - mark confirms Vietnamese intent
                (0, is_d_vowels_only_pattern && !has_mark_applied)
            }
        } else {
            // VNI: Allow delayed stroke - find first un-stroked 'd' anywhere in buffer
            // '9' is always intentional stroke command, not a letter
            let pos = self
                .buf
                .iter()
                .enumerate()
                .find(|(_, c)| c.key == keys::D && !c.stroke)
                .map(|(i, _)| i)?;
            (pos, false) // VNI never uses short pattern stroke
        };

        // Check revert: if last transform was stroke on same key at same position
        if let Some(Transform::Stroke(last_key)) = self.last_transform {
            if last_key == key {
                return Some(self.revert_stroke(key, pos));
            }
        }

        // Validate buffer structure before applying stroke
        // Only validate if buffer has vowels (complete syllable)
        // Allow stroke on initial consonant before vowel is typed (e.g., "dd" → "đ" then "đi")
        // Skip validation if free_tone mode is enabled
        // Also skip validation for circumflex trigger patterns (duoto → đuôt)
        // Also skip validation for Vietnamese triple-o words (đoòng) which have literal double-o
        if !self.free_tone_enabled
            && has_vowel
            && !has_circumflex_trigger_pattern
            && !self.is_vietnamese_triple_o_word()
            && !is_valid_for_transform_with_foreign(&buffer_keys, self.allow_foreign_consonants)
        {
            return None;
        }

        // Mark as stroked
        if let Some(c) = self.buf.get_mut(pos) {
            c.stroke = true;
        }

        // Track transform type for potential revert
        self.last_transform = if is_short_pattern_stroke {
            Some(Transform::ShortPatternStroke)
        } else {
            Some(Transform::Stroke(key))
        };
        self.had_any_transform = true;
        self.had_telex_transform = true; // dd pattern detected
        Some(self.rebuild_from(pos))
    }

    /// Try to apply tone transformation by scanning buffer for targets
    fn try_tone(
        &mut self,
        key: u16,
        caps: bool,
        tone_type: ToneType,
        targets: &[u16],
    ) -> Option<Result> {
        if self.buf.is_empty() {
            return None;
        }

        // Issue #44: Cancel pending breve if same modifier pressed again ("aww" → "aw")
        // When breve was deferred and user presses 'w' again, cancel without adding another 'w'
        if self.pending_breve_pos.is_some()
            && (tone_type == ToneType::Horn || tone_type == ToneType::Breve)
        {
            // Cancel the pending breve - user doesn't want Vietnamese
            self.pending_breve_pos = None;
            // Return "consumed but no change" to prevent 'w' from being typed
            // action=Send with 0 backspace and 0 chars effectively consumes the key
            return Some(Result::send(0, &[]));
        }

        // Check revert first (same key pressed twice)
        if let Some(Transform::Tone(last_key, _)) = self.last_transform {
            if last_key == key {
                return Some(self.revert_tone(key, caps));
            }
        }

        // Issue #211: Extended vowel mode - skip circumflex transform after revert
        // After aaa→aa revert, aaaa should become aaa (append raw), not aâ (re-transform)
        if self.reverted_circumflex_key == Some(key) && tone_type == ToneType::Circumflex {
            return None; // Let normal letter handling append raw vowel
        }

        // Validate buffer structure (not vowel patterns - those are checked after transform)
        // Skip validation if free_tone mode is enabled
        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();

        if !self.free_tone_enabled
            && !is_valid_for_transform_with_foreign(&buffer_keys, self.allow_foreign_consonants)
        {
            return None;
        }

        // Check for invalid "-ing" rhyme: Vietnamese uses "-inh", NOT "-ing" with tone
        // Examples: "thíng" is invalid (things), but "tính" is valid
        // If vowel is 'i' and final is 'ng', reject tone marks
        if !self.free_tone_enabled {
            let syllable = syllable::parse(&buffer_keys);
            if syllable.vowel.len() == 1 && syllable.final_c.len() == 2 {
                let vowel_key = buffer_keys[syllable.vowel[0]];
                let final_keys = [
                    buffer_keys[syllable.final_c[0]],
                    buffer_keys[syllable.final_c[1]],
                ];
                // i + ng = invalid Vietnamese rhyme for tone marks
                if vowel_key == keys::I && final_keys == [keys::N, keys::G] {
                    return None;
                }
            }
        }

        let tone_val = tone_type.value();

        // Check if we're switching from one tone to another (e.g., ô → ơ)
        // Find vowels that have a DIFFERENT tone (to switch) or NO tone (to add)
        let is_switching = self
            .buf
            .iter()
            .any(|c| targets.contains(&c.key) && c.tone != tone::NONE && c.tone != tone_val);

        // Scan buffer for eligible target vowels
        let mut target_positions = Vec::new();

        // Special case: uo/ou compound for horn - find adjacent pair only
        // But ONLY apply compound logic when BOTH vowels are plain (not when switching)
        if tone_type == ToneType::Horn && !is_switching {
            if let Some((pos1, pos2)) = self.find_uo_compound_positions() {
                if let (Some(c1), Some(c2)) = (self.buf.get(pos1), self.buf.get(pos2)) {
                    // Only apply compound when BOTH vowels have no tone
                    if c1.tone == tone::NONE && c2.tone == tone::NONE {
                        // Issue #133: Check if "uo" pattern is at end of syllable (no final)
                        // If no final consonant/vowel after "uo", only apply horn to 'o'
                        // Examples: "huow" → "huơ", "khuow" → "khuơ"
                        // But: "duowc" → "dược", "muowif" → "mười" (both get horn)
                        let is_uo_pattern = c1.key == keys::U && c2.key == keys::O;
                        let has_final = self.buf.get(pos2 + 1).is_some();

                        // Check if 'u' is preceded by 'Q' (qu-initial consonant cluster)
                        // In "Qu-", the 'u' is part of the initial and should not get horn
                        // Examples: "Quoiws" → "Quới" (not "Qưới"), "quốc" (not "qước")
                        let preceded_by_q =
                            pos1 > 0 && self.buf.get(pos1 - 1).map(|c| c.key) == Some(keys::Q);

                        // Check for "quoa" pattern: Q + U + O + A
                        // In this case, skip U+O compound and let phonology rules handle O+A
                        // W should apply breve to A, not horn to O
                        // Example: "quoắt" = qu + oă + t
                        let has_a_after_o = self
                            .buf
                            .get(pos2 + 1)
                            .map(|c| c.key == keys::A)
                            .unwrap_or(false);

                        if preceded_by_q && has_a_after_o {
                            // Skip compound handling - let find_horn_target_with_switch handle it
                            // This will trigger O+A breve pattern in phonology rules
                        } else if preceded_by_q {
                            // "Qu-" pattern - only second vowel gets horn
                            target_positions.push(pos2);
                            self.pending_u_horn_pos = None;
                        } else if is_uo_pattern && !has_final {
                            // "uơ" pattern - only 'o' gets horn initially
                            // Set pending so 'u' gets horn if final consonant/vowel is added
                            target_positions.push(pos2);
                            self.pending_u_horn_pos = Some(pos1);
                        } else {
                            // "ươ" pattern (or has final) - both get horn
                            target_positions.push(pos1);
                            target_positions.push(pos2);
                            self.pending_u_horn_pos = None;
                        }
                    }
                }
            }
        }

        // Normal case: find last matching target
        if target_positions.is_empty() {
            if is_switching {
                // When switching, ONLY target vowels that already have a diacritic
                // (don't add diacritics to plain vowels during switch)
                for (i, c) in self.buf.iter().enumerate().rev() {
                    if targets.contains(&c.key) && c.tone != tone::NONE && c.tone != tone_val {
                        target_positions.push(i);
                        break;
                    }
                }
            } else if tone_type == ToneType::Horn {
                // For horn modifier, apply smart vowel selection based on Vietnamese phonology
                target_positions = self.find_horn_target_with_switch(targets, tone_val);
            } else {
                // Non-horn modifiers (circumflex): use standard target matching
                // For Telex circumflex (aa, ee, oo pattern), require either:
                // 1. Target at LAST position (immediate doubling: "oo" → "ô")
                // 2. No consonants between target and end (delayed diphthong: "oio" → "ôi")
                // This prevents transformation in words like "teacher" where consonants
                // (c, h) appear between the two 'e's
                let is_telex_circumflex = self.method == 0
                    && tone_type == ToneType::Circumflex
                    && matches!(key, keys::A | keys::E | keys::O);

                // Issue #312: If any vowel already has a tone (horn/circumflex/breve),
                // don't trigger same-vowel circumflex. The typed vowel should append raw.
                // Example: "chưa" + "a" → "chưaa" (NOT "chưâ")
                //
                // Also check for English patterns like "expect":
                // E + X(mark) + P + E → mark comes BEFORE consonant, block circumflex
                // vs Vietnamese "onro": O + N + R(mark) + O → mark comes AFTER consonant, allow
                if is_telex_circumflex {
                    let any_vowel_has_tone = self
                        .buf
                        .iter()
                        .filter(|c| keys::is_vowel(c.key))
                        .any(|c| c.has_tone());

                    if any_vowel_has_tone {
                        // Skip circumflex, let the vowel append as raw letter
                        return None;
                    }

                    // Check if buffer has multiple vowel types and any has a mark
                    // Skip circumflex if it would create invalid diphthong (like ôà, âo)
                    // But allow if circumflex creates valid pattern (like uê, iê, yê)
                    // Examples:
                    // - "toà" + "a" → [O,A], âo invalid → skip → "toàa"
                    // - "ué" + "e" → [U,E], uê valid → allow → "uế"
                    let vowel_chars: Vec<_> =
                        self.buf.iter().filter(|c| keys::is_vowel(c.key)).collect();

                    let has_any_mark = vowel_chars.iter().any(|c| c.has_mark());

                    // Check for gi-initial pattern: G + I at start where I is part of consonant cluster
                    // In Vietnamese, "gi" is a consonant cluster, so "giấc" (giacsa) has only 1 vowel (a)
                    // The I in "gi" should not be counted as a separate vowel type
                    let is_gi_initial_here = self.buf.get(0).map(|c| c.key) == Some(keys::G)
                        && self.buf.get(1).is_some_and(|c| c.key == keys::I);

                    // Exclude I from vowel types if it's part of gi-initial
                    let unique_vowel_types: std::collections::HashSet<u16> = if is_gi_initial_here {
                        vowel_chars
                            .iter()
                            .filter(|c| c.key != keys::I)
                            .map(|c| c.key)
                            .collect()
                    } else {
                        vowel_chars.iter().map(|c| c.key).collect()
                    };
                    let has_multiple_vowel_types = unique_vowel_types.len() > 1;

                    if has_any_mark && has_multiple_vowel_types {
                        // Check if circumflex on V2 (the key) creates a valid pattern
                        // Valid V2 circumflex patterns: iê, uê, yê, uô
                        // Invalid: oa→oâ, ao→âo, ae→âe, etc.
                        let other_vowel = unique_vowel_types.iter().find(|&&v| v != key).copied();

                        // Check if this is a same-vowel trigger for V1 circumflex
                        // Example: "dausa" (d-á-u + a) → circumflex on first 'a' → "dấu"
                        // The trigger 'a' matches existing 'a' in buffer
                        let is_same_vowel_trigger = unique_vowel_types.contains(&key);

                        // V1 circumflex patterns: circumflex on FIRST vowel of diphthong
                        // These patterns have the trigger vowel + another vowel forming valid diphthong
                        // âu, ây (A with circumflex + U/Y)
                        // êu (E with circumflex + U) - already in V1_CIRCUMFLEX_REQUIRED
                        // ôi (O with circumflex + I)
                        let v1_circumflex_diphthongs: &[[u16; 2]] = &[
                            [keys::A, keys::U], // âu - "dấu"
                            [keys::A, keys::Y], // ây - "dây"
                            [keys::E, keys::U], // êu - "nếu"
                            [keys::O, keys::I], // ôi - "tối"
                        ];

                        let is_valid_v1_circumflex = is_same_vowel_trigger
                            && other_vowel
                                .is_some_and(|v| v1_circumflex_diphthongs.contains(&[key, v]));

                        // Patterns where circumflex on V2 is valid
                        let v2_circumflex_valid: &[[u16; 2]] = &[
                            [keys::I, keys::E], // iê
                            [keys::U, keys::E], // uê
                            [keys::Y, keys::E], // yê
                            [keys::U, keys::O], // uô
                        ];

                        let is_valid_v2_circumflex =
                            other_vowel.is_some_and(|v| v2_circumflex_valid.contains(&[v, key]));

                        if !is_valid_v2_circumflex && !is_valid_v1_circumflex {
                            // Invalid pattern → skip circumflex
                            return None;
                        }
                    }

                    // Check if adding this vowel would create a valid triphthong
                    // If so, skip circumflex and let the vowel append raw
                    // Example: "oe" + "o" → [O, E, O] = "oeo" triphthong → skip circumflex
                    // BUT: Only check this if the last char in buffer is a vowel
                    // If there's a consonant at the end (e.g., "boem"), then same-vowel
                    // trigger applies instead of triphthong building
                    let last_is_vowel = self.buf.last().is_some_and(|c| keys::is_vowel(c.key));

                    if last_is_vowel {
                        let vowels: Vec<u16> = self
                            .buf
                            .iter()
                            .filter(|c| keys::is_vowel(c.key))
                            .map(|c| c.key)
                            .collect();

                        if vowels.len() == 2 {
                            let potential_triphthong = [vowels[0], vowels[1], key];
                            if constants::VALID_TRIPHTHONGS.contains(&potential_triphthong) {
                                // This would create a valid triphthong, skip circumflex
                                return None;
                            }
                        }

                        // Check for V1-V2-V1 pattern in last 2 vowels + new key
                        // Example: "queue" has buffer vowels [U, E, U], new key = E
                        // Last 2 vowels = [E, U], new key = E → pattern is E-U-E (V1-V2-V1)
                        // ONLY block when:
                        // 1. NO Vietnamese indicators present (mark/stroke)
                        // 2. There's a consonant initial (foreign word pattern)
                        // 3. NOT a valid Vietnamese triphthong pattern
                        // This allows: "oio" → "ôi" (no initial, valid VN interjection)
                        // This allows: "hieu" + e → "hiêu" (iêu is valid VN triphthong)
                        // But blocks: "queue" → "quêu" (has "qu" initial, foreign word)
                        let has_vn_indicator = self.buf.iter().any(|c| c.mark > 0 || c.stroke);
                        let has_initial =
                            self.buf.get(0).is_some_and(|c| keys::is_consonant(c.key));

                        if !has_vn_indicator && has_initial && vowels.len() >= 2 {
                            let last_two = &vowels[vowels.len() - 2..];
                            let v1 = last_two[0]; // second-to-last vowel
                            let v2 = last_two[1]; // last vowel
                                                  // V1-V2-V1 pattern: new key matches v1 but not v2
                            if key == v1 && key != v2 {
                                // Exception: Allow circumflex for valid Vietnamese triphthongs
                                // e.g., [i, e, u] = iêu (hiểu), [y, e, u] = yêu, [u, e, u] = uêu (nguều)
                                // These require circumflex on E (middle vowel)
                                // The trigger 'e' is the same as v1, which triggers circumflex
                                //
                                // BUT: Exclude Q + U pattern (like "queue")
                                // In Vietnamese, Q only appears as part of "qu" initial cluster
                                // For Vietnamese "qu" words: qu + vowels where U is part of initial
                                // For English "queue": U appears again AFTER the first U
                                // Detection: initial Q + first vowel U + U appears again later
                                let initial_q = self.buf.get(0).is_some_and(|c| c.key == keys::Q);
                                let first_vowel_u = vowels.first().is_some_and(|&v| v == keys::U);
                                // Check if U appears again after the first position (English pattern)
                                // BUT: If the final U is part of a valid diphthong (like êu in quêu),
                                // it's Vietnamese, not English "queue"
                                let has_repeated_u =
                                    vowels.len() > 1 && vowels[1..].contains(&keys::U);

                                // Check if the repeated U is the final vowel in a valid diphthong
                                // For "quêu": vowels = [U, E, U] → [E, U] is valid diphthong êu
                                // This means the second U is a Vietnamese glide, not English queue
                                let final_u_is_valid_glide = if has_repeated_u && vowels.len() >= 2
                                {
                                    let last_vowel = vowels[vowels.len() - 1];
                                    let second_last = vowels[vowels.len() - 2];
                                    // If last vowel is U and [second_last, U] is a valid diphthong
                                    // then this U is a glide, not English pattern
                                    last_vowel == keys::U
                                        && matches!(
                                            second_last,
                                            keys::A | keys::E | keys::I | keys::O
                                        )
                                } else {
                                    false
                                };

                                // English "qu" pattern: Q initial + U first + U repeats later
                                // BUT NOT when the final U is a valid Vietnamese glide
                                // Vietnamese "qu" pattern: Q initial + U first + no U repeat (quây, quá)
                                // Vietnamese "quêu" pattern: Q initial + U first + U as final glide
                                let is_english_qu_pattern = initial_q
                                    && first_vowel_u
                                    && has_repeated_u
                                    && !final_u_is_valid_glide;

                                let is_valid_vn_triphthong = vowels.len() == 3
                                    && !is_english_qu_pattern
                                    && constants::VALID_TRIPHTHONGS
                                        .contains(&[vowels[0], vowels[1], vowels[2]]);

                                // Issue #183: Also allow V1-V2 diphthongs requiring circumflex on V1
                                // e.g., "neue" → [e, u] = êu (nếu), "xaua" → [a, u] = âu (xấu)
                                // When typing the second V1, it should trigger circumflex on first V1
                                // BUT: Exclude English "qu" patterns (like "queue")
                                let v1_circumflex_diphthongs: &[[u16; 2]] = &[
                                    [keys::A, keys::U], // âu - "dấu", "xấu"
                                    [keys::A, keys::Y], // ây - "dây"
                                    [keys::E, keys::U], // êu - "nếu", "kêu"
                                    [keys::O, keys::I], // ôi - "tối"
                                ];
                                let is_valid_v1_circumflex_diphthong = !is_english_qu_pattern
                                    && v1_circumflex_diphthongs.contains(&[v1, v2]);

                                if !is_valid_vn_triphthong && !is_valid_v1_circumflex_diphthong {
                                    return None;
                                }
                            }
                        }
                    }
                }

                for (i, c) in self.buf.iter().enumerate().rev() {
                    if targets.contains(&c.key) && c.tone == tone::NONE {
                        // For Telex circumflex, check if there are consonants after target
                        if is_telex_circumflex && i != self.buf.len() - 1 {
                            // Check for consonants between target position and end of buffer
                            let consonants_after: Vec<u16> = (i + 1..self.buf.len())
                                .filter_map(|j| {
                                    self.buf.get(j).and_then(|ch| {
                                        if !keys::is_vowel(ch.key) {
                                            Some(ch.key)
                                        } else {
                                            None
                                        }
                                    })
                                })
                                .collect();

                            if !consonants_after.is_empty() {
                                // Check if there's a NON-ADJACENT vowel between target and final
                                // "teacher": e-a-ch has 'a' between first 'e' and 'ch' → block
                                // "hongo": o-ng has no vowel between 'o' and 'ng' → allow
                                // "dau": a-u is a diphthong (adjacent vowels) → allow
                                // Adjacent vowels (position i+1) form diphthongs, not separate syllables
                                let has_non_adjacent_vowel = (i + 2..self.buf.len()).any(|j| {
                                    self.buf.get(j).is_some_and(|ch| keys::is_vowel(ch.key))
                                });

                                if has_non_adjacent_vowel {
                                    // A vowel exists after the adjacent position → different syllable
                                    // Skip this target (e.g., "teacher" → don't make "têacher")
                                    continue;
                                }

                                // Check if consonants form valid Vietnamese finals
                                // Valid finals: single (c,m,n,p,t) or pairs (ch,ng,nh)
                                // Double consonant finals (ng,nh,ch) are distinctly Vietnamese
                                // - "hongo" → "hông" (ng final, allow circumflex)
                                // - "khongo" → "không" (ng final, allow circumflex)
                                // Single consonant finals need additional context
                                // - "data" → should NOT become "dât" (t final, but English)
                                // - "nhana" → "nhân" (n final, but has nh initial)
                                let (all_are_valid_finals, is_double_final) = match consonants_after
                                    .len()
                                {
                                    1 => (
                                        constants::VALID_FINALS_1.contains(&consonants_after[0]),
                                        false,
                                    ),
                                    2 => {
                                        let pair = [consonants_after[0], consonants_after[1]];
                                        (constants::VALID_FINALS_2.contains(&pair), true)
                                    }
                                    _ => (false, false), // More than 2 consonants is invalid
                                };

                                // Double consonant finals (ng,nh,ch) are distinctly Vietnamese
                                // But still need to check: if there's an adjacent vowel, it must
                                // form a valid diphthong with the target. Otherwise skip.
                                // Example: "teacher" has 'e' at i=1 with adjacent 'a' at i+1,
                                // but "ea" is NOT a valid Vietnamese diphthong → skip
                                if is_double_final && all_are_valid_finals {
                                    // Check for adjacent vowel that doesn't form valid diphthong
                                    let adjacent_vowel_key = (i + 1 < self.buf.len())
                                        .then(|| self.buf.get(i + 1))
                                        .flatten()
                                        .filter(|ch| keys::is_vowel(ch.key))
                                        .map(|ch| ch.key);

                                    if let Some(adj_key) = adjacent_vowel_key {
                                        // Check if [target, adjacent] forms valid diphthong
                                        let diphthong =
                                            [self.buf.get(i).map(|c| c.key).unwrap_or(0), adj_key];
                                        if !constants::VALID_DIPHTHONGS.contains(&diphthong) {
                                            // Invalid diphthong like "ea" → skip this target
                                            continue;
                                        }
                                    }
                                    // Valid double final with valid diphthong (or no adjacent vowel)
                                    // This handles "hongo" → "hông", "khongo" → "không"
                                } else if !all_are_valid_finals {
                                    // Invalid final consonants → skip
                                    continue;
                                } else {
                                    // Single consonant final - need VALID diphthong or double initial
                                    // Check if there's another vowel adjacent to target that forms
                                    // a VALID Vietnamese diphthong (in correct order)
                                    // Example: "coup" + "o" → "ou" is NOT valid diphthong → block
                                    // Example: "daup" + "a" → "au" IS valid diphthong → allow
                                    // Note: diphthong order matters: [V1, V2] not [V2, V1]
                                    let target_key = self.buf.get(i).map(|c| c.key).unwrap_or(0);
                                    // Adjacent BEFORE: [adjacent, target] order
                                    let adjacent_before = i > 0
                                        && self.buf.get(i - 1).is_some_and(|ch| {
                                            keys::is_vowel(ch.key)
                                                && constants::VALID_DIPHTHONGS
                                                    .contains(&[ch.key, target_key])
                                        });
                                    // Adjacent AFTER: [target, adjacent] order
                                    let adjacent_after = i + 1 < self.buf.len()
                                        && self.buf.get(i + 1).is_some_and(|ch| {
                                            keys::is_vowel(ch.key)
                                                && constants::VALID_DIPHTHONGS
                                                    .contains(&[target_key, ch.key])
                                        });
                                    let has_valid_adjacent_diphthong =
                                        adjacent_before || adjacent_after;

                                    // Check for Vietnamese-specific double initial (nh, ch, th, ph, etc.)
                                    // This allows "nhana" → "nhân" (nh + a + n + a)
                                    // but still blocks "data" → "dât" (d is not a Vietnamese digraph)
                                    let has_vietnamese_double_initial = if i >= 2 {
                                        // Get first two consonants before the target vowel
                                        let initial_keys: Vec<u16> = (0..i)
                                            .filter_map(|j| self.buf.get(j).map(|ch| ch.key))
                                            .take_while(|k| !keys::is_vowel(*k))
                                            .collect();
                                        if initial_keys.len() >= 2 {
                                            let pair = [initial_keys[0], initial_keys[1]];
                                            constants::VALID_INITIALS_2.contains(&pair)
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };

                                    // Same-vowel trigger: typing the same vowel after consonant
                                    // Example: "nanag" → second 'a' triggers circumflex on first 'a'
                                    // Pattern: initial + vowel + consonant + SAME_VOWEL
                                    // Only allow immediate circumflex for middle consonants that
                                    // can form double finals (n→ng/nh, c→ch). These are clearly
                                    // Vietnamese patterns.
                                    // For other single finals (t,m,p), delay circumflex until
                                    // a mark key is typed to avoid false positives like "data"→"dât"
                                    let is_same_vowel_trigger =
                                        self.buf.get(i).is_some_and(|c| c.key == key);
                                    // Consonants that can form double finals: n→ng/nh, c→ch
                                    let middle_can_extend = consonants_after.len() == 1
                                        && matches!(consonants_after[0], keys::N | keys::C);

                                    // Check if initial consonant already has stroke (đ/Đ)
                                    // If so, it's clearly Vietnamese (from delayed stroke pattern)
                                    let initial_has_stroke = (0..i)
                                        .filter_map(|j| self.buf.get(j))
                                        .take_while(|c| !keys::is_vowel(c.key))
                                        .any(|c| c.stroke);

                                    // Check for non-extending middle consonant (t, m, p)
                                    // These require special handling for delayed circumflex
                                    let is_non_extending_final = consonants_after.len() == 1
                                        && matches!(
                                            consonants_after[0],
                                            keys::T | keys::M | keys::P
                                        );

                                    // Allow circumflex if any of these conditions are true:
                                    // 1. Has adjacent vowel forming VALID diphthong (au, oi, etc.)
                                    //    BUT NOT if final is non-extending (t,m,p) - diphthong+t/m/p rarely valid
                                    //    EXCEPTION: V2_CIRCUMFLEX_REQUIRED diphthongs (iê, uê, yê, uô) ARE
                                    //    valid with non-extending finals (viết, thiết, miếng, etc.)
                                    // 2. Has Vietnamese double initial (nh, th, ph, etc.)
                                    // 3. Same-vowel trigger with middle consonant that can extend (n,c)
                                    // 4. Initial has stroke (đ) - clearly Vietnamese
                                    let is_v2_circumflex_diphthong = adjacent_before && {
                                        let v1 = self.buf.get(i - 1).map(|c| c.key).unwrap_or(0);
                                        constants::V2_CIRCUMFLEX_REQUIRED
                                            .contains(&[v1, target_key])
                                    };
                                    let diphthong_allows = has_valid_adjacent_diphthong
                                        && (!is_non_extending_final || is_v2_circumflex_diphthong);
                                    let allow_circumflex = diphthong_allows
                                        || has_vietnamese_double_initial
                                        || (is_same_vowel_trigger && middle_can_extend)
                                        || initial_has_stroke;

                                    // Special case: same-vowel trigger with non-extending middle consonant
                                    // Apply circumflex immediately when typing second matching vowel
                                    // Example: "toto" → "tôt" (second 'o' triggers circumflex on first 'o')
                                    // Auto-restore on space will revert if invalid (e.g., "data " → "data ")
                                    // Only apply if target has NO mark - if it has a mark (like ngã from 'x'),
                                    // the user is building a different pattern (like "expect" → ẽ-p-e-c-t)
                                    // Also block if adjacent vowel forms INVALID diphthong
                                    // Example: "coupo" → [O, U] invalid → don't apply circumflex
                                    let target_has_no_mark =
                                        self.buf.get(i).is_some_and(|c| c.mark == 0);
                                    // Check if target has ANY adjacent vowel
                                    // Diphthong + non-extending final (t,m,p) is rarely valid Vietnamese
                                    // Examples: "âup", "oem", "aum" are all invalid syllables
                                    let has_adjacent_vowel_before = i > 0
                                        && self
                                            .buf
                                            .get(i - 1)
                                            .is_some_and(|ch| keys::is_vowel(ch.key));
                                    let has_adjacent_vowel_after = i + 1 < self.buf.len()
                                        && self
                                            .buf
                                            .get(i + 1)
                                            .is_some_and(|ch| keys::is_vowel(ch.key));
                                    // Check for valid 3-vowel pattern like "xuata" → "xuât", "buomo" → "buôm"
                                    // Requirements:
                                    // 1. Exactly 3 vowels total (2 in buffer + 1 trigger)
                                    // 2. First vowel is 'u' with no transformation
                                    // 3. Target is 'a' or 'o' (forming "ua"→"uâ" or "uo"→"uô" diphthong)
                                    // 4. First two vowels are adjacent
                                    // 5. Buffer length <= 4 (typical Vietnamese syllable size: C+V1+V2+C)
                                    // 6. Initial consonant must be common Vietnamese pattern for this diphthong
                                    //    Exclude: g, q (foreign gu- patterns), d (duomo is Italian)
                                    // This specifically handles: xuất, tuất, luật, buồm, cuốn, muốn, etc.
                                    let vowel_positions: Vec<(usize, u16)> = self
                                        .buf
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, c)| keys::is_vowel(c.key))
                                        .map(|(pos, c)| (pos, c.key))
                                        .collect();

                                    // Check if initial consonant is likely foreign pattern
                                    // g/q: "guatanamo", "quest"
                                    // d: "duomo" (Italian), but Vietnamese "đ" uses stroke
                                    // EXCEPTION: "gi" is a valid Vietnamese initial (giâm, giận, etc.)
                                    let is_gi_initial = self.buf.get(0).map(|c| c.key)
                                        == Some(keys::G)
                                        && self.buf.get(1).map(|c| c.key) == Some(keys::I)
                                        && vowel_positions.len() >= 2
                                        && vowel_positions[0].0 == 1; // I must be at position 1
                                    let has_foreign_initial = self.buf.get(0).is_some_and(|ch| {
                                        matches!(ch.key, keys::G | keys::Q | keys::D)
                                    }) && !is_gi_initial;

                                    // Target must be 'a' or 'o' (circumflex vowels in ua/uo diphthongs)
                                    let is_valid_circumflex_target =
                                        matches!(key, keys::A | keys::O);

                                    let is_valid_3_vowel_diphthong_pattern = self.buf.len() <= 4  // Max syllable size before trigger
                                            && vowel_positions.len() == 2  // Only 2 in buffer, 3rd is being typed
                                            && has_adjacent_vowel_before
                                            && is_valid_circumflex_target  // Trigger must be 'a' or 'o'
                                            && !has_foreign_initial  // Exclude foreign patterns
                                            && self.buf.get(i - 1).is_some_and(|ch| {
                                                ch.key == keys::U  // Only 'u' before target
                                                    && ch.tone == 0
                                                    && ch.mark == 0
                                            })
                                            && self.buf.get(i).is_some_and(|ch| ch.key == key); // Target matches trigger

                                    let has_any_adjacent_vowel =
                                        has_adjacent_vowel_before || has_adjacent_vowel_after;

                                    // Check for gi-initial pattern: gi + a + C + a → giâ + C
                                    // In Vietnamese, "gi" is a consonant cluster, so "giama" → "giâm"
                                    // The I in "gi" is part of the initial, not a separate vowel
                                    // For gi patterns, allow ANY single consonant final (not just t,m,p)
                                    // because "gi" prefix already indicates Vietnamese intent
                                    // Handle both 2-vowel (buffer: gi-a-m) and 3-vowel (buffer: gi-a-c-a) cases
                                    let has_single_consonant_final = consonants_after.len() == 1;
                                    let gi_effective_vowel_count = if is_gi_initial
                                        && vowel_positions.len() >= 2
                                        && vowel_positions[0].1 == keys::I
                                        && vowel_positions[0].0 == 1
                                    {
                                        // Ignore I as it's part of gi-initial, count remaining vowels
                                        vowel_positions.len() - 1
                                    } else {
                                        vowel_positions.len()
                                    };
                                    let is_gi_initial_pattern = is_gi_initial
                                        && gi_effective_vowel_count == 1  // Only 1 effective vowel (excluding I from gi)
                                        && vowel_positions[0].1 == keys::I  // First is I (part of gi)
                                        && is_same_vowel_trigger  // Trigger matches the vowel in buffer
                                        && has_single_consonant_final; // Any single consonant final

                                    // Block if: has adjacent vowel (diphthong pattern) with non-extending final
                                    // UNLESS it's the specific 3-vowel diphthong pattern (xuata)
                                    // OR it's the gi-initial pattern (giama, giacsa, etc.)
                                    //
                                    // Also allow when target already has mark AND no adjacent vowel:
                                    // Pattern: V(mark) + C + V → Circumflex on first V, preserve mark
                                    // Example: "afma" → à + m + a → ầ + m (a with huyền gets circumflex)
                                    // For gi-initial with mark, allow any single consonant final
                                    let allow_with_existing_mark = !has_any_adjacent_vowel
                                        && is_non_extending_final
                                        && self.buf.get(i).is_some_and(|c| c.mark > 0);

                                    // Special case: gi-initial with mark allows any single consonant
                                    let allow_gi_with_mark = is_gi_initial
                                        && has_single_consonant_final
                                        && self.buf.get(i).is_some_and(|c| c.mark > 0);

                                    // Special case: vowel-initial patterns (ua, uo) with mark
                                    // Pattern: U + A(mark) + T + A → U + Ấ + T (uất)
                                    // When first char is vowel U and target has mark, allow circumflex
                                    let is_vowel_initial_with_mark =
                                        self.buf.get(0).is_some_and(|c| c.key == keys::U)
                                            && is_non_extending_final
                                            && self.buf.get(i).is_some_and(|c| c.mark > 0)
                                            && is_valid_3_vowel_diphthong_pattern;

                                    if is_same_vowel_trigger
                                        && (is_non_extending_final
                                            || is_gi_initial_pattern
                                            || allow_gi_with_mark
                                            || is_vowel_initial_with_mark)
                                        && (target_has_no_mark
                                            || allow_with_existing_mark
                                            || allow_gi_with_mark
                                            || is_vowel_initial_with_mark)
                                        && (!has_any_adjacent_vowel
                                            || is_valid_3_vowel_diphthong_pattern
                                            || is_gi_initial_pattern
                                            || allow_gi_with_mark
                                            || is_vowel_initial_with_mark)
                                    {
                                        // Apply circumflex to first vowel
                                        if let Some(c) = self.buf.get_mut(i) {
                                            c.tone = tone::CIRCUMFLEX;
                                            self.had_any_transform = true;
                                            self.had_vowel_triggered_circumflex = true;
                                        }
                                        // Track this as delayed circumflex for potential revert
                                        // If next consonants create invalid pattern (like "pct" in "expect"),
                                        // the circumflex can be reverted
                                        self.last_transform =
                                            Some(Transform::DelayedCircumflex(key));
                                        // Don't add the trigger vowel - return result immediately
                                        // Need extra backspace because we're replacing displayed char
                                        let result = self.rebuild_from(i);
                                        let chars: Vec<char> = result.chars
                                            [..result.count as usize]
                                            .iter()
                                            .filter_map(|&c| char::from_u32(c))
                                            .collect();
                                        return Some(Result::send(result.backspace, &chars));
                                    }

                                    if !allow_circumflex {
                                        // Single final, no diphthong, no double initial, not valid same-vowel → likely English
                                        continue;
                                    }
                                }
                            }
                        }
                        target_positions.push(i);
                        break;
                    }
                }
            }
        }

        if target_positions.is_empty() {
            // Check if any target vowels already have the requested tone
            // This handles redundant tone keys like "u7o7" → "ươ" (second 7 absorbed)
            //
            // EXCEPTION: Don't absorb 'w' if last_transform was WAsVowel
            // because try_w_as_vowel needs to handle the revert (ww → w)
            let is_w_revert_pending =
                key == keys::W && matches!(self.last_transform, Some(Transform::WAsVowel));

            let has_tone_already = self
                .buf
                .iter()
                .any(|c| targets.contains(&c.key) && c.tone == tone_val);
            if has_tone_already && !is_w_revert_pending {
                // Absorb the key (no-op)
                return Some(Result::send(0, &[]));
            }
            return None;
        }

        // Track earliest position modified for rebuild
        let mut earliest_pos = usize::MAX;

        // If switching, clear old tones first for proper rebuild
        if is_switching {
            for &pos in &target_positions {
                if let Some(c) = self.buf.get_mut(pos) {
                    c.tone = tone::NONE;
                    earliest_pos = earliest_pos.min(pos);
                }
            }

            // Special case: switching from horn compound (ươ) to circumflex (uô)
            // When switching to circumflex on 'o', also clear horn from adjacent 'u'
            if tone_type == ToneType::Circumflex {
                for &pos in &target_positions {
                    if let Some(c) = self.buf.get(pos) {
                        if c.key == keys::O {
                            // Check for adjacent 'u' with horn and clear it
                            if pos > 0 {
                                if let Some(prev) = self.buf.get_mut(pos - 1) {
                                    if prev.key == keys::U && prev.tone == tone::HORN {
                                        prev.tone = tone::NONE;
                                        earliest_pos = earliest_pos.min(pos - 1);
                                    }
                                }
                            }
                            if pos + 1 < self.buf.len() {
                                if let Some(next) = self.buf.get_mut(pos + 1) {
                                    if next.key == keys::U && next.tone == tone::HORN {
                                        next.tone = tone::NONE;
                                        earliest_pos = earliest_pos.min(pos + 1);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Special case: switching from circumflex (uô) to horn compound (ươ)
            // For standalone uo compound (no final consonant), add horn to adjacent 'u'
            if tone_type == ToneType::Horn && self.has_uo_compound() {
                // Check if this is a standalone compound (o is last vowel, no final consonant)
                let has_final = target_positions.iter().any(|&pos| {
                    pos + 1 < self.buf.len()
                        && self
                            .buf
                            .get(pos + 1)
                            .is_some_and(|c| !keys::is_vowel(c.key))
                });

                if !has_final {
                    for &pos in &target_positions {
                        if let Some(c) = self.buf.get(pos) {
                            if c.key == keys::O {
                                // Add horn to adjacent 'u' for compound
                                if pos > 0 {
                                    if let Some(prev) = self.buf.get_mut(pos - 1) {
                                        if prev.key == keys::U && prev.tone == tone::NONE {
                                            prev.tone = tone::HORN;
                                            earliest_pos = earliest_pos.min(pos - 1);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply new tone
        for &pos in &target_positions {
            if let Some(c) = self.buf.get_mut(pos) {
                c.tone = tone_val;
                earliest_pos = earliest_pos.min(pos);
            }
        }

        // Validate result: check for breve (ă) followed by vowel - NEVER valid in Vietnamese
        // Issue #44: "tai" + 'w' → "tăi" is INVALID (ăi, ăo, ău, ăy don't exist)
        // Only check this specific pattern, not all vowel patterns, to allow Telex shortcuts
        // like "eie" → "êi" which may not be standard but are expected Telex behavior
        // Note: ToneType::Horn (Telex 'w') and ToneType::Breve (VNI '8') both create breve on 'a'
        if tone_type == ToneType::Horn || tone_type == ToneType::Breve {
            // Early check: "W at end after vowel (not U)" with earlier Vietnamese transforms
            // suggests English word like "seesaw" where:
            // - Earlier chars were transformed (sê, sế)
            // - But "aw" ending makes it look like English
            // Only restore if buffer has EARLIER transforms (tone or mark)
            // Don't restore for simple "aw" or "raw" - let breve deferral handle those
            // Only run if english_auto_restore is enabled (experimental feature)
            if self.english_auto_restore && key == keys::W && self.raw_input.len() >= 2 {
                let (prev_key, _, _) = self.raw_input[self.raw_input.len() - 2];
                if prev_key == keys::A {
                    // Check if there are earlier Vietnamese transforms in buffer
                    // (tone marks on OTHER vowels, or circumflex/horn on non-A vowels)
                    // IMPORTANT: Exclude positions we just modified in this call
                    let has_earlier_transforms = self.buf.iter().enumerate().any(|(i, c)| {
                        // Skip positions we just applied horn to - those aren't "earlier" transforms
                        if target_positions.contains(&i) {
                            return false;
                        }
                        // Check for any tone (circumflex, horn) or mark on NON-A vowels
                        // A itself might just be plain "a" waiting for breve
                        c.key != keys::A && (c.tone > 0 || c.mark > 0)
                    });

                    if has_earlier_transforms {
                        // "aw" ending is English (like "seesaw") - restore immediately
                        let raw_chars: Vec<char> = self
                            .raw_input
                            .iter()
                            .filter_map(|&(k, c, s)| utils::key_to_char_ext(k, c, s))
                            .collect();
                        let backspace = self.buf.len() as u8;
                        self.buf.clear();
                        self.raw_input.clear();
                        self.last_transform = None;
                        return Some(Result::send(backspace, &raw_chars));
                    }
                }
            }
            let has_breve_vowel_pattern = target_positions.iter().any(|&pos| {
                if let Some(c) = self.buf.get(pos) {
                    // Check if this is 'a' with horn (breve) followed by another vowel
                    if c.key == keys::A {
                        // Look for any vowel after this position
                        return (pos + 1..self.buf.len()).any(|i| {
                            self.buf
                                .get(i)
                                .map(|next| keys::is_vowel(next.key))
                                .unwrap_or(false)
                        });
                    }
                }
                false
            });

            if has_breve_vowel_pattern {
                // Revert: clear applied tones
                for &pos in &target_positions {
                    if let Some(c) = self.buf.get_mut(pos) {
                        c.tone = tone::NONE;
                    }
                }
                return None;
            }

            // Issue #44 (part 2): Always apply breve for "aw" pattern immediately
            // "aw" → "ă", "taw" → "tă", "raw" → "ră"
            // The breve is always applied - English auto-restore handles English words separately
            let has_breve_open_syllable = false;

            if has_breve_open_syllable {
                // Revert: clear applied tones, defer breve until final consonant
                for &pos in &target_positions {
                    if let Some(c) = self.buf.get_mut(pos) {
                        if c.key == keys::A {
                            c.tone = tone::NONE;
                            // Store position for deferred breve
                            self.pending_breve_pos = Some(pos);
                        }
                    }
                }
                // Return None to let 'w' fall through:
                // - try_w_as_vowel will fail (invalid vowel pattern)
                // - handle_normal_letter will add 'w' as regular letter
                // - When final consonant is typed, breve is applied
                return None;
            }
        }

        // Normalize ưo → ươ compound if horn was applied to 'u'
        if let Some(compound_pos) = self.normalize_uo_compound() {
            earliest_pos = earliest_pos.min(compound_pos);
        }

        self.last_transform = Some(Transform::Tone(key, tone_val));
        self.had_any_transform = true;
        self.had_telex_transform = true; // Track for whitelist-based auto-restore

        // Reposition tone mark if vowel pattern changed
        let mut rebuild_pos = earliest_pos;
        if let Some((old_pos, _)) = self.reposition_tone_if_needed() {
            rebuild_pos = rebuild_pos.min(old_pos);
        }

        Some(self.rebuild_from(rebuild_pos))
    }

    /// Try to apply mark transformation
    fn try_mark(&mut self, key: u16, caps: bool, mark_val: u8) -> Option<Result> {
        if self.buf.is_empty() {
            return None;
        }

        // Check revert first
        if let Some(Transform::Mark(last_key, _)) = self.last_transform {
            if last_key == key {
                return Some(self.revert_mark(key, caps));
            }
        }

        // Telex: Check for delayed stroke pattern (d + vowels + d)
        // When buffer is "dod" and mark key is typed, apply stroke to initial 'd'
        // This enables "dods" → "đó" while preventing "de" + "d" → "đe"
        // Skip if stroke was reverted (ddd → dd): user explicitly rejected đ,
        // so a mark key must not resurrect the stroke (e.g., "dayddr" stays "daydr")
        let had_delayed_stroke = self.method == 0
            && !self.stroke_reverted
            && self.buf.len() >= 2
            && self
                .buf
                .get(0)
                .is_some_and(|c| c.key == keys::D && !c.stroke)
            && self.buf.last().is_some_and(|c| c.key == keys::D)
            && {
                // Check vowels and validity in one pass
                let buf_len = self.buf.len();
                let has_vowel = self
                    .buf
                    .iter()
                    .take(buf_len - 1)
                    .any(|c| keys::is_vowel(c.key));
                has_vowel && {
                    let buffer_without_last: Vec<u16> =
                        self.buf.iter().take(buf_len - 1).map(|c| c.key).collect();
                    is_valid(&buffer_without_last) && {
                        // Apply delayed stroke: stroke initial 'd', remove trigger 'd'
                        if let Some(c) = self.buf.get_mut(0) {
                            c.stroke = true;
                        }
                        self.buf.pop();
                        true
                    }
                }
            };

        // Issue #44: Apply pending breve before adding mark
        // When user types "aws" (Telex) or "a81" (VNI), they want "ắ" (breve + sắc)
        // Breve was deferred due to open syllable, but adding mark confirms Vietnamese input
        let mut had_pending_breve = false;
        if let Some(breve_pos) = self.pending_breve_pos {
            had_pending_breve = true;
            // Try to find and remove the breve modifier from buffer
            // Both Telex 'w' and VNI '8' are stored in buffer (handle_normal_letter adds them)
            let modifier_pos = breve_pos + 1;
            if modifier_pos < self.buf.len() {
                if let Some(c) = self.buf.get(modifier_pos) {
                    // Remove 'w' (Telex) or '8' (VNI) breve modifier from buffer
                    if c.key == keys::W || c.key == keys::N8 {
                        self.buf.remove(modifier_pos);
                    }
                }
            }
            // Apply breve to 'a'
            if let Some(c) = self.buf.get_mut(breve_pos) {
                if c.key == keys::A {
                    c.tone = tone::HORN; // HORN on A = breve (ă)
                    self.had_any_transform = true;
                }
            }
            self.pending_breve_pos = None;
        }

        // Telex: Check for delayed circumflex pattern (V + C + V where both V are same)
        // When buffer is "toto" (t-o-t-o) and mark key is typed, apply circumflex + remove trigger
        // This enables "totos" → "tốt" while preventing "data" → "dât"
        // Pattern: C₁ + V + C₂ + V where V is same vowel (a, e, o)
        let mut had_delayed_circumflex = false;
        if self.method == 0 && self.buf.len() >= 3 {
            // Get vowel positions
            let vowel_positions: Vec<(usize, u16)> = self
                .buf
                .iter()
                .enumerate()
                .filter(|(_, c)| keys::is_vowel(c.key))
                .map(|(i, c)| (i, c.key))
                .collect();

            // Check for at least 2 vowels where the last two are the same (a, e, or o for circumflex)
            // This handles words like "xuata" (x-u-a-t-a) where we have 3 vowels but the last two 'a's should trigger circumflex
            // Only allow > 2 vowels case for exactly 3 vowels where:
            // - First vowel is 'u' or 'i' (common in Vietnamese diphthongs like "ua", "uô", "ia", "iê")
            // - First two vowels are adjacent (forming a diphthong like "ua" in xuata)
            // - First vowel is different from the pair
            // This prevents "roemer" → "roêm" (English word corruption)
            let valid_multi_vowel_pattern = if vowel_positions.len() == 3 {
                let (pos0, first_key) = vowel_positions[0];
                let (pos1, second_key) = vowel_positions[1];
                // First must be u/i (diphthong starter), different from pair, and adjacent
                let is_diphthong_starter = matches!(first_key, keys::U | keys::I);
                // Also check if first vowel already has any transformation (mark/tone)
                // This prevents "mỉama" + r from triggering (ỉ already has mark)
                let first_vowel_has_transform = self
                    .buf
                    .get(pos0)
                    .is_some_and(|c| c.tone != 0 || c.mark != 0);
                is_diphthong_starter
                    && first_key != second_key
                    && pos1 == pos0 + 1
                    && !first_vowel_has_transform
            } else {
                // For > 3 vowels, don't trigger delayed circumflex
                // For exactly 2 vowels, always valid (original behavior)
                vowel_positions.len() == 2
            };

            if vowel_positions.len() >= 2 && valid_multi_vowel_pattern {
                let (pos1, key1) = vowel_positions[vowel_positions.len() - 2];
                let (pos2, key2) = vowel_positions[vowel_positions.len() - 1];
                let is_circumflex_vowel = matches!(key1, keys::A | keys::E | keys::O);

                // Check if first vowel already has circumflex - skip delayed circumflex if so
                // This prevents "deeper" from being corrupted: after "dee" → "dê", then "deepe"
                // should NOT trigger delayed circumflex since first 'e' already has circumflex
                let first_vowel_already_has_circumflex = self
                    .buf
                    .get(pos1)
                    .is_some_and(|c| c.tone == tone::CIRCUMFLEX);

                // Must be same vowel, must have consonant(s) between them, first vowel must not already have circumflex
                if key1 == key2
                    && is_circumflex_vowel
                    && pos2 > pos1 + 1
                    && !first_vowel_already_has_circumflex
                {
                    // Check for consonants between the two vowels
                    let consonants_between: Vec<u16> = (pos1 + 1..pos2)
                        .filter_map(|j| {
                            self.buf.get(j).and_then(|c| {
                                if !keys::is_vowel(c.key) {
                                    Some(c.key)
                                } else {
                                    None
                                }
                            })
                        })
                        .collect();

                    // Must have exactly one consonant between and it must be a non-extending
                    // final (t, m, p). Consonants that can extend (n→ng/nh, c→ch) are
                    // handled immediately in try_tone.
                    let is_non_extending_final = consonants_between.len() == 1
                        && matches!(consonants_between[0], keys::T | keys::M | keys::P);

                    // Check if second vowel is at end of buffer (typical trigger position)
                    let second_vowel_at_end = pos2 == self.buf.len() - 1;

                    // Check initial consonants for Vietnamese validity
                    // Skip delayed circumflex if initial looks English (e.g., "pr" in "proposal")
                    let initial_keys: Vec<u16> = (0..pos1)
                        .filter_map(|j| self.buf.get(j).map(|ch| ch.key))
                        .take_while(|k| !keys::is_vowel(*k))
                        .collect();

                    // Validate initial consonants:
                    // - 0 initials: valid (vowel-only start)
                    // - 1 initial: valid (single consonant)
                    // - 2 initials: must be in VALID_INITIALS_2 (nh, th, ph, etc.)
                    // - 3+ initials: skip for delayed circumflex
                    //   (words like "proposal" with "pr" will be rejected here)
                    let has_valid_vietnamese_initial = match initial_keys.len() {
                        0 | 1 => true,
                        2 => {
                            let pair = [initial_keys[0], initial_keys[1]];
                            constants::VALID_INITIALS_2.contains(&pair)
                        }
                        _ => false,
                    };

                    // Check for double initial specifically (for immediate vs delayed handling)
                    let has_vietnamese_double_initial =
                        initial_keys.len() >= 2 && has_valid_vietnamese_initial;

                    // Only apply delayed circumflex if:
                    // - Has non-extending middle consonant (t, m, p)
                    // - Second vowel is at end (trigger position)
                    // - Has valid Vietnamese initial (skip English like "proposal")
                    // - No double initial (those work immediately without delay)
                    // - User didn't just revert a circumflex (typing 3rd vowel to cancel)
                    if is_non_extending_final
                        && second_vowel_at_end
                        && has_valid_vietnamese_initial
                        && !has_vietnamese_double_initial
                        && !self.had_circumflex_revert
                    {
                        // Skip delayed circumflex if raw_input is an English word
                        // This prevents "pasta" → "pất", "costa" → "côt", etc.
                        // The raw_input check works because English words like "pasta"
                        // are in our dictionary, while Vietnamese typing patterns are not.
                        let raw_str: String = self
                            .raw_input
                            .iter()
                            .filter_map(|&(k, caps, _)| utils::key_to_char(k, caps))
                            .collect::<String>()
                            .to_lowercase();
                        if english_dict::is_english_word(&raw_str) {
                            // Raw input is English - don't apply delayed circumflex
                            // Let the letter be added normally, auto-restore will handle it
                        } else {
                            // IMPORTANT: Check foreign word pattern BEFORE modifying buffer
                            // to avoid leaving buffer in inconsistent state if we need to return None.
                            // Example: "cete" + 'r' → "cêt" (delayed circumflex) + T+R check → foreign
                            // Without this check, buffer would be left as "cêt" even though we return None.
                            let temp_buffer_keys: Vec<u16> =
                                self.buf.iter().map(|c| c.key).collect();
                            let temp_buffer_tones: Vec<u8> =
                                self.buf.iter().map(|c| c.tone).collect();
                            // Check what buffer would look like after circumflex (keys without trigger)
                            let mut post_circumflex_keys = temp_buffer_keys.clone();
                            post_circumflex_keys.remove(pos2); // simulate removing trigger vowel
                            let post_circumflex_tones: Vec<u8> = post_circumflex_keys
                                .iter()
                                .enumerate()
                                .map(|(i, _)| {
                                    if i == pos1 {
                                        tone::CIRCUMFLEX
                                    } else {
                                        temp_buffer_tones.get(i).copied().unwrap_or(0)
                                    }
                                })
                                .collect();

                            // Skip delayed circumflex if the resulting buffer would trigger foreign pattern
                            if is_foreign_word_pattern(
                                &post_circumflex_keys,
                                &post_circumflex_tones,
                                key,
                            ) {
                                // Don't apply delayed circumflex - let the letter be added normally
                            } else {
                                had_delayed_circumflex = true;
                                // Apply circumflex to first vowel
                                if let Some(c) = self.buf.get_mut(pos1) {
                                    c.tone = tone::CIRCUMFLEX;
                                    self.had_any_transform = true;
                                }
                                // Remove second vowel (it was just a trigger)
                                self.buf.remove(pos2);
                            }
                        }
                    }
                }
            }
        }

        // Check if buffer has horn transforms - indicates intentional Vietnamese typing
        // (e.g., "rượu" has base keys [R,U,O,U] which looks like "ou" pattern,
        // but with horns applied it's valid "ươu")
        let has_horn_transforms = self.buf.iter().any(|c| c.tone == tone::HORN);

        // Check if buffer has stroke transforms (đ) - indicates intentional Vietnamese typing
        // Issue #48: "ddeso" → "đéo" (d was stroked to đ, so this is Vietnamese, not English)
        let has_stroke_transforms = self.buf.iter().any(|c| c.stroke);

        // Validate buffer structure (skip if has horn/stroke transforms - already intentional Vietnamese)
        // Also skip validation if free_tone mode is enabled
        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
        let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
        if !self.free_tone_enabled
            && !has_horn_transforms
            && !has_stroke_transforms
            && !is_valid_for_transform_with_foreign(&buffer_keys, self.allow_foreign_consonants)
        {
            return None;
        }

        // Check for invalid "-ing" rhyme: Vietnamese uses "-inh", NOT "-ing" with tone marks
        // Examples: "thíng" is invalid (things), but "tính" is valid
        // If vowel is 'i' and final is 'ng', reject marks
        if !self.free_tone_enabled && !has_horn_transforms && !has_stroke_transforms {
            let syllable = syllable::parse(&buffer_keys);
            if syllable.vowel.len() == 1 && syllable.final_c.len() == 2 {
                let vowel_key = buffer_keys[syllable.vowel[0]];
                let final_keys = [
                    buffer_keys[syllable.final_c[0]],
                    buffer_keys[syllable.final_c[1]],
                ];
                // i + ng = invalid Vietnamese rhyme for tone/mark
                if vowel_key == keys::I && final_keys == [keys::N, keys::G] {
                    return None;
                }
            }
        }

        // Checked-tone rule (issue #403): a syllable ending in a stop consonant
        // (p, t, c, ch, k) can only carry sắc or nặng. huyền/hỏi/ngã on a
        // stop-final syllable is phonologically impossible ("ỏt", "òc", "ãch"),
        // so reject the mark and let the key fall through as a literal letter.
        if !self.free_tone_enabled && matches!(mark_val, mark::HUYEN | mark::HOI | mark::NGA) {
            let syllable = syllable::parse(&buffer_keys);
            let is_stop_final = match syllable.final_c.len() {
                1 => matches!(
                    buffer_keys[syllable.final_c[0]],
                    keys::P | keys::T | keys::C | keys::K
                ),
                2 => {
                    buffer_keys[syllable.final_c[0]] == keys::C
                        && buffer_keys[syllable.final_c[1]] == keys::H
                }
                _ => false,
            };
            if is_stop_final {
                return None;
            }
        }

        // Skip modifier if buffer shows foreign word patterns.
        // Only check when NO horn/stroke transforms exist.
        //
        // Detected patterns:
        // - Invalid vowel combinations (ou, yo) that don't exist in Vietnamese
        // - Consonant clusters after finals common in English (T+R, P+R, C+R)
        //
        // Examples:
        // - "met" + 'r' → T+R cluster common in English → skip modifier
        // - "you" + 'r' → "ou" vowel pattern invalid → skip modifier
        // - "rươu" + 'j' → has horn transforms → DON'T skip, apply mark normally
        // - "đe" + 's' → has stroke transform → DON'T skip, apply mark normally (Issue #48)
        // Skip foreign word detection if free_tone mode is enabled
        if !self.free_tone_enabled
            && !has_horn_transforms
            && !has_stroke_transforms
            && is_foreign_word_pattern(&buffer_keys, &buffer_tones, key)
        {
            return None;
        }

        // Issue #29: Normalize ưo → ươ compound before placing mark
        // In Vietnamese, "ưo" is never valid - it's always "ươ"
        let rebuild_from_compound = self.normalize_uo_compound();

        let vowels = self.collect_vowels();
        if vowels.is_empty() {
            return None;
        }

        // Find mark position using phonology rules
        let last_vowel_pos = vowels.last().map(|v| v.pos).unwrap_or(0);
        let has_final = self.has_final_consonant(last_vowel_pos);
        let has_qu = self.has_qu_initial();
        let has_gi = self.has_gi_initial();
        let pos =
            Phonology::find_tone_position(&vowels, has_final, self.modern_tone, has_qu, has_gi);

        // Check if target vowel already has the same mark
        // This handles two cases:
        //
        // 1. "lists" pattern: After mark, user typed CONSONANT then same mark key
        //    - Buffer: [L, í, T] → has consonant after marked vowel
        //    - User wants to REVERT the mark → "lits"
        //
        // 2. "roofif" pattern: After mark, user typed VOWEL then same mark key
        //    - Buffer: [R, ồ, I] → only vowels after marked vowel (diphthong)
        //    - User is still in same syllable, second 'f' is likely accidental
        //    - Absorb → "rồi"
        //
        // EXCEPTION: Words starting with W are English - don't apply revert logic.
        // W is not a valid Vietnamese initial, so "writer", "wrong", "wrap" etc.
        // should NOT trigger delayed revert even if same mark key is pressed.
        // Auto-restore will handle these at word boundary.
        let starts_with_w = self
            .raw_input
            .first()
            .map(|(k, _, _)| *k == keys::W)
            .unwrap_or(false);

        if let Some(c) = self.buf.get(pos) {
            if c.mark == mark_val && !starts_with_w {
                // Check if there's a consonant after the marked vowel position
                let has_consonant_after = self
                    .buf
                    .iter()
                    .skip(pos + 1)
                    .any(|ch| !keys::is_vowel(ch.key));

                // Check if vowel is at the END of buffer (no chars after at all)
                // Issue #197: After backspace, vowel may be at end - pressing same
                // mark key should REVERT, not absorb
                // Example: "serv" → "sẻv" → backspace → "sẻ" → 'r' should → "ser"
                let is_vowel_at_end = pos + 1 >= self.buf.len();

                if has_consonant_after || is_vowel_at_end {
                    // Consonant after OR vowel at end: REVERT the mark (remove dấu)
                    // "lists" → "lits", user typed s twice to undo the mark
                    // "sẻ" → "se", user typed r after backspace to undo the mark
                    return Some(self.revert_mark(key, caps));
                } else {
                    // Vowels after (not at end): absorb (user double-tapped in same syllable)
                    // "roofif" → "rồi"
                    return Some(Result::send(0, &[]));
                }
            }
        }

        if let Some(c) = self.buf.get_mut(pos) {
            c.mark = mark_val;
            self.last_transform = Some(Transform::Mark(key, mark_val));
            self.had_any_transform = true;
            self.had_telex_transform = true; // Track for whitelist-based auto-restore
                                             // Rebuild from the earlier position if compound was formed
            let mut rebuild_pos = rebuild_from_compound.map_or(pos, |cp| cp.min(pos));

            // If delayed stroke was applied, rebuild from position 0
            // and add extra backspace for the trigger 'd' that was on screen
            if had_delayed_stroke {
                rebuild_pos = 0;
                let result = self.rebuild_from(rebuild_pos);
                let chars: Vec<char> = result.chars[..result.count as usize]
                    .iter()
                    .filter_map(|&c| char::from_u32(c))
                    .collect();
                // Add 1 to backspace for the trigger 'd' that was on screen but removed from buffer
                return Some(Result::send(result.backspace + 1, &chars));
            }

            // If there was pending breve, we need extra backspace
            // Screen has 'w' (Telex) or '8' (VNI) that needs to be deleted
            // Note: Telex 'w' was in buffer and removed, VNI '8' was never in buffer
            if had_pending_breve {
                let result = self.rebuild_from(rebuild_pos);
                // Convert u32 chars to char vec
                let chars: Vec<char> = result.chars[..result.count as usize]
                    .iter()
                    .filter_map(|&c| char::from_u32(c))
                    .collect();
                // Add 1 to backspace to account for modifier on screen
                return Some(Result::send(result.backspace + 1, &chars));
            }

            // If delayed circumflex was applied, rebuild from earliest vowel position
            // and add extra backspace for the trigger vowel that was on screen but removed
            if had_delayed_circumflex {
                rebuild_pos = rebuild_pos.min(1); // Start from first vowel position
                let result = self.rebuild_from(rebuild_pos);
                let chars: Vec<char> = result.chars[..result.count as usize]
                    .iter()
                    .filter_map(|&c| char::from_u32(c))
                    .collect();
                // Add 1 to backspace for the removed trigger vowel still on screen
                return Some(Result::send(result.backspace + 1, &chars));
            }

            return Some(self.rebuild_from(rebuild_pos));
        }

        None
    }

    /// Normalize ưo → ươ compound
    ///
    /// In Vietnamese, "ưo" (u with horn + plain o) is NEVER valid.
    /// It should always be "ươ" (both with horn).
    /// This function finds and fixes this pattern anywhere in the buffer.
    ///
    /// Returns Some(position) of the 'o' that was modified, None if no change.
    fn normalize_uo_compound(&mut self) -> Option<usize> {
        // Look for pattern: U with horn + O without horn (anywhere in buffer)
        for i in 0..self.buf.len().saturating_sub(1) {
            let c1 = self.buf.get(i)?;
            let c2 = self.buf.get(i + 1)?;

            // Check: U with horn + O plain → always normalize to ươ
            let is_u_with_horn = c1.key == keys::U && c1.tone == tone::HORN;
            let is_o_plain = c2.key == keys::O && c2.tone == tone::NONE;

            if is_u_with_horn && is_o_plain {
                // Apply horn to O to form the ươ compound
                if let Some(c) = self.buf.get_mut(i + 1) {
                    c.tone = tone::HORN;
                    return Some(i + 1);
                }
            }
        }
        None
    }

    /// Find positions of U+O or O+U compound (adjacent vowels)
    /// Returns Some((first_pos, second_pos)) if found, None otherwise
    fn find_uo_compound_positions(&self) -> Option<(usize, usize)> {
        for i in 0..self.buf.len().saturating_sub(1) {
            if let (Some(c1), Some(c2)) = (self.buf.get(i), self.buf.get(i + 1)) {
                let is_uo = c1.key == keys::U && c2.key == keys::O;
                let is_ou = c1.key == keys::O && c2.key == keys::U;
                if is_uo || is_ou {
                    return Some((i, i + 1));
                }
            }
        }
        None
    }

    /// Check for uo compound in buffer (any tone state)
    fn has_uo_compound(&self) -> bool {
        self.find_uo_compound_positions().is_some()
    }

    /// Check for complete ươ compound (both u and o have horn)
    fn has_complete_uo_compound(&self) -> bool {
        if let Some((pos1, pos2)) = self.find_uo_compound_positions() {
            if let (Some(c1), Some(c2)) = (self.buf.get(pos1), self.buf.get(pos2)) {
                // Check ư + ơ pattern (both with horn)
                let is_u_horn = c1.key == keys::U && c1.tone == tone::HORN;
                let is_o_horn = c2.key == keys::O && c2.tone == tone::HORN;
                return is_u_horn && is_o_horn;
            }
        }
        false
    }

    /// Find target position for horn modifier with switching support
    /// Allows selecting vowels that have a different tone (for switching circumflex ↔ horn)
    fn find_horn_target_with_switch(&self, targets: &[u16], new_tone: u8) -> Vec<usize> {
        // Find vowel positions that match targets and either:
        // - have no tone (normal case)
        // - have a different tone (switching case)
        let vowels: Vec<usize> = self
            .buf
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                targets.contains(&c.key) && (c.tone == tone::NONE || c.tone != new_tone)
            })
            .map(|(i, _)| i)
            .collect();

        if vowels.is_empty() {
            return vec![];
        }

        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();

        // Use centralized phonology rules (context inferred from buffer)
        let mut result = Phonology::find_horn_positions(&buffer_keys, &vowels);

        // Special case: standalone "ua" pattern where U already has a mark
        // If user typed "uaf" → "ùa", then 'w' should go to U (making "ừa"), not A
        // This ensures consistent behavior: mark placement indicates user's intent
        if result.len() == 1 {
            if let Some(&pos) = result.first() {
                if let Some(c) = self.buf.get(pos) {
                    // If horn target is A, check if U exists before it with a mark
                    if c.key == keys::A && pos > 0 {
                        if let Some(prev) = self.buf.get(pos - 1) {
                            // Adjacent U with a mark → user wants horn on U, not breve on A
                            if prev.key == keys::U && prev.mark > 0 {
                                result = vec![pos - 1]; // Return U position instead
                            }
                        }
                    }
                }
            }
        }

        // Fix for ươu triphthong pattern:
        // In Vietnamese, ươu has horn on ư and ơ only, final u stays plain.
        // When we have [U(horn), O, U] or [U(horn), O(horn), U], don't apply horn to final U.
        let buf_len = self.buf.len();
        if buf_len >= 3 {
            let last_pos = buf_len - 1;
            let last_is_plain_u = self
                .buf
                .get(last_pos)
                .map(|c| c.key == keys::U && c.tone == tone::NONE)
                .unwrap_or(false);

            if last_is_plain_u && result.contains(&last_pos) {
                // Check if there's O immediately before the final U
                let o_before_u = self
                    .buf
                    .get(last_pos - 1)
                    .map(|c| c.key == keys::O)
                    .unwrap_or(false);

                if o_before_u {
                    // Check if there's U with horn somewhere before the O
                    let has_u_horn_before_o = (0..last_pos - 1).any(|i| {
                        self.buf
                            .get(i)
                            .map(|c| c.key == keys::U && c.tone == tone::HORN)
                            .unwrap_or(false)
                    });

                    if has_u_horn_before_o {
                        // We're in ươu triphthong pattern - don't apply horn to final U
                        result.retain(|&pos| pos != last_pos);
                    }
                }
            }
        }

        result
            .into_iter()
            .filter(|&pos| {
                self.buf
                    .get(pos)
                    .map(|c| {
                        targets.contains(&c.key) && (c.tone == tone::NONE || c.tone != new_tone)
                    })
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Reposition tone (sắc/huyền/hỏi/ngã/nặng) after vowel pattern changes
    ///
    /// When user types out-of-order (e.g., "osa" instead of "oas"), the tone may be
    /// placed on wrong vowel. This function moves it to the correct position based
    /// on Vietnamese phonology rules.
    ///
    /// Returns Some((old_pos, new_pos)) if tone was moved, None otherwise.
    fn reposition_tone_if_needed(&mut self) -> Option<(usize, usize)> {
        // Check if raw_input is an English word (used later with diphthong check)
        let raw_str: String = self
            .raw_input
            .iter()
            .filter_map(|&(k, caps, _)| utils::key_to_char(k, caps))
            .collect::<String>()
            .to_lowercase();
        let is_english_word = english_dict::is_english_word(&raw_str);

        // Find vowel with tone mark (sắc/huyền/hỏi/ngã/nặng)
        let tone_info: Option<(usize, u8)> = self
            .buf
            .iter()
            .enumerate()
            .find(|(_, c)| c.mark > mark::NONE && keys::is_vowel(c.key))
            .map(|(i, c)| (i, c.mark));

        if let Some((old_pos, tone_value)) = tone_info {
            let vowels = self.collect_vowels();
            if vowels.is_empty() {
                return None;
            }

            // Skip tone repositioning if raw_input is an English word AND vowels don't form
            // a valid Vietnamese diphthong pattern.
            // This prevents "costa" → "cotá" (O→A tone move with consonants between)
            // while allowing "usee" → "uế" (valid UE diphthong pattern).
            if is_english_word && !self.vowels_form_valid_diphthong(&vowels) {
                return None;
            }

            // Check for syllable boundary: if there's a consonant between the toned vowel
            // and any later vowel, the toned vowel is in a closed syllable - don't reposition.
            // Example: "bủn" + "o" → 'n' closes "bủn", so 'o' starts new syllable.
            //
            // EXCEPTION: If vowels form a valid Vietnamese diphthong pattern, allow repositioning.
            // This handles interleaved typing like "kisna" where tone on 'i' should move to 'a'
            // because "ia" is a valid diphthong with tone on first vowel, but the user typed
            // the consonant 'n' before 'a'.
            let has_consonant_after_tone = (old_pos + 1..self.buf.len()).any(|i| {
                self.buf
                    .get(i)
                    .is_some_and(|c| !keys::is_vowel(c.key) && c.key != keys::W)
            });
            let has_vowel_after_consonant = has_consonant_after_tone
                && vowels
                    .iter()
                    .any(|v| v.pos > old_pos && self.has_consonant_between(old_pos, v.pos));

            if has_vowel_after_consonant {
                // Check if vowels form a valid diphthong pattern
                // If yes, this is NOT a syllable boundary - user just typed out of order
                if !self.vowels_form_valid_diphthong(&vowels) {
                    // Syllable boundary detected - tone is in previous syllable, don't move it
                    return None;
                }
            }

            // Pre-calculate qu/gi initial for extended vowel check
            let has_qu = self.has_qu_initial();
            let has_gi = self.has_gi_initial();

            // Issue #162 fix: Don't reposition if vowels are identical (doubled vowels like "oo", "aa", "ee").
            // These are NOT valid Vietnamese diphthongs and should keep mark on first vowel.
            // This prevents VNI "o2o" from incorrectly producing "oò" instead of "òo".
            // Issue #211: Extended to handle 3+ same vowels (e.g., "asaaa" → "áaa", not "aáa")
            // Issue #211 fix for qu/gi: Skip the first vowel when checking for extended vowel patterns
            // With "quasaaa", vowels = [u, a, a, a], but 'u' is part of "qu" consonant.
            // We should check [a, a, a] which ARE all same key.
            //
            // Special case for "gi" + "i" pattern (e.g., "giri"):
            // When has_gi and all vowels are 'i', don't reposition.
            // "gir" → "gỉ", "giri" → "gỉi" (not "giỉ")
            if has_gi && vowels.iter().all(|v| v.key == keys::I) {
                return None;
            }

            let effective_vowels: &[Vowel] = if vowels.len() >= 2
                && ((has_qu && vowels[0].key == keys::U) || (has_gi && vowels[0].key == keys::I))
            {
                &vowels[1..]
            } else {
                &vowels
            };

            // Issue #162 fix: Don't reposition if vowels are identical (doubled vowels like "oo")
            // UNLESS there's a final consonant (NG/C) - then Vietnamese tone rules apply.
            // Example: "bosoong" → "boóng" (mark moves to second O before NG final)
            let last_vowel_pos = vowels.last().map(|v| v.pos).unwrap_or(0);
            let has_final = self.has_final_consonant(last_vowel_pos);

            if effective_vowels.len() >= 2
                && effective_vowels
                    .iter()
                    .all(|v| v.key == effective_vowels[0].key)
                && !has_final
            // Allow repositioning if there's a final consonant
            {
                return None;
            }

            let new_pos =
                Phonology::find_tone_position(&vowels, has_final, self.modern_tone, has_qu, has_gi);

            if new_pos != old_pos {
                // Move tone from old position to new position
                if let Some(c) = self.buf.get_mut(old_pos) {
                    c.mark = mark::NONE;
                }
                if let Some(c) = self.buf.get_mut(new_pos) {
                    c.mark = tone_value;
                }
                return Some((old_pos, new_pos));
            }
        }
        None
    }

    /// Check if there's a consonant between two positions
    fn has_consonant_between(&self, start: usize, end: usize) -> bool {
        (start + 1..end).any(|i| {
            self.buf
                .get(i)
                .is_some_and(|c| !keys::is_vowel(c.key) && c.key != keys::W)
        })
    }

    /// Check if vowels form a valid Vietnamese diphthong pattern
    ///
    /// This allows tone repositioning even when consonants are between vowels
    /// (typed out of order). Valid diphthongs: ia, ua, oa, ai, ao, oi, etc.
    fn vowels_form_valid_diphthong(&self, vowels: &[Vowel]) -> bool {
        use crate::data::vowel::{TONE_FIRST_PATTERNS, TONE_SECOND_PATTERNS};

        if vowels.len() < 2 {
            return false;
        }

        // Check if first two vowels form a valid diphthong pattern
        let pair = [vowels[0].key, vowels[1].key];

        // Check against all known diphthong patterns
        TONE_FIRST_PATTERNS
            .iter()
            .any(|p| p[0] == pair[0] && p[1] == pair[1])
            || TONE_SECOND_PATTERNS
                .iter()
                .any(|p| p[0] == pair[0] && p[1] == pair[1])
    }

    /// Reorder buffer when a vowel completes a diphthong with earlier vowel,
    /// and there are consonants between that should be final consonants.
    ///
    /// Example: "kisna" → buffer is k-í-n-a, but Vietnamese order is k-í-a-n
    /// because "ia" is a diphthong and 'n' is the final consonant.
    ///
    /// Returns Some(reorder_start_pos) if reordering happened, None otherwise.
    fn reorder_diphthong_with_final(&mut self) -> Option<usize> {
        use crate::data::constants::VALID_FINALS_1;

        let len = self.buf.len();
        if len < 3 {
            return None; // Need at least: vowel + consonant + vowel
        }

        // Only reorder if buffer has Vietnamese transforms (tone marks or diacritics)
        // This prevents reordering for English words like "final" → "fianl"
        let has_vn_transforms = self.buf.iter().any(|c| c.mark > 0 || c.tone > 0);
        if !has_vn_transforms {
            return None;
        }

        // Skip reordering if raw_input is an English word
        // This prevents corrupting English words like "vista" → "víat"
        // The auto-restore will handle restoring "vísta" to "vista" since it's invalid VN
        // But if we reorder, "víat" looks like valid VN structure and won't restore
        let raw_str: String = self
            .raw_input
            .iter()
            .filter_map(|&(key, caps, _)| utils::key_to_char(key, caps))
            .collect::<String>()
            .to_lowercase();
        if english_dict::is_english_word(&raw_str) {
            return None;
        }

        // The new vowel is the last character in buffer
        let new_vowel_pos = len - 1;
        let new_vowel_key = self.buf.get(new_vowel_pos)?.key;

        // Find the previous vowel (before any consonants)
        let mut prev_vowel_pos = None;
        let mut consonants_between = Vec::new();

        for i in (0..new_vowel_pos).rev() {
            let c = self.buf.get(i)?;
            if keys::is_vowel(c.key) {
                prev_vowel_pos = Some(i);
                break;
            } else if c.key != keys::W {
                // Collect consonants between vowels
                consonants_between.push(i);
            }
        }

        let prev_vowel_pos = prev_vowel_pos?;
        if consonants_between.is_empty() {
            return None; // No consonants between - no reordering needed
        }

        // Check if there are other vowels before prev_vowel_pos
        // If yes, don't reorder - the consonant might belong to an earlier vowel cluster
        // Example: "coupo" - don't reorder because "ou" vowel cluster exists before 'p'
        let has_earlier_vowels =
            (0..prev_vowel_pos).any(|i| self.buf.get(i).is_some_and(|c| keys::is_vowel(c.key)));
        if has_earlier_vowels {
            return None;
        }

        // Check if the two vowels form a valid diphthong
        let prev_vowel = self.buf.get(prev_vowel_pos)?;
        let prev_vowel_key = prev_vowel.key;

        // If previous vowel has a vowel modifier (circumflex/breve/horn), it can't form
        // a diphthong with the new vowel. Example: ô+a is NOT valid, only o+a is valid.
        // Circumflex/breve/horn marks make a vowel "complete" and non-combinable.
        // Note: `tone` field = vowel modifier (^, ư, ơ, ă), `mark` field = accent (sắc, huyền, etc.)
        if prev_vowel.tone > 0 {
            return None;
        }

        let pair = [prev_vowel_key, new_vowel_key];

        // Only reorder for specific diphthongs that are commonly typed "out of order"
        // (tone modifier between vowels). Other diphthongs like OE, OA, EO are typically
        // typed in natural order and shouldn't trigger reordering.
        //
        // Patterns that support interleaved typing (V + tone + C + V → V + V + C):
        // - IA: "misa" → "mía", "kisna" → "kían"
        // - UA: "musa" → "mùa", "kusna" → "kùan"
        //
        // This prevents false positives like "copses" → "coeps" (OE pattern)
        let is_reorderable_diphthong = matches!(pair, [keys::I, keys::A] | [keys::U, keys::A]);

        if !is_reorderable_diphthong {
            return None;
        }

        // Check raw_input pattern to distinguish valid interleaved typing from foreign words
        //
        // Valid: "misa" = m + i + s + a → only tone mod (S) between I and A
        // Valid: "kisna" = k + i + s + n + a → tone mod (S) + final consonant (N) between I and A
        //        In buffer: [K, Í, N, A] - the N between I and A will become the final
        // Invalid: "gusta" = g + u + s + t + a → tone mod (S) + T between U and A
        //          BUT in buffer: [G, Ú, T, A] - T is also between U and A, same as kisna!
        //
        // The difference: "kisna" is meant to be Vietnamese, "gusta" is foreign.
        // We can't distinguish purely from structure, so use heuristics:
        // 1. Check if the pattern is common Vietnamese interleaved typing
        // 2. For IA/UA patterns with single consonant between, allow reordering
        //    (auto-restore will fix English words on space)
        //
        // This allows "kisna" → "kían" while relying on auto-restore to fix "gusta" → "gusta"

        // Check if consonants could be valid final consonants
        // For simplicity, only handle single consonant final (most common case)
        if consonants_between.len() > 2 {
            return None; // Too many consonants - probably not a reorder case
        }

        // Check if consonants form valid final (ng, nh, ch, or single consonant)
        let consonant_keys: Vec<u16> = consonants_between
            .iter()
            .rev()
            .filter_map(|&i| self.buf.get(i).map(|c| c.key))
            .collect();

        let is_valid_final = match consonant_keys.len() {
            1 => VALID_FINALS_1.contains(&consonant_keys[0]),
            2 => {
                // Check for valid 2-char finals: ng, nh, ch
                matches!(
                    (consonant_keys[0], consonant_keys[1]),
                    (keys::N, keys::G) | (keys::N, keys::H) | (keys::C, keys::H)
                )
            }
            _ => false,
        };

        if !is_valid_final {
            return None;
        }

        // Reorder: move new vowel to right after previous vowel
        // Buffer: [... prev_vowel ... consonants ... new_vowel]
        // Target: [... prev_vowel new_vowel ... consonants ...]
        //
        // Since new_vowel is already at end, we need to:
        // 1. Save the new vowel
        // 2. Move consonants one position forward (toward end)
        // 3. Insert new vowel right after prev_vowel

        let new_vowel = *self.buf.get(new_vowel_pos)?;

        // Shift consonants one position forward
        // consonants_between is in reverse order (highest pos first)
        for &pos in &consonants_between {
            if let Some(c) = self.buf.get(pos) {
                let c_copy = *c;
                if let Some(next) = self.buf.get_mut(pos + 1) {
                    *next = c_copy;
                }
            }
        }

        // Place new vowel right after prev_vowel
        let insert_pos = prev_vowel_pos + 1;
        if let Some(slot) = self.buf.get_mut(insert_pos) {
            *slot = new_vowel;
        }

        // Return position to rebuild from
        Some(insert_pos)
    }

    /// Common revert logic: clear modifier, add key to buffer, rebuild output
    fn revert_and_rebuild(&mut self, pos: usize, key: u16, caps: bool) -> Result {
        // Calculate backspace BEFORE adding key (based on old buffer state)
        // Use saturating_sub to prevent underflow if pos > buf.len()
        let backspace = self.buf.len().saturating_sub(pos) as u8;

        // Add the reverted key to buffer so validation sees the full sequence
        self.buf.push(Char::new(key, caps));

        // Build output from position (includes new key)
        // Use chars::to_char to preserve mark (sắc/huyền/etc) on reverted vowels
        let mut output = Vec::with_capacity(self.buf.len().saturating_sub(pos));
        for i in pos..self.buf.len() {
            if let Some(c) = self.buf.get(i) {
                if c.key == keys::D && c.stroke {
                    output.push(chars::get_d(c.caps));
                } else if let Some(ch) = chars::to_char(c.key, c.caps, c.tone, c.mark) {
                    output.push(ch);
                } else if let Some(ch) = utils::key_to_char(c.key, c.caps) {
                    output.push(ch);
                }
            }
        }

        Result::send(backspace, &output)
    }

    /// Revert tone transformation
    fn revert_tone(&mut self, key: u16, caps: bool) -> Result {
        self.last_transform = None;
        // Issue #211: Track which vowel triggered revert for extended vowel mode
        // After revert, subsequent same-key vowels append raw instead of re-transforming
        self.reverted_circumflex_key = Some(key);

        for pos in self.buf.find_vowels().into_iter().rev() {
            if let Some(c) = self.buf.get_mut(pos) {
                if c.tone > tone::NONE {
                    // Track circumflex revert for literal double vowel behavior
                    // When ooo→oo, eee→ee, aaa→aa, subsequent keys should be literal
                    if c.tone == tone::CIRCUMFLEX {
                        self.had_circumflex_revert = true;
                    }
                    c.tone = tone::NONE;
                    // Track for auto-restore logic (double ss/ff detection)
                    self.had_mark_revert = true;
                    // Track ww pattern for whitelist-based restore
                    self.had_telex_transform = true;
                    // Store raw_input BEFORE modification for whitelist lookup
                    self.telex_double_raw = Some(self.get_raw_input_string_preserve_case());
                    // Fix raw_input: "ww" typed → raw has [w,w] but buffer is "w"
                    // Remove the tone-triggering key from raw_input so restore works correctly
                    // raw_input: [a, w, w] → [a, w] (remove first 'w' that triggered tone)
                    // This ensures "awwait" → "await" not "awwait" on auto-restore
                    if self.raw_input.len() >= 2 {
                        let current = self.raw_input.pop(); // current key (just added)
                        self.raw_input.pop(); // tone-trigger key (consumed, discard)
                        if let Some(c) = current {
                            self.raw_input.push(c);
                        }
                    }
                    // Store length AFTER modification
                    self.telex_double_raw_len = self.raw_input.len();
                    return self.revert_and_rebuild(pos, key, caps);
                }
            }
        }
        Result::none()
    }

    /// Revert mark transformation
    /// When mark is reverted, only the reverting key appears as a letter.
    /// Standard behavior: "ass" → "as" (first 's' was modifier, second 's' reverts + outputs one 's')
    /// This matches standard Vietnamese IME behavior (UniKey, ibus-unikey, etc.)
    fn revert_mark(&mut self, key: u16, caps: bool) -> Result {
        self.last_transform = None;
        self.had_mark_revert = true; // Track for auto-restore
                                     // Set had_telex_transform for whitelist-based auto-restore
                                     // This allows "taxxi" → "taxi" (not in whitelist → keep buffer)
        self.had_telex_transform = true;
        // Store raw_input for whitelist lookup
        self.telex_double_raw = Some(self.get_raw_input_string_preserve_case());
        self.telex_double_raw_len = self.raw_input.len();

        for pos in self.buf.find_vowels().into_iter().rev() {
            if let Some(c) = self.buf.get_mut(pos) {
                if c.mark > mark::NONE {
                    c.mark = mark::NONE;

                    // Set flag to defer raw_input pop until next key
                    // If next key is CONSONANT: pop the mark key (user intended revert)
                    //   Example: "tesst" → next is 't' (consonant) → pop → "test"
                    // If next key is VOWEL: don't pop (user typing English word like "issue")
                    //   Example: "issue" → next is 'u' (vowel) → keep → "issue"
                    self.pending_mark_revert_pop = true;

                    // Add only the reverting key (current key being pressed)
                    // The original mark key was consumed as a modifier and doesn't produce output
                    self.buf.push(Char::new(key, caps));

                    // Calculate backspace and output
                    let backspace = (self.buf.len() - pos - 1) as u8; // -1 because we added 1 char
                    let output: Vec<char> = (pos..self.buf.len())
                        .filter_map(|i| self.buf.get(i))
                        .filter_map(|c| utils::key_to_char(c.key, c.caps))
                        .collect();

                    return Result::send(backspace, &output);
                }
            }
        }
        Result::none()
    }

    /// Revert stroke transformation at specific position
    fn revert_stroke(&mut self, key: u16, pos: usize) -> Result {
        self.last_transform = None;

        if let Some(c) = self.buf.get_mut(pos) {
            if c.key == keys::D && !c.stroke {
                // Un-stroked d found at pos - this means we need to add another d
                let caps = c.caps;
                self.buf.push(Char::new(key, caps));
                return self.rebuild_from(pos);
            }
        }
        Result::none()
    }

    /// Try to apply remove modifier
    /// Returns Some(Result) if a mark/tone was removed, None if nothing to remove
    /// When None is returned, the key falls through to handle_normal_letter()
    fn try_remove(&mut self) -> Option<Result> {
        self.last_transform = None;
        for pos in self.buf.find_vowels().into_iter().rev() {
            if let Some(c) = self.buf.get_mut(pos) {
                if c.mark > mark::NONE {
                    c.mark = mark::NONE;
                    return Some(self.rebuild_from(pos));
                }
                if c.tone > tone::NONE {
                    c.tone = tone::NONE;
                    return Some(self.rebuild_from(pos));
                }
            }
        }
        // Nothing to remove - return None so key can be processed as normal letter
        // This allows shortcuts like "zz" to work
        None
    }

    /// Handle normal letter input
    fn handle_normal_letter(&mut self, key: u16, caps: bool) -> Result {
        // Special case: "o" after "w→ư" should form "ươ" compound
        // This only handles the WAsVowel case (typing "w" alone creates ư)
        // For "uw" pattern, the compound is normalized in try_mark via normalize_uo_compound
        if key == keys::O && matches!(self.last_transform, Some(Transform::WAsVowel)) {
            // Add O with horn to form ươ compound
            let mut c = Char::new(key, caps);
            c.tone = tone::HORN;
            self.buf.push(c);
            self.last_transform = None;

            // Return the ơ character (o with horn)
            let vowel_char = chars::to_char(keys::O, caps, tone::HORN, 0).unwrap();
            return Result::send(0, &[vowel_char]);
        }

        // Note: ShortPatternStroke revert is now handled at the beginning of process()
        // before any modifiers are applied, so we don't need to check it here.

        // Telex: Revert delayed circumflex when same vowel is typed again
        // Pattern: After "data" → "dât" (delayed circumflex), typing 'a' again should revert to "data"
        // Buffer ends with: vowel-with-circumflex + non-extending-final (t, m, p)
        // Typed key matches the base of the circumflex vowel (a→â, e→ê, o→ô)
        // IMPORTANT: Only apply this revert for DELAYED circumflex (V+C+V pattern), not for
        // immediate circumflex (VV pattern like "deep" → "dêp"). For immediate circumflex,
        // typing another vowel should NOT revert (allows words like "deeper").
        if self.method == 0
            && self.had_vowel_triggered_circumflex
            && matches!(key, keys::A | keys::E | keys::O)
            && self.buf.len() >= 2
        {
            let last_idx = self.buf.len() - 1;
            let vowel_idx = self.buf.len() - 2;

            // Check if last char is a non-extending final consonant
            let last_is_non_extending = self
                .buf
                .get(last_idx)
                .is_some_and(|c| matches!(c.key, keys::T | keys::M | keys::P));

            // Check if second-to-last has circumflex and matches typed vowel
            let should_revert = last_is_non_extending
                && self.buf.get(vowel_idx).is_some_and(|c| {
                    c.tone == tone::CIRCUMFLEX
                        && c.key == key
                        && matches!(c.key, keys::A | keys::E | keys::O)
                });

            if should_revert {
                // Remove circumflex from the vowel
                if let Some(c) = self.buf.get_mut(vowel_idx) {
                    c.tone = tone::NONE;
                }
                // Reset vowel-triggered circumflex flag since we're reverting
                self.had_vowel_triggered_circumflex = false;
                // Track circumflex revert for auto-restore (used to collapse double vowel at end)
                self.had_circumflex_revert = true;

                // Add the typed vowel to buffer (the one that triggered revert)
                // "dataa" flow: "dât" (3 chars) → revert â → "dat" → add 'a' → "data" (4 chars)
                self.buf.push(Char::new(key, caps));

                // Rebuild from vowel position using after_insert (new char not yet on screen)
                // Screen has: "dât" (3 chars), buffer now has: "data" (4 chars)
                // Need to delete "ât" (2 chars) and output "ata" (3 chars) → screen becomes "data"
                return self.rebuild_from_after_insert(vowel_idx);
            }
        }

        // Telex: Post-tone delayed circumflex (xepse → xếp)
        // Pattern: initial-consonant + vowel-with-mark + non-extending-final (t, m, p) + same vowel
        // When user types tone BEFORE circumflex modifier: "xeps" → "xép", then 'e' → "xếp"
        // The second vowel triggers circumflex on the first vowel (keeping existing mark)
        // IMPORTANT: Must have initial consonant to form valid Vietnamese syllable
        // "expect" (e-x-p-e) should NOT trigger because no initial consonant
        if self.method == 0 && matches!(key, keys::A | keys::E | keys::O) && self.buf.len() >= 3 {
            let last_idx = self.buf.len() - 1;
            let vowel_idx = self.buf.len() - 2;

            // Check if there's at least one initial consonant before the vowel
            let has_initial_consonant =
                vowel_idx > 0 && self.buf.get(0).is_some_and(|c| keys::is_consonant(c.key));

            // Check if last char is a non-extending final consonant
            let last_is_non_extending = self
                .buf
                .get(last_idx)
                .is_some_and(|c| matches!(c.key, keys::T | keys::M | keys::P));

            // Check if second-to-last has mark but NO circumflex, and matches typed vowel
            let should_add_circumflex = has_initial_consonant
                && last_is_non_extending
                && self.buf.get(vowel_idx).is_some_and(|c| {
                    c.mark > 0 // has tone mark (sắc, huyền, etc.)
                        && c.tone == tone::NONE // but no circumflex yet
                        && c.key == key // matches typed vowel
                        && matches!(c.key, keys::A | keys::E | keys::O)
                });

            if should_add_circumflex {
                // Skip circumflex if raw_input is an English word
                // This prevents "pasta" → "pất", "costa" → "côt", etc.
                // raw_input includes the current key (pushed before process() is called)
                let raw_str: String = self
                    .raw_input
                    .iter()
                    .filter_map(|&(k, caps, _)| utils::key_to_char(k, caps))
                    .collect::<String>()
                    .to_lowercase();
                if english_dict::is_english_word(&raw_str) {
                    // Raw input is English - skip circumflex, add vowel normally
                    // The auto-restore will handle restoring the English word
                } else {
                    // Add circumflex to the vowel (keeping existing mark)
                    if let Some(c) = self.buf.get_mut(vowel_idx) {
                        c.tone = tone::CIRCUMFLEX;
                        self.had_any_transform = true;
                    }

                    // Note: raw_input already has the key (pushed at on_key_ext before process)

                    // Rebuild from vowel position (second vowel is NOT added to buffer - it's modifier)
                    // Screen has: "xép" (3 chars), buffer stays: "xếp" (3 chars, vowel updated)
                    // Need to delete "ép" (2 chars) and output "ếp" (2 chars)
                    return self.rebuild_from(vowel_idx);
                }
            }
        }

        self.last_transform = None;
        // Add letters to buffer, and numbers in both Telex and VNI modes
        // This ensures buffer.len() stays in sync with screen chars for correct backspace count
        // Issue #162: Numbers must be added to buffer in Telex mode too, otherwise patterns
        // like "o2o" have buffer = [O] (missing '2') causing the second 'o' to incorrectly
        // trigger circumflex (thinking it's "oo" → "ô")
        if keys::is_letter(key) || keys::is_number(key) {
            // Add the letter/number to buffer
            self.buf.push(Char::new(key, caps));

            // Issue #44 (part 2): Apply deferred breve when valid final consonant is typed
            // "trawm" → after "traw" (pending breve on 'a'), typing 'm' applies breve → "trăm"
            if let Some(breve_pos) = self.pending_breve_pos {
                // Valid final consonants that make breve valid: c, k, m, n, p, t
                // Note: k is included for ethnic minority words (Đắk Lắk)
                if matches!(
                    key,
                    keys::C | keys::K | keys::M | keys::N | keys::P | keys::T
                ) {
                    // Find and remove the breve modifier from buffer
                    // Telex uses 'w', VNI uses '8' - it should be right after 'a' at breve_pos
                    let modifier_pos = breve_pos + 1;
                    if modifier_pos < self.buf.len() {
                        if let Some(c) = self.buf.get(modifier_pos) {
                            // Remove 'w' (Telex) or '8' (VNI)
                            if c.key == keys::W || c.key == keys::N8 {
                                self.buf.remove(modifier_pos);
                            }
                        }
                    }

                    // Apply breve to the 'a' at pending position
                    let a_caps = self.buf.get(breve_pos).map(|c| c.caps).unwrap_or(false);
                    if let Some(c) = self.buf.get_mut(breve_pos) {
                        if c.key == keys::A {
                            c.tone = tone::HORN; // HORN on A = breve (ă)
                            self.had_any_transform = true;
                        }
                    }
                    self.pending_breve_pos = None;

                    // Rebuild from breve position: delete "aw" (or "awX"), output "ăX"
                    // Buffer now has: ...ă (at breve_pos) + consonant (just added)
                    // Screen has: ...aw (need to delete "aw", output "ă" + consonant)
                    let vowel_char = chars::to_char(keys::A, a_caps, tone::HORN, 0).unwrap_or('ă');
                    // Skip the consonant char entirely if it has no mapping, rather
                    // than emitting a literal '?' the user never typed.
                    let mut out_chars = vec![vowel_char];
                    if let Some(cons_char) = crate::utils::key_to_char(key, caps) {
                        out_chars.push(cons_char);
                    }
                    return Result::send(2, &out_chars); // backspace 2 ("aw"), output "ăm"
                } else if key == keys::W {
                    // 'w' is the breve modifier - don't clear pending_breve_pos
                    // It will be added as a regular letter and removed later
                } else if keys::is_vowel(key) {
                    // Vowel after "aw" pattern - breve not valid, clear pending
                    self.pending_breve_pos = None;
                }
                // For other consonants (not finals, not W), keep pending_breve_pos
                // They might be followed by more letters that complete the syllable
            }

            // Issue #133: Apply deferred horn to 'u' when final consonant/vowel is typed
            // "duow" → "duơ" (pending on u), then "c" → apply horn to u → "dược"
            if let Some(u_pos) = self.pending_u_horn_pos {
                // Apply horn to 'u' at pending position
                if let Some(c) = self.buf.get_mut(u_pos) {
                    if c.key == keys::U && c.tone == tone::NONE {
                        c.tone = tone::HORN;
                        self.had_any_transform = true;
                    }
                }
                self.pending_u_horn_pos = None;

                // Rebuild from u position: screen has "...uơ...", buffer has "...ươ...+new_char"
                // The new char was already pushed at line 1799 but not yet on screen
                // Use rebuild_from_after_insert which accounts for this
                return self.rebuild_from_after_insert(u_pos);
            }

            // Revert mark on B-initial triple-o when invalid consonant follows
            // "booos" → "boó", but "booost" → "boost" (revert mark when T follows)
            // Only revert for consonants that can't form valid finals (not N for NG)
            if self.had_circumflex_revert
                && self.method == 0
                && keys::is_consonant(key)
                && key != keys::N
            {
                let buf_len = self.buf.len();
                // Check for pattern: B + Oó (O with mark) at end, consonant just added
                // Buffer: [B, O, Oó, consonant] where Oó has mark != 0
                if buf_len >= 3 {
                    let first_key = self.buf.get(0).map(|c| c.key).unwrap_or(0);
                    let is_b_initial = first_key == keys::B;

                    // Check if second-to-last char (before just-added consonant) is O with mark
                    // buf_len-1 is the just-added consonant, buf_len-2 is the potential marked O
                    let marked_o_pos = buf_len - 2;
                    let has_marked_o = self
                        .buf
                        .get(marked_o_pos)
                        .map(|c| c.key == keys::O && c.mark != 0)
                        .unwrap_or(false);

                    // Check if there's another O before the marked O (double-O pattern)
                    let has_oo_pattern = marked_o_pos >= 1
                        && self
                            .buf
                            .get(marked_o_pos - 1)
                            .map(|c| c.key == keys::O)
                            .unwrap_or(false);

                    if is_b_initial && has_marked_o && has_oo_pattern {
                        // Get the mark value before clearing (to determine which key to insert)
                        let mark_val = self.buf.get(marked_o_pos).map(|c| c.mark).unwrap_or(0);
                        let mark_key = if mark_val == 1 { keys::S } else { keys::F }; // sắc=1=s, huyền=2=f

                        // Clear the mark on O
                        if let Some(c) = self.buf.get_mut(marked_o_pos) {
                            c.mark = 0;
                        }

                        // Insert the mark key as literal BEFORE the just-added consonant
                        // Use pop/push since Buffer doesn't have insert
                        // Before: [B, O, Oó, T] where Oó has mark
                        // After:  [B, O, O, S, T] (mark cleared, S inserted)
                        if let Some(consonant) = self.buf.pop() {
                            self.buf.push(Char::new(mark_key, false));
                            self.buf.push(consonant);
                        }

                        // Manual rebuild: screen has "boó" (T not yet shown)
                        // Need to delete "ó" (1 char) and output "ost" (3 chars)
                        // Output from marked_o_pos to end of buffer
                        let mut output = Vec::new();
                        for i in marked_o_pos..self.buf.len() {
                            if let Some(c) = self.buf.get(i) {
                                if let Some(ch) = chars::to_char(c.key, c.caps, c.tone, c.mark) {
                                    output.push(ch);
                                } else if let Some(ch) = crate::utils::key_to_char(c.key, c.caps) {
                                    output.push(ch);
                                }
                            }
                        }
                        // Backspace 1 to delete "ó", output "ost"
                        return Result::send(1, &output);
                    }
                }
            }

            // Detect and apply mark for triple-o word patterns (booofng → boòng)
            // When NG final is typed after a pattern like "boo" + mark_key (f/s),
            // retroactively apply the mark and remove the literal mark key
            // This handles B/C/M initials that were excluded from is_vietnamese_triple_o_word
            if key == keys::G && self.had_circumflex_revert && self.method == 0 {
                let buf_len = self.buf.len();
                // Check for pattern: [initial] + OO + [f/s] + N + G (just added)
                // Buffer now has: [B, O, O, F, N, G] or [M, O, O, S, N, G]
                if buf_len >= 5 {
                    let first_key = self.buf.get(0).map(|c| c.key).unwrap_or(0);
                    let is_bcm_initial = matches!(first_key, keys::B | keys::C | keys::M);
                    let has_n_before_g = self.buf.get(buf_len - 2).map(|c| c.key) == Some(keys::N);

                    if is_bcm_initial && has_n_before_g {
                        // Find mark key (f/s) between double-O and N
                        // Pattern: OO + mark_key + N + G
                        let mark_pos = buf_len - 3; // Position before N
                        if let Some(mark_char) = self.buf.get(mark_pos) {
                            let is_mark_key = mark_char.key == keys::F || mark_char.key == keys::S;
                            let mark_key = mark_char.key;

                            // Check for double-O before mark key
                            let has_oo_before = mark_pos >= 2
                                && self.buf.get(mark_pos - 1).map(|c| c.key) == Some(keys::O)
                                && self.buf.get(mark_pos - 2).map(|c| c.key) == Some(keys::O);

                            if is_mark_key && has_oo_before {
                                // Calculate mark value: f=huyền(2), s=sắc(1)
                                let mark_val = if mark_key == keys::F { 2u8 } else { 1u8 };
                                let o_pos = mark_pos - 1; // Last O position

                                // Remove the mark key from buffer
                                self.buf.remove(mark_pos);

                                // Apply mark to the O
                                if let Some(c) = self.buf.get_mut(o_pos) {
                                    if c.key == keys::O {
                                        c.mark = mark_val;
                                        self.had_any_transform = true;
                                    }
                                }

                                // Rebuild: screen has "boofng", buffer now has "boòng"
                                // Need extra backspace for removed mark key
                                let result = self.rebuild_from_after_insert(o_pos);
                                let chars: Vec<char> = result.chars[..result.count as usize]
                                    .iter()
                                    .filter_map(|&c| char::from_u32(c))
                                    .collect();
                                return Result::send(result.backspace + 1, &chars);
                            }
                        }
                    }
                }
            }

            // Normalize ưo → ươ immediately when 'o' is typed after 'ư'
            // This ensures "dduwo" → "đươ" (Telex) and "u7o" → "ươ" (VNI)
            // Works for both methods since "ưo" alone is not valid Vietnamese
            if key == keys::O && self.normalize_uo_compound().is_some() {
                // ươ compound formed - reposition tone if needed (ư→ơ)
                if let Some((old_pos, _)) = self.reposition_tone_if_needed() {
                    return self.rebuild_from_after_insert(old_pos);
                }

                // No tone to reposition - just output ơ
                let vowel_char = chars::to_char(keys::O, caps, tone::HORN, 0).unwrap();
                return Result::send(0, &[vowel_char]);
            }

            // Reorder buffer when a vowel completes a diphthong with earlier vowel
            // and there are consonants between that should be final consonants.
            // Example: "kisna" → buffer is k-í-n, adding 'a' should produce k-í-a-n (kían)
            // because "ia" is a diphthong and 'n' is a valid final consonant.
            if keys::is_vowel(key) {
                if let Some(reorder_pos) = self.reorder_diphthong_with_final() {
                    // After reordering, also reposition tone if needed
                    // Example: "musno" → buffer reordered to m-ú-o-n, but tone should be on 'o'
                    // because "uo" diphthong has tone on second vowel.
                    let tone_reposition = self.reposition_tone_if_needed();
                    let rebuild_pos = tone_reposition.map(|(old, _)| old).unwrap_or(reorder_pos);
                    return self.rebuild_from_after_insert(rebuild_pos);
                }
            }

            // Auto-correct tone position when new character changes the correct placement
            //
            // Two scenarios:
            // 1. New vowel changes diphthong pattern:
            //    "osa" → tone on 'o', then 'a' added → "oa" needs tone on 'a'
            // 2. New consonant creates final, which changes tone position:
            //    "muas" → tone on 'u' (ua open), then 'n' added → "uan" needs tone on 'a'
            //
            // Both cases need to reposition the tone mark based on Vietnamese phonology.
            if let Some((old_pos, _new_pos)) = self.reposition_tone_if_needed() {
                // Tone was moved - rebuild output from the old position
                // Note: the new char was just added to buffer but NOT yet displayed
                // So backspace = (chars from old_pos to BEFORE new char)
                // And output = (chars from old_pos to end INCLUDING new char)
                return self.rebuild_from_after_insert(old_pos);
            }

            // Check if adding this letter creates invalid vowel pattern (foreign word detection)
            // Only revert if the horn transforms are from w-as-vowel (standalone w→ư),
            // not from w-as-tone (adding horn to existing vowels like in "rượu")
            //
            // w-as-vowel: first horn is U at position 0 (was standalone 'w')
            // w-as-tone: horns are on vowels after initial consonant
            //
            // Exception: complete ươ compound + vowel = valid Vietnamese triphthong
            // (like "rượu" = ươu, "mười" = ươi) - don't revert in these cases
            // Only skip for vowels that form valid triphthongs (u, i), not for consonants
            // Only run foreign word detection if english_auto_restore is enabled
            if self.english_auto_restore {
                let is_valid_triphthong_ending =
                    self.has_complete_uo_compound() && (key == keys::U || key == keys::I);
                if self.has_w_as_vowel_transform() && !is_valid_triphthong_ending {
                    let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
                    let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
                    if is_foreign_word_pattern(&buffer_keys, &buffer_tones, key) {
                        return self.revert_w_as_vowel_transforms();
                    }
                }
            }

            // Auto-restore when consonant after mark creates clear English pattern
            // Example: "tex" → "tẽ", then 't' typed → "tẽt" has English modifier pattern → restore "text"
            //
            // IMPORTANT: Mid-word, only restore for clear English PATTERNS (modifier+consonant clusters),
            // NOT just structural invalidity. Words like "dọd" are invalid but user might still be typing.
            // Full structural validation happens at word boundary (space/break).
            //
            // This catches: "tex" + 't' where 'x' modifier before 't' creates English cluster
            // But preserves: "dọ" + 'd' where 'j' modifier before 'd' doesn't indicate English
            //
            // IMPORTANT: Skip mark keys (s, f, r, x, j in Telex) because they're tone modifiers,
            // not true consonants. User typing "đườ" + 's' wants to add sắc mark, not restore.
            //
            // Only run if english_auto_restore is enabled (experimental feature)
            let im = input::get(self.method);
            let is_mark_key = im.mark(key).is_some();
            if self.english_auto_restore
                && keys::is_consonant(key)
                && !is_mark_key
                && self.buf.len() >= 2
            {
                // Check if consonant immediately follows a marked character
                // Only check for mark (sắc, huyền, etc.), NOT tone (circumflex, horn, breve)
                // Circumflex from double vowel (oo→ô) should NOT trigger restore
                // Example: "ôk" from "ook" should stay as "ôk", not revert to "ok"
                if let Some(prev_char) = self.buf.get(self.buf.len() - 2) {
                    let prev_has_mark = prev_char.mark > 0;

                    if prev_has_mark && self.has_english_modifier_pattern(false) {
                        // Clear English pattern detected - restore to raw
                        if let Some(raw_chars) = self.build_raw_chars() {
                            let backspace = (self.buf.len() - 1) as u8;

                            // Repopulate buffer with restored content (plain chars, no marks)
                            // IMPORTANT: Use raw_chars (collapsed output) not raw_input
                            // This ensures buffer length matches screen after restore
                            // Example: "ook" -> raw_input=[o,o,k] but raw_chars=[o,k] after collapse
                            self.buf.clear();
                            for ch in &raw_chars {
                                let key = utils::char_to_key(*ch);
                                if key != 255 {
                                    self.buf.push(Char::new(key, ch.is_uppercase()));
                                }
                            }

                            self.last_transform = None;
                            // Reset had_any_transform since buffer now has plain chars
                            // This prevents backspace from incorrectly popping stale keys
                            self.had_any_transform = false;
                            return Result::send(backspace, &raw_chars);
                        }
                    }
                }
            }
        } else {
            // Non-letter character (number, symbol, etc.)
            // Mark that this word has non-letter prefix to prevent false shortcut matches
            // e.g., "149k" should NOT trigger shortcut "k" → "không"
            // e.g., "@abc" should NOT trigger shortcut "abc"
            self.has_non_letter_prefix = true;
        }
        Result::none()
    }

    /// Check if buffer has w-as-vowel transform (standalone w→ư at start)
    /// This is different from w-as-tone which adds horn to existing vowels
    fn has_w_as_vowel_transform(&self) -> bool {
        // w-as-vowel creates U with horn at position 0 or after consonants
        // The key distinguishing feature: the U with horn was created from 'w',
        // meaning there was no preceding vowel at that position
        //
        // Simple heuristic: if first char is U with horn, it's w-as-vowel
        // (words like "rượu" start with consonant R, not U)
        self.buf
            .get(0)
            .map(|c| c.key == keys::U && c.tone == tone::HORN)
            .unwrap_or(false)
    }

    /// Revert w-as-vowel transforms and rebuild output
    /// Used when foreign word pattern is detected after w→ư transformation
    fn revert_w_as_vowel_transforms(&mut self) -> Result {
        // Only revert if first char is U with horn (w-as-vowel pattern)
        if !self.has_w_as_vowel_transform() {
            return Result::none();
        }

        // Find all horn transforms to revert
        let horn_positions: Vec<usize> = self
            .buf
            .iter()
            .enumerate()
            .filter(|(_, c)| c.tone == tone::HORN)
            .map(|(i, _)| i)
            .collect();

        if horn_positions.is_empty() {
            return Result::none();
        }

        let first_pos = horn_positions[0];

        // Clear horn tones and change U back to W (for w-as-vowel positions)
        for &pos in &horn_positions {
            if let Some(c) = self.buf.get_mut(pos) {
                // U with horn was from 'w' → change key to W
                if c.key == keys::U {
                    c.key = keys::W;
                }
                c.tone = tone::NONE;
            }
        }

        // Use rebuild_from_after_insert because the triggering character (e.g., 'l' in "would")
        // was already pushed to buffer but NOT yet displayed on screen.
        // rebuild_from would count it in backspace, causing 1 extra backspace.
        self.rebuild_from_after_insert(first_pos)
    }

    /// Collect vowels from buffer
    fn collect_vowels(&self) -> Vec<Vowel> {
        utils::collect_vowels(&self.buf)
    }

    /// Check for final consonant after position
    fn has_final_consonant(&self, after_pos: usize) -> bool {
        utils::has_final_consonant(&self.buf, after_pos)
    }

    /// Check for qu initial
    fn has_qu_initial(&self) -> bool {
        utils::has_qu_initial(&self.buf)
    }

    /// Check for gi initial (gi + vowel)
    fn has_gi_initial(&self) -> bool {
        utils::has_gi_initial(&self.buf)
    }

    /// Rebuild output from position
    fn rebuild_from(&self, from: usize) -> Result {
        let mut output = Vec::with_capacity(self.buf.len().saturating_sub(from));
        let mut backspace = 0u8;

        for i in from..self.buf.len() {
            if let Some(c) = self.buf.get(i) {
                backspace += 1;

                if c.key == keys::D && c.stroke {
                    output.push(chars::get_d(c.caps));
                } else if let Some(ch) = chars::to_char(c.key, c.caps, c.tone, c.mark) {
                    output.push(ch);
                } else if let Some(ch) = utils::key_to_char(c.key, c.caps) {
                    output.push(ch);
                }
            }
        }

        if output.is_empty() {
            Result::none()
        } else {
            Result::send(backspace, &output)
        }
    }

    /// Rebuild output from position after a new character was inserted
    ///
    /// Unlike rebuild_from, this accounts for the fact that the last character
    /// in the buffer was just added but NOT yet displayed on screen.
    /// So backspace count = (chars from `from` to end - 1) because last char isn't on screen.
    fn rebuild_from_after_insert(&self, from: usize) -> Result {
        if self.buf.is_empty() {
            return Result::none();
        }

        let mut output = Vec::with_capacity(self.buf.len().saturating_sub(from));
        // Backspace = number of chars from `from` to BEFORE the new char
        // The new char (last in buffer) hasn't been displayed yet
        let backspace = (self.buf.len().saturating_sub(1).saturating_sub(from)) as u8;

        for i in from..self.buf.len() {
            if let Some(c) = self.buf.get(i) {
                if c.key == keys::D && c.stroke {
                    output.push(chars::get_d(c.caps));
                } else if let Some(ch) = chars::to_char(c.key, c.caps, c.tone, c.mark) {
                    output.push(ch);
                } else if let Some(ch) = utils::key_to_char(c.key, c.caps) {
                    output.push(ch);
                }
            }
        }

        if output.is_empty() {
            Result::none()
        } else {
            Result::send(backspace, &output)
        }
    }

    /// Clear buffer and raw input history
    /// Note: Does NOT clear word_history to preserve backspace-after-space feature
    /// Also restores pending_capitalize if auto_capitalize was used (for selection-delete)
    pub fn clear(&mut self) {
        // Restore pending_capitalize if auto_capitalize was used
        // This handles selection-delete: user selects and deletes text,
        // we should restore pending state so next letter is capitalized
        if self.auto_capitalize_used {
            self.pending_capitalize = true;
            self.auto_capitalize_used = false;
        }
        self.buf.clear();
        self.raw_input.clear();
        self.last_transform = None;
        self.has_non_letter_prefix = false;
        self.pending_breve_pos = None;
        self.pending_u_horn_pos = None;
        self.stroke_reverted = false;
        self.had_mark_revert = false;
        self.pending_mark_revert_pop = false;
        self.had_any_transform = false;
        self.had_vowel_triggered_circumflex = false;
        self.had_circumflex_revert = false;
        self.reverted_circumflex_key = None;
        self.had_telex_transform = false;
        self.telex_double_raw = None;
        self.telex_double_raw_len = 0;
        self.restored_pending_clear = false;
        self.restored_is_ascii = false;
        self.shortcut_prefix.clear();
    }

    /// Re-detect pending_u_horn_pos by scanning buffer for "u(no tone) + o(horn)" pattern
    /// Used after restoring buffer from word history where this state was lost on clear()
    fn re_detect_pending_u_horn(&mut self) {
        self.pending_u_horn_pos = None;
        let len = self.buf.len();
        if len < 2 {
            return;
        }
        // Check last two chars for u + ơ pattern (no final consonant after)
        if let (Some(c1), Some(c2)) = (self.buf.get(len - 2), self.buf.get(len - 1)) {
            if c1.key == keys::U
                && c1.tone == tone::NONE
                && c2.key == keys::O
                && c2.tone == tone::HORN
            {
                self.pending_u_horn_pos = Some(len - 2);
            }
        }
    }

    /// Reconstruct `last_transform` after a committed word is restored into the
    /// buffer for further editing.
    ///
    /// The toggle/revert logic (`try_tone`, `try_mark`, `try_stroke`) decides
    /// whether a repeated modifier key reverts a diacritic by inspecting
    /// `last_transform`. That state is per-keystroke and is dropped when a word is
    /// committed with Space, so a restored word would otherwise ignore the next
    /// toggle key — e.g. restored "sơ" + "w" stayed "sơ" instead of reverting to
    /// "sow". (Tone-mark keys s/f/r/x/j had a separate vowel-at-end revert path and
    /// were unaffected, so the gap was tone-only.)
    ///
    /// We rebuild it from the diacritics on the final buffer character, mapping
    /// each diacritic back to the modifier key that produces it in the current
    /// method, so editing a restored word behaves like editing it before commit.
    fn re_detect_last_transform(&mut self) {
        use crate::data::chars::{mark, tone};

        self.last_transform = None;
        let is_vni = self.method == 1;

        // These arms are the inverse of the forward key maps in input/telex.rs and
        // input/vni.rs — keep them in sync if a method's bindings ever change.
        //
        // A tone mark (sắc/huyền/hỏi/ngã/nặng) is always the last diacritic applied
        // to a syllable and may sit on a non-final vowel ("bía" marks 'í', not the
        // trailing 'a'), so scan the whole buffer for it. A repeated mark key then
        // reverts it after restore, re-arming whitelist auto-restore ("biass"→"bias").
        if let Some(&c) = self.buf.iter().rev().find(|c| c.mark != mark::NONE) {
            let key = if is_vni {
                match c.mark {
                    mark::HUYEN => keys::N2,
                    mark::HOI => keys::N3,
                    mark::NGA => keys::N4,
                    mark::NANG => keys::N5,
                    _ => keys::N1, // SAC
                }
            } else {
                match c.mark {
                    mark::HUYEN => keys::F,
                    mark::HOI => keys::R,
                    mark::NGA => keys::X,
                    mark::NANG => keys::J,
                    _ => keys::S, // SAC
                }
            };
            self.last_transform = Some(Transform::Mark(key, c.mark));
            return;
        }

        // A vowel tone (circumflex/horn/breve) only counts as the last transform when
        // it is on the FINAL character. A trailing consonant means a later keystroke
        // (the consonant) was the actual last action, so leave last_transform cleared —
        // matching continuous typing where "tuân" + 'a' appends instead of reverting.
        let Some(&c) = self.buf.last() else { return };
        if c.tone != tone::NONE {
            let key = if is_vni {
                match c.tone {
                    tone::CIRCUMFLEX => keys::N6,
                    // HORN on 'a' is breve (key 8); on o/u it is horn (key 7).
                    _ if c.key == keys::A => keys::N8,
                    _ => keys::N7,
                }
            } else {
                match c.tone {
                    tone::CIRCUMFLEX => c.key, // a/e/o double themselves: aa→â
                    _ => keys::W,              // horn & breve both use 'w'
                }
            };
            self.last_transform = Some(Transform::Tone(key, c.tone));
        }
    }

    /// Clear everything including word history
    /// Used when cursor position changes (mouse click, arrow keys, etc.)
    /// to prevent accidental restore from stale history
    /// Issue #274: Also reset auto-capitalize state to prevent incorrect
    /// capitalization after paste/cursor change
    pub fn clear_all(&mut self) {
        self.clear();
        self.word_history.clear();
        self.spaces_after_commit = 0;
        // Issue #274: Reset auto-capitalize state on cursor change
        // This prevents incorrect capitalization after copy-paste
        self.pending_capitalize = false;
        self.saw_sentence_ending = false;
    }

    /// Get the full composed buffer as a Vietnamese string with diacritics.
    ///
    /// Used for "Select All + Replace" injection method.
    pub fn get_buffer_string(&self) -> String {
        self.buf.to_full_string()
    }

    /// Debug: Check if vowel-triggered circumflex flag is set
    pub fn had_vowel_circumflex(&self) -> bool {
        self.had_vowel_triggered_circumflex
    }

    /// Debug: Get raw_input length
    pub fn raw_input_len(&self) -> usize {
        self.raw_input.len()
    }

    /// Try to convert bracket key to vowel: ] → ư, [ → ơ (Issue #159)
    ///
    /// Returns Some(Result) if bracket was converted, None otherwise.
    /// Handles:
    /// - ] at word start or after consonant → ư
    /// - [ at word start or after consonant → ơ
    /// - Double bracket reverts: ]] → ], [[ → [, uppercase revert → } or {
    /// - Valid Vietnamese vowel combinations: ươ (from ][)
    fn try_bracket_as_vowel(&mut self, key: u16, caps: bool) -> Option<Result> {
        // Check if bracket shortcut is enabled
        if !self.bracket_shortcut {
            return None;
        }

        // Check for revert: if last transform was BracketAsVowel with same bracket
        if self.last_transform == Some(Transform::BracketAsVowel) && !self.buf.is_empty() {
            if let Some(last_char) = self.buf.last() {
                // Check if last char matches the bracket we're typing
                let should_revert = match key {
                    keys::RBRACKET => last_char.key == keys::U && last_char.tone == tone::HORN,
                    keys::LBRACKET => last_char.key == keys::O && last_char.tone == tone::HORN,
                    _ => false,
                };

                if should_revert {
                    // Remove the vowel we added
                    self.buf.pop();
                    // Also remove from raw_input
                    self.raw_input.pop();
                    // Clear transform
                    self.last_transform = None;

                    // Return the original bracket character
                    // Use caps (Shift or CapsLock) to decide: uppercase → {/}, lowercase → [/]
                    let bracket_char = match (key, caps) {
                        (keys::RBRACKET, true) => '}',
                        (keys::RBRACKET, false) => ']',
                        (keys::LBRACKET, true) => '{',
                        (keys::LBRACKET, false) => '[',
                        (_, true) => '{',  // fallback (shouldn't happen)
                        (_, false) => '[', // fallback (shouldn't happen)
                    };
                    return Some(Result::send_consumed(1, &[bracket_char]));
                }
            }
        }

        // Determine target vowel based on bracket key
        let base_key = if key == keys::RBRACKET {
            keys::U // ] → ư (U with horn)
        } else {
            keys::O // [ → ơ (O with horn)
        };

        // Add vowel to buffer (similar to W shortcut pattern)
        self.buf.push(Char::new(base_key, caps));

        // Set horn tone to make ư or ơ
        if let Some(c) = self.buf.get_mut(self.buf.len() - 1) {
            c.tone = tone::HORN;
        }

        // Validate: is this valid Vietnamese?
        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
        let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
        if !is_valid_with_tones(&buffer_keys, &buffer_tones) {
            // Invalid - remove the vowel we added
            self.buf.pop();
            return None;
        }

        // Track raw input for ESC restore
        self.raw_input.push((key, caps, false));

        // Mark transform
        self.last_transform = Some(Transform::BracketAsVowel);
        self.had_any_transform = true;

        // Return result with key consumed (don't pass through bracket)
        let vowel_char = chars::to_char(base_key, caps, tone::HORN, 0).unwrap();
        Some(Result::send_consumed(0, &[vowel_char]))
    }

}

#[cfg(test)]
mod tests_inline;
