//! Inline unit tests for the Vietnamese input engine (moved out of mod.rs).

use super::Engine;
use crate::utils::{telex, type_word, vni};

const TELEX_BASIC: &[(&str, &str)] = &[
    ("as", "á"),
    ("af", "à"),
    ("ar", "ả"),
    ("ax", "ã"),
    ("aj", "ạ"),
    ("aa", "â"),
    // Issue #44: Breve deferred in open syllable until final consonant or mark
    ("aw", "ă"),   // standalone aw → ă
    ("awm", "ăm"), // breve applied when final consonant typed
    ("aws", "ắ"),  // breve applied when mark typed
    ("ee", "ê"),
    ("oo", "ô"),
    ("ow", "ơ"),
    ("uw", "ư"),
    ("dd", "đ"),
    // Mark after consonant
    ("tex", "tẽ"), // t + e + x(ngã) → tẽ
    ("ver", "vẻ"), // v + e + r(hỏi) → vẻ (test for #issue)
    // Post-tone delayed circumflex: o + n + r(hỏi) + o(circumflex) → ổn
    ("onro", "ổn"),
    // ===== Invalid diphthong blocking =====
    // When vowel combination is NOT valid Vietnamese diphthong,
    // same-vowel circumflex should NOT be triggered
    // Pattern: initial + V1 + V2(invalid) + consonant + V1
    ("coupo", "coupo"), // [O,U] invalid diphthong → stays raw
    ("soupo", "soupo"), // [O,U] invalid → stays raw
    ("beapa", "beapa"), // [E,A] invalid → stays raw
    ("beipi", "beipi"), // [E,I] invalid → stays raw
    ("daupa", "daupa"), // [A,U] valid diphthong but "aup" invalid syllable → stays raw
    ("boemo", "boemo"), // [O,E] valid diphthong but "oem" invalid syllable → stays raw
];

const VNI_BASIC: &[(&str, &str)] = &[
    ("a1", "á"),
    ("a2", "à"),
    ("a3", "ả"),
    ("a4", "ã"),
    ("a5", "ạ"),
    ("a6", "â"),
    // Issue #44: Breve deferred in open syllable until final consonant or mark
    ("a8", "ă"),   // standalone a8 → ă
    ("a8m", "ăm"), // breve applied when final consonant typed
    ("a81", "ắ"),  // breve applied when mark typed
    ("e6", "ê"),
    ("o6", "ô"),
    ("o7", "ơ"),
    ("u7", "ư"),
    ("d9", "đ"),
];

const TELEX_COMPOUND: &[(&str, &str)] =
    &[("duocw", "dươc"), ("nguoiw", "ngươi"), ("tuoiws", "tưới")];

// ESC restore test cases: input with ESC (\x1b) → expected raw ASCII
// ESC restores to exactly what user typed (including modifier keys)
const TELEX_ESC_RESTORE: &[(&str, &str)] = &[
    ("text\x1b", "text"),     // tẽt → text
    ("user\x1b", "user"),     // úẻ → user
    ("esc\x1b", "esc"),       // éc → esc
    ("dd\x1b", "dd"),         // đ → dd (stroke restore)
    ("vieejt\x1b", "vieejt"), // việt → vieejt (all typed keys)
    ("Vieejt\x1b", "Vieejt"), // Việt → Vieejt (preserve case)
    // Mark revert cases: second modifier reverts, ESC should restore full raw input
    ("of\x1b", "of"),   // ò → of (mark applied)
    ("off\x1b", "off"), // of → off (mark applied then reverted by 2nd f)
    ("ass\x1b", "ass"), // as → ass (mark applied then reverted by 2nd s)
    ("arr\x1b", "arr"), // ar → arr (mark applied then reverted by 2nd r)
    ("axx\x1b", "axx"), // ax → axx (mark applied then reverted by 2nd x)
    ("ajj\x1b", "ajj"), // aj → ajj (mark applied then reverted by 2nd j)
    // More complex mark revert cases
    ("bass\x1b", "bass"), // bás → bas → bass
    ("boss\x1b", "boss"), // bòs → bos → boss
    ("buff\x1b", "buff"), // bùf → buf → buff
    ("diff\x1b", "diff"), // dìf → dif → diff
    ("miss\x1b", "miss"), // mìs → mis → miss
    ("pass\x1b", "pass"), // pás → pas → pass
    ("jazz\x1b", "jazz"), // jaz → jazz (no mark on a, j only at start)
    // Tone mark cases
    ("too\x1b", "too"), // tô → to → too (circumflex applied then reverted)
    ("see\x1b", "see"), // sê → se → see
    ("bee\x1b", "bee"), // bê → be → bee
];

