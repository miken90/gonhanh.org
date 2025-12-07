//! Vietnamese Vowel System
//!
//! Implements phonological classification of Vietnamese vowels based on:
//! - docs/vietnamese-language-system.md
//! - https://vi.wikipedia.org/wiki/Quy_tắc_đặt_dấu_thanh_của_chữ_Quốc_ngữ
//!
//! ## Vowel Classification
//!
//! Vietnamese has 12 vowels with 3 modifier types:
//! - Simple: a, e, i, o, u, y
//! - Circumflex (^): â, ê, ô
//! - Horn (móc): ơ, ư
//! - Breve (trăng): ă
//!
//! ## Phonological Roles
//!
//! In Vietnamese syllable structure (C)(G)V(C):
//! - **Medial (âm đệm)**: o, u when followed by main vowel (oa, oe, uy, ua, uê)
//! - **Main (âm chính)**: The primary vowel carrying tone
//! - **Glide (bán nguyên âm)**: i/y, u/o at syllable end (ai, ao, iu, oi)

use super::keys;

/// Vowel modifier type (dấu phụ)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Modifier {
    None = 0,       // a, e, i, o, u, y
    Circumflex = 1, // â, ê, ô (^)
    Horn = 2,       // ơ, ư (móc) / ă (trăng)
}

/// Phonological role in syllable
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Main,   // Primary vowel (carries tone)
    Medial, // Glide before main vowel (o in oa, u in uy)
    Final,  // Glide at syllable end (i in ai, u in au)
}

/// Vowel information
#[derive(Clone, Copy, Debug)]
pub struct Vowel {
    pub key: u16,
    pub modifier: Modifier,
    pub pos: usize,
}

impl Vowel {
    pub fn new(key: u16, modifier: Modifier, pos: usize) -> Self {
        Self { key, modifier, pos }
    }

    /// Check if this vowel has a diacritic modifier (^, ơ, ư, ă)
    pub fn has_diacritic(&self) -> bool {
        self.modifier != Modifier::None
    }
}

/// Vietnamese vowel phonology analyzer
pub struct Phonology;

