//! English auto-restore logic for the Vietnamese input engine (moved out
//! of mod.rs). Detects when the current buffer's Vietnamese transform is
//! actually an English word typed through, and restores the raw ASCII.

use super::validation::{self, is_valid_with_tones_and_foreign};
use super::{syllable, Buffer, Char, Engine, Result};
use crate::data::{
    chars::{self, tone},
    constants, dictionary, english_dict, keys, telex_doubles,
};
use crate::utils;

impl Engine {
    /// Debug: Check if raw_input is valid English
    pub fn is_raw_english(&self) -> bool {
        self.is_raw_input_valid_english()
    }

    /// Restore buffer from a Vietnamese word string
    ///
    /// Used when native app detects cursor at word boundary and wants to edit.
    /// Parses Vietnamese characters back to buffer components.
    pub fn restore_word(&mut self, word: &str) {
        self.clear();
        let mut is_ascii = true;
        for c in word.chars() {
            if let Some(parsed) = chars::parse_char(c) {
                let mut ch = Char::new(parsed.key, parsed.caps);
                ch.tone = parsed.tone;
                ch.mark = parsed.mark;
                ch.stroke = parsed.stroke;
                self.buf.push(ch);
                self.raw_input.push((parsed.key, parsed.caps, false));
                // Check if this char has any Vietnamese diacritics
                if parsed.tone != 0 || parsed.mark != 0 || parsed.stroke {
                    is_ascii = false;
                }
            }
        }
        // Mark that buffer was restored from screen - if user types a regular consonant,
        // clear buffer first (they want fresh word, not append to restored word)
        // This allows: click on "shortcuts" → type "Nuw" → get "Nư" (not "shortcutsNuw")
        // But mark/tone keys like 's' will still work to modify the restored word
        if !self.buf.is_empty() {
            self.restored_pending_clear = true;
            self.restored_is_ascii = is_ascii;
            // Rebuild last_transform so a repeated modifier key toggles the
            // diacritic off, matching pre-commit editing behavior.
            self.re_detect_last_transform();
        }
    }