const VNI_ESC_RESTORE: &[(&str, &str)] = &[
    ("a1\x1b", "a1"),         // á → a1
    ("vie65t\x1b", "vie65t"), // việt → vie65t
    ("d9\x1b", "d9"),         // đ → d9
    // Mark revert cases in VNI mode
    ("a11\x1b", "a11"), // á → a → a11 (mark applied then reverted by 2nd 1)
    ("a22\x1b", "a22"), // à → a → a22 (huyền reverted)
    ("a33\x1b", "a33"), // ả → a → a33 (hỏi reverted)
    ("a44\x1b", "a44"), // ã → a → a44 (ngã reverted)
    ("a55\x1b", "a55"), // ạ → a → a55 (nặng reverted)
    ("a66\x1b", "a66"), // â → a → a66 (circumflex reverted)
];

// Normal Vietnamese transforms apply
const TELEX_NORMAL: &[(&str, &str)] = &[
    ("gox", "gõ"),      // Without prefix: "gox" → "gõ"
    ("tas", "tá"),      // Without prefix: Vietnamese transforms (s adds sắc)
    ("vieejt", "việt"), // Normal Vietnamese typing
];

#[test]
fn test_telex_basic() {
    telex(TELEX_BASIC);
}

#[test]
fn test_vni_basic() {
    vni(VNI_BASIC);
}

#[test]
fn test_telex_compound() {
    telex(TELEX_COMPOUND);
}

#[test]
fn test_telex_esc_restore() {
    // ESC restore is disabled by default, enable it for this test
    for (input, expected) in TELEX_ESC_RESTORE {
        let mut e = Engine::new();
        e.set_esc_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(result, *expected, "[Telex] '{}' → '{}'", input, result);
    }
}

#[test]
fn test_vni_esc_restore() {
    // ESC restore is disabled by default, enable it for this test
    for (input, expected) in VNI_ESC_RESTORE {
        let mut e = Engine::new();
        e.set_method(1);
        e.set_esc_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(result, *expected, "[VNI] '{}' → '{}'", input, result);
    }
}

#[test]
fn test_telex_normal() {
    telex(TELEX_NORMAL);
}

#[test]
fn test_nurses_horses_esc_restore() {
    // Debug test for nurses/horses ESC restore
    let cases: &[(&str, &str)] = &[("nurses\x1b", "nurses"), ("horses\x1b", "horses")];
    for (input, expected) in cases {
        let mut e = Engine::new();
        e.set_esc_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(result, *expected, "[ESC] '{}' → '{}'", input, result);
    }
}

#[test]
fn test_nurses_horses_auto_restore() {
    // Test patterns where multiple modifiers are consumed but not added to buffer
    // These patterns have: mark1 + mark2 + vowel + revert_of_mark2
    // Example: "nurses" = n-u-r(hỏi)-s(sắc replaces hỏi)-e-s(reverts sắc)
    // Buffer becomes "nues" but raw_input has "nurses" which should restore
    let cases: &[(&str, &str)] = &[
        // Pattern: C-V-mark1-mark2-V-revert
        ("nurses ", "nurses "),
        ("horses ", "horses "),
        ("verses ", "verses "),
        ("curses ", "curses "),
        ("purses ", "purses "),
        // Pattern: C-V-mark1-mark2-V-C-revert (different ending)
        ("nursest ", "nursest "), // Additional consonant before revert
        // Pattern with different marks: r(hỏi)-f(huyền) then f reverts
        ("surfed ", "surfed "), // s-u-r(hỏi)-f(huyền replaces)-e-d → buffer "sued", raw "surfed"
        // ESC should work the same
        ("nurses\x1b", "nurses"),
        ("horses\x1b", "horses"),
        ("verses\x1b", "verses"),
    ];
    for (input, expected) in cases {
        let mut e = Engine::new();
        e.set_english_auto_restore(true);
        e.set_esc_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(result, *expected, "[Auto] '{}' → '{}'", input, result);
    }
}

