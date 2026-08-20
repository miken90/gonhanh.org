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
mod modifiers;
pub mod shortcut;
mod shortcut_flow;
pub mod syllable;
pub mod transform;
pub mod validation;

use crate::data::{
    chars::{self, tone},
    constants, english_dict, keys,
    vowel::Vowel,
};
use crate::input;
use crate::utils;
use buffer::{Buffer, Char, MAX};
use capitalize::{is_sentence_ending_punctuation, should_reset_pending_capitalize};
use helpers::break_key_to_char;
use shortcut::{InputMethod, ShortcutTable};
use validation::{is_foreign_word_pattern, is_valid};

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

}

#[cfg(test)]
mod tests_inline;