    /// Check if buffer has transforms and is invalid Vietnamese
    /// Returns the raw chars if restore is needed, None otherwise
    ///
    /// `is_word_complete`: true when called on space/break (word is complete)
    ///                     false when called mid-word (during typing)
    fn should_auto_restore(&self, is_word_complete: bool) -> Option<Vec<char>> {
        // Only run auto-restore if the feature is enabled
        if !self.english_auto_restore {
            return None;
        }

        if self.raw_input.is_empty() || self.buf.is_empty() {
            return None;
        }

        // If no Vietnamese transforms were ever applied this word, nothing to restore
        // This prevents false restore for words with numbers/symbols like "nhatkha1407@gmail.com"
        // where the buffer is invalid Vietnamese but no transforms were ever attempted
        // Also handles words with invalid initials like "forr" - since 'f' is not valid,
        // no mark was ever applied, so the result stays "forr" (not collapsed to "for")
        if !self.had_any_transform {
            return None;
        }

        // Issue #211: Skip auto-restore for extended character patterns
        // When user types "ơiiiiii", "điiii", "ôiiii", "vàooooo", etc.
        // This is intentional Vietnamese (casual messaging) not English.
        //
        // Check if buffer has ANY Vietnamese-specific modifier:
        // - HORN: ơ, ư, ă (from 'w' key)
        // - CIRCUMFLEX: ô, â, ê (from double vowel)
        // - MARK: sắc, huyền, hỏi, ngã, nặng (from s/f/r/x/j keys)
        // - STROKE: đ (from 'dd')
        // AND has CONSECUTIVE extended characters (3+ same key in a row)
        //
        // NOTE: Must be CONSECUTIVE - "WebSocket" has 'e' twice but not consecutive
        // NOTE: reverted_circumflex_key handles Issue #211 case (áaa pattern after revert)
        //
        // Examples:
        //   "cuwuuuuus" → "cứuuuuu" → horn + consecutive u's → keep Vietnamese
        //   "owiiiiii" → "ơiiiiii" → horn + consecutive i's → keep Vietnamese
        //   "ooiiii" → "ôiiii" → circumflex + consecutive i's → keep Vietnamese
        //   "ddiiii" → "điiii" → stroke + consecutive i's → keep Vietnamese
        //   "vafooooo" → "vàooooo" → mark (huyền) + consecutive o's → keep Vietnamese
        let has_vn_specific_modifier = self.buf.iter().any(|c| {
            c.tone == tone::HORN      // ơ, ư, ă
                || c.tone == tone::CIRCUMFLEX // ô, â, ê
                || c.mark > 0         // sắc, huyền, hỏi, ngã, nặng
                || c.key == keys::D && self.buf.iter().filter(|x| x.key == keys::D).count() == 0
                    && self.had_telex_transform // đ from dd
        });

        // Also check for stroke (đ) - when D in buffer but had_telex_transform and raw had 'dd'
        let has_stroke = self
            .raw_input
            .windows(2)
            .any(|pair| pair[0].0 == keys::D && pair[1].0 == keys::D)
            && self.buf.iter().any(|c| c.key == keys::D);

        let has_vn_modifier = has_vn_specific_modifier || has_stroke;

        // Check for consecutive extended characters: 3+ of the same key in a row
        // Can be vowels (iiiii, uuuuu) or consonants (ggggg for emphasis)
        // Require 3+ to distinguish from English words like "sweet" (ee), "flood" (oo)
        let buf_chars: Vec<_> = self.buf.iter().collect();
        let has_consecutive_extended = buf_chars
            .windows(3)
            .any(|triple| triple[0].key == triple[1].key && triple[1].key == triple[2].key);

        // Also check original Issue #211 pattern: reverted_circumflex + all vowels same key
        let all_vowels_same = if self.reverted_circumflex_key.is_some() {
            let vowels: Vec<u16> = self
                .buf
                .iter()
                .filter(|c| keys::is_vowel(c.key))
                .map(|c| c.key)
                .collect();
            vowels.len() >= 2 && vowels.iter().all(|&k| k == vowels[0])
        } else {
            false
        };

        if (has_vn_modifier && has_consecutive_extended) || all_vowels_same {
            // Extended character pattern - skip auto-restore
            return None;
        }

        // VIETNAMESE PRIORITY: Only keep Vietnamese when buffer has Vietnamese-SPECIFIC marks
        // Vietnamese-specific: circumflex (ô,â,ê), horn (ơ,ư), breve (ă), stroke (đ)
        // These marks indicate intentional Vietnamese typing
        // Plain tone marks (sắc/huyền/hỏi/ngã/nặng) alone are NOT enough - could be English typo
        // EXCEPTIONS that skip Vietnamese priority:
        //   - W patterns: produce HORN but clearly English (law, saw, west)
        //   - Telex doubles (oo, ee, aa, dd): let whitelist logic handle them (poor, bees, add)
        // Examples:
        //   "boos" → "bố" (oo = circumflex, in whitelist) → let whitelist handle
        //   "bore" → "boẻ" (only hỏi tone, no VN-specific mark) → check other logic
        //   "law" → "lă" (W produces breve/HORN) → skip priority, let W restore handle it
        let has_w_in_raw = self.raw_input.iter().any(|(key, _, _)| *key == keys::W);
        // Check for telex double patterns:
        // 1. Consecutive same vowels (oo, ee, aa) or dd
        // 2. VCV patterns with same vowel (oto→ôt, ata→ât, ete→êt) - delayed circumflex
        let has_telex_double = self.raw_input.windows(2).any(|pair| {
            let (k1, _, _) = pair[0];
            let (k2, _, _) = pair[1];
            k1 == k2 && (k1 == keys::O || k1 == keys::E || k1 == keys::A || k1 == keys::D)
        }) || self.raw_input.windows(3).any(|triple| {
            let (k1, _, _) = triple[0];
            let (k2, _, _) = triple[1];
            let (k3, _, _) = triple[2];
            // VCV pattern: same vowel with consonant in between (delayed circumflex)
            k1 == k3 && keys::is_vowel(k1) && !keys::is_vowel(k2)
        });
        let has_vn_specific_mark = self.buf.iter().any(|c| {
            c.tone == tone::CIRCUMFLEX  // ô, â, ê
                || c.tone == tone::HORN // ơ, ư, ă (breve uses HORN value)
                || c.stroke // đ
        });
        // Only apply Vietnamese priority if NO W pattern AND NO telex double
        if has_vn_specific_mark
            && !has_w_in_raw
            && !has_telex_double
            && !self.is_buffer_invalid_vietnamese()
        {
            return None;
        }

        // TELEX DOUBLES WHITELIST CHECK
        // Check whitelist for words with telex patterns (s/f/r/x/j tones, aa/ee/oo marks, dd stroke)
        if self.had_telex_transform {
            // Build raw string for whitelist lookup
            let raw_str = if let Some(ref stored) = self.telex_double_raw {
                // Double revert pattern occurred (xx, ss, dd, etc.)
                // Build full raw string including subsequent chars typed after revert
                let subsequent_start = if self.raw_input.len() < self.telex_double_raw_len {
                    self.telex_double_raw_len.saturating_sub(1)
                } else {
                    self.telex_double_raw_len
                };
                let subsequent: String = self
                    .raw_input
                    .iter()
                    .skip(subsequent_start)
                    .filter_map(|&(key, caps, shift)| utils::key_to_char_ext(key, caps, shift))
                    .collect();
                format!("{}{}", stored.to_lowercase(), subsequent.to_lowercase())
            } else {
                self.get_raw_input_string()
            };

            if telex_doubles::contains(&raw_str) {
                // Word is in English telex doubles whitelist
                // Decision logic with Vietnamese-first principle:
                //
                // 1. Double vowel patterns (oo, ee, aa) + in English dict → RESTORE to English
                //    These are distinctive English patterns (poor, beer, teen)
                // 2. Stroke (đ) + NOT in English dict → keep Vietnamese (abbreviation)
                // 3. Buffer is VALID Vietnamese + NOT double vowel → keep Vietnamese
                // 4. Buffer is INVALID Vietnamese → restore to English
                //
                // Examples:
                //   "poor" → has "oo" pattern + in dict → restore "poor"
                //   "bits" → no double vowel, buffer "bít" valid VN → keep "bít"
                //   "chir" → no double vowel, buffer "chỉ" valid VN → keep "chỉ"
                //   "daddy" → buffer "đady" invalid VN, in dict → restore "daddy"
                //   "ddc" → has stroke, not in dict → keep "đc"

                let has_stroke = self.buf.iter().any(|c| c.stroke);
                let buffer_invalid_vn = self.is_buffer_invalid_vietnamese();
                let raw_in_english_dict = english_dict::is_english_word(&raw_str);

                // W at end pattern: foreign words like moscow, warsaw, saw, law
                let w_at_end = self
                    .raw_input
                    .last()
                    .map(|(k, _, _)| *k == keys::W)
                    .unwrap_or(false);

                // Simple logic: buffer invalid VN + raw in English dict → restore
                // Special case: W at end + in dict → restore (foreign word pattern)
                // Issue #247: Standalone "đ" (buffer len 1 with stroke) should NOT restore
                // This is intentional Vietnamese typing, not English "dd"
                let is_standalone_stroke = self.buf.len() == 1 && has_stroke;
                if has_stroke && (!raw_in_english_dict || is_standalone_stroke) {
                    // Skip restore - Vietnamese abbreviation like đc, đt, or standalone đ
                } else if w_at_end && raw_in_english_dict {
                    // W at end + in dict → restore foreign words (moscow, warsaw, saw)
                    return self.build_raw_chars_exact();
                } else if buffer_invalid_vn && raw_in_english_dict {
                    // Check if collapsed buffer is also a valid English word or in keep list
                    // If buffer is a known English word, keep it (e.g., "lissa" → "lisa")
                    // If buffer is in keep list, keep it (e.g., "sess" → "ses")
                    // If buffer is NOT a known word, restore original (e.g., "larissa" → "larissa")
                    let buffer_str = self.get_buffer_string().to_lowercase();
                    if !english_dict::is_english_word(&buffer_str)
                        && !dictionary::should_keep(&buffer_str)
                    {
                        // Buffer not in dict and not in keep list → restore to original English
                        return self.build_raw_chars_exact();
                    }
                    // Buffer IS in dict or keep list → keep buffer
                }
                // Otherwise keep buffer (valid VN or not in dict)
            }

            // DOUBLE SS/FF DICTIONARY CHECK (before "keep clean buffer" logic)
            // Words ending with ss/ff that are in dictionary should restore to English.
            // Examples: mass, bass, pass, buff, cuff → restore to English
            // This check must come BEFORE "keep clean buffer" logic because buffer "mas"
            // looks clean but should restore to "mass" if "mass" is in dictionary.
            // IMPORTANT: Only apply when ss/ff is at END of complete word (no subsequent chars)
            // For "masson" (ss in middle), let normal collapse logic handle it → "mason"
            if let Some(ref stored) = self.telex_double_raw {
                // Check if any chars were typed AFTER the double pattern
                let has_subsequent_chars = self.raw_input.len() > self.telex_double_raw_len;

                // Only apply this check when ss/ff is at the END of the word
                if !has_subsequent_chars {
                    let chars: Vec<char> = stored.chars().collect();
                    if chars.len() >= 2 {
                        let last = chars[chars.len() - 1].to_ascii_lowercase();
                        let second_last = chars[chars.len() - 2].to_ascii_lowercase();
                        let is_double_ss = last == 's' && second_last == 's';
                        let is_double_ff = last == 'f' && second_last == 'f';

                        if is_double_ss || is_double_ff {
                            let original_lower = stored.to_lowercase();
                            if english_dict::is_english_word(&original_lower) {
                                // Check if buffer should be kept (in keep list, valid Vietnamese, or valid English word)
                                // e.g., "buss" → buffer "bus" is valid English → keep "bus"
                                // e.g., "mass" → buffer "mas" is NOT valid English → restore "mass"
                                let buffer_str = self.get_buffer_string().to_lowercase();
                                if dictionary::should_keep(&buffer_str)
                                    || english_dict::is_english_word(&buffer_str)
                                {
                                    // Buffer is in keep list or is a valid English word → don't restore
                                } else {
                                    return self.build_raw_chars_exact();
                                }
                            }
                        }
                    }
                }
            }

            // Word NOT in whitelist:
            // If double revert pattern occurred AND buffer has NO Vietnamese marks,
            // AND buffer doesn't have repeated consonants (indicating incomplete collapse),
            // keep buffer (user intentionally typed double to get clean result).
            // Example: "taxxi" → buffer "taxi" (no marks, no repeats) → keep "taxi"
            // Example: "reff" → buffer "ref" (no marks, no repeats) → keep "ref"
            // But: "assssess" → buffer "asssess" (has repeated 's') → continue to collapse
            // But: "prooff" → buffer "prôf" (has mark ô) → continue to other logic
            if let Some(ref stored) = self.telex_double_raw {
                let has_marks = self.buf.iter().any(|c| c.tone > 0 || c.mark > 0);
                let has_stroke = self.buf.iter().any(|c| c.stroke);
                let buffer_str = self.get_buffer_string();
                // Check for repeated consonants (ss, ff, rr, etc.) in buffer
                let has_repeated_consonant = buffer_str
                    .as_bytes()
                    .windows(2)
                    .any(|w| w[0] == w[1] && matches!(w[0], b's' | b'f' | b'r' | b'x' | b'j'));
                // Check if full restored input is longer than buffer
                // This happens when modifiers were consumed (e.g., "nurses" → "nues")
                // Calculate full restore length: stored + subsequent chars after telex_double_raw_len
                // "taxxi" → stored="taxx"(4) + subsequent="i"(1) = 5, buf="taxi"(4) → 5 > 4? Yes but ok (1 diff)
                // "nurses" → stored="nurses"(6) + ""(0) = 6, buf="nues"(4) → 6 > 4? Yes (2 diff) → restore
                // "nursest" → stored="nurses"(6) + "t"(1) = 7, buf="nuest"(5) → 7 > 5? Yes (2 diff) → restore
                let subsequent_len = self
                    .raw_input
                    .len()
                    .saturating_sub(self.telex_double_raw_len);
                let full_restore_len = stored.len() + subsequent_len;
                // If restored is more than 1 char longer than buffer, modifiers were consumed → restore
                let raw_much_longer = full_restore_len > self.buf.len() + 1;
                if !has_marks && !has_stroke && !has_repeated_consonant && !raw_much_longer {
                    return None; // Keep buffer (clean, no Vietnamese transforms)
                }

                // Issue #230: Alternating pattern fix (herere → here, therere → there)
                // Detect alternating vowel-mark-vowel-mark pattern at end of raw_input.
                // Pattern like `h-e-r-e-r-e` (V-M-V-M-V at end) indicates English typing where
                // marks got applied then reverted, and user continued with vowel.
                //
                // NOTE: We check self.raw_input (full input like "Therere"), not stored
                // (telex_double_raw = "There" at time of revert). The pattern appears in full input.
                //
                // IMPORTANT: Only apply this fix when:
                // 1. raw input is NOT a valid English word (avoid breaking "theses" etc.)
                // 2. raw input matches alternating V-M-V-M or V-M-V-M-V pattern with SAME vowels
                //    - "herere": e-r-e-r-e (V-M-V-M-V) → keep "here"
                //    - "herer": e-r-e-r (V-M-V-M) → keep "her"
                //    - "harare": a-r-a-r-e (different vowels a≠e) → skip fix
                let raw_input_str = self.get_raw_input_string();
                let raw_is_english = english_dict::is_english_word(&raw_input_str);
                let chars: Vec<char> = raw_input_str.chars().collect();

                if !raw_is_english && chars.len() >= 4 {
                    let len = chars.len();
                    let is_mark_char = |c: char| matches!(c, 's' | 'f' | 'r' | 'x' | 'j');
                    let is_vowel_char = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
                    let last = chars[len - 1].to_ascii_lowercase();

                    // Check for two alternating patterns:
                    // 1. V-M-V-M-V (ends with vowel): "herere" = e-r-e-r-e → keep "here"
                    // 2. V-M-V-M (ends with mark): "herer" = e-r-e-r → keep "her"

                    let is_vmvmv_pattern = len >= 5
                        && is_vowel_char(last) // ends with vowel
                        && {
                            let v1 = last;
                            let m1 = chars[len - 2].to_ascii_lowercase();
                            let v2 = chars[len - 3].to_ascii_lowercase();
                            let m2 = chars[len - 4].to_ascii_lowercase();
                            let v3 = chars[len - 5].to_ascii_lowercase();

                            is_mark_char(m1)
                                && is_vowel_char(v2)
                                && is_mark_char(m2)
                                && is_vowel_char(v3)
                                && m1 == m2           // Same mark repeated
                                && v1 == v2 && v2 == v3 // Same vowel repeated
                        };

                    let is_vmvm_pattern = is_mark_char(last) // ends with mark
                        && {
                            let m1 = last;
                            let v1 = chars[len - 2].to_ascii_lowercase();
                            let m2 = chars[len - 3].to_ascii_lowercase();
                            let v2 = chars[len - 4].to_ascii_lowercase();

                            is_vowel_char(v1)
                                && is_mark_char(m2)
                                && is_vowel_char(v2)
                                && m1 == m2       // Same mark repeated
                                && v1 == v2       // Same vowel repeated
                        };

                    // For alternating pattern, only check for actual diacritic marks (sắc/huyền/hỏi/ngã/nặng),
                    // not vowel modifiers (circumflex/horn/breve from doubled vowels like ee→ê).
                    let has_diacritic_marks = self.buf.iter().any(|c| c.mark > 0);

                    if (is_vmvmv_pattern || is_vmvm_pattern) && !has_diacritic_marks && !has_stroke
                    {
                        return None; // Keep buffer (alternating pattern reverted cleanly)
                    }
                }
            }
        }

        // No telex double pattern → continue with existing logic for other cases

        // Check if any transforms remain in buffer
        // - Marks (sắc, huyền, hỏi, ngã, nặng): indicate Vietnamese typing intent
        // - Vowel tones (â, ê, ô, ư, ă): indicate Vietnamese typing intent
        // - Stroke (đ): included for longer words that are structurally invalid
        let has_marks_or_tones = self.buf.iter().any(|c| c.tone > 0 || c.mark > 0);
        let has_stroke = self.buf.iter().any(|c| c.stroke);

        // If no transforms remain in buffer AND user reverted at END of word,
        // keep the result (user intentionally reverted)
        // Examples: "ass" → "as", "maxx" → "max" (double modifier at end)
        // But "issue" → "isue" should still check validity (more letters typed after revert)
        // EXCEPTION: If buffer is INVALID Vietnamese (like "cofee" with F), still restore
        // This handles cases like "coffee" where 'ee' is part of English word, not Telex revert
        if !has_marks_or_tones && !has_stroke && self.ends_with_double_modifier() {
            // Only skip restore if buffer is actually valid Vietnamese
            // Invalid buffers (containing F, W at wrong positions, etc.) should still restore
            let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
            let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
            if validation::is_valid_with_tones(&buffer_keys, &buffer_tones) {
                return None;
            }
        }

        // Issue #367: Keep buffer when circumflex was reverted and no marks remain.
        // When user types "totoo" (t-o-t-o-o), circumflex fires on 4th 'o' (tôt),
        // then 5th 'o' reverts it → buffer="toto" (clean, no marks). The user explicitly
        // typed double vowel to revert, so keep buffer content.
        // Without this, auto-restore would output raw "totoo" (extra vowel from revert).
        // Applies to: TOTO, MAMA, TETE, SATA, PAPA, etc.
        if self.had_circumflex_revert && !has_marks_or_tones && !has_stroke {
            return None;
        }

        // UNIFIED LOGIC: Restore ONLY when BOTH conditions are met:
        // 1. buffer != valid Vietnamese (is_buffer_invalid_vietnamese)
        // 2. raw_input == valid English (is_raw_input_valid_english)
        //
        // This replaces the previous multi-check pattern-based approach.
        // Benefits:
        // - Simpler, more predictable logic
        // - Fewer false positives for valid Vietnamese words
        // - Works correctly with "sims", "homo", and other edge cases

        // First check: Is buffer invalid Vietnamese?
        let buffer_invalid_vn = self.is_buffer_invalid_vietnamese();

        // For stroke-only transforms (no marks/tones), only restore if word is long enough
        // Short words like "đd" from "ddd" should stay; long invalid words like "đealine" should restore
        if buffer_invalid_vn && has_stroke && !has_marks_or_tones && self.buf.len() < 4 {
            return None;
        }

        // If user typed double TONE modifier (rr) at END of SHORT word, keep reverted form
        if self.had_mark_revert && self.raw_input.len() >= 2 && self.raw_input.len() <= 4 {
            let (last_key, _, _) = self.raw_input[self.raw_input.len() - 1];
            let (second_last_key, _, _) = self.raw_input[self.raw_input.len() - 2];
            // Double 'rr' at end of short word → keep reverted form
            if last_key == second_last_key && last_key == keys::R {
                return None;
            }
        }

        // Second check: Is raw_input valid English?
        let raw_input_valid_en = self.is_raw_input_valid_english();

        // NOTE: Double ss/ff dictionary check is handled earlier (lines 3388-3421)
        // in the telex_doubles section, BEFORE the "keep clean buffer" logic.

        // SPECIAL CASE: Doubled modifier pattern handling
        // Distinguish between:
        // - V + doubled_modifier (issue, offer) → restore to raw (common English)
        // - C + V + doubled_modifier (carre) → keep buffer (Telex revert pattern)
        if self.had_mark_revert && buffer_invalid_vn && raw_input_valid_en {
            let tone_mods = [keys::S, keys::F, keys::R, keys::X, keys::J];

            // Find position of doubled modifier in raw_input
            let mut doubled_pos = None;
            for i in 0..self.raw_input.len().saturating_sub(1) {
                let (k1, _, _) = self.raw_input[i];
                let (k2, _, _) = self.raw_input[i + 1];
                if tone_mods.contains(&k1) && k1 == k2 {
                    doubled_pos = Some(i);
                    break;
                }
            }

            if let Some(pos) = doubled_pos {
                // Check if doubled modifier is at END of word (like "bass", "varr")
                let is_at_end = pos + 2 >= self.raw_input.len();

                // Check if doubled modifier is RIGHT AFTER initial vowel (like i-ss, o-ff)
                // Pattern: V + doubled_modifier (position 1)
                let is_after_initial_vowel = pos == 1 && {
                    let (first_key, _, _) = self.raw_input[0];
                    keys::is_vowel(first_key)
                };

                // Check how many chars follow the doubled modifier
                // - "carre": rr + 1 char (e) → likely Telex pattern
                // - "mirror": rr + 2 chars (or) → likely English word
                // - "sorry": rr + 1 char (y) → English word (not Telex)
                let chars_after = self.raw_input.len() - pos - 2;

                // Only consider Telex pattern if:
                // 1. Exactly 1 char after doubled modifier
                // 2. That char is 'e' (common Telex ending: carre→care, barre→bare)
                let ends_with_e = self
                    .raw_input
                    .last()
                    .map(|(k, _, _)| *k == keys::E)
                    .unwrap_or(false);
                let is_telex_pattern = chars_after == 1 && ends_with_e;

                // Check if 'w' at start was converted to 'ư' (Telex w-vowel)
                // Words like "worry" start with 'w' in raw but 'ư' in buffer
                let w_converted_to_horn = !self.raw_input.is_empty() && {
                    let (first_key, _, _) = self.raw_input[0];
                    first_key == keys::W && self.buf.get(0).map(|c| c.key) != Some(keys::W)
                };

                if !is_at_end && !is_after_initial_vowel && is_telex_pattern && !w_converted_to_horn
                {
                    // Pattern like "carre" (C + V + rr + single_char) → keep buffer "care"
                    // Buffer already has the collapsed result from Telex revert
                    // But NOT if raw is English word and buffer is not (like "giraffe" → "giafe")
                    return None;
                }
            }
        }

        // CIRCUMFLEX FROM DOUBLE VOWEL CHECK: Preserve circumflex from intentional double vowel
        // "ook" → "ôk", "eecu" → "êcu" - user typed double vowel to get circumflex
        // Skip restore when:
        // 1. Buffer has circumflex (ô, â, ê) that was NOT reverted
        // 2. Buffer does NOT have any mark (sắc, huyền, hỏi, ngã, nặng)
        // 3. Raw input has corresponding double vowel pattern (oo, aa, ee)
        // This preserves intentional circumflex typing
        let has_circumflex_in_buffer = self.buf.iter().any(|c| c.tone == tone::CIRCUMFLEX);
        let has_mark_in_buffer = self.buf.iter().any(|c| c.mark > 0);
        let has_raw_double_vowel = self.raw_input.windows(2).any(|pair| {
            let (k1, _, _) = pair[0];
            let (k2, _, _) = pair[1];
            k1 == k2 && matches!(k1, keys::O | keys::A | keys::E)
        });
        if has_circumflex_in_buffer
            && !has_mark_in_buffer
            && has_raw_double_vowel
            && !self.had_circumflex_revert
        {
            // Keep buffer - circumflex from intentional double vowel input
            return None;
        }

        // UNIFIED: Restore only when buffer is invalid Vietnamese AND raw_input is valid English
        if buffer_invalid_vn && raw_input_valid_en {
            return self.build_raw_chars();
        }

        // OW PATTERN CHECK: raw_input has 'ow' but buffer has ơ/ở/ờ/ớ/ỡ/ợ (horn-o)
        // English words: power, tower, down, town, etc.
        // Telex 'w' converts 'o' to 'ơ' (horn mark), which is wrong for English
        // Example: "power" → buffer "pởe" (wrong), should restore to "power"
        // BUT: "bow" → buffer "bơ" (valid Vietnamese), keep it
        // Only restore when buffer is INVALID Vietnamese
        if is_word_complete && buffer_invalid_vn && raw_input_valid_en {
            let has_ow_in_raw = self.raw_input.windows(2).any(|w| {
                let (k1, _, _) = w[0];
                let (k2, _, _) = w[1];
                k1 == keys::O && k2 == keys::W
            });
            let has_horn_o_in_buffer = self
                .buf
                .iter()
                .any(|c| c.key == keys::O && c.tone == tone::HORN);
            if has_ow_in_raw && has_horn_o_in_buffer {
                return self.build_raw_chars();
            }
        }

        // W-START CHECK: If raw input starts with 'w', restore in specific cases
        // Vietnamese doesn't have 'w', so words starting with 'w' are likely English
        if is_word_complete && !self.raw_input.is_empty() {
            let (first_key, _, _) = self.raw_input[0];
            if first_key == keys::W {
                // Case 1: English consonant cluster at start (wr, wh) - ALWAYS restore
                // These are English-only clusters that don't exist in Vietnamese
                // Examples: wra, wri, wro (wr-), whi, who (wh-)
                // But NOT: wng, wn, wm (these are w→ư + final consonant = valid Vietnamese)
                if self.raw_input.len() >= 2 {
                    let (second_key, _, _) = self.raw_input[1];
                    // Only restore for English consonant clusters: wr, wh
                    // (r and h after w form English onset clusters)
                    if second_key == keys::R || second_key == keys::H {
                        return self.build_raw_chars();
                    }
                }
                // Case 2: W+vowel with invalid VN buffer - restore
                // Examples: "wmd", "wtf" with invalid structure
                if buffer_invalid_vn {
                    return self.build_raw_chars();
                }
            }
        }

        // Additional check: English patterns in raw_input even when buffer appears valid
        // This catches patterns like "text", "their", "law", "saw", etc.
        // EXCEPTION: If buffer has stroke (đ), this is intentional Vietnamese
        // Example: "derde" → "để" has stroke, keep it (valid VN word)
        // Example: "law" → "lă" has no stroke, restore to "law" (English)
        // EXCEPTION: If buffer is VALID Vietnamese with VN-specific marks (breve/horn/circumflex),
        // AND W is NOT at final position (has consonants after W),
        // AND W comes AFTER a vowel (medial position, not initial),
        // skip restore. This handles patterns like "banwfg" → "bằng" where W is a vowel modifier.
        // The pattern "a + consonants + w + finals" produces breve on 'a' (ă), which is valid Vietnamese.
        // But "law", "saw", "raw" have W at end - these should restore to English.
        // And "west", "water" have W at start - these should restore to English.
        if is_word_complete && self.has_english_modifier_pattern(true) && raw_input_valid_en {
            // Skip restore if buffer has stroke - user intentionally typed Vietnamese đ
            if !has_stroke {
                // Skip restore if buffer is VALID Vietnamese with VN-specific marks
                // AND W is not at final position (has consonants after W)
                // AND W comes after a vowel (not at initial position)
                // This handles "banwfg" → "bằng" but NOT "law" or "west"
                let buffer_valid_vn = !self.is_buffer_invalid_vietnamese();

                // Check if W is at end (final position) - English pattern like "law", "saw"
                let w_at_end = self
                    .raw_input
                    .last()
                    .map(|(k, _, _)| *k == keys::W)
                    .unwrap_or(false);

                // Find W position and check context
                let w_pos = self.raw_input.iter().rposition(|(k, _, _)| *k == keys::W);

                // Check if there are consonants after the last W in raw_input
                // Pattern: "banwfg" has W at pos 3, then "fg" (consonants) - Vietnamese pattern
                // Pattern: "law" has W at end - English pattern
                let has_consonants_after_w = w_pos.is_some_and(|pos| {
                    self.raw_input[pos + 1..].iter().any(|(k, _, _)| {
                        keys::is_consonant(*k)
                            && !matches!(*k, keys::S | keys::F | keys::R | keys::X | keys::J)
                    })
                });

                // Check if W comes after a vowel (medial position, not initial)
                // Pattern: "banwfg" has vowel 'a' before W (pos 1) → medial W → Vietnamese
                // Pattern: "west" has W at pos 0 (initial) → no vowel before → English
                let has_vowel_before_w = w_pos.is_some_and(|pos| {
                    self.raw_input[..pos]
                        .iter()
                        .any(|(k, _, _)| keys::is_vowel(*k))
                });

                if buffer_valid_vn
                    && has_vn_specific_mark
                    && !w_at_end
                    && has_consonants_after_w
                    && has_vowel_before_w
                {
                    // Valid Vietnamese with VN marks, W not at end, consonants after W, vowel before W
                    // Examples: "banwfg" → "bằng", "thanwfg" → "thằng"
                } else {
                    return self.build_raw_chars();
                }
            }
        }

        // Check 3: Significant character consumption with circumflex
        // If raw_input is 2+ chars longer than buffer AND buffer has circumflex without mark,
        // this suggests transforms consumed chars that shouldn't have been consumed.
        // Example: "await" (5 chars) → "âit" (3 chars) - diff of 2
        // - "aw" triggers breve on 'a'
        // - second 'a' triggers circumflex (double-vowel), consuming 'w' and second 'a'
        // - Result: buffer is valid but user typed English word
        // EXCEPTION: If buffer has stroke (đ), it's intentional Vietnamese
        if is_word_complete
            && self.raw_input.len() >= self.buf.len() + 2
            && !has_stroke
            && raw_input_valid_en
        {
            let has_circumflex = self.buf.iter().any(|c| c.tone == tone::CIRCUMFLEX);
            let has_marks = self.buf.iter().any(|c| c.mark > 0);
            if has_circumflex && !has_marks {
                return self.build_raw_chars();
            }
        }

        // Check 4: V+C+V circumflex with stop consonant final
        // Pattern: "data" → "dât", "tata" → "tât", "papa" → "pâp"
        // V+C+V triggers circumflex, consuming 1 char (raw_input.len = buf.len + 1)
        // If buffer ends with circumflex + stop consonant (t/c/p) without mark,
        // these are rarely valid Vietnamese words → restore to English
        // Compare: "hôm" (circumflex + m) and "sân" (circumflex + n) are valid Vietnamese
        // NOTE: Use `had_vowel_triggered_circumflex` flag for accurate detection
        if is_word_complete
            && self.had_vowel_triggered_circumflex
            && !has_stroke
            && raw_input_valid_en
        {
            let has_marks = self.buf.iter().any(|c| c.mark > 0);
            if !has_marks {
                let buf_str = self.buf.to_full_string().to_lowercase();
                // Stop consonants after circumflex without mark → likely English
                // Examples: dât, tât, pât, sêt, bôc, etc.
                if buf_str.ends_with("ât")
                    || buf_str.ends_with("êt")
                    || buf_str.ends_with("ôt")
                    || buf_str.ends_with("âc")
                    || buf_str.ends_with("êc")
                    || buf_str.ends_with("ôc")
                    || buf_str.ends_with("âp")
                    || buf_str.ends_with("êp")
                    || buf_str.ends_with("ôp")
                {
                    return self.build_raw_chars();
                }
            }
        }

        // Check 5: Same modifier doubled + vowel = Telex revert pattern
        // When user typed double modifier to revert unwanted Vietnamese transform,
        // the resulting buffer might be valid VN but user intended English.
        // Example: "arro" → user wanted "aro", typed 'rr' to cancel hỏi
        // Only apply to short buffers (<=3 chars) to avoid false positives on words
        // like "issue" (buffer "isue" = 4 chars) or "worry" (buffer "wory" = 4 chars)
        // For no-initial patterns: V + modifier + modifier + V → buf = 3 chars
        if is_word_complete
            && self.had_mark_revert
            && self.buf.len() <= 3
            && raw_input_valid_en
            && !has_stroke
        {
            let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];
            let has_same_modifier_doubled_vowel =
                (0..self.raw_input.len().saturating_sub(2)).any(|i| {
                    let (key, _, _) = self.raw_input[i];
                    let (next_key, _, _) = self.raw_input[i + 1];
                    let (after_key, _, _) = self.raw_input[i + 2];
                    tone_modifiers.contains(&key)
                        && key == next_key // Same modifier doubled (rr, ss, ff)
                        && keys::is_vowel(after_key)
                });
            if has_same_modifier_doubled_vowel {
                return self.build_raw_chars();
            }
        }