// =========================================================================
// AUTO-RESTORE TESTS
// Test space-triggered auto-restore for all Telex modifiers (s/f/r/x/j)
// When user types double modifier to revert, then continues typing,
// pressing space should restore to the buffer form (with revert applied)
// =========================================================================

// =========================================================================
// AUTO-RESTORE TESTS for double modifier patterns
//
// Generic check handles: double 'r', 'x', 'j' with EXACTLY 2 chars after
// Double 's' is handled by existing specific check (5 chars raw, 4 chars buf)
// Double 'f' has too many legitimate English words (effect, different, etc.)
//
// Constraint: suffix after double must be exactly 2 chars (V+C pattern)
// This avoids false positives like "current" (suffix "ent" = 3 chars)
// =========================================================================

// Auto-restore with double 'r' (hỏi mark)
// Pattern: double 'r' + exactly 2 chars (V+C)
const TELEX_AUTO_RESTORE_R: &[(&str, &str)] = &[
    ("sarrah ", "sarah "), // s-a-rr-a-h: suffix "ah" = 2 chars ✓
    ("barrut ", "barut "), // b-a-rr-u-t: suffix "ut" = 2 chars ✓
    ("tarrep ", "tarep "), // t-a-rr-e-p: suffix "ep" = 2 chars ✓
];

// Auto-restore with double 'x' (ngã mark)
// Pattern: double 'x' + exactly 2 chars
const TELEX_AUTO_RESTORE_X: &[(&str, &str)] = &[
    ("maxxat ", "maxat "), // m-a-xx-a-t: suffix "at" = 2 chars ✓
    ("texxup ", "texup "), // t-e-xx-u-p: suffix "up" = 2 chars ✓
];

// Auto-restore with double 'j' (nặng mark)
// Pattern: double 'j' + exactly 2 chars
const TELEX_AUTO_RESTORE_J: &[(&str, &str)] = &[
    ("majjam ", "majam "), // m-a-jj-a-m: suffix "am" = 2 chars ✓
    ("bajjut ", "bajut "), // b-a-jj-u-t: suffix "ut" = 2 chars ✓
];