impl Phonology {
    /// Find the position where tone mark should be placed
    ///
    /// ## Vietnamese Tone Placement Rules
    ///
    /// Based on docs/vietnamese-language-system.md section 7:
    ///
    /// 1. **Single vowel**: Mark directly on it
    ///
    /// 2. **Two vowels with final consonant**: Mark on 2nd vowel
    ///    - toán, hoàn, tiến, muốn, biển
    ///
    /// 3. **Two vowels open syllable**:
    ///    - Medial + Main (oa, oe, uy, ua, uê): Mark on 2nd (main)
    ///    - Main + Glide (ai, ao, au, oi, ui): Mark on 1st (main)
    ///    - Compound (ươ, uô, iê): Mark on 2nd (main has diacritic)
    ///    - ưa pattern: Mark on 1st (ư has diacritic, a is simple)
    ///
    /// 4. **Three+ vowels**: Mark on middle vowel
    ///    - ươi, oai, uôi: Mark on middle
    ///
    /// 5. **Diacritic priority**: When vowel has diacritic (ă, â, ê, ô, ơ, ư),
    ///    it often receives the mark
    pub fn find_tone_position(vowels: &[Vowel], has_final_consonant: bool, modern: bool) -> usize {
        let n = vowels.len();
        if n == 0 {
            return 0;
        }
        if n == 1 {
            return vowels[0].pos;
        }

        // Two vowels
        if n == 2 {
            let v1 = &vowels[0];
            let v2 = &vowels[1];

            // With final consonant: always mark on 2nd vowel
            if has_final_consonant {
                return v2.pos;
            }

            // ưa pattern: ư has diacritic, a doesn't → mark on ư
            // This must be checked BEFORE compound vowels because ưa is not ươ
            // General rule: if 1st has diacritic and 2nd doesn't, mark on 1st
            // Note: when 1st has diacritic, it's no longer a simple medial pair
            // e.g., "ua" (qua) is medial pair, but "ưa" (sứa) is not
            if v1.has_diacritic() && !v2.has_diacritic() {
                return v1.pos;
            }

            // Compound vowels ươ, uô, iê: mark on 2nd (has diacritic)
            if Self::is_compound_vowel(v1.key, v2.key) {
                return v2.pos;
            }

            // 2nd has diacritic → mark on 2nd
            if v2.has_diacritic() {
                return v2.pos;
            }

            // Medial pairs (oa, oe, uy, ua, uê): mark on 2nd (main vowel)
            if Self::is_medial_pair(v1.key, v2.key) {
                return if modern { v2.pos } else { v1.pos };
            }

            // Main + glide (ai, ao, au, oi, ui): mark on 1st (main vowel)
            if Self::is_main_glide_pair(v1.key, v2.key) {
                return v1.pos;
            }

            // Default: mark on 2nd
            return v2.pos;
        }

        // Three+ vowels
        if n == 3 {
            let k0 = vowels[0].key;
            let k1 = vowels[1].key;
            let k2 = vowels[2].key;

            // Priority 1: Middle vowel with diacritic
            // ươi (mười): ư and ơ both have diacritic, mark on ơ (middle)
            if vowels[1].has_diacritic() {
                return vowels[1].pos;
            }

            // Priority 2: Last vowel with diacritic (and middle doesn't)
            // uyê (khuyên): mark on ê (has diacritic)
            if vowels[2].has_diacritic() {
                return vowels[2].pos;
            }

            // Priority 3: ươi, uôi patterns: mark on middle
            if k0 == keys::U && k1 == keys::O {
                return vowels[1].pos;
            }

            // Priority 4: oai, oay patterns: mark on middle (a)
            if k0 == keys::O && k1 == keys::A {
                return vowels[1].pos;
            }

            // Priority 5: uyê pattern (no diacritic on ê yet): mark on ê (last)
            if k0 == keys::U && k1 == keys::Y && k2 == keys::E {
                return vowels[2].pos;
            }
        }

        // For 4+ vowels: find middle vowel with diacritic first
        let mid = n / 2;
        if vowels[mid].has_diacritic() {
            return vowels[mid].pos;
        }

        // Then any vowel with diacritic
        for v in vowels {
            if v.has_diacritic() {
                return v.pos;
            }
        }

        // Default: middle vowel
        vowels[mid].pos
    }

    /// Determine the role of each vowel in a syllable
    pub fn classify_roles(vowels: &[Vowel], has_final_consonant: bool) -> Vec<(usize, Role)> {
        let n = vowels.len();
        if n == 0 {
            return vec![];
        }
        if n == 1 {
            return vec![(vowels[0].pos, Role::Main)];
        }

        let mut roles = vec![Role::Main; n];

        if n == 2 {
            let (k1, k2) = (vowels[0].key, vowels[1].key);

            if Self::is_medial_pair(k1, k2) || Self::is_compound_vowel(k1, k2) || has_final_consonant {
                roles[0] = Role::Medial;
                roles[1] = Role::Main;
            } else if Self::is_main_glide_pair(k1, k2) {
                roles[0] = Role::Main;
                roles[1] = Role::Final;
            }
            // ưa pattern: ư is main
            else if vowels[0].has_diacritic() && !vowels[1].has_diacritic() {
                roles[0] = Role::Main;
                roles[1] = Role::Final;
            }
        } else {
            // Three+ vowels
            roles[0] = Role::Medial;
            if !has_final_consonant {
                roles[n - 1] = Role::Final;
            }

            // Find main vowel
            let main_idx = n / 2;
            roles[main_idx] = Role::Main;
        }

        vowels.iter().zip(roles).map(|(v, r)| (v.pos, r)).collect()
    }

    /// Check if v1+v2 forms a medial+main pair (âm đệm + âm chính)
    ///
    /// Medial pairs: oa, oe, ua, uê, uy
    /// These are when the first vowel acts as a glide before the main vowel
    fn is_medial_pair(v1: u16, v2: u16) -> bool {
        matches!(
            (v1, v2),
            (keys::O, keys::A) | // oa
            (keys::O, keys::E) | // oe
            (keys::U, keys::A) | // ua (qua)
            (keys::U, keys::E) | // uê
            (keys::U, keys::Y)   // uy
        )
    }