        // Check 6: V1-V2-V1 vowel pattern that collapsed via circumflex
        // Pattern: raw input has 3+ consecutive vowels ending with same vowel that started
        // Example: "queue" raw=[q,u,e,u,e] → consecutive vowels "eue" → buffer "quêu"
        // The third vowel triggers circumflex on first vowel and gets consumed
        // EXCEPTION: If buffer has stroke (đ), it's intentional Vietnamese
        // EXCEPTION: If buffer has valid Vietnamese triphthong (iêu, yêu, uôi, etc.)
        if is_word_complete && !has_stroke && raw_input_valid_en {
            // Extract consecutive vowel sequence from end of raw_input
            let raw_vowels: Vec<u16> = self
                .raw_input
                .iter()
                .map(|(k, _, _)| *k)
                .filter(|k| keys::is_vowel(*k))
                .collect();

            // Check for V1-V2-V1 pattern (3+ vowels where first and last are same, middle is different)
            if raw_vowels.len() >= 3 {
                let last_three = &raw_vowels[raw_vowels.len() - 3..];
                let v1 = last_three[0];
                let v2 = last_three[1];
                let v3 = last_three[2];

                // V1-V2-V1 pattern: first and last are same vowel, middle is different
                if v1 == v3 && v1 != v2 {
                    // EXCEPTION: Check if buffer contains a valid Vietnamese triphthong
                    // Valid triphthongs: iêu, yêu, uôi, oai, etc. (defined in constants)
                    // If the first 3 raw vowels form a triphthong that matches buffer, it's valid VN
                    // Example: "yeue" → raw_vowels=[Y,E,U,E] → first3=[Y,E,U] → yêu is valid VN
                    // Example: "queue" → raw_vowels=[U,E,U,E] → first3=[U,E,U] → not a VN triphthong
                    let first_three = [raw_vowels[0], raw_vowels[1], raw_vowels[2]];
                    if constants::VALID_TRIPHTHONGS.contains(&first_three) {
                        // Check if buffer actually has this triphthong with proper circumflex
                        let buf_vowels: Vec<(u16, u8)> = self
                            .buf
                            .iter()
                            .filter(|c| keys::is_vowel(c.key))
                            .map(|c| (c.key, c.tone))
                            .collect();
                        if buf_vowels.len() == 3 {
                            let (bv0, _) = buf_vowels[0];
                            let (bv1, bv1_tone) = buf_vowels[1];
                            let (bv2, _) = buf_vowels[2];
                            // Check if buffer matches the triphthong pattern with circumflex on middle vowel
                            // iêu/yêu: circumflex on E (middle)
                            // uôi: circumflex on O (middle)
                            if bv0 == first_three[0]
                                && bv1 == first_three[1]
                                && bv2 == first_three[2]
                                && bv1_tone == tone::CIRCUMFLEX
                            {
                                // Valid Vietnamese triphthong - don't restore
                                return None;
                            }
                        }
                    }

                    // Check if buffer has circumflex on v1 type followed by v2
                    let buf_vowels: Vec<(u16, u8)> = self
                        .buf
                        .iter()
                        .filter(|c| keys::is_vowel(c.key))
                        .map(|c| (c.key, c.tone))
                        .collect();

                    // Buffer should have 2 vowels (V1' with circumflex, V2)
                    if buf_vowels.len() >= 2 {
                        let buf_last_two = &buf_vowels[buf_vowels.len() - 2..];
                        let (buf_v1, buf_v1_tone) = buf_last_two[0];
                        let (buf_v2, _) = buf_last_two[1];

                        // V1 in buffer has circumflex and matches raw V1, V2 matches
                        if buf_v1 == v1
                            && buf_v1_tone == tone::CIRCUMFLEX
                            && buf_v2 == v2
                            && !self.buf.iter().any(|c| c.mark > 0)
                        {
                            return self.build_raw_chars();
                        }
                    }
                }
            }
        }