#[test]
fn test_auto_restore_double_r() {
    for (input, expected) in TELEX_AUTO_RESTORE_R {
        let mut e = Engine::new();
        e.set_english_auto_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Auto-restore R] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

#[test]
fn test_auto_restore_double_x() {
    for (input, expected) in TELEX_AUTO_RESTORE_X {
        let mut e = Engine::new();
        e.set_english_auto_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Auto-restore X] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

#[test]
fn test_auto_restore_double_j() {
    for (input, expected) in TELEX_AUTO_RESTORE_J {
        let mut e = Engine::new();
        e.set_english_auto_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Auto-restore J] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

/// Issue: Typing "DDD" with shift/capslock held → should produce "DD", not "Dd"
/// When stroke is reverted (ddd → dd), the added 'd' must preserve caps state
#[test]
fn test_stroke_revert_preserves_caps() {
    // Uppercase: DDD → Đ → DD (both chars uppercase)
    let cases: &[(&str, &str)] = &[
        ("DDD", "DD"),   // caps: D→D, DD→Đ, DDD→DD (both uppercase)
        ("ddd", "dd"),   // no caps: d→d, dd→đ, ddd→dd
        ("DDd", "Dd"),   // mixed: first two caps, third lowercase → Dd
        ("ddD", "dD"),   // mixed: first two lowercase, third caps → dD
        ("DDDD", "DDD"), // 4 D's: after revert, stroke_reverted=true, 4th D added
        ("dddd", "ddd"), // 4 d's: after revert, stroke_reverted=true, 4th d added
    ];

    for (input, expected) in cases {
        let mut e = Engine::new();
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Stroke caps] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

/// Comprehensive tests for Vietnamese diphthong patterns with interleaved tone marks.
/// Pattern: C + V1 + tone + V2 (e.g., "misa" = m + i + s + a → mía)
/// These patterns should NOT trigger auto-restore because they are valid Vietnamese.
const TELEX_INTERLEAVED_DIPHTHONG: &[(&str, &str)] = &[
    // IA diphthong: mía, kia, chia, tía
    ("misa ", "mía "), // m + i + s(sắc) + a → mía
    ("kisa ", "kía "), // k + i + s + a → kía
    ("lisa ", "lía "), // l + i + s + a → lía
    ("tifa ", "tìa "), // t + i + f(huyền) + a → tìa
    ("nira ", "nỉa "), // n + i + r(hỏi) + a → nỉa
    // UA diphthong: múa, lúa, cúa
    ("musa ", "múa "), // m + u + s + a → múa
    ("lusa ", "lúa "), // l + u + s + a → lúa
    ("cufa ", "cùa "), // c + u + f + a → cùa
    // UE diphthong: huế, tuệ, quế (valid Vietnamese, tone on E)
    ("huse ", "hué "), // h + u + s + e → hué (tone moves to E)
    ("tuse ", "tué "), // t + u + s + e → tué (tone on E)
    // OA diphthong: hoá, toá (tone on A, second vowel)
    ("hosa ", "hoá "), // h + o + s + a → hoá (tone on A)
    ("tosa ", "toá "), // t + o + s + a → toá (tone on A)
    ("lora ", "loả "), // l + o + r + a → loả (tone on A)
    // AI diphthong: mái, lái, tái
    ("masi ", "mái "), // m + a + s + i → mái
    ("lasi ", "lái "), // l + a + s + i → lái
    ("tafi ", "tài "), // t + a + f + i → tài
    // AO diphthong: cáo, náo, báo
    ("caso ", "cáo "), // c + a + s + o → cáo
    ("naso ", "náo "), // n + a + s + o → náo
    ("bafo ", "bào "), // b + a + f + o → bào
    // AU diphthong: máu, láu, càu
    ("masu ", "máu "), // m + a + s + u → máu
    ("lasu ", "láu "), // l + a + s + u → láu
    ("cafu ", "càu "), // c + a + f + u → càu (huyền, not circumflex)
    // AY diphthong: máy, cáy, lấy
    ("masy ", "máy "), // m + a + s + y → máy
    ("casy ", "cáy "), // c + a + s + y → cáy
    ("lafy ", "lày "), // l + a + f + y → lày
    // OI diphthong: bói, hói, đói
    ("bosi ", "bói "), // b + o + s + i → bói
    ("hofi ", "hòi "), // h + o + f + i → hòi
    ("lori ", "lỏi "), // l + o + r + i → lỏi
    // UI diphthong: núi, cúi, tủi
    ("nusi ", "núi "), // n + u + s + i → núi
    ("cufi ", "cùi "), // c + u + f + i → cùi
    ("turi ", "tủi "), // t + u + r + i → tủi
    // IU diphthong: chịu, nịu, lịu
    ("chiju ", "chịu "), // ch + i + j(nặng) + u → chịu
    ("niju ", "nịu "),   // n + i + j + u → nịu
    ("lisu ", "líu "),   // l + i + s + u → líu
    // EO diphthong: méo, kẹo, bèo
    ("meso ", "méo "), // m + e + s + o → méo
    ("kejo ", "kẹo "), // k + e + j + o → kẹo
    ("befo ", "bèo "), // b + e + f + o → bèo
];

#[test]
fn test_interleaved_diphthong_auto_restore() {
    for (input, expected) in TELEX_INTERLEAVED_DIPHTHONG {
        let mut e = Engine::new();
        e.set_english_auto_restore(true);
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Interleaved diphthong] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

/// Issue #217: After typing "eee" (revert to "ee") and deleting, should be able to type "ê" again
/// Bug: reverted_circumflex_key was not reset on backspace, blocking circumflex in new words
#[test]
fn test_circumflex_revert_reset_on_backspace() {
    // Test case from issue: type "Meee", delete all, type "Phee" → should get "Phê"
    // '<' = backspace in test utilities
    let cases: &[(&str, &str)] = &[
        // After eee→ee revert, delete all, new word should work
        ("meee<<<<<phee", "phê"),
        ("ooo<<<<<choo", "chô"),
        ("aaa<<<<<caa", "câ"),
        // After revert but only partial delete, new 'e' in same buffer should also work
        ("eee<<ee", "ê"),
        // Mixed: revert one vowel, delete, type another vowel type
        ("ooo<<<<<mee", "mê"),
        ("aaa<<<<<boo", "bô"),
    ];

    for (input, expected) in cases {
        let mut e = Engine::new();
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Issue #217 circumflex reset] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

/// After typing "ooo" (revert to "oo"), "eee" (revert to "ee"), or "aaa" (revert to "aa"),
/// subsequent keys should be treated as literal (no tone/mark modification).
/// Examples:
/// - "booo" → "boo" (revert), then "s" → "boos" (not "boós")
/// - "seee" → "see" (revert), then "m" → "seem" (not "seém")
/// - "booo" + "k" → "book" (consonant also literal)
/// Note: Only works with valid Vietnamese initials (b, c, d, h, l, m, n, p, s, t, etc.)
#[test]
fn test_literal_after_circumflex_revert() {
    let cases: &[(&str, &str)] = &[
        // B-initial: mark applied when pattern ends with mark key
        ("booos", "boó"), // booo→boo + s → boó (mark applied, ends with s)
        // Auto-restore: consonant after mark → English word
        ("booost", "boost"), // booo→boo + s + t → boost (auto-restore)
        // Other patterns remain literal
        ("seeem", "seem"), // seee→see + m → seem
        ("mooos", "moos"), // mooo→moo + s → moos (M not in safe partial)
        ("beeef", "beef"), // beee→bee + f → beef (not bèef)
        ("gooox", "goox"), // gooo→goo + x → goox (x is not valid mark)
        ("leeef", "leef"), // leee→lee + f → leef (not lèef)
        // Consonant after revert → literal
        ("boook", "book"), // booo→boo + k → book
        ("seeen", "seen"), // seee→see + n → seen
        ("looox", "loox"), // looo→loo + x → loox
        // Multiple chars after revert
        ("boooks", "books"), // booo→boo + k + s → books
        ("seeend", "seend"), // seee→see + n + d → seend
        // SaaS pattern should stay as "saas"
        ("saaas", "saas"), // saaa→saa + s → saas
    ];

    for (input, expected) in cases {
        let mut e = Engine::new();
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Literal after revert] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}

/// Vietnamese triple-o words should accept tone modifiers after circumflex revert.
/// These are rare Vietnamese words with literal double-o (not circumflex):
/// - boóng, choòng, coóc, đoòng, goòng, moóc, soóc, toòng
///
/// IMPORTANT: Tone modifier must come AFTER final consonants (NG or C) for the
/// engine to recognize it as Vietnamese. This distinguishes from English words
/// like "boos", "books" where the letter after "oo" is literal.
///
/// Typing order: [initial] + ooo + [final: ng/c] + [tone: s/f]
#[test]
fn test_vietnamese_triple_o_words() {
    let cases: &[(&str, &str)] = &[
        // Vietnamese triple-o words with sắc tone (s) - final before tone
        ("booongs", "boóng"), // booo + ng + s → boóng
        ("mooocs", "moóc"),   // mooo + c + s → moóc
        ("sooocs", "soóc"),   // sooo + c + s → soóc
        ("cooocs", "coóc"),   // cooo + c + s → coóc
        // Vietnamese triple-o words with huyền tone (f) - final before tone
        ("chooongf", "choòng"), // chooo + ng + f → choòng
        ("gooongf", "goòng"),   // gooo + ng + f → goòng
        ("tooongf", "toòng"),   // tooo + ng + f → toòng
        ("booongf", "boòng"),   // booo + ng + f → boòng
        // Vietnamese triple-o words with tone BEFORE final (typing order variant)
        ("booofng", "boòng"), // booo + f + ng → boòng
        ("booosng", "boóng"), // booo + s + ng → boóng
        // đoòng with stroke (dd) + triple-o
        ("ddooongf", "đoòng"), // dd + ooo + ng + f → đoòng
        ("dooongdf", "đoòng"),
        ("dooodngf", "đoòng"),
        ("dooodfng", "đoòng"),
        ("dooofdng", "đoòng"),
        ("dooongfd", "đoòng"),
        // Alternative typing order: stroke at end
        ("dooongfd", "đoòng"), // d + ooo + ng + f + d → đoòng (stroke at end)
        // Typing order: mark BEFORE double-o (b + o + s + oo + ng)
        ("bosoong", "boóng"), // b + o + s + oo + ng → boóng (mark before oo)
        // Vowel-initial triple-o words (no consonant initial)
        ("ooosng", "oóng"), // ooo + s + ng → oóng
        ("ooofng", "oòng"), // ooo + f + ng → oòng
        ("ooongs", "oóng"), // ooo + ng + s → oóng (tone after final)
    ];

    for (input, expected) in cases {
        let mut e = Engine::new();
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "[Vietnamese triple-o] '{}' → '{}', expected '{}'",
            input, result, expected
        );
    }
}
