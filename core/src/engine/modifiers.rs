//! Modifier dispatch (stroke/tone/mark/remove, w-as-vowel, bracket-as-vowel)
//! for the Vietnamese input engine (moved out of mod.rs), plus their
//! revert/gate helpers.

use super::validation::{
    is_foreign_word_pattern, is_valid, is_valid_for_transform_with_foreign, is_valid_with_foreign,
    is_valid_with_tones,
};
use super::{syllable, Char, Engine, Result, Transform};
use crate::data::{
    chars::{self, mark, tone},
    constants, english_dict, keys,
    vowel::{Phonology, Vowel},
};
use crate::input::ToneType;
use crate::utils;

impl Engine {
    /// Try "w" as vowel "ư" in Telex mode
    ///
    /// Rules:
    /// - "w" alone → "ư"
    /// - "nhw" → "như" (valid consonant + ư)
    /// - "kw" → "kw" (invalid, k cannot precede ư)
    /// - "ww" → revert to "w" (shortcut skipped)
    /// - "www" → "ww" (subsequent w just adds normally)
    pub(super) fn try_w_as_vowel(&mut self, caps: bool) -> Option<Result> {
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
    pub(super) fn try_stroke(&mut self, key: u16, caps: bool) -> Option<Result> {
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
    pub(super) fn try_tone(
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
    pub(super) fn try_mark(&mut self, key: u16, caps: bool, mark_val: u8) -> Option<Result> {
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
    pub(super) fn normalize_uo_compound(&mut self) -> Option<usize> {
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
    pub(super) fn has_complete_uo_compound(&self) -> bool {
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
    pub(super) fn reposition_tone_if_needed(&mut self) -> Option<(usize, usize)> {
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
    pub(super) fn reorder_diphthong_with_final(&mut self) -> Option<usize> {
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
    pub(super) fn try_remove(&mut self) -> Option<Result> {
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

    /// Check if buffer has w-as-vowel transform (standalone w→ư at start)
    /// This is different from w-as-tone which adds horn to existing vowels
    pub(super) fn has_w_as_vowel_transform(&self) -> bool {
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
    pub(super) fn revert_w_as_vowel_transforms(&mut self) -> Result {
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

    /// Try to convert bracket key to vowel: ] → ư, [ → ơ (Issue #159)
    ///
    /// Returns Some(Result) if bracket was converted, None otherwise.
    /// Handles:
    /// - ] at word start or after consonant → ư
    /// - [ at word start or after consonant → ơ
    /// - Double bracket reverts: ]] → ], [[ → [, uppercase revert → } or {
    /// - Valid Vietnamese vowel combinations: ươ (from ][)
    pub(super) fn try_bracket_as_vowel(&mut self, key: u16, caps: bool) -> Option<Result> {
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
