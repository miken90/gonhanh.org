//! Vietnamese Unicode Character System
//!
//! Provides character conversion between base vowels + modifiers + marks
//! and composed Vietnamese Unicode characters.
//!
//! ## Design Principles
//! - Single lookup table for all vowel combinations (12 bases × 6 marks = 72)
//! - Uses Rust's built-in `to_uppercase()` for case conversion
//! - No hardcoded case-by-case matching
//!
//! ## Character Components
//! - Base vowel: a, ă, â, e, ê, i, o, ô, ơ, u, ư, y
//! - Mark (dấu thanh): none, sắc, huyền, hỏi, ngã, nặng
//! - Case: lowercase, uppercase

use super::keys;

/// Vietnamese vowel lookup table
/// Each entry: (base_char, [sắc, huyền, hỏi, ngã, nặng])
const VOWEL_TABLE: [(char, [char; 5]); 12] = [
    ('a', ['á', 'à', 'ả', 'ã', 'ạ']),
    ('ă', ['ắ', 'ằ', 'ẳ', 'ẵ', 'ặ']),
    ('â', ['ấ', 'ầ', 'ẩ', 'ẫ', 'ậ']),
    ('e', ['é', 'è', 'ẻ', 'ẽ', 'ẹ']),
    ('ê', ['ế', 'ề', 'ể', 'ễ', 'ệ']),
    ('i', ['í', 'ì', 'ỉ', 'ĩ', 'ị']),
    ('o', ['ó', 'ò', 'ỏ', 'õ', 'ọ']),
    ('ô', ['ố', 'ồ', 'ổ', 'ỗ', 'ộ']),
    ('ơ', ['ớ', 'ờ', 'ở', 'ỡ', 'ợ']),
    ('u', ['ú', 'ù', 'ủ', 'ũ', 'ụ']),
    ('ư', ['ứ', 'ừ', 'ử', 'ữ', 'ự']),
    ('y', ['ý', 'ỳ', 'ỷ', 'ỹ', 'ỵ']),
];

/// Get base character from key + tone modifier
///
/// # Arguments
/// * `key` - Virtual keycode (a, e, i, o, u, y)
/// * `tone` - Tone modifier: 0=none, 1=circumflex(^), 2=horn/breve
///
/// # Returns
/// Base vowel character: a, ă, â, e, ê, i, o, ô, ơ, u, ư, y
fn get_base_char(key: u16, tone: u8) -> Option<char> {
    match key {
        keys::A => Some(match tone {
            1 => 'â', // circumflex
            2 => 'ă', // breve
            _ => 'a',
        }),
        keys::E => Some(match tone {
            1 => 'ê', // circumflex
            _ => 'e',
        }),
        keys::I => Some('i'),
        keys::O => Some(match tone {
            1 => 'ô', // circumflex
            2 => 'ơ', // horn
            _ => 'o',
        }),
        keys::U => Some(match tone {
            2 => 'ư', // horn
            _ => 'u',
        }),
        keys::Y => Some('y'),
        _ => None,
    }
}

/// Apply mark to base vowel character
///
/// Uses lookup table to find the marked variant.
///
/// # Arguments
/// * `base` - Base vowel character (a, ă, â, e, ê, i, o, ô, ơ, u, ư, y)
/// * `mark` - Mark: 0=none, 1=sắc, 2=huyền, 3=hỏi, 4=ngã, 5=nặng
fn apply_mark(base: char, mark: u8) -> char {
    if mark == 0 || mark > 5 {
        return base;
    }

    VOWEL_TABLE
        .iter()
        .find(|(b, _)| *b == base)
        .map(|(_, marks)| marks[(mark - 1) as usize])
        .unwrap_or(base)
}

/// Convert to uppercase using Rust's Unicode-aware method
///
/// This handles all Vietnamese characters correctly without
/// explicit character mapping.
fn to_upper(ch: char) -> char {
    ch.to_uppercase().next().unwrap_or(ch)
}

/// Convert key + modifiers to Vietnamese character
///
/// # Arguments
/// * `key` - Virtual keycode
/// * `caps` - Uppercase flag
/// * `tone` - Tone modifier: 0=none, 1=circumflex(^), 2=horn/breve
/// * `mark` - Mark: 0=none, 1=sắc, 2=huyền, 3=hỏi, 4=ngã, 5=nặng
pub fn to_char(key: u16, caps: bool, tone: u8, mark: u8) -> Option<char> {
    // Handle D specially (not a vowel but needs conversion)
    if key == keys::D {
        return Some(if caps { 'D' } else { 'd' });
    }

    let base = get_base_char(key, tone)?;
    let marked = apply_mark(base, mark);
    Some(if caps { to_upper(marked) } else { marked })
}

