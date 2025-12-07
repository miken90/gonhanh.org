//! Vietnamese Language Data Modules
//!
//! This module contains all linguistic data for Vietnamese input:
//! - `keys`: Virtual keycode definitions (platform-specific)
//! - `chars`: Unicode character conversion
//! - `vowel`: Vietnamese vowel phonology system

pub mod chars;
pub mod keys;
pub mod vowel;

pub use chars::{get_d, to_char};
pub use keys::{is_break, is_letter, is_vowel};
pub use vowel::{Modifier, Phonology, Role, Vowel};