        // Buffer is valid Vietnamese AND no English patterns → KEEP
        None
    }

    /// Check if this is an intentional revert at end of word that should be kept.
    /// Returns true when double modifier is at end AND it's likely intentional (not English word).
    ///
    /// Heuristics:
    /// - Very short words (raw_input <= 3 chars): likely intentional revert → keep
    /// - Double vowel tone keys (a, e, o, w): always intentional → keep
    /// - Double 'x' or 'j': not common in English → keep
    /// - Double 's', 'f', 'r' in longer words (4+ chars): common English pattern → restore
    ///
    /// Examples:
    /// - "ass" (3 chars, ss) → keep "as"
    /// - "aaa" (3 chars, aa) → keep "aa" (circumflex revert)
    /// - "maxx" (4 chars, xx) → keep "max" (xx not common in English)
    /// - "bass" (4 chars, ss) → restore to "bass" (ss very common in English)
    fn ends_with_double_modifier(&self) -> bool {
        if self.raw_input.len() < 2 {
            return false;
        }

        let (last_key, _, _) = self.raw_input[self.raw_input.len() - 1];
        let (second_last_key, _, _) = self.raw_input[self.raw_input.len() - 2];

        // Must be same key pressed twice
        if last_key != second_last_key {
            return false;
        }

        // Check if it's a vowel tone key (Telex: a, e, o for circumflex; w for horn/breve)
        // These are always intentional reverts - no English words use double vowels like this
        if self.method == 0 {
            if matches!(last_key, keys::A | keys::E | keys::O | keys::W) {
                return true;
            }
        } else {
            // VNI: 6, 7, 8 for vowel tones
            if matches!(last_key, keys::N6 | keys::N7 | keys::N8) {
                return true;
            }
        }

        // Check if it's a mark key
        let is_mark_key = if self.method == 0 {
            // Telex tone modifiers: s, f, r, x, j
            matches!(last_key, keys::S | keys::F | keys::R | keys::X | keys::J)
        } else {
            // VNI tone modifiers: 1, 2, 3, 4, 5
            matches!(
                last_key,
                keys::N1 | keys::N2 | keys::N3 | keys::N4 | keys::N5
            )
        };

        if !is_mark_key {
            return false;
        }

        // Very short words (3 chars or less raw input) → likely intentional revert
        // EXCEPTION: Double 'f' should NOT be treated as intentional revert because
        // 'ff' is extremely common in English (off, iff, aff-, eff-, etc.)
        // These short words should still trigger auto-restore to preserve 'ff'
        // For 'ff', continue to more checks below - don't return true
        if self.raw_input.len() <= 3 && last_key != keys::F {
            return true;
        }

        // For 4-char raw input producing 3-char result (e.g., "SOSS" → "SOS", "varr" → "var"),
        // keep the reverted result. The user explicitly typed double modifier to revert.
        // EXCEPTION: Double 'ss' with buffer ending in 's' - this is invalid VN final
        // Words like "bass", "pass", "boss", "less" should restore to English.
        if self.raw_input.len() == 4 && self.buf.len() == 3 {
            // Double 'ss' at end → buffer ends with 's' → invalid VN final → restore
            if last_key == keys::S {
                if let Some(last_char) = self.buf.last() {
                    if last_char.key == keys::S {
                        // Buffer ends with 's' = invalid Vietnamese final
                        // Return false to allow restore
                        return false;
                    }
                }
            }
            return true;
        }

        // For longer words (5+ chars), check modifier type:
        // - 'x', 'j' (Telex) or VNI numbers: not common doubles in English → keep
        // - 's', 'f', 'r' (Telex): very common doubles in English (bass, staff, error) → restore
        if self.method == 0 {
            // Telex: only keep for uncommon double letters (x, j)
            matches!(last_key, keys::X | keys::J)
        } else {
            // VNI: number modifiers are always intentional → keep
            true
        }
    }

    /// Get raw_input as lowercase ASCII string
    fn get_raw_input_string(&self) -> String {
        self.raw_input
            .iter()
            .filter_map(|&(key, caps, _)| utils::key_to_char(key, caps))
            .collect::<String>()
            .to_lowercase()
    }

    /// Get raw_input as ASCII string preserving original case
    pub(super) fn get_raw_input_string_preserve_case(&self) -> String {
        self.raw_input
            .iter()
            .filter_map(|&(key, caps, shift)| utils::key_to_char_ext(key, caps, shift))
            .collect()
    }

    /// Check if buffer is NOT valid Vietnamese (for unified auto-restore logic)
    ///
    /// Uses full validation including tone requirements (circumflex for êu, etc.)
    /// Also checks for patterns that are structurally valid but not real Vietnamese words.
    /// Returns true if buffer is structurally or phonetically invalid Vietnamese.
    fn is_buffer_invalid_vietnamese(&self) -> bool {
        if self.buf.is_empty() {
            return false;
        }

        // DICTIONARY-BASED VALIDATION (when english_auto_restore is enabled)
        // If word is in Vietnamese dictionary, it's definitely valid Vietnamese.
        if self.english_auto_restore {
            let buffer_str = self.buf.to_full_string();
            if dictionary::is_vietnamese(&buffer_str, self.allow_foreign_consonants) {
                return false; // Valid VN word in dictionary
            }

            // If buffer is NOT in VN dictionary AND raw_input is a valid English word,
            // AND raw_input has TELEX DOUBLE PATTERN (oo, ee, aa, dd, ss, ff...),
            // consider buffer as INVALID Vietnamese to trigger auto-restore.
            // This implements the "Not VN AND Is EN → restore" rule.
            //
            // IMPORTANT: Only apply for telex double patterns to maintain Vietnamese-first.
            // Words like "lisa" → "lía" should stay Vietnamese (no double pattern).
            // Words like "choose" (oo), "see" (ee), "add" (dd) should restore.
            let raw_str = self.get_raw_input_string();
            let has_telex_double = self.raw_input.windows(2).any(|pair| {
                let (k1, _, _) = pair[0];
                let (k2, _, _) = pair[1];
                k1 == k2
                    && matches!(
                        k1,
                        keys::O
                            | keys::E
                            | keys::A
                            | keys::D
                            | keys::S
                            | keys::F
                            | keys::R
                            | keys::X
                            | keys::J
                    )
            });

            if has_telex_double {
                // If buffer IS a valid English word, keep it regardless of raw spelling
                // e.g., "bussiness" → buffer "business" (in dict) → not invalid
                let buffer_str_lower = self.get_buffer_string().to_lowercase();
                if english_dict::is_english_word(&buffer_str_lower) {
                    return false; // Buffer is valid English → keep it
                }
                // Raw is English word but buffer isn't → invalid VN (trigger restore)
                if english_dict::is_english_word(&raw_str) {
                    return true; // Telex double + Not in VN dict + IS in EN dict → invalid VN
                }
            }
        }

        // Get keys and tones from buffer
        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
        let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
        let buffer_marks: Vec<u8> = self.buf.iter().map(|c| c.mark).collect();

        // Check 1: Basic structural validation (with foreign consonants support)
        if !is_valid_with_tones_and_foreign(
            &buffer_keys,
            &buffer_tones,
            self.allow_foreign_consonants,
        ) {
            return true;
        }

        // Check 2: -ing + tone mark is NOT valid Vietnamese
        // Vietnamese uses -inh (tính, kính), not -ing with tone marks
        // Pattern: [vowel I with tone] + [N] + [G] at the end
        if buffer_keys.len() >= 3 {
            let len = buffer_keys.len();
            if buffer_keys[len - 2] == keys::N
                && buffer_keys[len - 1] == keys::G
                && buffer_keys[len - 3] == keys::I
                && buffer_marks[len - 3] > 0
            {
                // 'i' has a tone mark + ends with 'ng' = invalid (thíng, kíng)
                return true;
            }
        }

        // Check 3: Single vowel validation
        // ALL single vowels with tone marks are valid Vietnamese words
        // Vietnamese-first logic: valid VN → keep VN
        // Examples: á, à, ả, ã, ạ, é, è, ẻ, ẽ, ẹ, í, ì, ỉ, ĩ, ị, ...
        // Also: ồ, ố, ổ, ỗ, ộ (circumflex), ừ, ứ, ử, ữ, ự (horn)
        if buffer_keys.len() == 1 && keys::is_vowel(buffer_keys[0]) && buffer_marks[0] > 0 {
            // Single vowel + any mark → valid Vietnamese, skip restore
            return false;
        }

        // Check 4: C + circumflex vowel (from double vowel) + NO MARK + no final = uncommon
        // "sê", "tê", "pê" are not real Vietnamese words (no mark)
        // But "số" (number), "tế" (cell), etc. with marks ARE valid Vietnamese
        // And "bê" (calf), "mê" (obsessed), "lê" (pear) are valid even without marks
        if buffer_keys.len() == 2 {
            let initial = buffer_keys[0];
            let vowel = buffer_keys[1];
            let vowel_tone = buffer_tones[1];
            let vowel_mark = buffer_marks[1];
            if keys::is_consonant(initial)
                && keys::is_vowel(vowel)
                && vowel_tone == tone::CIRCUMFLEX
                && vowel_mark == 0
            // Only invalid when there's NO mark
            {
                // Check if this is an uncommon pattern
                if constants::UNCOMMON_CIRCUMFLEX_NO_FINAL.contains(&initial) {
                    return true;
                }
            }
        }

        // Check 5: Open diphthong + consonant final = INVALID
        // Open diphthongs (ai, ao, au, ay, eo, iu, oi, ui, ưu) cannot take consonant finals.
        // Example: "mason" → "máon" has diphthong "ao" + final "n" → invalid
        // This catches English words like mason, reason, poison, etc.
        let syllable = syllable::parse(&buffer_keys);
        if syllable.vowel.len() == 2 && !syllable.final_c.is_empty() {
            let vowel_pair = [
                buffer_keys[syllable.vowel[0]],
                buffer_keys[syllable.vowel[1]],
            ];
            // Check if final is a consonant (not semi-vowel that's part of diphthong)
            let final_key = buffer_keys[syllable.final_c[0]];
            let is_consonant_final = matches!(
                final_key,
                keys::C | keys::K | keys::M | keys::N | keys::P | keys::T
            ) || (syllable.final_c.len() == 2); // CH, NG, NH are always consonant finals

            if is_consonant_final && constants::OPEN_DIPHTHONGS.contains(&vowel_pair) {
                return true;
            }
        }

        // Check 5b: Invalid circumflex diphthong WITHOUT final = INVALID Vietnamese
        // In Vietnamese, circumflex on V2 is only valid for: iê, yê, uê, uô
        // Other V1 + circumflex-V2 (e.g., uâ, oâ) are invalid when standalone (no final)
        // BUT with finals they can be valid: uân (tuân, luận), uất (tuất)
        // Only flag as invalid when there's no final consonant
        if syllable.vowel.len() == 2 && syllable.final_c.is_empty() {
            let v1_key = buffer_keys[syllable.vowel[0]];
            let v2_key = buffer_keys[syllable.vowel[1]];
            let v2_tone = buffer_tones[syllable.vowel[1]];
            if v2_tone == tone::CIRCUMFLEX {
                let is_valid_v2_circumflex = matches!(
                    (v1_key, v2_key),
                    (keys::I, keys::E)
                        | (keys::Y, keys::E)
                        | (keys::U, keys::E)
                        | (keys::U, keys::O)
                );
                if !is_valid_v2_circumflex {
                    return true;
                }
            }
        }

        // Check 6: HORN-O + E pattern is INVALID Vietnamese
        // "oe" is valid diphthong (xoe, hoe), but "ơe" / "ởe" doesn't exist
        // This catches English words like "power" → "pởe", "tower" → "tởe"
        for i in 0..buffer_keys.len().saturating_sub(1) {
            if buffer_keys[i] == keys::O
                && buffer_tones[i] == tone::HORN
                && buffer_keys[i + 1] == keys::E
            {
                return true; // ơe is invalid Vietnamese
            }
        }

        // Check 7: "ươu" triphthong at word start (no initial) is INVALID Vietnamese
        // Vietnamese syllables with "ươu" REQUIRE an initial consonant:
        // - Valid: cươu, hươu, bươu (with initial)
        // - Invalid: ươu (no initial - doesn't exist in Vietnamese)
        // BUT "ươ" alone or with consonant final IS valid: ươ, ương, ươn, ươm
        // This catches English words like "wou" → "ươu", "would" → "ươuld"
        // Pattern: U+horn (ư) + O+horn (ơ) + U (plain) at word start
        if buffer_keys.len() >= 3
            && syllable.initial.is_empty()
            && buffer_keys[0] == keys::U
            && buffer_tones[0] == tone::HORN
            && buffer_keys[1] == keys::O
            && buffer_tones[1] == tone::HORN
            && buffer_keys[2] == keys::U
        {
            return true; // ươu at word start without initial is invalid
        }

        // Check 8: K final + CIRCUMFLEX vowel = INVALID Vietnamese
        // K final is only valid in ethnic minority words with:
        // - Breve vowels (ă): Đắk, Lắk
        // - Plain vowels with tone: Búk
        // K final is NOT valid with circumflex (ô, ê, â): cổk, têk, âk are invalid
        // This catches English words like "cowork" → "cổk", "network" → "netwổk"
        if !syllable.final_c.is_empty() {
            let final_key = buffer_keys[syllable.final_c[0]];
            if final_key == keys::K {
                // Check if vowel has circumflex - if so, invalid
                for &i in &syllable.vowel {
                    if buffer_tones[i] == tone::CIRCUMFLEX {
                        return true; // circumflex + K final is invalid Vietnamese
                    }
                }
            }
        }

        false
    }

    /// Check if buffer matches a Vietnamese triple-o word pattern
    ///
    /// Vietnamese has rare words with literal double-o (not circumflex):
    /// - boóng, choòng, đoòng, goòng, toòng → final is NG
    /// - coóc, moóc, soóc → final is C
    ///
    /// When typing these words, user types triple-o (ooo) which reverts circumflex to oo.
    /// After revert, tone modifiers (s, f) should STILL be applied.
    /// This function identifies these patterns to allow tone modifier application.
    ///
    /// Supports two scenarios:
    /// 1. Complete pattern: initial + OO + final (NG/C) → allow tone after final
    /// 2. Partial pattern: initial + OO (no final yet) → allow tone before final
    ///
    /// For partial patterns (no final), we only match triple-o word initials.
    pub(super) fn is_vietnamese_triple_o_word(&self) -> bool {
        if self.buf.len() < 2 {
            // Minimum: OO (2 chars for vowel-initial pattern) or initial + OO (3 chars)
            return false;
        }

        let keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
        let len = keys.len();

        // Check for valid finals: NG or C
        let has_ng_final = len >= 2 && keys[len - 2] == keys::N && keys[len - 1] == keys::G;
        let has_c_final = keys[len - 1] == keys::C;
        let has_valid_final = has_ng_final || has_c_final;

        // Find double O position
        let double_o_pos = keys
            .windows(2)
            .position(|pair| pair[0] == keys::O && pair[1] == keys::O);
        if double_o_pos.is_none() {
            return false;
        }
        let oo_pos = double_o_pos.unwrap();

        // Check valid initials for Vietnamese triple-o words
        let first_key = keys[0];

        // Single consonant initials: B, C, G, M, S, T, D at position 0, OO at position 1,2
        let single_initial_match = matches!(
            first_key,
            keys::B | keys::C | keys::G | keys::M | keys::S | keys::T | keys::D
        ) && oo_pos == 1;

        // CH initial: CH at position 0,1, OO at position 2,3
        let ch_initial_match =
            first_key == keys::C && len >= 2 && keys[1] == keys::H && oo_pos == 2;

        // Vowel-initial: OO at position 0 (no consonant initial)
        // Example: "ooosng" → "oóng", "ooosfng" → "oòng"
        let vowel_initial_match = first_key == keys::O && oo_pos == 0;

        if !single_initial_match && !ch_initial_match && !vowel_initial_match {
            return false;
        }

        // CASE 1: Complete pattern with valid final (NG/C) → always match
        if has_valid_final {
            return true;
        }

        // CASE 2: Partial pattern (no final yet) → only for initials without English collision
        // Allow: B, CH, D, G, T, S (Vietnamese triple-o patterns)
        // B is included: "booos" → "boó" (mark applied, auto-restored if consonant follows)
        // Exclude: C, M (collide with English "coos", "moos")
        // Also allow vowel-initial: "ooo" → "oo" + tone
        let oo_at_end = oo_pos + 2 == len;
        let safe_partial_initial =
            matches!(first_key, keys::B | keys::D | keys::G | keys::T | keys::S);
        (ch_initial_match || vowel_initial_match || (safe_partial_initial && single_initial_match))
            && oo_at_end
    }

    /// Check if raw_input is valid English (for unified auto-restore logic)
    ///
    /// Checks that raw_input contains only basic ASCII letters (A-Z, a-z)
    /// and doesn't have patterns that would indicate Vietnamese typing intent.
    /// Returns true if raw_input looks like an English word.
    fn is_raw_input_valid_english(&self) -> bool {
        if self.raw_input.is_empty() {
            return false;
        }

        // All keys must be ASCII letters (A-Z)
        let all_ascii_letters = self.raw_input.iter().all(|(k, _, _)| {
            // Keys are in range A-Z (from keys.rs)
            // Consonants and vowels are valid English letters
            keys::is_consonant(*k) || keys::is_vowel(*k)
        });

        if !all_ascii_letters {
            return false;
        }

        // Check raw_input is structurally valid (can be parsed as English word)
        // Simplified check: must have at least one vowel (except for short abbreviations)
        let has_vowel = self.raw_input.iter().any(|(k, _, _)| keys::is_vowel(*k));

        // Short words (1-2 chars) without vowels might be abbreviations
        if self.raw_input.len() <= 2 {
            return true;
        }

        has_vowel
    }

    /// Build raw chars from telex_double_raw (if stored) + subsequent raw_input,
    /// or fall back to converting all of raw_input to chars.
    ///
    /// telex_double_raw stores the original input before a mark/stroke revert modified
    /// raw_input. Subsequent chars typed after the revert are appended from raw_input.
    fn collect_raw_chars(&self) -> Vec<char> {
        if let Some(ref base_raw) = self.telex_double_raw {
            if !base_raw.is_empty() && self.telex_double_raw_len > 0 {
                let mut result: Vec<char> = base_raw.chars().collect();
                // For stroke revert (dd): raw_input had 1 char removed → start 1 earlier
                // For mark revert (ss): raw_input unchanged → start at stored len
                let subsequent_start = if self.raw_input.len() < self.telex_double_raw_len {
                    self.telex_double_raw_len.saturating_sub(1)
                } else {
                    self.telex_double_raw_len
                };
                // Clamp to raw_input length to avoid out-of-bounds slice when
                // raw_input has been shortened further (e.g. backspace) after
                // telex_double_raw was stored.
                let subsequent_start = subsequent_start.min(self.raw_input.len());
                for &(key, caps, shift) in &self.raw_input[subsequent_start..] {
                    if let Some(ch) = utils::key_to_char_ext(key, caps, shift) {
                        result.push(ch);
                    }
                }
                return result;
            }
        }
        self.raw_input
            .iter()
            .filter_map(|&(key, caps, shift)| utils::key_to_char_ext(key, caps, shift))
            .collect()
    }

    /// Build raw chars from raw_input EXACTLY as typed (no collapsing).
    /// Used for whitelist-based restore where we want the exact English word.
    fn build_raw_chars_exact(&self) -> Option<Vec<char>> {
        let chars = self.collect_raw_chars();
        if chars.is_empty() {
            None
        } else {
            Some(chars)
        }
    }

    /// Build raw chars from raw_input for restore
    ///
    /// When a mark was reverted (e.g., "ss" → "s"), decide between buffer and raw_input:
    /// - If after revert there's vowel + consonant pattern → use buffer ("dissable" → "disable")
    /// - If after revert there's only vowels → use raw_input ("issue" → "issue")
    ///
    /// Also handles triple vowel collapse (e.g., "saaas" → "saas"):
    /// - Triple vowel (aaa, eee, ooo) is collapsed to double vowel
    /// - This handles circumflex revert in Telex (aa=â, aaa=aa)
    pub(super) fn build_raw_chars(&self) -> Option<Vec<char>> {
        let raw_chars: Vec<char> = if self.had_mark_revert && self.should_use_buffer_for_revert() {
            // Use buffer content which already has the correct reverted form
            // e.g., "dissable" → "disable", "usser" → "user"
            self.buf.to_string_preserve_case().chars().collect()
        } else {
            let mut chars: Vec<char> = self.collect_raw_chars();

            // Collapse vowel patterns for English restore (Telex circumflex patterns)
            // Only collapse when double/triple vowel is IMMEDIATELY followed by tone modifier at END
            // This distinguishes Telex patterns (saax → sax) from real English doubles (wheel, looks)

            // Check for SaaS pattern: same consonant at start and end
            // SaaS, FaaS, etc. should keep the double vowel
            let is_saas_pattern = chars.len() >= 3
                && chars.first().map(|c| c.to_ascii_lowercase())
                    == chars.last().map(|c| c.to_ascii_lowercase())
                && chars
                    .first()
                    .map(|c| !matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y'))
                    .unwrap_or(false);

            // Check if double vowel is immediately followed by tone modifier at end
            // Example: "saax" (s-aa-x) → double 'a' at index 1-2, 'x' at index 3 (end)
            // Counter-example: "looks" (l-oo-k-s) → double 'o' at index 1-2, 'k' at index 3 (NOT modifier)
            // Counter-example: "career" (c-a-r-ee-r) → double 'e' but 'r' is part of English word
            // IMPORTANT: Only collapse for SHORT words (<=4 chars) which are clearly Telex patterns
            // Longer words like "career", "beer", "peer" should keep their double vowels
            let tone_modifiers = ['s', 'f', 'r', 'x', 'j'];
            let has_double_vowel_at_end = chars.len() >= 3 && chars.len() <= 4 && {
                let last = chars[chars.len() - 1].to_ascii_lowercase();
                let second_last = chars[chars.len() - 2].to_ascii_lowercase();
                let third_last = chars[chars.len() - 3].to_ascii_lowercase();
                // Check: double vowel (same letter) + tone modifier at end
                matches!(second_last, 'a' | 'e' | 'o')
                    && second_last == third_last
                    && tone_modifiers.contains(&last)
            };

            // 1. Triple vowel → collapse to double when NOT at end: "saaas" → "saas"
            // Only collapse when there are chars after the triple (i+3 < len),
            // preserving triple vowels at word end for exact raw restore
            // ("mufaaa" keeps all 3 a's since user typed them intentionally)
            let mut had_triple_vowel_collapse = false;
            let mut i = 0;
            while i + 2 < chars.len() {
                let c = chars[i].to_ascii_lowercase();
                if matches!(c, 'a' | 'e' | 'o')
                    && chars[i].eq_ignore_ascii_case(&chars[i + 1])
                    && chars[i + 1].eq_ignore_ascii_case(&chars[i + 2])
                    && i + 3 < chars.len()
                {
                    chars.remove(i + 1);
                    had_triple_vowel_collapse = true;
                    continue;
                }
                i += 1;
            }

            // 1b. Triple consonant → collapse to double: "bufffer" → "buffer", "afffair" → "affair"
            // English never has 3 consecutive same consonants, so this is always a revert artifact
            let mut i = 0;
            while i + 2 < chars.len() {
                let c = chars[i].to_ascii_lowercase();
                // Check for triple same consonant (common modifiers: s, f, r, x, j)
                if chars[i].eq_ignore_ascii_case(&chars[i + 1])
                    && chars[i + 1].eq_ignore_ascii_case(&chars[i + 2])
                    && !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
                {
                    chars.remove(i + 1);
                    continue;
                }
                i += 1;
            }

            // 2. Double vowel → single ONLY if:
            //    - Double vowel immediately precedes tone modifier at end (Telex pattern)
            //    - NOT SaaS pattern (same consonant at start/end)
            // Example: "saax" → "sax" (aa + x at end)
            // Counter-example: "looks" → "looks" (oo + k, not tone modifier)
            // Counter-example: "saas" → "saas" (SaaS pattern)
            if has_double_vowel_at_end && !is_saas_pattern {
                // Collapse the double vowel (remove one of the paired letters)
                // Position: third_last and second_last are the double vowel
                let pos = chars.len() - 3;
                chars.remove(pos);
            }

            // 3. Double vowel collapse when circumflex was applied then reverted
            // Scans entire word (end AND middle) for double a/e/o from circumflex revert.
            // Dict priority: if double form is in dict → keep it; if collapsed form is in dict → collapse.
            // If neither in dict → keep double form (no collapse).
            // IMPORTANT: Use had_circumflex_revert, NOT had_mark_revert
            // had_mark_revert is set for tone marks (ff in coffee), which should NOT collapse
            if self.had_circumflex_revert && chars.len() >= 2 {
                let current_str: String = chars.iter().collect::<String>().trim().to_lowercase();
                // Only attempt collapse if current (double) form is NOT in dict
                if !english_dict::is_english_word(&current_str) {
                    let mut i = 0;
                    while i + 1 < chars.len() {
                        let c = chars[i].to_ascii_lowercase();
                        let next = chars[i + 1].to_ascii_lowercase();
                        if matches!(c, 'a' | 'e' | 'o') && c == next {
                            let mut collapsed = chars.clone();
                            collapsed.remove(i);
                            let collapsed_str: String =
                                collapsed.iter().collect::<String>().trim().to_lowercase();
                            if english_dict::is_english_word(&collapsed_str) {
                                chars = collapsed;
                                continue; // re-check same position
                            }
                        }
                        i += 1;
                    }
                }
            }

            // Collapse double 'w' at start to single 'w' only if there's more after
            // Example: "wwax" → "wax" (double 'w' is Telex revert pattern)
            // But "ww" or "www" alone → keep "ww" (user typed this intentionally)
            if chars.len() > 2
                && chars[0].eq_ignore_ascii_case(&'w')
                && chars[1].eq_ignore_ascii_case(&'w')
            {
                chars.remove(0);
            }

            // Collapse consecutive double tone modifiers when mark was reverted
            // AND one of these conditions:
            // 1. Short buffer (<=3 chars) - user just wanted a diphthong
            //    Example: "arro" → "aro" (buffer="aro" = 3 chars, collapse double 'r')
            // 2. Word starts with "u + doubled_modifier" - rare pattern in English
            //    English words rarely start with u+ss, u+ff, u+rr, etc.
            //    Example: "ussers" → "users" (u+ss is revert artifact)
            //    Counter-example: "issue" (i+ss is common: issue, issuer)
            //    Counter-example: "offers" (o+ff is common: offer, office)
            //
            // EXCEPTION: Never collapse 'ff' because it's very common in English:
            // - off, offer, office, coffee, effect, effort, afford, differ, etc.
            // - Collapsing 'ff' → 'f' would break many common English words
            let tone_modifiers_char = ['s', 'r', 'x', 'j']; // Exclude 'f'
            let starts_with_u_doubled_modifier = chars.len() >= 3
                && chars[0].eq_ignore_ascii_case(&'u')
                && tone_modifiers_char.contains(&chars[1].to_ascii_lowercase())
                && chars[1].eq_ignore_ascii_case(&chars[2]);

            if self.had_mark_revert && (self.buf.len() <= 3 || starts_with_u_doubled_modifier) {
                // Collapse consecutive double modifiers, but skip 'ss'/'ff' at the VERY END
                // Examples:
                // - "usser" → "user" (ss in middle, collapse)
                // - "bass" → "bass" (ss at end, keep)
                // - "buff" → "buff" (ff at end, keep)
                let tone_modifiers = ['s', 'f', 'r', 'x', 'j'];
                let mut i = 0;
                while i + 1 < chars.len() {
                    let c = chars[i].to_ascii_lowercase();
                    let next = chars[i + 1].to_ascii_lowercase();

                    // Skip 'ss' or 'ff' at the VERY END of the word
                    let is_at_end = i + 2 == chars.len();
                    let is_ss_or_ff = (c == 's' && next == 's') || (c == 'f' && next == 'f');
                    if is_at_end && is_ss_or_ff {
                        i += 1;
                        continue;
                    }

                    // Same tone modifier doubled → collapse to single
                    if tone_modifiers.contains(&c) && c == next {
                        chars.remove(i);
                        continue; // Check again at same position for triple+
                    }
                    i += 1;
                }
            }

            // 4. Dictionary-based double vowel collapse for ALL vowels (including u, i)
            // This handles cases where double vowel comes from backspace + retype, not circumflex
            // Example: "sur<upervisor" → "su" + "upervisor" = "suupervisor" → "supervisor"
            // Only collapse when:
            //   - Current form is NOT in dictionary, AND
            //   - Collapsed form IS in dictionary
            //   - Double vowel is NOT at the very end (preserve "free", "agree", "aree")
            //   - NOT a SaaS pattern (same consonant at start and end)
            // This section does NOT require had_circumflex_revert flag
            if chars.len() >= 3 && !is_saas_pattern {
                let current_str: String = chars.iter().collect::<String>().trim().to_lowercase();
                if !english_dict::is_english_word(&current_str) {
                    let mut i = 0;
                    // Skip double vowels at the very end (i + 1 == chars.len() - 1)
                    while i + 2 < chars.len() {
                        let c = chars[i].to_ascii_lowercase();
                        let next = chars[i + 1].to_ascii_lowercase();
                        // Check for double vowel (all vowels: a, e, i, o, u)
                        if matches!(c, 'a' | 'e' | 'i' | 'o' | 'u') && c == next {
                            let mut collapsed = chars.clone();
                            collapsed.remove(i);
                            let collapsed_str: String =
                                collapsed.iter().collect::<String>().trim().to_lowercase();
                            if english_dict::is_english_word(&collapsed_str) {
                                chars = collapsed;
                                continue; // re-check same position
                            }
                        }
                        i += 1;
                    }
                }
            }

            // Partial restore: Telex tone modifier + doubled/tripled vowel patterns.
            // Applies the tone mark to the correct vowel and collapses extra vowels.
            // Works with any consonant cluster length (ch, tr, ng, ngh, etc.)
            //
            // Tail patterns (after consonant prefix):
            //   A: V+tone+V+V    - "mufaa"→"muàa", "chufaa"→"chuàa"
            //   B: V1+V2+tone+V2 - "muafa"→"muàa", "chaofo"→"chàoo"
            // Triple vowel → strip last char, then reuse A/B:
            //   V+tone+VVV       - "mufaaa"→"mùaa"  (pattern A, from_triple=true)
            //   V1+V2+tone+V2V2  - "muafaa"→"muàa"  (pattern B)
            if self.method == 0 && !had_triple_vowel_collapse && chars.len() >= 5 {
                let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
                let is_tone = |c: char| matches!(c, 's' | 'f' | 'r' | 'x' | 'j');
                let apply_t = |v: char, t: char| -> char {
                    let tones: &[char] = match v {
                        'a' => &['á', 'à', 'ả', 'ã', 'ạ'],
                        'e' => &['é', 'è', 'ẻ', 'ẽ', 'ẹ'],
                        'i' => &['í', 'ì', 'ỉ', 'ĩ', 'ị'],
                        'o' => &['ó', 'ò', 'ỏ', 'õ', 'ọ'],
                        'u' => &['ú', 'ù', 'ủ', 'ũ', 'ụ'],
                        'y' => &['ý', 'ỳ', 'ỷ', 'ỹ', 'ỵ'],
                        _ => return v,
                    };
                    let idx = match t {
                        's' => 0,
                        'f' => 1,
                        'r' => 2,
                        'x' => 3,
                        'j' => 4,
                        _ => return v,
                    };
                    tones[idx]
                };
                let pcase = |toned: char, orig: char| -> char {
                    if orig.is_uppercase() {
                        toned.to_uppercase().next().unwrap_or(toned)
                    } else {
                        toned
                    }
                };

                // Split into leading consonants + vowel tail
                let cons_end = chars
                    .iter()
                    .position(|c| is_vowel(c.to_ascii_lowercase()))
                    .unwrap_or(chars.len());

                if cons_end > 0 && cons_end < chars.len() {
                    let cons = &chars[..cons_end];
                    let tail = &chars[cons_end..];
                    let tail_lc: Vec<char> = tail.iter().map(|c| c.to_ascii_lowercase()).collect();

                    // Normalize triple-vowel tail (len 5) → len 4 by stripping last char
                    let (vtail, from_triple) = if tail_lc.len() == 5 {
                        if is_vowel(tail_lc[0])
                            && is_tone(tail_lc[1])
                            && matches!(tail_lc[2], 'a' | 'e' | 'o')
                            && tail_lc[2] == tail_lc[3]
                            && tail_lc[3] == tail_lc[4]
                        {
                            (&tail[..4], true) // V+tone+VVV: tone targets first vowel
                        } else if is_vowel(tail_lc[0])
                            && matches!(tail_lc[1], 'a' | 'e' | 'o')
                            && is_tone(tail_lc[2])
                            && tail_lc[1] == tail_lc[3]
                            && tail_lc[3] == tail_lc[4]
                        {
                            (&tail[..4], false) // V1+V2+tone+V2V2: maps to pattern B
                        } else {
                            (tail, false)
                        }
                    } else {
                        (tail, false)
                    };

                    if vtail.len() == 4 {
                        let vl: Vec<char> = vtail.iter().map(|c| c.to_ascii_lowercase()).collect();

                        // True when the buffer mark lands on the second vowel
                        let mark_on_later_vowel = {
                            let bv: Vec<_> =
                                self.buf.iter().filter(|c| keys::is_vowel(c.key)).collect();
                            bv.len() >= 2 && bv.iter().skip(1).any(|c| c.mark > 0)
                        };

                        // Pattern A: V + tone + V + V (doubled vowel after tone key)
                        if is_vowel(vl[0])
                            && is_tone(vl[1])
                            && matches!(vl[2], 'a' | 'e' | 'o')
                            && vl[2] == vl[3]
                        {
                            let mut result = cons.to_vec();
                            if !from_triple && mark_on_later_vowel {
                                result.push(vtail[0]);
                                result.push(pcase(apply_t(vl[2], vl[1]), vtail[2]));
                            } else {
                                result.push(pcase(apply_t(vl[0], vl[1]), vtail[0]));
                                result.push(vtail[2]);
                            }
                            result.push(vtail[3]);
                            return Some(result);
                        }

                        // Pattern B: V1 + V2 + tone + V2 (diphthong + tone key)
                        if is_vowel(vl[0]) && is_vowel(vl[1]) && is_tone(vl[2]) && vl[1] == vl[3] {
                            let mut result = cons.to_vec();
                            if mark_on_later_vowel {
                                result.push(vtail[0]);
                                result.push(pcase(apply_t(vl[1], vl[2]), vtail[1]));
                            } else {
                                result.push(pcase(apply_t(vl[0], vl[2]), vtail[0]));
                                result.push(vtail[1]);
                            }
                            result.push(vtail[3]);
                            return Some(result);
                        }
                    }
                }
            }

            chars
        };

        if raw_chars.is_empty() {
            return None;
        }

        // Optimization: If raw_chars equals current buffer, no restore needed
        // This happens when user manually reverted (e.g., "usser" → "user")
        // Avoids unnecessary backspace + retype of the same content
        // NOTE: Use to_full_string() to include diacritics for proper comparison
        let buffer_str: String = self.buf.to_full_string();
        let raw_str: String = raw_chars.iter().collect();
        if buffer_str == raw_str {
            return None;
        }

        Some(raw_chars)
    }

    /// Determine if buffer should be used for restore after a mark revert
    ///
    /// Heuristic: Use buffer when it forms a recognizable English word pattern,
    /// OR when raw_input looks like a typo (double letter + single vowel at end).
    ///
    /// Examples:
    /// - "dissable" → buffer "disable" has dis- prefix → use buffer
    /// - "soffa" → double ff + single vowel 'a' at end → use buffer "sofa"
    /// - "issue" → iss + ue pattern (double + multiple chars) → use raw_input "issue"
    /// - "error" → err + or pattern (double + multiple chars) → use raw_input "error"
    fn should_use_buffer_for_revert(&self) -> bool {
        let buf_str = self.buf.to_lowercase_string();

        // Common English prefixes that suggest intentional revert
        const PREFIXES: &[&str] = &[
            "dis", "mis", "un", "re", "de", "pre", "per", "anti", "non", "sub", "trans", "con",
        ];

        // Common English suffixes
        const SUFFIXES: &[&str] = &[
            "able", "ible", "tion", "sion", "ment", "ness", "less", "ful", "ing", "ive", "ified",
            "ous", "ory",
        ];

        // Short suffixes for common words (need minimum buffer length check)
        // Examples: "user" (ends with -er), "color" (ends with -or)
        const SHORT_SUFFIXES: &[&str] = &["er", "or"];

        // Check if buffer matches common English word patterns
        // Use >= to include short words like "transit" (7 chars) with "trans" (5 chars)
        for prefix in PREFIXES {
            if buf_str.starts_with(prefix) && buf_str.len() >= prefix.len() + 2 {
                return true;
            }
        }

        // Check if raw_input contains double 'ss' or 'ff' with MULTIPLE chars after
        // The rule: double letter + multiple chars after → use raw (English word)
        //           double letter + single char after → use buffer (revert pattern)
        // Examples:
        // - "massive" (ss at pos 2-3, then ive) → use raw "massive"
        // - "soffa" (ff at pos 2-3, then just 'a') → use buffer "sofa"
        // - "masson" (ss at pos 2-3, then on) → use buffer "mason" (open diphthong case)
        let raw_len = self.raw_input.len();
        for i in 0..raw_len.saturating_sub(1) {
            let (k1, _, _) = self.raw_input[i];
            let (k2, _, _) = self.raw_input[i + 1];
            if (k1 == keys::S && k2 == keys::S) || (k1 == keys::F && k2 == keys::F) {
                // Found double at position i, i+1
                // Check how many chars follow after the double
                let chars_after_double = raw_len - (i + 2);

                // Special case: buffer ends with common single-consonant patterns
                // like "-son", "-ton", "-ron" (mason, reason, person, etc.)
                // These are much more common than double-consonant versions
                // so prefer buffer when this pattern is detected
                let common_single_consonant_endings = ["son", "ton", "ron", "non", "mon"];
                let use_buffer_for_ending = common_single_consonant_endings
                    .iter()
                    .any(|ending| buf_str.ends_with(ending));

                if chars_after_double >= 2 && !use_buffer_for_ending {
                    // Multiple chars after double → likely English word, use raw
                    return false;
                }
                // Only 0-1 char after double, or common ending → likely revert pattern
                if use_buffer_for_ending {
                    return true;
                }
                break;
            }
        }

        // Suffix check: only use buffer if raw_input is exactly 1 char longer
        // This indicates user typed double modifier to revert, and buffer has the collapsed form.
        // Example: "verrified" (9 chars) → buffer "verified" (8 chars) → use buffer
        // Counter-example: "massive" (7 chars) → buffer "masive" (6 chars) → raw has 7, buf has 6
        //   But raw_input for "massive" should be 7 chars... let me check
        // Actually for double 's' at end, we skip the pop, so raw stays at 7 chars.
        // For double 'r' in "verrified", we don't skip the pop, so raw becomes 8 chars? No wait...
        // The issue is that suffix check runs AFTER the pop logic on space.
        // For "verrified": pop removes one 'r', so raw becomes 8 chars = buffer.len
        // For "massive": we skip pop for double 's', so raw stays 7 chars > buffer 6
        // So the condition should be: raw_input.len() == buf_str.len() + 1 means double was NOT at end
        // and raw_input.len() == buf_str.len() means pop happened (double was at end of pattern)
        for suffix in SUFFIXES {
            if buf_str.ends_with(suffix) && buf_str.len() >= suffix.len() + 2 {
                // Only use buffer if lengths match (pop happened) or diff is 1 (expected revert pattern)
                // This filters out cases like "massive" where raw has legitimate double letter
                if self.raw_input.len() <= buf_str.len() + 1 {
                    return true;
                }
            }
        }

        // Check short suffixes with stricter conditions:
        // - Buffer must be exactly 4 chars (short words like "user", not longer like "userer")
        // - Must end with -er or -or
        // - Raw input must have exactly 5 chars (one more than buffer due to double modifier)
        // - The double must be 'ss' only (not 'ff', 'rr', etc.) because:
        //   - "usser" → "user" is a common typing pattern when reverting sắc mark
        //   - "offer", "differ", "suffer" are legitimate English words with double 'f'
        //   - "error", "mirror" have double 'r' as legitimate English
        // - The double 's' must appear exactly twice (not "assessor")
        if buf_str.len() == 4 && self.raw_input.len() == 5 {
            for suffix in SHORT_SUFFIXES {
                if buf_str.ends_with(suffix) {
                    // Only check for double 's' at position 1,2 (0-indexed)
                    // Pattern: V-SS-V-C like "usser" → "user"
                    let (key_1, _, _) = self.raw_input[1];
                    let (key_2, _, _) = self.raw_input[2];
                    if key_1 == keys::S && key_2 == keys::S {
                        // Check 's' appears exactly twice
                        let s_count = self
                            .raw_input
                            .iter()
                            .filter(|(k, _, _)| *k == keys::S)
                            .count();
                        if s_count == 2 {
                            return true;
                        }
                    }
                }
            }
        }

        // Check if raw_input has double 'f' followed by single vowel at end
        // Pattern: "soffa" → double 'f' + single 'a' → likely typo, use buffer "sofa"
        // Only apply for 'f' because:
        // - Double 'f' + vowel at end is rare in English (no common words like "staffa")
        // - Double 's'/'r' + vowel has many valid words (worry, sorry, carry, etc.)
        if self.raw_input.len() >= 4 {
            let len = self.raw_input.len();
            let (last_key, _, _) = self.raw_input[len - 1];
            let (second_last_key, _, _) = self.raw_input[len - 2];
            let (third_last_key, _, _) = self.raw_input[len - 3];

            // Only for double 'f' + single vowel at end
            if keys::is_vowel(last_key) && second_last_key == keys::F && third_last_key == keys::F {
                return true;
            }

            // Double 's' + single vowel at end (but not 'y' to avoid "sorry" → "sory")
            // Pattern: "raisse" → buffer "raise" (double 's' + single 'e' → use buffer)
            // This handles cases where user typed extra 's' for sắc mark then reverted
            // Exclude 'y' because words like "sorry", "carry" are common English
            let is_core_vowel = matches!(
                last_key,
                k if k == keys::A || k == keys::E || k == keys::I || k == keys::O || k == keys::U
            );
            if is_core_vowel && second_last_key == keys::S && third_last_key == keys::S {
                return true;
            }

            // Double 's' at very end: distinguish revert pattern from English words
            // "thiss" → buffer "this" (revert pattern, buffer is valid word)
            // "guess" → raw "guess" (valid English word with double 's')
            // Heuristics for when buffer is a valid English word:
            // 1. Buffer starts with common English digraph (th, wh, ch, sh)
            // 2. Buffer ends with consonant + 's' (common plural: sims, gaps, maps)
            //    vs buffer ends with vowel + 's' (less common: gues, mues)
            if last_key == keys::S && second_last_key == keys::S && len == buf_str.len() + 1 {
                let starts_with_digraph = buf_str.starts_with("th")
                    || buf_str.starts_with("wh")
                    || buf_str.starts_with("ch")
                    || buf_str.starts_with("sh");
                if starts_with_digraph {
                    return true;
                }

                // Check if buffer ends with consonant + 's' (common English plural pattern)
                // "sims" = m + s (consonant + s) → use buffer
                // "gues" = e + s (vowel + s) → use raw "guess"
                if buf_str.len() >= 2 {
                    let chars: Vec<char> = buf_str.chars().collect();
                    let second_last_char = chars[chars.len() - 2];
                    let last_char = chars[chars.len() - 1];
                    // Check consonant + 's' pattern (plural)
                    let is_plural_pattern = last_char == 's'
                        && !matches!(second_last_char, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
                    if is_plural_pattern {
                        return true;
                    }
                }
            }
        }

        // Generic check: double Telex modifier in middle with EXACTLY 2 chars after
        // Pattern: raw has double modifier (ss/ff/rr/xx/jj) followed by V+C (vowel+consonant)
        // Examples:
        // - "sarrah" → "sarah" (double 'r' + "ah" = V+C)
        // - "usser" → "user" (double 's' + "er" = V+C) [also handled by specific check above]
        //
        // IMPORTANT constraints to avoid false positives on real English words:
        // 1. Buffer must be plain ASCII (no Vietnamese transforms)
        // 2. Raw must end with consonant (not vowel like "issue")
        // 3. Suffix after double must be short: exactly 2 chars (V+C pattern)
        //    This excludes "current" (suffix "ent" = 3 chars), "effect" (suffix "ect" = 3 chars)
        // 4. Only apply if exactly 2 occurrences of the modifier (not "assess")
        // 5. For safety, only apply to double 'r', 'x', 'j' (not 's' or 'f' which are more
        //    common in legitimate English doubles like "professor", "different")
        //    Double 's' is already handled by specific check above.
        // 6. Exclude common English suffixes after double consonant:
        //    - "ow" (borrow, sorrow, tomorrow), "or" (error, mirror, horror)
        //    - "ry"/"y" (carry, sorry, worry), "ed" (occurred, referred)
        //    These are legitimate English words, not typing mistakes.
        const RARE_DOUBLE_MODIFIERS: &[u16] = &[keys::R, keys::X, keys::J];

        if self.raw_input.len() >= 4 && self.raw_input.len() == buf_str.len() + 1 {
            // Constraint 1: Buffer must be plain ASCII (no Vietnamese transforms)
            let has_transforms = self
                .buf
                .iter()
                .any(|c| c.tone > 0 || c.mark > 0 || c.stroke);
            if has_transforms {
                return false;
            }

            // Constraint 2: Raw must end with consonant
            let (last_key, _, _) = self.raw_input[self.raw_input.len() - 1];
            if !keys::is_consonant(last_key) {
                return false;
            }

            // Constraint 6: Exclude common English suffixes after double consonant
            // Get last 2 key codes
            let (last_key_1, _, _) = self.raw_input[self.raw_input.len() - 1];
            let (last_key_2, _, _) = self.raw_input[self.raw_input.len() - 2];

            // Common English suffixes that appear after double consonants:
            // - "ow" (borrow, sorrow), "or" (error, mirror), "ry" (carry, sorry, worry)
            // - "ed" (occurred, referred), "ly" (hurriedly)
            // Note: "er" is NOT excluded here because the SHORT_SUFFIXES check above
            // handles 4-char words ending with "er", and longer words like "error"
            // have 3+ occurrences which is excluded by occurrence count check.
            let is_common_suffix = matches!(
                (last_key_2, last_key_1),
                (keys::O, keys::W)   // ow: borrow, sorrow
                    | (keys::O, keys::R) // or: error, mirror, horror
                    | (keys::R, keys::Y) // ry: carry, sorry, worry
                    | (keys::E, keys::D) // ed: occurred, referred
                    | (keys::L, keys::Y) // ly: hurriedly
            );
            if is_common_suffix {
                return false;
            }

            // Find double modifier with exactly 2 chars after (V+C or C+C pattern)
            for i in 0..self.raw_input.len().saturating_sub(2) {
                let (key_i, _, _) = self.raw_input[i];
                let (key_next, _, _) = self.raw_input[i + 1];

                if RARE_DOUBLE_MODIFIERS.contains(&key_i) && key_i == key_next {
                    // Double modifier found at position i, i+1
                    let chars_after_double = self.raw_input.len() - (i + 2);

                    // Constraint 3: Exactly 2 chars after double
                    // This excludes longer suffixes like "ent" (current), "ect" (effect)
                    if chars_after_double == 2 {
                        // Count total occurrences of this modifier
                        let occurrence_count = self
                            .raw_input
                            .iter()
                            .filter(|(k, _, _)| *k == key_i)
                            .count();

                        // Constraint 4: Only 2 occurrences
                        if occurrence_count == 2 {
                            return true;
                        }
                    }
                }
            }
        }

        // Check for short words with double modifier at end that reverted
        // Pattern: "thiss" → buffer "this"
        // Raw input ends with double modifier (ss, rr, ff, xx, jj)
        // Buffer has 4+ chars ending with that consonant
        // Only apply if double modifier at end is the ONLY occurrence of that char
        // This preserves "assess" (multiple 's') while converting "thiss" → "this"
        // IMPORTANT: Only use buffer if it's VALID Vietnamese structure.
        // If buffer is invalid (like "gues" ending with 's'), use raw input "guess" instead.
        if self.raw_input.len() >= 4 && buf_str.len() >= 4 && buf_str.len() <= 6 {
            let len = self.raw_input.len();
            let (last_key, _, _) = self.raw_input[len - 1];
            let (second_last_key, _, _) = self.raw_input[len - 2];

            // Check for double modifier at end (ss, rr, ff, xx, jj)
            let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];
            if tone_modifiers.contains(&last_key) && last_key == second_last_key {
                // Count occurrences of this modifier key in raw_input
                let occurrence_count = self
                    .raw_input
                    .iter()
                    .filter(|(k, _, _)| *k == last_key)
                    .count();
                // Only apply if the double at end is the only occurrence (exactly 2)
                if occurrence_count == 2 {
                    // Buffer should end with that consonant (after revert)
                    let expected_char = match last_key {
                        k if k == keys::S => 's',
                        k if k == keys::F => 'f',
                        k if k == keys::R => 'r',
                        k if k == keys::X => 'x',
                        k if k == keys::J => 'j',
                        _ => '\0',
                    };
                    if expected_char != '\0' && buf_str.ends_with(expected_char) {
                        // Check if buffer is valid Vietnamese structure
                        // If not (like "gues" ending with invalid final 's'), don't use buffer
                        let buffer_keys: Vec<u16> = self.buf.iter().map(|c| c.key).collect();
                        let buffer_tones: Vec<u8> = self.buf.iter().map(|c| c.tone).collect();
                        if validation::is_valid_with_tones(&buffer_keys, &buffer_tones) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Check for English patterns in raw_input that suggest non-Vietnamese
    ///
    /// Patterns detected:
    /// 1. Modifier (s/f/r/x/j in Telex) followed by consonant: "text" (x before t)
    /// 2. Modifier at end of long word (>2 chars): "their" (r at end)
    /// 3. Modifier after first vowel then another vowel: "use" (s between u and e)
    /// 4. Consonant + W + vowel without tone modifiers (only on word complete): "swim"
    pub(super) fn has_english_modifier_pattern(&self, is_word_complete: bool) -> bool {
        let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];

        // CRITICAL: Detect tone override pattern - vowel + mod1 + mod2 + vowel
        // Example: "chajfo" = ch + [A + J + F + O] → A is first vowel, J→F tone override, O completes diphthong
        // Pattern must be: first_vowel + modifier1 + modifier2 + second_vowel (all consecutive)
        // BUT: if raw_input is in English dictionary (like "cursor"), restore to English
        if is_word_complete && !self.buf.is_empty() {
            // Find position of first vowel
            let first_vowel_pos = self
                .raw_input
                .iter()
                .position(|(k, _, _)| keys::is_vowel(*k));

            if let Some(vowel_pos) = first_vowel_pos {
                // Check for pattern: vowel + mod1 + mod2 + vowel at positions vowel_pos..vowel_pos+4
                if vowel_pos + 3 < self.raw_input.len() {
                    let (k1, _, _) = self.raw_input[vowel_pos + 1];
                    let (k2, _, _) = self.raw_input[vowel_pos + 2];
                    let (k3, _, _) = self.raw_input[vowel_pos + 3];

                    let has_tone_override = tone_modifiers.contains(&k1)
                        && tone_modifiers.contains(&k2)
                        && k1 != k2  // Different modifiers (j→f, not jj)
                        && keys::is_vowel(k3); // Followed by vowel

                    if has_tone_override {
                        // Check if raw is in English dictionary
                        let raw_str: String = self
                            .raw_input
                            .iter()
                            .filter_map(|&(k, c, s)| utils::key_to_char_ext(k, c, s))
                            .collect();
                        let raw_in_dict = english_dict::is_english_word(&raw_str);

                        // If raw is NOT in English dict AND buffer is valid Vietnamese, keep it
                        if !raw_in_dict && !self.is_buffer_invalid_vietnamese() {
                            return false;
                        }
                    }
                }
            }
        }

        // Vietnamese diphthong with tone modifier in middle (3 chars)
        // Pattern: vowel + tone_modifier + vowel → valid Vietnamese diphthong with tone
        // Examples: "ira" → "ỉa", "ofa" → "òa", "ore" → "ỏe", "iru" → "ỉu"
        // Only match VALID Vietnamese diphthongs (not all vowel pairs are valid)
        if self.raw_input.len() == 3 {
            let (k0, _, _) = self.raw_input[0];
            let (k1, _, _) = self.raw_input[1];
            let (k2, _, _) = self.raw_input[2];
            if keys::is_vowel(k0) && tone_modifiers.contains(&k1) && keys::is_vowel(k2) {
                // Check if this vowel pair is a valid Vietnamese diphthong
                // Valid: ia, iu, ua, uo, oa, oe, oi, ai, ao, au, ay, ei, eo, eu
                // Invalid: ue, ae, ie, io, ea, ou (these should restore to English)
                let is_valid_diphthong = matches!(
                    (k0, k2),
                    (keys::I, keys::A)
                        | (keys::I, keys::U)
                        | (keys::U, keys::A)
                        | (keys::U, keys::O)
                        | (keys::O, keys::A)
                        | (keys::O, keys::E)
                        | (keys::O, keys::I)
                        | (keys::A, keys::I)
                        | (keys::A, keys::O)
                        | (keys::A, keys::U)
                        | (keys::A, keys::Y)
                        | (keys::E, keys::I)
                        | (keys::E, keys::O)
                        | (keys::E, keys::U)
                );
                if is_valid_diphthong {
                    return false; // Keep Vietnamese
                }
            }
        }

        // Single vowel + modifiers only → valid Vietnamese (á, é, í, ó, ú, ý, etc.)
        // ALL single vowels with tone marks are valid Vietnamese words
        // Examples: "as" → "á", "es" → "é", "is" → "í", "or" → "ỏ", "us" → "ú"
        // Vietnamese-first logic: valid VN → keep VN (don't check if raw looks English)
        if self.raw_input.len() >= 2 {
            let (first, _, _) = self.raw_input[0];
            if keys::is_vowel(first) && first != keys::W {
                let all_after_are_modifiers = self.raw_input[1..]
                    .iter()
                    .all(|(k, _, _)| tone_modifiers.contains(k));
                if all_after_are_modifiers {
                    // Vowel + mark modifiers only → valid Vietnamese, not English
                    return false;
                }
            }
        }

        // Check for W at start - W is not a valid Vietnamese initial consonant
        // Words like "wow", "window", "water" start with W
        // Exception: standalone "w" → "ư" is valid Vietnamese
        if self.raw_input.len() >= 2 {
            let (first, _, _) = self.raw_input[0];
            if first == keys::W {
                // Check if there's another W later (non-adjacent) → English pattern like "wow"
                let has_later_w = self.raw_input[2..].iter().any(|(k, _, _)| *k == keys::W);
                if has_later_w {
                    return true;
                }

                // W-as-vowel pattern: When W is converted to ư, treat it as a vowel position
                // This means mark modifiers (s, f, r, x, j) immediately after W are tone marks
                // for the ư vowel, not consonants.
                // Examples: "wf" → "ừ", "ws" → "ứ", "wmf" → "ừm"

                // Check for "W + only mark modifiers" pattern → valid Vietnamese (ừ, ứ, ử, ữ, ự)
                // This handles standalone W with tone marks like "wf " → "ừ "
                let all_are_modifiers = self.raw_input[1..]
                    .iter()
                    .all(|(k, _, _)| tone_modifiers.contains(k));
                if all_are_modifiers && !self.raw_input[1..].is_empty() {
                    // W + mark modifiers only → valid Vietnamese, not English
                    return false;
                }

                // Check for "W + consonant + mark modifier" pattern → valid Vietnamese
                // Examples: "wmf" → "ừm", "wms" → "ứm", "wng" → "ưng"
                // Pattern: W (→ư) + valid_final_consonant + optional_mark (NO other vowels!)
                // "west" has vowel E, so it should NOT match this pattern
                if self.raw_input.len() >= 2 {
                    // First check if there are any other vowels after W
                    let has_other_vowels = self.raw_input[1..]
                        .iter()
                        .any(|(k, _, _)| keys::is_vowel(*k) && *k != keys::W);

                    // Only apply W+consonant+mark pattern if there are NO other vowels
                    if !has_other_vowels {
                        let non_modifier_consonants: Vec<u16> = self.raw_input[1..]
                            .iter()
                            .filter(|(k, _, _)| {
                                keys::is_consonant(*k) && !tone_modifiers.contains(k)
                            })
                            .map(|(k, _, _)| *k)
                            .collect();

                        let has_mark_modifier = self.raw_input[1..]
                            .iter()
                            .any(|(k, _, _)| tone_modifiers.contains(k));

                        // W + valid_final + mark → valid Vietnamese (ừm, ứng, etc.)
                        if !non_modifier_consonants.is_empty() && has_mark_modifier {
                            let is_valid_final = match non_modifier_consonants.len() {
                                1 => {
                                    constants::VALID_FINALS_1.contains(&non_modifier_consonants[0])
                                }
                                2 => {
                                    let pair =
                                        [non_modifier_consonants[0], non_modifier_consonants[1]];
                                    constants::VALID_FINALS_2.contains(&pair)
                                }
                                _ => false,
                            };
                            if is_valid_final {
                                return false; // Valid Vietnamese pattern
                            }
                        }
                    }
                }

                // Analyze pattern: W + vowels + consonants
                // Find position of first vowel to distinguish consonants from modifiers
                let first_vowel_pos = self.raw_input[1..]
                    .iter()
                    .position(|(k, _, _)| keys::is_vowel(*k) && *k != keys::W);

                let vowels_after: Vec<u16> = self.raw_input[1..]
                    .iter()
                    .filter(|(k, _, _)| keys::is_vowel(*k) && *k != keys::W)
                    .map(|(k, _, _)| *k)
                    .collect();

                // Only exclude Telex mark modifiers (s, f, r, x, j) when they come AFTER a vowel
                // If they come BEFORE any vowel, they're consonants (e.g., "wra" has 'r' as consonant)
                // EXCEPTION: When W is at start (w-as-vowel) and NO other vowels, modifiers are marks
                let consonants_after: Vec<u16> = self.raw_input[1..]
                    .iter()
                    .enumerate()
                    .filter(|(i, (k, _, _))| {
                        if !keys::is_consonant(*k) || *k == keys::W {
                            return false;
                        }
                        // For W-as-vowel WITHOUT other vowels, treat modifiers as marks
                        // e.g., "wf" → "ừ", "wmf" → "ừm" (no other vowels, so f is mark)
                        // But "wra" has vowel A, so R should be treated as consonant
                        if vowels_after.is_empty() && tone_modifiers.contains(k) {
                            return false;
                        }
                        // Modifier keys AFTER first vowel are tone modifiers, not consonants
                        if let Some(vowel_pos) = first_vowel_pos {
                            if *i > vowel_pos && tone_modifiers.contains(k) {
                                return false;
                            }
                        }
                        true
                    })
                    .map(|(_, (k, _, _))| *k)
                    .collect();

                // W + vowel + consonant → likely English like "win", "water"
                // W + consonant only → valid Vietnamese (ưng, ưn, ưm)
                // EXCEPTION: W+O+final is valid Vietnamese "ươ+final" (ương, ươn, ươm, ươc, ươt, ươp)
                if !vowels_after.is_empty() && !consonants_after.is_empty() {
                    // Check for W+O+valid_final pattern (ương, ươn, ươm, etc.)
                    // raw_input: [W, O, N, G] → valid Vietnamese ương
                    // raw_input: [W, O, M] → valid Vietnamese ươm
                    let is_wo_final_pattern = vowels_after.len() == 1
                        && vowels_after[0] == keys::O
                        && match consonants_after.len() {
                            1 => {
                                // Single consonant finals: n, m, c, t, p
                                matches!(
                                    consonants_after[0],
                                    keys::N | keys::M | keys::C | keys::T | keys::P
                                )
                            }
                            2 => {
                                // Double consonant finals: ng, nh
                                let pair = [consonants_after[0], consonants_after[1]];
                                pair == [keys::N, keys::G] || pair == [keys::N, keys::H]
                            }
                            _ => false,
                        };
                    if is_wo_final_pattern {
                        // Valid Vietnamese "ươ+final", don't restore
                        return false;
                    }
                    // Both vowels and consonants after W → likely English
                    return true;
                }

                // W + vowel only → check if valid Vietnamese pattern
                // Valid: ưa (W+A), ươ (W+O), ưu (W+U)
                // Invalid: ưe (W+E), ưi (W+I), ưy (W+Y) → restore as English
                if !vowels_after.is_empty() && consonants_after.is_empty() {
                    let valid_vowels_after_w = [keys::A, keys::O, keys::U];
                    let has_invalid_vowel = vowels_after
                        .iter()
                        .any(|v| !valid_vowels_after_w.contains(v));
                    if has_invalid_vowel {
                        return true;
                    }
                }

                // W + consonants only → check if valid Vietnamese final
                if !consonants_after.is_empty() && vowels_after.is_empty() {
                    let is_valid_final = match consonants_after.len() {
                        1 => constants::VALID_FINALS_1.contains(&consonants_after[0]),
                        2 => {
                            let pair = [consonants_after[0], consonants_after[1]];
                            constants::VALID_FINALS_2.contains(&pair)
                        }
                        _ => false, // 3+ consonants is invalid
                    };

                    if !is_valid_final {
                        return true;
                    }
                }
            }

            // Check for consonant + W + vowel pattern without tone modifiers
            // Only check when word is complete (on space/break), not mid-word
            // Mid-word we can't tell if user will add tone modifiers later
            // - "nwoc" during typing → might become "nwocj" → "nược" (Vietnamese)
            // - "swim" on space → no tone modifiers → restore to English
            if is_word_complete {
                let (second, _, _) = self.raw_input[1];
                if second == keys::W && keys::is_consonant(first) && first != keys::Q {
                    // Q+W is valid Vietnamese (qu-), but other consonant+W may be English
                    if self.raw_input.len() >= 3 {
                        let (third, _, _) = self.raw_input[2];
                        // Check if third char is a vowel (not a tone modifier like j)
                        if keys::is_vowel(third) {
                            // Exception: C+W+O+NG pattern is Vietnamese "ương" (tương, sương, etc.)
                            // Pattern: consonant + W + O + N + G → valid Vietnamese diphthong
                            if third == keys::O && self.raw_input.len() >= 5 {
                                let (fourth, _, _) = self.raw_input[3];
                                let (fifth, _, _) = self.raw_input[4];
                                if fourth == keys::N && fifth == keys::G {
                                    // This is Vietnamese "ương" pattern, don't restore
                                    return false;
                                }
                            }

                            // Issue #151: C+W+A pattern is Vietnamese "ưa" (mưa, cưa, lưa, etc.)
                            // Pattern: consonant + W + A → valid Vietnamese diphthong
                            // When raw_input is exactly 3 chars (C+W+A), this is Vietnamese
                            // Examples: mwa → mưa, cwa → cưa, lwa → lưa, twa → tưa
                            if third == keys::A && self.raw_input.len() == 3 {
                                return false;
                            }

                            // C+W+U pattern is Vietnamese "ưu" (lưu, mưu, cưu, etc.)
                            // Pattern: consonant + W + U → valid Vietnamese diphthong
                            // Examples: lwu → lưu, mwu → mưu, cwu → cưu
                            if third == keys::U && self.raw_input.len() == 3 {
                                return false;
                            }

                            // Check if there's ANY tone modifier (j/s/f/r/x) in the rest of the word
                            let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];
                            let has_tone_modifier = self.raw_input[2..]
                                .iter()
                                .any(|(k, _, _)| tone_modifiers.contains(k));

                            // No tone modifier + consonant+W+vowel → likely English like "swim"
                            if !has_tone_modifier {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Telex modifiers that add tone marks
        let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];

        // Pattern: Consecutive tone modifiers followed by VOWEL (English pattern)
        // Example: "cursor" = c-u-r-s-o-r → "rs" followed by vowel 'o' → English
        // Counter-example: "đướng" typed as dduowfsng → "fs" followed by consonant 'n' → Vietnamese
        // Vietnamese allows consecutive modifiers for tone adjustment (f→s changes huyền to sắc)
        for i in 0..self.raw_input.len().saturating_sub(2) {
            let (key, _, _) = self.raw_input[i];
            let (next_key, _, _) = self.raw_input[i + 1];
            let (after_key, _, _) = self.raw_input[i + 2];
            // Two DIFFERENT consecutive modifiers followed by vowel → English
            // Example: "cursor" = c-u-r-s-o-r → "rs" (r≠s) followed by vowel 'o' → English
            // Same modifier doubled (rr, ss, ff) is Telex revert pattern, NOT English
            // Example: "arro" = a-r-r-o → "rr" (r=r) is revert pattern → skip
            if tone_modifiers.contains(&key)
                && tone_modifiers.contains(&next_key)
                && key != next_key // Only different modifiers indicate English
                && keys::is_vowel(after_key)
            {
                return true;
            }
        }

        // Find positions of modifiers in raw_input
        for i in 0..self.raw_input.len() {
            let (key, _, _) = self.raw_input[i];

            if !tone_modifiers.contains(&key) {
                continue;
            }

            // Found a modifier at position i

            // Pattern 1: Modifier followed by consonant → English
            // Example: "text" has X followed by T, "expect" has X followed by P
            // Counter-example: "muwowjt" has J followed by T (Vietnamese - multiple vowels)
            // Counter-example: "dojdc" = D+O+J+D+C (Vietnamese "đọc" - j + consonants is valid)
            if i + 1 < self.raw_input.len() {
                let (next_key, _, _) = self.raw_input[i + 1];
                // W is a vowel modifier in Telex, not a true consonant for this check
                // Also exclude tone modifier keys (S, F, R, X, J) - these are mark keys, not consonants
                // when they appear after a vowel. Example: "dduowfs" has 'f' then 's', both are modifiers.
                let is_true_consonant = keys::is_consonant(next_key)
                    && next_key != keys::W
                    && !tone_modifiers.contains(&next_key);
                if is_true_consonant {
                    // Heuristic: In Vietnamese, tone modifiers + consonant is common:
                    // - nặng (j) + consonant: học, bọc, bật, cặp, đọc, etc.
                    // - sắc (s) + consonant: bức, đất, ất, etc.
                    // - huyền (f) + consonant: làm, hàng, dùng, vàng, etc.
                    // - hỏi (r) + consonant: tỉnh, đỉnh, nhỉnh, mỉnh, etc.
                    // - ngã (x) + consonant: mãnh, hãnh, etc.
                    //
                    // Skip restore for ALL tone modifiers followed by consonant
                    // This handles:
                    // - "dojc" → "dọc" (j + final c)
                    // - "lafm" → "làm" (f + final m)
                    // - "tirnh" → "tỉnh" (r + final nh)
                    // - "maxnh" → "mãnh" (x + final nh)
                    // Vietnamese tone modifiers have different likelihood with consonants:
                    // - nặng (j) + any consonant: COMMON (học, bọc, bật, làm, etc.)
                    // - sắc (s) + any consonant: COMMON (bức, đất, sắm, etc.)
                    // - huyền (f) + sonorant (m,n,ng,nh): COMMON (làm, hàng, dùng, cũng)
                    // - hỏi (r) + sonorant (m,n,ng,nh): COMMON (tỉnh, đỉnh, nhỉnh, cửng)
                    // - ngã (x) + sonorant (m,n,ng,nh): COMMON (mãnh, hãnh, cũng)
                    // - huyền/hỏi/ngã + stop (c,p,t): RARE in Vietnamese
                    let is_common_viet_mark = key == keys::J || key == keys::S;
                    let is_rare_with_stop = key == keys::F || key == keys::R || key == keys::X;
                    // Sonorants: M, N, or G/H when following N (part of ng, nh finals)
                    let is_sonorant_or_part_of_final = next_key == keys::M
                        || next_key == keys::N
                        || (next_key == keys::G && i >= 1 && self.raw_input[i - 1].0 == keys::N)
                        || (next_key == keys::H && i >= 1 && self.raw_input[i - 1].0 == keys::N);

                    // Always skip for J and S - these are very common in Vietnamese
                    if is_common_viet_mark {
                        continue;
                    }

                    // For F, R, X: skip only if followed by sonorant (m, n, ng, nh)
                    // This allows "text" to restore but keeps "tỉnh", "làm", "mãnh", "cũng"
                    if is_rare_with_stop && is_sonorant_or_part_of_final {
                        continue;
                    }

                    // EXCEPTION: When next_key is D and there's another D earlier,
                    // this is Vietnamese đ (stroke) pattern with d...d
                    // Example: "daafdm" = d + aa + f + D + m → "đầm"
                    // The two D's form the stroke pattern for đ
                    if next_key == keys::D {
                        let d_count = self
                            .raw_input
                            .iter()
                            .filter(|(k, _, _)| *k == keys::D)
                            .count();
                        if d_count >= 2 {
                            continue; // Vietnamese stroke pattern
                        }
                    }

                    // Case 1a: More letters after the consonant → definitely English
                    // Example: "expect" = E+X+P+E+C+T (X followed by P, then more)
                    if i + 2 < self.raw_input.len() {
                        return true;
                    }

                    // Case 1b: Final consonant but only 1 vowel before modifier → likely English
                    // Example: "text" = T+E+X+T (only 1 vowel E before X)
                    let vowels_before: usize = (0..i)
                        .filter(|&j| keys::is_vowel(self.raw_input[j].0))
                        .count();
                    if vowels_before == 1 {
                        return true;
                    }
                }
            }

            // Pattern 2: Modifier at end AND suspicious vowel pair before → English
            // Example: "their" → t-h-e-i-r, "ei" before r → suspicious English pattern
            // Example: "pair" → p-a-i-r, "ai" before r (only 2 vowels) → suspicious English pattern
            // Counter-example: "booj" → b-o-o-j, "oo" (same vowel) → Telex doubling, Vietnamese
            // Counter-example: "chiuj" → c-h-i-u-j, "iu" → valid Vietnamese diphthong
            // Counter-example: "hoaij" → h-o-a-i-j, "oai" (3 vowels) → valid Vietnamese
            if i + 1 == self.raw_input.len() && i >= 2 {
                let (v1, _, _) = self.raw_input[i - 2];
                let (v2, _, _) = self.raw_input[i - 1];
                // Check for suspicious English vowel patterns before modifier
                // Same vowel doubling (oo, aa, ee) is Telex pattern, not suspicious
                if keys::is_vowel(v1) && keys::is_vowel(v2) && v1 != v2 {
                    // Count total vowels before modifier
                    let total_vowels: usize = (0..i)
                        .filter(|&j| keys::is_vowel(self.raw_input[j].0))
                        .count();

                    // EI before modifier is very English (their, weird, vein)
                    if v1 == keys::E && v2 == keys::I {
                        return true;
                    }
                    // AI before modifier is English ONLY if:
                    // 1. Exactly 2 vowels (not "oai" in "hoại")
                    // 2. AND initial is P alone (not PH) - P is rare in native Vietnamese
                    // This catches "pair" but not "mái", "cái", "xài" (common Vietnamese)
                    if v1 == keys::A && v2 == keys::I && total_vowels == 2 {
                        // Check if initial is just P (rare in native Vietnamese)
                        if !self.raw_input.is_empty() && self.raw_input[0].0 == keys::P {
                            // Make sure it's not PH (PH is common Vietnamese)
                            let is_ph = self.raw_input.len() >= 2 && self.raw_input[1].0 == keys::H;
                            if !is_ph {
                                return true;
                            }
                        }
                    }
                    // OE before S modifier at end is English "-oes" pattern
                    // Examples: "goes", "does", "toes", "woes", "foes", "hoes"
                    // Vietnamese "oe" diphthong (hoè, xoè) typically has different tone placement
                    // or final consonant, not bare "oe + sắc at end"
                    // IMPORTANT: Only check for S (sắc) modifier, NOT F/R/X/J
                    // This allows "moef" → "moè", "boef" → "boè" (Vietnamese huyền)
                    // EXCEPTION: "oes" without initial consonant is Vietnamese exclamation "oé"
                    // EXCEPTION: Vietnamese-specific initials (kh, gh, ngh, tr, ph, etc.) + oe + modifier
                    //            Example: "khoer" → "khoẻ" (healthy), "nhoer" → "nhoẻ"
                    if v1 == keys::O && v2 == keys::E && total_vowels == 2 && key == keys::S {
                        // Check for Vietnamese-specific initials (both digraphs and single consonants)
                        // Vietnamese OE words from dictionary: hòe, loè, tóe, xòe, khoẻ, ngoé...
                        let is_vietnamese_oe_initial = if self.raw_input.len() >= 2 {
                            let (c1, _, _) = self.raw_input[0];
                            let (c2, _, _) = self.raw_input[1];

                            // Vietnamese digraphs: kh, gh, ph, ch, th, nh (end with H), tr, ng
                            let ends_with_h = c2 == keys::H
                                && matches!(
                                    c1,
                                    k if k == keys::K
                                        || k == keys::G
                                        || k == keys::P
                                        || k == keys::C
                                        || k == keys::T
                                        || k == keys::N
                                );
                            let is_tr = c1 == keys::T && c2 == keys::R;
                            let is_ng = c1 == keys::N && c2 == keys::G;

                            // Single consonant + OE: check if c2 is O (meaning c1 is single initial)
                            // Valid Vietnamese single initials for OE: H, L, T, X, B, S
                            // Dictionary: hòe, loè, tóe, xòe, boe (boẻ), soé
                            let is_single_initial_oe = c2 == keys::O
                                && matches!(
                                    c1,
                                    keys::H | keys::L | keys::T | keys::X | keys::B | keys::S
                                );

                            ends_with_h || is_tr || is_ng || is_single_initial_oe
                        } else {
                            false
                        };

                        if is_vietnamese_oe_initial {
                            continue; // Vietnamese word, don't restore
                        }

                        // Only return true if there's an initial consonant (goes, does, foes, woes)
                        // Words without initial like "oes" → "oé" should stay Vietnamese
                        let has_initial =
                            !self.raw_input.is_empty() && keys::is_consonant(self.raw_input[0].0);
                        if has_initial {
                            return true;
                        }
                    }
                }

                // Pattern 2b: P + single vowel + modifier at end → English
                // P alone (not PH) is rare in native Vietnamese
                // Example: "per" = P + E + R → pẻ (but "per" is English preposition)
                if self.raw_input.len() >= 2 && self.raw_input[0].0 == keys::P {
                    let is_ph = self.raw_input.len() >= 2 && self.raw_input[1].0 == keys::H;
                    if !is_ph {
                        // Count vowels before modifier
                        let vowels_before: usize = (0..i)
                            .filter(|&j| keys::is_vowel(self.raw_input[j].0))
                            .count();
                        // P + single vowel + modifier at end (no more chars after modifier)
                        if vowels_before == 1 && i + 1 == self.raw_input.len() {
                            return true;
                        }
                    }
                }
            }

            // Pattern 3: Modifier immediately after single vowel, then another vowel
            // AND no initial consonant before the vowel
            // Example: "use" → U (vowel) + S (modifier) + E (vowel) = starts with vowel → English
            // Counter-example: "cura" → C + U + R + A = starts with consonant → Vietnamese "của"
            let vowels_before: usize = (0..i)
                .filter(|&j| keys::is_vowel(self.raw_input[j].0))
                .count();

            // If only 1 vowel before modifier AND vowel after AND no initial consonant → English
            if vowels_before == 1 && i + 1 < self.raw_input.len() {
                let (next_key, _, _) = self.raw_input[i + 1];
                if keys::is_vowel(next_key) {
                    // Find first vowel position
                    let first_vowel_pos = (0..i)
                        .find(|&j| keys::is_vowel(self.raw_input[j].0))
                        .unwrap_or(0);
                    // Check if there's a consonant before the first vowel
                    let has_initial_consonant = first_vowel_pos > 0
                        && keys::is_consonant(self.raw_input[first_vowel_pos - 1].0);
                    // Only restore if NO initial consonant (pure vowel-start like "use")
                    // EXCEPT: Vietnamese diphthongs without initial consonant
                    // U + modifier + A: ủa, ùa, úa, ũa, ụa (interjections)
                    //
                    // LINGUISTIC RULE: Vietnamese syllables have consonant BETWEEN vowel and modifier
                    // - "onro" = O + N + R + O → N separates first O from R → Vietnamese "ổn"
                    // - "use"  = U + S + E     → S directly after U → English
                    // This distinguishes intentional Vietnamese (vowel-consonant-modifier-vowel)
                    // from accidental English (vowel-modifier-vowel without consonant)
                    let has_consonant_between = (first_vowel_pos + 1 < i)
                        && keys::is_consonant(self.raw_input[first_vowel_pos + 1].0);
                    if !has_initial_consonant && !has_consonant_between {
                        let first_vowel = self.raw_input[first_vowel_pos].0;
                        // Vietnamese no-initial patterns:
                        // - Same vowel doubling: OFO → ồ, EFE → ề, AFA → ầ (circumflex + tone)
                        // - U + modifier + A: ủa, ùa, úa (interjections)
                        // - U + modifier + I: ủi, ùi, úi (ủi quần, úi chà)
                        // - U + modifier + Y: ủy, ùy, úy, uỵ (uy tín, uỵch - valid Vietnamese)
                        // - A + modifier + O: ảo, ào, áo (ảo giác, ảo tưởng)
                        // - A + modifier + I: ải, ái, ài (interjections)
                        // - A + modifier + Y: ảy, áy, ày (cảy, máy when no initial)
                        // - O + modifier + I: ỏi, ói, òi (interjections)
                        // - O + modifier + E: oẻ, oẹ, oé (interjections)
                        // - E + modifier + O: ẻo, ẹo, éo (interjections)
                        let is_vietnamese_no_initial = first_vowel == next_key // Same vowel = Telex circumflex
                            || (first_vowel == keys::U && (next_key == keys::A || next_key == keys::I || next_key == keys::Y))
                            || (first_vowel == keys::A && (next_key == keys::O || next_key == keys::I || next_key == keys::Y))
                            || (first_vowel == keys::O && (next_key == keys::I || next_key == keys::E))
                            || (first_vowel == keys::E && next_key == keys::O);

                        // Special case: no-initial O + modifier + E
                        // "ore" is common English word, should restore
                        // "oer", "oje" are not English, keep Vietnamese (oẻ, oẹ)
                        if first_vowel == keys::O && next_key == keys::E {
                            let raw_str: String = self
                                .raw_input
                                .iter()
                                .filter_map(|&(k, c, s)| utils::key_to_char_ext(k, c, s))
                                .collect();
                            if english_dict::is_english_word(&raw_str) {
                                return true; // Restore to English
                            }
                            // Not English word, keep Vietnamese
                            continue;
                        }

                        if !is_vietnamese_no_initial {
                            return true;
                        }
                    }

                    // Pattern 4: vowel + modifier + DIFFERENT vowel → English
                    // EXCEPT for Vietnamese diphthong patterns with tone in middle:
                    // - U + modifier + A/O: ưa, ươ (của, được)
                    // - A + modifier + I/Y/O: ai, ay, ao (gái, máy, nào)
                    // - O + modifier + I/A: oi, oa (bói, hói, hoá)
                    // Example: "core" = c + o + r + e → o+r+e is NOT Vietnamese pattern
                    // Example: "cura" = c + u + r + a → u+r+a IS Vietnamese (cửa)
                    // Example: "gasi" = g + a + s + i → a+s+i IS Vietnamese (gái)
                    // Example: "nafo" = n + a + f + o → a+f+o IS Vietnamese (nào)
                    if has_initial_consonant {
                        let (prev_char, _, _) = self.raw_input[i - 1];
                        // Skip if prev char is not a vowel (e.g., "ddense" has n before s)
                        // Pattern requires vowel + modifier + vowel
                        if !keys::is_vowel(prev_char) {
                            continue;
                        }
                        let prev_vowel = prev_char;
                        // Same vowel is Telex circumflex doubling (aa, ee, oo)
                        // Example: "loxoi" = l+o+x+O+i → O after X is same vowel doubling
                        // EXCEPTION: If a CONSONANT follows the doubled vowel, it's likely English
                        // Example: "param" = p+a+r+A+m → 'A' after 'r' same as before, 'm' (consonant) follows
                        // Counter-example: "loxoi" has 'i' (vowel) after → Vietnamese diphthong
                        if prev_vowel == next_key {
                            // Check if there are more chars after the second vowel
                            if i + 2 < self.raw_input.len() {
                                let (char_after, _, _) = self.raw_input[i + 2];
                                // Only English if followed by CONSONANT (param has 'm')
                                // If followed by vowel (loxoi has 'i'), it's Vietnamese diphthong
                                //
                                // EXCEPTION: Vietnamese delayed circumflex + final consonant
                                // "vajan" = v + a + j + a + n → "vận" (valid Vietnamese)
                                // "hajan" = h + a + j + a + n → "hận"
                                // Pattern: initial + vowel + mark + same_vowel + valid_final
                                // This creates circumflex+mark on vowel (ậ, ẫ, ầ, ẩ, ặ, etc.)
                                // BUT: "param" has same pattern but IS in English dict → restore
                                if keys::is_consonant(char_after) {
                                    // EXCEPTION: u + modifier + u + w is Vietnamese ưu diphthong
                                    // Pattern: "cufuw" = c + u + f + u + w → "cừu" (c + ừu)
                                    // The 'w' is the horn modifier converting u to ư
                                    // This is a valid Vietnamese typing order, not English
                                    if prev_vowel == keys::U && char_after == keys::W {
                                        continue; // Vietnamese ưu pattern
                                    }

                                    // EXCEPTION: When char_after is D and there are 2+ D's,
                                    // this is Vietnamese stroke pattern d...d for đ
                                    // Pattern: "dafadm" = d + a + f + a + D + m → "đầm"
                                    // The two D's form the stroke pattern for đ
                                    if char_after == keys::D {
                                        let d_count = self
                                            .raw_input
                                            .iter()
                                            .filter(|(k, _, _)| *k == keys::D)
                                            .count();
                                        if d_count >= 2 {
                                            continue; // Vietnamese stroke pattern
                                        }
                                    }

                                    // Check if this forms valid Vietnamese syllable with circumflex
                                    // Circumflex vowels (â, ê, ô) + valid finals
                                    let is_circumflex_vowel =
                                        matches!(prev_vowel, keys::A | keys::E | keys::O);
                                    let is_valid_final =
                                        constants::VALID_FINALS_1.contains(&char_after);

                                    if is_circumflex_vowel && is_valid_final {
                                        // Could be Vietnamese delayed circumflex (vận, hận)
                                        // But if raw input is in English dict → restore to English
                                        let raw_str: String = self
                                            .raw_input
                                            .iter()
                                            .filter_map(|&(k, c, s)| {
                                                utils::key_to_char_ext(k, c, s)
                                            })
                                            .collect();
                                        if english_dict::is_english_word(&raw_str) {
                                            return true; // English word (param, etc.)
                                        }
                                        // Not in English dict → keep Vietnamese (vận, hận, etc.)
                                    } else {
                                        return true; // Not Vietnamese pattern → English
                                    }
                                }
                            }
                            continue; // Same vowel without consonant after is Telex pattern
                        }
                        // Vietnamese exceptions: diphthongs with tone modifier in middle
                        let is_vietnamese_pattern = match prev_vowel {
                            k if k == keys::U => {
                                // ua: của, mủa; uo: được; uy: thuỷ, quỷ; ui: tụi, mủi, cúi, núi
                                // ue: handled separately below (like OE) to check English dict
                                next_key == keys::A
                                    || next_key == keys::O
                                    || next_key == keys::Y
                                    || next_key == keys::I
                            }
                            k if k == keys::A => {
                                // au: màu, náu, cau, lau, etc.
                                next_key == keys::I
                                    || next_key == keys::Y
                                    || next_key == keys::O
                                    || next_key == keys::U
                            }
                            k if k == keys::O => {
                                // oi: bói, hói; oa: hoá, toá
                                // oe: NOT included here - handled separately via English dict check
                                // Words like "lore", "bore", "core" are common English
                                next_key == keys::I || next_key == keys::A
                            }
                            k if k == keys::E => {
                                // eo: đeo, kẹo, mèo
                                // eu: nếu, kêu (êu diphthong with tone on ê)
                                next_key == keys::O || next_key == keys::U
                            }
                            k if k == keys::I => {
                                // iu: chịu, nịu, lịu; ia: mía, kia, chia, tía
                                next_key == keys::U || next_key == keys::A
                            }
                            _ => false,
                        };
                        if !is_vietnamese_pattern {
                            // Special case: O + modifier + E
                            // Vietnamese-first for specific patterns, English otherwise
                            // "chose" → "choé" (CH is Vietnamese digraph, keep VN)
                            // "lore", "bore" → English (single initial + raw in EN dict)
                            // "xofe", "hofe" → Vietnamese (raw not in EN dict)
                            if prev_vowel == keys::O && next_key == keys::E {
                                // Check for Vietnamese digraph initials (CH, KH, GH, TH, PH, NH, NG, TR)
                                // These are Vietnamese-specific, so keep Vietnamese for OE words
                                let has_vn_digraph = if self.raw_input.len() >= 2 {
                                    let (c1, _, _) = self.raw_input[0];
                                    let (c2, _, _) = self.raw_input[1];
                                    // Digraphs ending with H: CH, KH, GH, TH, PH, NH
                                    let ends_with_h = c2 == keys::H
                                        && matches!(
                                            c1,
                                            keys::C
                                                | keys::K
                                                | keys::G
                                                | keys::T
                                                | keys::P
                                                | keys::N
                                        );
                                    // Other digraphs: NG, TR
                                    let is_ng = c1 == keys::N && c2 == keys::G;
                                    let is_tr = c1 == keys::T && c2 == keys::R;
                                    ends_with_h || is_ng || is_tr
                                } else {
                                    false
                                };

                                if has_vn_digraph {
                                    // Vietnamese digraph + OE → keep Vietnamese
                                    continue;
                                }

                                // For single initial + OE: only restore if raw is English word
                                let raw_str: String = self
                                    .raw_input
                                    .iter()
                                    .filter_map(|&(k, c, s)| utils::key_to_char_ext(k, c, s))
                                    .collect();
                                if !english_dict::is_english_word(&raw_str) {
                                    // Not a common English word, keep Vietnamese
                                    continue;
                                }
                            }

                            // Special case: U + modifier + E (similar to OE handling)
                            // Vietnamese-first for specific patterns, English otherwise
                            // "huse" → "hué" (H + UE is valid Vietnamese pattern)
                            // "cure", "pure", "sure" → English (common English words)
                            // "xuse", "zuse" → Vietnamese (raw not in EN dict)
                            if prev_vowel == keys::U && next_key == keys::E {
                                // Check for Vietnamese digraph initials (QU is special for UE)
                                let has_vn_digraph = if self.raw_input.len() >= 2 {
                                    let (c1, _, _) = self.raw_input[0];
                                    let (c2, _, _) = self.raw_input[1];
                                    // QU + E pattern: quế, qué (valid Vietnamese)
                                    let is_qu = c1 == keys::Q && c2 == keys::U;
                                    // Digraphs ending with H: CH, KH, GH, TH, PH, NH
                                    let ends_with_h = c2 == keys::H
                                        && matches!(
                                            c1,
                                            keys::C
                                                | keys::K
                                                | keys::G
                                                | keys::T
                                                | keys::P
                                                | keys::N
                                        );
                                    // Other digraphs: NG, TR
                                    let is_ng = c1 == keys::N && c2 == keys::G;
                                    let is_tr = c1 == keys::T && c2 == keys::R;
                                    is_qu || ends_with_h || is_ng || is_tr
                                } else {
                                    false
                                };

                                if has_vn_digraph {
                                    // Vietnamese digraph + UE → keep Vietnamese
                                    continue;
                                }

                                // For single initial + UE: only restore if raw is English word
                                let raw_str: String = self
                                    .raw_input
                                    .iter()
                                    .filter_map(|&(k, c, s)| utils::key_to_char_ext(k, c, s))
                                    .collect();
                                if !english_dict::is_english_word(&raw_str) {
                                    // Not a common English word, keep Vietnamese
                                    continue;
                                }
                            }
                            return true;
                        }
                    }
                }
            }
        }

        // Pattern 5: W at end after vowel → English (like "raw", "law", "saw", "view")
        // W as final is not valid Vietnamese, it's an English pattern
        // Exception: "uw" ending is Vietnamese (tuw → tư)
        // Exception: "ow" ending is Vietnamese (cow → cơ)
        // Exception: W modified a diphthong (oiw → ơi where OI is diphthong, W adds horn to O)
        if self.raw_input.len() >= 2 {
            let (last, _, _) = self.raw_input[self.raw_input.len() - 1];
            if last == keys::W {
                let (second_last, _, _) = self.raw_input[self.raw_input.len() - 2];
                // W after vowel (not U or O) at end is English: raw, law, saw
                // W after U is Vietnamese: tuw → tư
                // W after O is Vietnamese: cow → cơ
                if keys::is_vowel(second_last) && second_last != keys::U && second_last != keys::O {
                    // Check if W was absorbed (modified existing vowel vs created new ư)
                    // "oiw" → "ơi": 3 chars → 2 chars (absorbed)
                    // "view" → "vieư": 4 chars → 4 chars (not absorbed)
                    let w_was_absorbed = self.buf.len() < self.raw_input.len();

                    // Count vowels before W in raw_input
                    let vowel_count = self.raw_input[..self.raw_input.len() - 1]
                        .iter()
                        .filter(|(k, _, _)| keys::is_vowel(*k))
                        .count();

                    // Only skip restore if BOTH conditions are true:
                    // 1. W was absorbed (actually modified an existing vowel)
                    // 2. There are 2+ vowels before W (diphthong like OI in "oiw")
                    // Otherwise, this is likely English (bow, view) - restore
                    if !(w_was_absorbed && vowel_count >= 2) {
                        return true;
                    }
                }
            }
        }

        // Pattern 6: Double vowel (oo, aa, ee) followed by K → English
        // Vietnamese uses single vowel + breve + K (đắk = aw+k)
        // English uses double vowel + K (looks, took, book)
        // This distinguishes "looks" (English) from "đắk" (Vietnamese)
        if self.raw_input.len() >= 3 {
            for i in 0..self.raw_input.len() - 2 {
                let (v1, _, _) = self.raw_input[i];
                let (v2, _, _) = self.raw_input[i + 1];
                let (next, _, _) = self.raw_input[i + 2];

                // Check for double vowel (same vowel twice) followed by K
                if keys::is_vowel(v1) && v1 == v2 && next == keys::K {
                    return true;
                }
            }
        }

        // Pattern 6a: Double E (ee) followed by P at END → English (keep, deep, sleep, seep)
        // Only EE+P, not AA+P or OO+P which can be valid Vietnamese (cấp = caaps)
        // ONLY check at word boundary - mid-word "kêp" could still become valid Vietnamese
        // Exceptions:
        //   - I+EE+P is Vietnamese "iệp" pattern (nghiệp, hiệp, kiệp, v.v.)
        //   - X+EE+P is Vietnamese "xếp" pattern (xếp = to arrange)
        if is_word_complete && self.raw_input.len() >= 3 {
            let len = self.raw_input.len();
            let (last, _, _) = self.raw_input[len - 1];
            if last == keys::P {
                let (v1, _, _) = self.raw_input[len - 3];
                let (v2, _, _) = self.raw_input[len - 2];
                // Only match EE (not AA or OO)
                if v1 == keys::E && v2 == keys::E {
                    // Exception: I+EE+P or X+EE+P are Vietnamese patterns
                    // Check if there's an I or X before the double E
                    if len >= 4 {
                        let (before_ee, _, _) = self.raw_input[len - 4];
                        if before_ee == keys::I || before_ee == keys::X {
                            // This is Vietnamese "iêp" or "xêp" pattern, don't restore
                            // Continue to check other patterns
                        } else {
                            return true;
                        }
                    } else {
                        return true;
                    }
                }
            }
        }

        // Pattern 6b: Double vowel (aa, ee, oo) followed by tone modifier at end → English
        // ONLY when initial is rare in Vietnamese (S alone, F alone)
        // Example: "saas" = s + aa + s → S initial + double 'a' + tone modifier 's' → SaaS pattern
        // Example: "saax" = s + aa + x → S initial + double 'a' + tone modifier 'x' → English
        // Counter-example: "leex" = l + ee + x → L is common Vietnamese initial → keep "lễ"
        // Counter-example: "meex" = m + ee + x → M is common Vietnamese initial → keep "mễ"
        // Counter-example: "soos" = s + oo + s → "số" (Vietnamese for "number")
        // Counter-example: "seef" = s + ee + f → "sề" (valid Vietnamese word)
        let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];
        if self.raw_input.len() >= 4 {
            let (first, _, _) = self.raw_input[0];
            let (last, _, _) = self.raw_input[self.raw_input.len() - 1];
            // Only match if initial is S or F (rare alone in Vietnamese)
            // S alone (not SH) and F are English patterns
            if (first == keys::S || first == keys::F) && tone_modifiers.contains(&last) {
                // Check for double vowel just before the last key
                let (v1, _, _) = self.raw_input[self.raw_input.len() - 3];
                let (v2, _, _) = self.raw_input[self.raw_input.len() - 2];
                if keys::is_vowel(v1) && v1 == v2 {
                    // Exception: S/F + OO/EE + modifier → Vietnamese
                    // - số, sở, sỗ, sổ (number-related words)
                    // - sề, sể, sễ, sệ (valid Vietnamese words)
                    // S/F + AA + modifier → English (SaaS, FaaS patterns)
                    if v1 != keys::O && v1 != keys::E {
                        return true;
                    }
                }
            }
        }

        // Pattern 6c: S + A + X pattern → English "sax" (saxophone)
        // Only match "sax" specifically, not "six" (which is Vietnamese "sĩ")
        // "sax" = s + a + x → "sã" but should restore to "sax"
        // "six" = s + i + x → "sĩ" (valid Vietnamese: soldier, scholar)
        if self.raw_input.len() == 3 {
            let (first, _, _) = self.raw_input[0];
            let (second, _, _) = self.raw_input[1];
            let (third, _, _) = self.raw_input[2];
            // Only S + A + X (not other vowels)
            if first == keys::S && second == keys::A && third == keys::X {
                return true;
            }
        }

        // Pattern 7: C + V + tone_modifier + double_vowel → partial English restore
        // Example: "tafoo" = t + a + f + o + o → restore to "tàoo"
        // Example: "mufaa" = m + u + f + a + a → restore to "mùaa"
        // This pattern detects when someone types like "tattoo" with Vietnamese tone
        if self.raw_input.len() == 5 {
            let (c0, _, _) = self.raw_input[0];
            let (c1, _, _) = self.raw_input[1];
            let (c2, _, _) = self.raw_input[2];
            let (c3, _, _) = self.raw_input[3];
            let (c4, _, _) = self.raw_input[4];

            let is_consonant_0 = keys::is_consonant(c0);
            let is_vowel_1 = keys::is_vowel(c1);
            let is_tone_2 = matches!(c2, keys::S | keys::F | keys::R | keys::X | keys::J);
            let is_circumflex_vowel_34 = matches!(c3, keys::A | keys::E | keys::O) && c3 == c4;

            if is_consonant_0 && is_vowel_1 && is_tone_2 && is_circumflex_vowel_34 {
                return true;
            }
        }

        // Pattern 8: tone_modifier + K at end → English (risk, disk, task, mask)
        // K as final is only valid in Vietnamese with breve vowels (Đắk Lắk ethnic minority words)
        // or other ethnic minority patterns like "Búk"
        // Example: "risk" = r + i + s + k → should restore to "risk" (s NOT consumed)
        // Counter-example: "đắk" = dd + aw + k → "đắk" (breve 'ắ', valid Vietnamese)
        // Counter-example: "Busk" = B + u + s + k → "Búk" (s consumed as sắc, valid Vietnamese)
        if self.raw_input.len() >= 4 {
            let (last, _, _) = self.raw_input[self.raw_input.len() - 1];
            if last == keys::K {
                let (second_last, _, _) = self.raw_input[self.raw_input.len() - 2];
                let tone_modifiers = [keys::S, keys::F, keys::R, keys::X, keys::J];
                // Check if second_last is a tone modifier (s, f, r, x, j)
                if tone_modifiers.contains(&second_last) {
                    // Key insight: if modifier was consumed (applied to vowel),
                    // buf.len() < raw_input.len() → Vietnamese
                    // If modifier was NOT consumed (stayed as letter),
                    // buf.len() == raw_input.len() → English
                    // Example: "Busk" → "Búk" (4 chars → 3 chars, s consumed)
                    // Example: "risk" → "rík" (4 chars → 3 chars, s consumed)
                    // Both have s consumed, so we need another check...

                    // Vietnamese ethnic minority words have breve: ắ, ẳ, ẵ (from 'aw')
                    // Check if there's a 'w' in raw_input before the modifier (indicating breve)
                    let has_breve_marker = self.raw_input[..self.raw_input.len() - 2]
                        .iter()
                        .any(|(k, _, _)| *k == keys::W);

                    // Also check for common English -Vsk patterns where V is i, a, e, o, u
                    // but NOT ethnic minority patterns
                    // The key difference: ethnic minority words are usually short (3-4 letters)
                    // and have specific structures. English -sk words often have more consonants.
                    let (third_last, _, _) = self.raw_input[self.raw_input.len() - 3];
                    let is_isk_ask_pattern = keys::is_vowel(third_last)
                        && second_last == keys::S
                        && !has_breve_marker
                        && self.raw_input.len() >= 4;

                    // Only restore if it's a common English -Vsk pattern (V+s+k)
                    // AND there's no breve marker (aw pattern)
                    // AND word has at least one consonant before the vowel (like r-i-s-k, d-i-s-k)
                    if is_isk_ask_pattern {
                        // Check if there's a consonant initial before the vowel
                        let has_consonant_before_vowel =
                            self.raw_input.len() >= 4 && keys::is_consonant(self.raw_input[0].0);

                        // For short words (4 chars like "risk", "disk", "task"),
                        // only restore if initial is a common English consonant pattern
                        if has_consonant_before_vowel {
                            // Skip restore for ethnic minority initials that commonly use K final
                            // B, L are common in Vietnamese ethnic minority words (Búk, Lắk)
                            // Note: Đắk uses DD (double D) for Đ, not single D
                            // So D initial (disk, desk, dusk) should restore as English
                            let (first, _, _) = self.raw_input[0];
                            let is_ethnic_initial = first == keys::B || first == keys::L;

                            if !is_ethnic_initial {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Pattern 9: C + V + M + S at end → English plural pattern (-ms)
        // Example: "sims" = s + i + m + s → English (The Sims, rims)
        // Example: "gems" = g + e + m + s → English plural
        // Counter-example: "dims" = d + i + m + s → "dím" is valid Vietnamese (to press/push down)
        // Counter-example: "sems" = s + e + m + s → "sém" is valid Vietnamese (burnt/scorched)
        // Counter-example: "hems" = h + e + m + s → "hẻm" is valid Vietnamese (alley)
        //
        // KEY INSIGHT: If buffer has a TONE MARK applied, the 's' was consumed as Vietnamese tone
        // modifier (sắc), not as English plural suffix. Keep Vietnamese in this case.
        // Words that DON'T get tone mark applied are true English plurals.
        if self.raw_input.len() == 4 {
            let (c0, _, _) = self.raw_input[0];
            let (c1, _, _) = self.raw_input[1];
            let (c2, _, _) = self.raw_input[2];
            let (c3, _, _) = self.raw_input[3];

            // Pattern: single consonant + i/e + m + s (tone modifier)
            if keys::is_consonant(c0)
                && (c1 == keys::I || c1 == keys::E)
                && c2 == keys::M
                && c3 == keys::S
            {
                // Check if this C+ÍM/ÉM combination is a real Vietnamese word.
                // Vietnamese dictionary 22k contains these C+ÍM/ÉM words:
                // - *ím: bím, dím, mím, tím (initials: B, D, M, T)
                // - *ém: kém, lém, ném, sém, tém (initials: K, L, N, S, T)
                // English plurals: sims, rims, gems, hems (NOT Vietnamese words)
                // Vietnamese CÍM/CÉM words (with sắc tone):
                // - *ím: bím, dím, mím, tím (initials: B, D, M, T)
                // - *ém: kém, lém, mém, ném, sém, tém (initials: K, L, M, N, S, T)
                let is_vietnamese_cim_word = (c1 == keys::I
                    && matches!(c0, keys::B | keys::D | keys::M | keys::T))
                    || (c1 == keys::E
                        && matches!(
                            c0,
                            keys::K | keys::L | keys::M | keys::N | keys::S | keys::T
                        ));

                if !is_vietnamese_cim_word {
                    // Not a known Vietnamese word → English plural pattern
                    return true;
                }
                // Vietnamese word → don't trigger English pattern
            }
        }

        false
    }

    /// Auto-restore invalid Vietnamese to raw English on space
    ///
    /// Called when SPACE is pressed. If buffer has transforms but result is not
    /// valid Vietnamese, restore to original English + space.
    /// Example: "tẽt" (from typing "text") → "text " (restored + space)
    /// Example: "ễpct" (from typing "expect") → "expect " (restored + space)
    pub(super) fn try_auto_restore_on_space(&self) -> Result {
        if let Some(mut raw_chars) = self.should_auto_restore(true) {
            // Add space at the end
            raw_chars.push(' ');
            // Backspace count = current buffer length (displayed chars)
            let backspace = self.buf.len() as u8;
            Result::send(backspace, &raw_chars)
        } else {
            Result::none()
        }
    }

    /// Auto-restore invalid Vietnamese to raw English on break key
    ///
    /// Called when punctuation/break key is pressed. If buffer has transforms
    /// but result is not valid Vietnamese, restore to original English.
    /// Does NOT include the break key (it's passed through by the app).
    /// Example: "ễpct" + comma → "expect" (comma added by app)
    pub(super) fn try_auto_restore_on_break(&self) -> Result {
        if let Some(raw_chars) = self.should_auto_restore(true) {
            // Backspace count = current buffer length (displayed chars)
            let backspace = self.buf.len() as u8;
            Result::send(backspace, &raw_chars)
        } else {
            Result::none()
        }
    }

    /// Restore buffer to raw ASCII (undo all Vietnamese transforms)
    ///
    /// Called when ESC is pressed. Replaces transformed output with original keystrokes.
    /// Example: "tẽt" (from typing "text" in Telex) → "text"
    /// Example: "of" → "ò" → ESC → "of" (mark was applied)
    /// Example: "off" → "of" → ESC → "off" (mark was applied then reverted)
    pub(super) fn restore_to_raw(&self) -> Result {
        if self.raw_input.is_empty() || self.buf.is_empty() {
            return Result::none();
        }

        // Build raw ASCII output from raw_input history
        // If telex_double_raw is set (revert happened), use it as base and append subsequent chars
        // This ensures "aww" → ESC → "aww" (not "aw"), "a66" → ESC → "a66" (not "a6")
        let raw_chars: Vec<char> = if let Some(ref base_raw) = self.telex_double_raw {
            // Start with the original raw string before revert modification
            let mut chars: Vec<char> = base_raw.chars().collect();
            // Append any characters typed after the revert
            for &(key, caps, shift) in self.raw_input.iter().skip(self.telex_double_raw_len) {
                if let Some(ch) = utils::key_to_char_ext(key, caps, shift) {
                    chars.push(ch);
                }
            }
            chars
        } else {
            // Normal case: use raw_input directly
            self.raw_input
                .iter()
                .filter_map(|&(key, caps, shift)| utils::key_to_char_ext(key, caps, shift))
                .collect()
        };

        if raw_chars.is_empty() {
            return Result::none();
        }

        // Get current buffer content for comparison
        let buffer_str = self.buf.to_full_string();
        let raw_str: String = raw_chars.iter().collect();

        // Only restore if:
        // 1. Any transform was ever applied (even if later reverted), OR
        // 2. Buffer differs from raw input (handles edge cases)
        if !self.had_any_transform && buffer_str == raw_str {
            return Result::none();
        }

        // Backspace count = current buffer length (displayed chars)
        let backspace = self.buf.len() as u8;

        Result::send(backspace, &raw_chars)
    }

    /// Restore raw_input from buffer (for ESC restore to work after backspace-restore)
    pub(super) fn restore_raw_input_from_buffer(&mut self, buf: &Buffer) {
        self.raw_input.clear();
        for c in buf.iter() {
            self.raw_input.push((c.key, c.caps, false));
        }
    }
}