/// Get đ/Đ character
pub fn get_d(caps: bool) -> char {
    if caps { 'Đ' } else { 'đ' }
}

/// Check if a character is a Vietnamese vowel
pub fn is_vowel_char(ch: char) -> bool {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    VOWEL_TABLE.iter().any(|(base, marks)| {
        *base == lower || marks.iter().any(|&m| m == lower)
    })
}

/// Get the base (unmarked) form of a Vietnamese vowel
pub fn get_base_vowel(ch: char) -> Option<char> {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    VOWEL_TABLE
        .iter()
        .find(|(base, marks)| *base == lower || marks.iter().any(|&m| m == lower))
        .map(|(base, _)| *base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_vowels() {
        // Basic vowels without modifiers
        assert_eq!(to_char(keys::A, false, 0, 0), Some('a'));
        assert_eq!(to_char(keys::E, false, 0, 0), Some('e'));
        assert_eq!(to_char(keys::I, false, 0, 0), Some('i'));
        assert_eq!(to_char(keys::O, false, 0, 0), Some('o'));
        assert_eq!(to_char(keys::U, false, 0, 0), Some('u'));
        assert_eq!(to_char(keys::Y, false, 0, 0), Some('y'));
    }

    #[test]
    fn test_tone_modifiers() {
        // Circumflex (^)
        assert_eq!(to_char(keys::A, false, 1, 0), Some('â'));
        assert_eq!(to_char(keys::E, false, 1, 0), Some('ê'));
        assert_eq!(to_char(keys::O, false, 1, 0), Some('ô'));

        // Horn/Breve
        assert_eq!(to_char(keys::A, false, 2, 0), Some('ă'));
        assert_eq!(to_char(keys::O, false, 2, 0), Some('ơ'));
        assert_eq!(to_char(keys::U, false, 2, 0), Some('ư'));
    }

    #[test]
    fn test_marks() {
        // All 5 marks on 'a'
        assert_eq!(to_char(keys::A, false, 0, 1), Some('á')); // sắc
        assert_eq!(to_char(keys::A, false, 0, 2), Some('à')); // huyền
        assert_eq!(to_char(keys::A, false, 0, 3), Some('ả')); // hỏi
        assert_eq!(to_char(keys::A, false, 0, 4), Some('ã')); // ngã
        assert_eq!(to_char(keys::A, false, 0, 5), Some('ạ')); // nặng
    }

    #[test]
    fn test_combined_tone_and_mark() {
        // â + sắc = ấ
        assert_eq!(to_char(keys::A, false, 1, 1), Some('ấ'));
        // ơ + huyền = ờ
        assert_eq!(to_char(keys::O, false, 2, 2), Some('ờ'));
        // ư + nặng = ự
        assert_eq!(to_char(keys::U, false, 2, 5), Some('ự'));
    }

    #[test]
    fn test_uppercase() {
        assert_eq!(to_char(keys::A, true, 0, 0), Some('A'));
        assert_eq!(to_char(keys::A, true, 0, 1), Some('Á'));
        assert_eq!(to_char(keys::A, true, 1, 1), Some('Ấ'));
        assert_eq!(to_char(keys::O, true, 2, 2), Some('Ờ'));
        assert_eq!(to_char(keys::U, true, 2, 5), Some('Ự'));
    }

    #[test]
    fn test_d() {
        assert_eq!(get_d(false), 'đ');
        assert_eq!(get_d(true), 'Đ');
    }

    #[test]
    fn test_is_vowel_char() {
        assert!(is_vowel_char('a'));
        assert!(is_vowel_char('á'));
        assert!(is_vowel_char('ầ'));
        assert!(is_vowel_char('Ự'));
        assert!(!is_vowel_char('b'));
        assert!(!is_vowel_char('đ'));
    }

    #[test]
    fn test_get_base_vowel() {
        assert_eq!(get_base_vowel('á'), Some('a'));
        assert_eq!(get_base_vowel('ầ'), Some('â'));
        assert_eq!(get_base_vowel('ự'), Some('ư'));
        assert_eq!(get_base_vowel('b'), None);
    }
}
