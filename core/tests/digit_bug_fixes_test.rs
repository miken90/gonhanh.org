//! Phase 2 digit-bug regression tests (H0/B1 fixes)
//!
//! Covers: VNI digit modifiers only fire in a markable context (buffer ends
//! in a letter), shift+digit still yields a symbol not a mark, digits after
//! a diacritic keep intentional VNI syllables working, and Issue #162's o2o
//! pattern stays correct.

mod common;
use common::vni;
use gonhanh_core::engine::Engine;
use gonhanh_core::utils::type_word;

// ============================================================
// VNI intentional digit modifiers (must keep working after the B1 fix)
// ============================================================

const VNI_INTENTIONAL_MODIFIERS: &[(&str, &str)] = &[
    ("a1", "á"), // mark: sắc
    ("a2", "à"), // mark: huyền
    ("a6", "â"), // tone: circumflex
    ("o6", "ô"),
    ("o7", "ơ"), // tone: horn
    ("u7", "ư"),
    ("d9", "đ"),   // stroke
    ("an1", "án"), // mark on a closed syllable, final consonant kept
    ("o2o", "òo"), // Issue #162: huyền on first o, second o stays plain
];

#[test]
fn vni_intentional_digit_modifiers_still_work() {
    vni(VNI_INTENTIONAL_MODIFIERS);
}

// ============================================================
// B1 fix: digit in a non-markable context must pass through literally
// ============================================================

const VNI_DIGIT_NON_MARKABLE_PASSTHROUGH: &[(&str, &str)] = &[
    ("123", "123"), // buffer starts empty, pure digits
    ("789", "789"),
];

#[test]
fn vni_digit_non_markable_context_passes_through() {
    vni(VNI_DIGIT_NON_MARKABLE_PASSTHROUGH);
}

/// "at" is a valid stop-final syllable; huyền (VNI '2') is phonologically
/// invalid on a stop-final (checked-tone rule), so it falls through as a
/// literal digit, leaving the buffer's last char as '2' (not a letter). A
/// second digit typed right after must NOT hunt the earlier 'a' vowel - it
/// must also pass through literally. This is the exact H0/B1 repro shape:
/// a digit that lands in the buffer without being consumed as a modifier,
/// followed by another digit.
#[test]
fn vni_digit_after_rejected_digit_stays_literal() {
    let mut e = Engine::new();
    e.set_method(1);
    let result = type_word(&mut e, "at23");
    assert_eq!(
        result, "at23",
        "[VNI] 'at23': '2' rejected as mark (stop-final), '3' must not retroactively mark 'a', got '{}'",
        result
    );
}

// ============================================================
// Shift+digit: symbol, never a VNI mark
// ============================================================

const VNI_SHIFT_DIGIT_SYMBOLS: &[(&str, &str)] = &[
    ("a@", "a@"), // Shift+2 -> '@', not huyền mark
    ("a!", "a!"), // Shift+1 -> '!', not sắc mark
];

#[test]
fn vni_shift_digit_yields_symbol_not_mark() {
    vni(VNI_SHIFT_DIGIT_SYMBOLS);
}

// ============================================================
// Telex: digits are always literal (method never gates Telex digits)
// ============================================================

#[test]
fn telex_digits_always_literal() {
    let mut e = Engine::new();
    let result = type_word(&mut e, "abc123");
    assert_eq!(
        result, "abc123",
        "[Telex] digits stay literal, got '{}'",
        result
    );
}

#[test]
fn telex_o2o_unaffected_by_vni_only_gate() {
    // Issue #162: "o2o" must stay raw in Telex mode (digit is not a Telex
    // modifier at all), confirming the VNI-only B1 gate doesn't touch Telex.
    let mut e = Engine::new();
    let result = type_word(&mut e, "o2o");
    assert_eq!(
        result, "o2o",
        "[Telex] 'o2o' should stay 'o2o', got '{}'",
        result
    );
}

// ============================================================
// Digit-after-diacritic: a second digit right after a mark was applied
// stays a modifier as long as the buffer still ends in the marked letter -
// switching or reverting the mark, never hunting a different/earlier vowel.
// ============================================================

const VNI_DIGIT_AFTER_DIACRITIC: &[(&str, &str)] = &[
    ("a12", "à"), // sắc applied by '1', then '2' switches it to huyền
    ("a11", "a1"), // sắc applied by '1', then repeated '1' reverts the mark
                   // (revert pushes the reverting digit as a literal char)
];

#[test]
fn vni_digit_after_diacritic_still_modifies() {
    vni(VNI_DIGIT_AFTER_DIACRITIC);
}