    /// Check if v1+v2 forms a main+glide pair (âm chính + bán nguyên âm)
    ///
    /// Final glide pairs: ai, ay, ao, au, oi, ôi, ơi, ui, ưi, êu, iu, etc.
    fn is_main_glide_pair(v1: u16, v2: u16) -> bool {
        // Second vowel must be a glide (i, y, o, u)
        let is_glide = matches!(v2, keys::I | keys::Y | keys::O | keys::U);
        if !is_glide {
            return false;
        }

        // Not a medial or compound pattern
        !Self::is_medial_pair(v1, v2) && !Self::is_compound_vowel(v1, v2)
    }

    /// Check if v1+v2 forms a compound vowel (ươ, uô, iê)
    ///
    /// These are diphthongs where second vowel (with diacritic) carries the tone
    fn is_compound_vowel(v1: u16, v2: u16) -> bool {
        matches!(
            (v1, v2),
            (keys::U, keys::O) | // ươ, uô
            (keys::I, keys::E)   // iê
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(key: u16, modifier: Modifier, pos: usize) -> Vowel {
        Vowel::new(key, modifier, pos)
    }

    #[test]
    fn test_single_vowel() {
        let vowels = vec![v(keys::A, Modifier::None, 0)];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 0);
    }

    #[test]
    fn test_medial_pairs() {
        // oa → mark on a (pos 1)
        let vowels = vec![
            v(keys::O, Modifier::None, 0),
            v(keys::A, Modifier::None, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 1);

        // uy → mark on y (pos 1)
        let vowels = vec![
            v(keys::U, Modifier::None, 0),
            v(keys::Y, Modifier::None, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 1);
    }

    #[test]
    fn test_main_glide_pairs() {
        // ai → mark on a (pos 0)
        let vowels = vec![
            v(keys::A, Modifier::None, 0),
            v(keys::I, Modifier::None, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 0);

        // ao → mark on a (pos 0)
        let vowels = vec![
            v(keys::A, Modifier::None, 0),
            v(keys::O, Modifier::None, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 0);
    }

    #[test]
    fn test_with_final_consonant() {
        // oan → mark on a (pos 1)
        let vowels = vec![
            v(keys::O, Modifier::None, 0),
            v(keys::A, Modifier::None, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, true, true), 1);
    }

    #[test]
    fn test_compound_vowels() {
        // ươ → mark on ơ (pos 1)
        let vowels = vec![
            v(keys::U, Modifier::Horn, 0),
            v(keys::O, Modifier::Horn, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 1);

        // iê → mark on ê (pos 1)
        let vowels = vec![
            v(keys::I, Modifier::None, 0),
            v(keys::E, Modifier::Circumflex, 1),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 1);
    }

    #[test]
    fn test_three_vowels() {
        // ươi → mark on ơ (pos 1, middle with diacritic)
        let vowels = vec![
            v(keys::U, Modifier::Horn, 0),
            v(keys::O, Modifier::Horn, 1),
            v(keys::I, Modifier::None, 2),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 1);

        // oai → mark on a (pos 1, middle)
        let vowels = vec![
            v(keys::O, Modifier::None, 0),
            v(keys::A, Modifier::None, 1),
            v(keys::I, Modifier::None, 2),
        ];
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 1);
    }

    #[test]
    fn test_diacritic_priority() {
        // ưa → mark on ư (pos 0, has diacritic)
        let vowels = vec![
            v(keys::U, Modifier::Horn, 0),
            v(keys::A, Modifier::None, 1),
        ];
        // ưa is NOT a compound vowel (compound is ươ, not ưa)
        // ư has diacritic, a doesn't → mark on ư
        assert_eq!(Phonology::find_tone_position(&vowels, false, true), 0);
    }
}
