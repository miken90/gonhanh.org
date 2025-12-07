//! Vietnamese IME Engine
//!
//! Core engine for Vietnamese input method processing.
//! Supports Telex and VNI input methods.
//!
//! ## Architecture
//!
//! The engine uses a phonology-based approach:
//! 1. Buffer stores raw key presses with modifiers
//! 2. Vowel classification determines phonological roles
//! 3. Algorithmic tone placement based on Vietnamese rules
//!
//! ## References
//! - /docs/vietnamese-language-system.md (project root)
//! - https://vi.wikipedia.org/wiki/Quy_tắc_đặt_dấu_thanh_của_chữ_Quốc_ngữ

pub mod buffer;

use crate::data::{
    chars, keys,
    vowel::{Modifier, Phonology, Vowel},
};
use crate::input;
use buffer::{Buffer, Char, MAX};

/// Convert key code to character (letters and numbers)
fn key_to_char(key: u16, caps: bool) -> Option<char> {
    let ch = match key {
        keys::A => 'a',
        keys::B => 'b',
        keys::C => 'c',
        keys::D => 'd',
        keys::E => 'e',
        keys::F => 'f',
        keys::G => 'g',
        keys::H => 'h',
        keys::I => 'i',
        keys::J => 'j',
        keys::K => 'k',
        keys::L => 'l',
        keys::M => 'm',
        keys::N => 'n',
        keys::O => 'o',
        keys::P => 'p',
        keys::Q => 'q',
        keys::R => 'r',
        keys::S => 's',
        keys::T => 't',
        keys::U => 'u',
        keys::V => 'v',
        keys::W => 'w',
        keys::X => 'x',
        keys::Y => 'y',
        keys::Z => 'z',
        keys::N0 => return Some('0'),
        keys::N1 => return Some('1'),
        keys::N2 => return Some('2'),
        keys::N3 => return Some('3'),
        keys::N4 => return Some('4'),
        keys::N5 => return Some('5'),
        keys::N6 => return Some('6'),
        keys::N7 => return Some('7'),
        keys::N8 => return Some('8'),
        keys::N9 => return Some('9'),
        _ => return None,
    };
    Some(if caps { ch.to_ascii_uppercase() } else { ch })
}

/// Engine action result
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    None = 0,    // Pass through
    Send = 1,    // Delete + send new chars
    Restore = 2, // Invalid, restore original
}

/// Result for FFI - fields ordered for alignment
#[repr(C)]
pub struct Result {
    pub chars: [u32; MAX], // Unicode output array
    pub action: u8,
    pub backspace: u8,
    pub count: u8,
    pub _pad: u8,
}

impl Result {
    pub fn none() -> Self {
        Self {
            chars: [0; MAX],
            action: Action::None as u8,
            backspace: 0,
            count: 0,
            _pad: 0,
        }
    }

    pub fn send(backspace: u8, chars: &[char]) -> Self {
        let mut result = Self {
            chars: [0; MAX],
            action: Action::Send as u8,
            backspace,
            count: chars.len().min(MAX) as u8,
            _pad: 0,
        };
        for (i, &c) in chars.iter().take(MAX).enumerate() {
            result.chars[i] = c as u32;
        }
        result
    }
}

/// Transform type for revert tracking
#[derive(Clone, Copy, Debug, PartialEq)]
enum Transform {
    Mark(u16, u8),      // (key, mark_value)
    Tone(u16, u8, u16), // (key, tone_value, target_vowel_key)
}

/// Main Vietnamese IME engine
pub struct Engine {
    buf: Buffer,
    method: u8,
    enabled: bool,
    modern: bool,
    last_transform: Option<Transform>,
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
            modern: true,
            last_transform: None,
        }
    }

    pub fn set_method(&mut self, method: u8) {
        self.method = method;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.buf.clear();
        }
    }

    pub fn set_modern(&mut self, modern: bool) {
        self.modern = modern;
    }

    /// Handle key event - main entry point
    pub fn on_key(&mut self, key: u16, caps: bool, ctrl: bool) -> Result {
        if !self.enabled || ctrl {
            self.buf.clear();
            return Result::none();
        }

        if keys::is_break(key) {
            self.buf.clear();
            return Result::none();
        }

        if key == keys::DELETE {
            self.buf.pop();
            return Result::none();
        }

        self.process(key, caps)
    }

    fn process(&mut self, key: u16, caps: bool) -> Result {
        let m = input::get(self.method);
        let prev_key = self.buf.last().map(|c| c.key);

        // Handle đ (dd/d9) - immediate mode
        if m.is_d(key, prev_key) {
            self.last_transform = None;
            return self.handle_d();
        }

        // Collect buffer keys for delayed operations (excluding already-converted đ)
        let buffer_keys: Vec<u16> = self
            .buf
            .iter()
            .filter(|c| !c.stroke) // Skip already converted 'd' -> 'đ'
            .map(|c| c.key)
            .collect();

        // Handle delayed đ (VNI: dung9 -> đung)
        if m.is_d_for(key, &buffer_keys) {
            self.last_transform = None;
            return self.handle_delayed_d();
        }

        // Handle tone modifiers (aa, aw, a6, a7, etc.)
        // Only include vowels that don't have a tone yet (for new tone application)
        let vowel_keys: Vec<u16> = self
            .buf
            .iter()
            .filter(|c| keys::is_vowel(c.key) && c.tone == 0)
            .map(|c| c.key)
            .collect();

        // First try to apply tone to a new vowel
        if let Some((tone, target_key)) = m.is_tone_for(key, &vowel_keys) {
            return self.handle_tone(key, tone, target_key);
        }

        // No new vowel to apply → check for revert (double-key press on same target)
        if let Some(Transform::Tone(last_key, _, last_target)) = self.last_transform {
            if last_key == key {
                // Check if this key would have targeted the same vowel
                let all_vowel_keys: Vec<u16> = self
                    .buf
                    .iter()
                    .filter(|c| keys::is_vowel(c.key))
                    .map(|c| c.key)
                    .collect();
                if let Some((_, target_key)) = m.is_tone_for(key, &all_vowel_keys) {
                    if target_key == last_target {
                        return self.revert_tone(key, caps);
                    }
                }
            }
        }

        // Handle marks (s/f/r/x/j or 1-5)
        if let Some(mark) = m.is_mark(key) {
            // Check for double-key revert
            if let Some(Transform::Mark(last_key, _)) = self.last_transform {
                if last_key == key {
                    return self.revert_mark(key, caps);
                }
            }
            return self.handle_mark(key, mark);
        }

        // Handle remove mark (z or 0)
        if m.is_remove(key) {
            self.last_transform = None;
            return self.handle_remove();
        }

        // Normal letter - add to buffer
        self.last_transform = None;
        if keys::is_letter(key) {
            self.buf.push(Char::new(key, caps));
        } else {
            self.buf.clear();
        }

        Result::none()
    }

    /// Handle đ transformation (dd/d9) - immediate mode
    fn handle_d(&mut self) -> Result {
        let caps = self.buf.last().map(|c| c.caps).unwrap_or(false);
        self.buf.pop();
        Result::send(1, &[chars::get_d(caps)])
    }

    /// Handle delayed đ transformation (VNI: dung9 -> đung)
    /// Find 'd' in buffer, convert to 'đ', rebuild from that position
    fn handle_delayed_d(&mut self) -> Result {
        // Find position of unconverted 'd' in buffer
        let d_pos = self
            .buf
            .iter()
            .enumerate()
            .find(|(_, c)| c.key == keys::D && !c.stroke)
            .map(|(i, _)| i);

        if let Some(pos) = d_pos {
            // Mark 'd' as converted to 'đ'
            if let Some(c) = self.buf.get_mut(pos) {
                c.stroke = true;
            }

            // Rebuild from 'd' position
            self.rebuild_from(pos)
        } else {
            Result::none()
        }
    }

    /// Handle tone modifier (^, ơ, ư, ă)
    fn handle_tone(&mut self, key: u16, tone: u8, target_key: u16) -> Result {
        if let Some(pos) = self.buf.find_vowel_by_key(target_key) {
            if let Some(c) = self.buf.get_mut(pos) {
                c.tone = tone;
                self.last_transform = Some(Transform::Tone(key, tone, target_key));
                return self.rebuild_from(pos);
            }
        }
        Result::none()
    }

    /// Revert tone transformation
    fn revert_tone(&mut self, key: u16, caps: bool) -> Result {
        self.last_transform = None;

        // Find and remove tone from any vowel
        for pos in self.buf.find_vowels().into_iter().rev() {
            if let Some(c) = self.buf.get_mut(pos) {
                if c.tone > 0 {
                    c.tone = 0;
                    let mut result = self.rebuild_from(pos);
                    // Append the revert key character
                    if let Some(ch) = key_to_char(key, caps) {
                        if result.count < MAX as u8 {
                            result.chars[result.count as usize] = ch as u32;
                            result.count += 1;
                        }
                    }
                    return result;
                }
            }
        }
        Result::none()
    }

    /// Handle mark (dấu thanh: sắc, huyền, hỏi, ngã, nặng)
    fn handle_mark(&mut self, key: u16, mark: u8) -> Result {
        let vowels = self.collect_vowels();
        if vowels.is_empty() {
            return Result::none();
        }

        // Use phonology-based algorithm to find mark position
        let last_vowel_pos = vowels.last().map(|v| v.pos).unwrap_or(0);
        let has_final = self.has_final_consonant(last_vowel_pos);
        let pos = Phonology::find_tone_position(&vowels, has_final, self.modern);

        if let Some(c) = self.buf.get_mut(pos) {
            c.mark = mark;
            self.last_transform = Some(Transform::Mark(key, mark));
            return self.rebuild_from(pos);
        }

        Result::none()
    }

    /// Revert mark transformation
    fn revert_mark(&mut self, key: u16, caps: bool) -> Result {
        self.last_transform = None;

        // Find and remove mark from any vowel
        for pos in self.buf.find_vowels().into_iter().rev() {
            if let Some(c) = self.buf.get_mut(pos) {
                if c.mark > 0 {
                    c.mark = 0;
                    let mut result = self.rebuild_from(pos);
                    // Append the revert key character
                    if let Some(ch) = key_to_char(key, caps) {
                        if result.count < MAX as u8 {
                            result.chars[result.count as usize] = ch as u32;
                            result.count += 1;
                        }
                    }
                    return result;
                }
            }
        }
        Result::none()
    }

    /// Handle remove mark/tone (z or 0)
    fn handle_remove(&mut self) -> Result {
        // Remove mark first, then tone
        for pos in self.buf.find_vowels().into_iter().rev() {
            if let Some(c) = self.buf.get_mut(pos) {
                if c.mark > 0 {
                    c.mark = 0;
                    return self.rebuild_from(pos);
                }
                if c.tone > 0 {
                    c.tone = 0;
                    return self.rebuild_from(pos);
                }
            }
        }
        Result::none()
    }

    /// Collect vowels from buffer with phonological information
    fn collect_vowels(&self) -> Vec<Vowel> {
        self.buf
            .iter()
            .enumerate()
            .filter(|(_, c)| keys::is_vowel(c.key))
            .map(|(pos, c)| {
                let modifier = match c.tone {
                    1 => Modifier::Circumflex,
                    2 => Modifier::Horn,
                    _ => Modifier::None,
                };
                Vowel::new(c.key, modifier, pos)
            })
            .collect()
    }

    /// Check if there's a consonant after the given position
    fn has_final_consonant(&self, after_pos: usize) -> bool {
        (after_pos + 1..self.buf.len()).any(|i| {
            self.buf
                .get(i)
                .map(|c| keys::is_consonant(c.key))
                .unwrap_or(false)
        })
    }

    /// Rebuild output from position
    fn rebuild_from(&self, from: usize) -> Result {
        let mut output = Vec::with_capacity(self.buf.len() - from);
        let mut backspace = 0u8;

        for i in from..self.buf.len() {
            if let Some(c) = self.buf.get(i) {
                backspace += 1;

                // Handle 'd' -> 'đ' conversion
                if c.key == keys::D && c.stroke {
                    output.push(chars::get_d(c.caps));
                }
                // Try vowel conversion
                else if let Some(ch) = chars::to_char(c.key, c.caps, c.tone, c.mark) {
                    output.push(ch);
                }
                // Consonant
                else if let Some(ch) = key_to_char(c.key, c.caps) {
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

    /// Clear buffer
    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_keys(e: &mut Engine, s: &str) -> Vec<Result> {
        s.chars()
            .map(|c| {
                let key = match c.to_ascii_lowercase() {
                    'a' => keys::A,
                    'b' => keys::B,
                    'c' => keys::C,
                    'd' => keys::D,
                    'e' => keys::E,
                    'f' => keys::F,
                    'g' => keys::G,
                    'h' => keys::H,
                    'i' => keys::I,
                    'j' => keys::J,
                    'k' => keys::K,
                    'l' => keys::L,
                    'm' => keys::M,
                    'n' => keys::N,
                    'o' => keys::O,
                    'p' => keys::P,
                    'q' => keys::Q,
                    'r' => keys::R,
                    's' => keys::S,
                    't' => keys::T,
                    'u' => keys::U,
                    'v' => keys::V,
                    'w' => keys::W,
                    'x' => keys::X,
                    'y' => keys::Y,
                    'z' => keys::Z,
                    '0' => keys::N0,
                    '1' => keys::N1,
                    '2' => keys::N2,
                    '3' => keys::N3,
                    '4' => keys::N4,
                    '5' => keys::N5,
                    '6' => keys::N6,
                    '7' => keys::N7,
                    '8' => keys::N8,
                    '9' => keys::N9,
                    _ => 0,
                };
                e.on_key(key, c.is_uppercase(), false)
            })
            .collect()
    }

    fn last_char(r: &Result) -> Option<char> {
        if r.action == Action::Send as u8 && r.count > 0 {
            char::from_u32(r.chars[0])
        } else {
            None
        }
    }

    #[test]
    fn telex_basic() {
        let mut e = Engine::new();
        let r = type_keys(&mut e, "as");
        assert_eq!(last_char(&r[1]), Some('á'));
    }

    #[test]
    fn vni_basic() {
        let mut e = Engine::new();
        e.set_method(1);
        let r = type_keys(&mut e, "a1");
        assert_eq!(last_char(&r[1]), Some('á'));
    }

    #[test]
    fn telex_circumflex() {
        let mut e = Engine::new();
        let r = type_keys(&mut e, "aa");
        assert_eq!(last_char(&r[1]), Some('â'));
    }

    #[test]
    fn telex_horn() {
        let mut e = Engine::new();
        let r = type_keys(&mut e, "ow");
        assert_eq!(last_char(&r[1]), Some('ơ'));
    }

    #[test]
    fn telex_d() {
        let mut e = Engine::new();
        let r = type_keys(&mut e, "dd");
        assert_eq!(last_char(&r[1]), Some('đ'));
    }
}
