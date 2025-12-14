//! Dynamic Tests - Auto-generated Vietnamese syllable combinations
//!
//! Instead of declaring test cases one-by-one, this module generates
//! all valid Vietnamese syllable patterns and tests tone placement.

#![allow(dead_code)] // Constants below serve as documentation for test patterns

mod common;
use common::telex;
use rstest::rstest;

// ============================================================
// VOWEL PATTERNS WITH EXPECTED TONE POSITION
// ============================================================
// Format: (vowel_pattern, telex_input_suffix, expected_output, tone_description)
// The tone position is encoded in the expected output

/// Single vowels - tone always on the vowel itself
const SINGLE_VOWELS: &[(&str, &str)] = &[
    ("a", "á"),
    ("e", "é"),
    ("i", "í"),
    ("o", "ó"),
    ("u", "ú"),
    ("y", "ý"),
];

/// Modified single vowels (circumflex, horn, breve)
const MODIFIED_VOWELS: &[(&str, &str, &str)] = &[
    // (telex_input, expected_with_sac, description)
    ("aa", "ấ", "â + sắc"),
    ("ee", "ế", "ê + sắc"),
    ("oo", "ố", "ô + sắc"),
    ("ow", "ớ", "ơ + sắc"),
    ("uw", "ứ", "ư + sắc"),
    ("aw", "ắ", "ă + sắc"),
];

/// Two-vowel patterns: Medial + Main (tone on 2nd/main vowel)
/// oa, oe, uy, uê - tone goes on the main vowel (2nd)
const MEDIAL_MAIN_PAIRS: &[(&str, &str)] = &[
    ("oa", "oá"),  // hoá, toá
    ("oe", "oé"),  // xoè, loè
    ("uy", "uý"),  // quý, tuý
    ("uee", "uế"), // huế, quế (uê)
];

/// Two-vowel patterns: Main + Glide (tone on 1st/main vowel)
/// ai, ao, au, ay, oi, ui, etc. - tone goes on the main vowel (1st)
const MAIN_GLIDE_PAIRS: &[(&str, &str)] = &[
    ("ai", "ái"),
    ("ao", "áo"),
    ("au", "áu"),
    ("ay", "áy"),
    ("oi", "ói"),
    ("ui", "úi"),
    ("eo", "éo"),
    ("eu", "éu"),
    ("iu", "íu"),
];

/// Special two-vowel patterns with specific rules
const SPECIAL_PAIRS: &[(&str, &str, &str)] = &[
    // (telex_input, expected_with_sac, description)
    ("ia", "ía", "ia: tone on i (descending diphthong)"),
    ("uwa", "ứa", "ưa: tone on ư (ư has diacritic)"),
];

/// Compound vowels: ươ, uô, iê - tone on 2nd (both have diacritics or 2nd is main)
const COMPOUND_VOWELS: &[(&str, &str, &str)] = &[
    ("uow", "ướ", "ươ compound"),
    ("uoo", "uố", "uô compound"),
    ("iee", "iế", "iê compound"),
];

/// Three-vowel patterns - tone typically on middle vowel
const THREE_VOWELS: &[(&str, &str, &str)] = &[
    ("oai", "oái", "oai: tone on a (middle)"),
    ("oay", "oáy", "oay: tone on a (middle)"),
    ("uoi", "uối", "uôi: tone on ô (middle)"),
    ("uowi", "ưới", "ươi: tone on ơ (middle)"),
    ("uyee", "uyế", "uyê: tone on ê (last, has diacritic)"),
];

// ============================================================
// INITIALS FOR COMBINATION
// ============================================================

const SIMPLE_INITIALS: &[&str] = &[
    "", "b", "c", "d", "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "x",
];

const DOUBLE_INITIALS: &[&str] = &[
    "ch", "gh", "kh", "ng", "nh", "ph", "th", "tr",
];

// Special initials that affect vowel patterns
const GI_INITIAL: &str = "gi";
const QU_INITIAL: &str = "qu";

// ============================================================
// FINALS FOR COMBINATION
// ============================================================

const SIMPLE_FINALS: &[&str] = &["", "c", "m", "n", "p", "t"];

const DOUBLE_FINALS: &[&str] = &["ch", "ng", "nh"];

// ============================================================
// TONE MARKS (Telex)
// ============================================================

const TONE_MARKS: &[(&str, &str, &str)] = &[
    // (telex_key, mark_char, description)
    ("s", "\u{0301}", "sắc"),   // ́ combining acute
    ("f", "\u{0300}", "huyền"), // ̀ combining grave
    ("r", "\u{0309}", "hỏi"),   // ̉ combining hook above
    ("x", "\u{0303}", "ngã"),   // ̃ combining tilde
    ("j", "\u{0323}", "nặng"), // ̣ combining dot below
];

// ============================================================
// DYNAMIC TEST: Single vowels with all tones
// ============================================================

#[rstest]
#[case("a", "s", "á")]
#[case("a", "f", "à")]
#[case("a", "r", "ả")]
#[case("a", "x", "ã")]
#[case("a", "j", "ạ")]
#[case("e", "s", "é")]
#[case("e", "f", "è")]
#[case("e", "r", "ẻ")]
#[case("e", "x", "ẽ")]
#[case("e", "j", "ẹ")]
#[case("i", "s", "í")]
#[case("i", "f", "ì")]
#[case("i", "r", "ỉ")]
#[case("i", "x", "ĩ")]
#[case("i", "j", "ị")]
#[case("o", "s", "ó")]
#[case("o", "f", "ò")]
#[case("o", "r", "ỏ")]
#[case("o", "x", "õ")]
#[case("o", "j", "ọ")]
#[case("u", "s", "ú")]
#[case("u", "f", "ù")]
#[case("u", "r", "ủ")]
#[case("u", "x", "ũ")]
#[case("u", "j", "ụ")]
#[case("y", "s", "ý")]
#[case("y", "f", "ỳ")]
#[case("y", "r", "ỷ")]
#[case("y", "x", "ỹ")]
#[case("y", "j", "ỵ")]
fn single_vowel_all_tones(#[case] vowel: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowel, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Modified vowels (circumflex, horn, breve)
// ============================================================

#[rstest]
// Circumflex: â, ê, ô
#[case("aa", "s", "ấ")]
#[case("aa", "f", "ầ")]
#[case("aa", "r", "ẩ")]
#[case("aa", "x", "ẫ")]
#[case("aa", "j", "ậ")]
#[case("ee", "s", "ế")]
#[case("ee", "f", "ề")]
#[case("ee", "r", "ể")]
#[case("ee", "x", "ễ")]
#[case("ee", "j", "ệ")]
#[case("oo", "s", "ố")]
#[case("oo", "f", "ồ")]
#[case("oo", "r", "ổ")]
#[case("oo", "x", "ỗ")]
#[case("oo", "j", "ộ")]
// Horn: ơ, ư
#[case("ow", "s", "ớ")]
#[case("ow", "f", "ờ")]
#[case("ow", "r", "ở")]
#[case("ow", "x", "ỡ")]
#[case("ow", "j", "ợ")]
#[case("uw", "s", "ứ")]
#[case("uw", "f", "ừ")]
#[case("uw", "r", "ử")]
#[case("uw", "x", "ữ")]
#[case("uw", "j", "ự")]
// Breve: ă
#[case("aw", "s", "ắ")]
#[case("aw", "f", "ằ")]
#[case("aw", "r", "ẳ")]
#[case("aw", "x", "ẵ")]
#[case("aw", "j", "ặ")]
fn modified_vowel_all_tones(#[case] vowel: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowel, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Two-vowel patterns (medial + main)
// Tone on 2nd vowel (main)
// ============================================================

#[rstest]
// oa pattern
#[case("oa", "s", "oá")]
#[case("oa", "f", "oà")]
#[case("oa", "r", "oả")]
#[case("oa", "x", "oã")]
#[case("oa", "j", "oạ")]
// oe pattern
#[case("oe", "s", "oé")]
#[case("oe", "f", "oè")]
#[case("oe", "r", "oẻ")]
#[case("oe", "x", "oẽ")]
#[case("oe", "j", "oẹ")]
// uy pattern
#[case("uy", "s", "uý")]
#[case("uy", "f", "uỳ")]
#[case("uy", "r", "uỷ")]
#[case("uy", "x", "uỹ")]
#[case("uy", "j", "uỵ")]
fn medial_main_pair_tones(#[case] vowels: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowels, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Two-vowel patterns (main + glide)
// Tone on 1st vowel (main)
// ============================================================

#[rstest]
// ai pattern
#[case("ai", "s", "ái")]
#[case("ai", "f", "ài")]
#[case("ai", "r", "ải")]
#[case("ai", "x", "ãi")]
#[case("ai", "j", "ại")]
// ao pattern
#[case("ao", "s", "áo")]
#[case("ao", "f", "ào")]
#[case("ao", "r", "ảo")]
#[case("ao", "x", "ão")]
#[case("ao", "j", "ạo")]
// au pattern
#[case("au", "s", "áu")]
#[case("au", "f", "àu")]
#[case("au", "r", "ảu")]
#[case("au", "x", "ãu")]
#[case("au", "j", "ạu")]
// ay pattern
#[case("ay", "s", "áy")]
#[case("ay", "f", "ày")]
#[case("ay", "r", "ảy")]
#[case("ay", "x", "ãy")]
#[case("ay", "j", "ạy")]
// oi pattern
#[case("oi", "s", "ói")]
#[case("oi", "f", "òi")]
#[case("oi", "r", "ỏi")]
#[case("oi", "x", "õi")]
#[case("oi", "j", "ọi")]
// ui pattern
#[case("ui", "s", "úi")]
#[case("ui", "f", "ùi")]
#[case("ui", "r", "ủi")]
#[case("ui", "x", "ũi")]
#[case("ui", "j", "ụi")]
// eo pattern
#[case("eo", "s", "éo")]
#[case("eo", "f", "èo")]
#[case("eo", "r", "ẻo")]
#[case("eo", "x", "ẽo")]
#[case("eo", "j", "ẹo")]
// iu pattern
#[case("iu", "s", "íu")]
#[case("iu", "f", "ìu")]
#[case("iu", "r", "ỉu")]
#[case("iu", "x", "ĩu")]
#[case("iu", "j", "ịu")]
fn main_glide_pair_tones(#[case] vowels: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowels, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Special patterns (ia, ưa)
// ia: tone on i (descending diphthong)
// ưa: tone on ư (has diacritic)
// ============================================================

#[rstest]
// ia pattern - tone on i
#[case("ia", "s", "ía")]
#[case("ia", "f", "ìa")]
#[case("ia", "r", "ỉa")]
#[case("ia", "x", "ĩa")]
#[case("ia", "j", "ịa")]
// ưa pattern - tone on ư
#[case("uwa", "s", "ứa")]
#[case("uwa", "f", "ừa")]
#[case("uwa", "r", "ửa")]
#[case("uwa", "x", "ữa")]
#[case("uwa", "j", "ựa")]
fn special_pair_tones(#[case] vowels: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowels, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Compound vowels (ươ, uô, iê)
// Tone on 2nd vowel
// ============================================================

#[rstest]
// ươ compound
#[case("uow", "s", "ướ")]
#[case("uow", "f", "ườ")]
#[case("uow", "r", "ưở")]
#[case("uow", "x", "ưỡ")]
#[case("uow", "j", "ượ")]
// uô compound
#[case("uoo", "s", "uố")]
#[case("uoo", "f", "uồ")]
#[case("uoo", "r", "uổ")]
#[case("uoo", "x", "uỗ")]
#[case("uoo", "j", "uộ")]
// iê compound
#[case("iee", "s", "iế")]
#[case("iee", "f", "iề")]
#[case("iee", "r", "iể")]
#[case("iee", "x", "iễ")]
#[case("iee", "j", "iệ")]
fn compound_vowel_tones(#[case] vowels: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowels, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Three-vowel patterns
// ============================================================

#[rstest]
// oai - tone on a (middle)
#[case("oai", "s", "oái")]
#[case("oai", "f", "oài")]
#[case("oai", "r", "oải")]
#[case("oai", "x", "oãi")]
#[case("oai", "j", "oại")]
// oay - tone on a (middle)
#[case("oay", "s", "oáy")]
#[case("oay", "f", "oày")]
#[case("oay", "r", "oảy")]
#[case("oay", "x", "oãy")]
#[case("oay", "j", "oạy")]
// uôi - tone on ô (middle)
#[case("uooi", "s", "uối")]
#[case("uooi", "f", "uồi")]
#[case("uooi", "r", "uổi")]
#[case("uooi", "x", "uỗi")]
#[case("uooi", "j", "uội")]
// ươi - tone on ơ (middle)
#[case("uowi", "s", "ưới")]
#[case("uowi", "f", "ười")]
#[case("uowi", "r", "ưởi")]
#[case("uowi", "x", "ưỡi")]
#[case("uowi", "j", "ượi")]
fn three_vowel_tones(#[case] vowels: &str, #[case] tone: &str, #[case] expected: &str) {
    let input = format!("{}{}", vowels, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: With initials
// ============================================================

#[rstest]
#[case("b", "a", "s", "bá")]
#[case("c", "a", "s", "cá")]
#[case("d", "a", "s", "dá")]
#[case("l", "a", "s", "lá")]
#[case("m", "a", "s", "má")]
#[case("n", "a", "s", "ná")]
#[case("t", "a", "s", "tá")]
#[case("ch", "a", "s", "chá")]
#[case("kh", "a", "s", "khá")]
#[case("ng", "a", "s", "ngá")]
#[case("nh", "a", "s", "nhá")]
#[case("ph", "a", "s", "phá")]
#[case("th", "a", "s", "thá")]
#[case("tr", "a", "s", "trá")]
// k before e/i/y
#[case("k", "e", "s", "ké")]
#[case("k", "i", "s", "kí")]
// gh before e/i
#[case("gh", "e", "s", "ghé")]
#[case("gh", "i", "s", "ghí")]
// ngh before e/i
#[case("ngh", "e", "s", "nghé")]
#[case("ngh", "i", "s", "nghí")]
fn initial_vowel_tone(
    #[case] initial: &str,
    #[case] vowel: &str,
    #[case] tone: &str,
    #[case] expected: &str,
) {
    let input = format!("{}{}{}", initial, vowel, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: With finals (tone on correct vowel position)
// ============================================================

#[rstest]
// Single vowel + final: tone on vowel
#[case("a", "n", "s", "án")]
#[case("a", "m", "s", "ám")]
#[case("a", "c", "s", "ác")]
#[case("a", "t", "s", "át")]
#[case("a", "p", "s", "áp")]
#[case("a", "ng", "s", "áng")]
#[case("a", "nh", "s", "ánh")]
#[case("a", "ch", "s", "ách")]
// Two vowels + final: tone on 2nd vowel
#[case("oa", "n", "s", "oán")]
#[case("oa", "ng", "s", "oáng")]
#[case("ie", "n", "s", "iến")] // iê + n
#[case("ie", "ng", "s", "iếng")] // Note: "ie" triggers iê compound
fn vowel_final_tone(
    #[case] vowel: &str,
    #[case] final_c: &str,
    #[case] tone: &str,
    #[case] expected: &str,
) {
    // For vowel patterns that need modification
    let vowel_input = match vowel {
        "ie" => "iee",
        _ => vowel,
    };
    let input = format!("{}{}{}", vowel_input, final_c, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: Full syllables (initial + vowel + final + tone)
// ============================================================

#[rstest]
// Common words
#[case("b", "a", "n", "s", "bán")]
#[case("c", "a", "m", "s", "cám")]
#[case("h", "oa", "n", "s", "hoán")]
#[case("t", "oa", "n", "s", "toán")]
#[case("kh", "oa", "n", "s", "khoán")]
#[case("ng", "uo", "n", "s", "nguốn")] // nguồn with ô
#[case("tr", "uow", "ng", "s", "trướng")] // trường with ươ
#[case("ngh", "iee", "ng", "s", "nghiếng")]
fn full_syllable_tone(
    #[case] initial: &str,
    #[case] vowel: &str,
    #[case] final_c: &str,
    #[case] tone: &str,
    #[case] expected: &str,
) {
    // Handle vowel patterns
    let vowel_input = match vowel {
        "uo" => "uoo", // uô
        _ => vowel,
    };
    let input = format!("{}{}{}{}", initial, vowel_input, final_c, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: gi + vowel (special initial)
// gi is treated as initial, vowel after it gets the tone
// ============================================================

#[rstest]
#[case("gi", "a", "s", "giá")]
#[case("gi", "a", "f", "già")]
#[case("gi", "au", "s", "giáu")]
#[case("gi", "au", "f", "giàu")]
#[case("gi", "eo", "s", "giéo")]
fn gi_initial_tone(
    #[case] initial: &str,
    #[case] vowel: &str,
    #[case] tone: &str,
    #[case] expected: &str,
) {
    let input = format!("{}{}{}", initial, vowel, tone);
    telex(&[(&input, expected)]);
}

// ============================================================
// DYNAMIC TEST: qu + vowel (special initial)
// qu is treated as initial, tone on the main vowel
// ============================================================

#[rstest]
#[case("quas", "quá")]
#[case("quaf", "quà")]
#[case("quans", "quán")]
#[case("quoocs", "quốc")] // quốc with ô
#[case("quys", "quý")]
#[case("quyeens", "quyến")] // quyến with uyê
fn qu_initial_tone(#[case] input: &str, #[case] expected: &str) {
    telex(&[(input, expected)]);
}

// ============================================================
// COMPREHENSIVE: Generate all vowel + tone combinations
// ============================================================

/// Test all basic vowel patterns with sắc tone
/// This catches any missing patterns
#[test]
fn comprehensive_vowel_patterns_sac() {
    let patterns: &[(&str, &str)] = &[
        // Single vowels
        ("as", "á"),
        ("es", "é"),
        ("is", "í"),
        ("os", "ó"),
        ("us", "ú"),
        ("ys", "ý"),
        // Modified vowels
        ("aas", "ấ"),
        ("ees", "ế"),
        ("oos", "ố"),
        ("ows", "ớ"),
        ("uws", "ứ"),
        ("aws", "ắ"),
        // Two-vowel: medial+main (tone on 2nd)
        ("oas", "oá"),
        ("oes", "oé"),
        ("uys", "uý"),
        ("uees", "uế"),
        // Two-vowel: main+glide (tone on 1st)
        ("ais", "ái"),
        ("aos", "áo"),
        ("aus", "áu"),
        ("ays", "áy"),
        ("ois", "ói"),
        ("uis", "úi"),
        ("eos", "éo"),
        ("ius", "íu"),
        // Special patterns
        ("ias", "ía"),    // ia: tone on i
        ("uwas", "ứa"),   // ưa: tone on ư
        // Compound vowels (tone on 2nd)
        ("uows", "ướ"),   // ươ
        ("uoos", "uố"),   // uô
        ("iees", "iế"),   // iê
        // Three-vowel (tone on middle)
        ("oais", "oái"),
        ("oays", "oáy"),
        ("uoois", "uối"), // uôi
        ("uowis", "ưới"), // ươi
    ];

    telex(patterns);
}

/// Test all basic vowel patterns with huyền tone
#[test]
fn comprehensive_vowel_patterns_huyen() {
    let patterns: &[(&str, &str)] = &[
        // Single vowels
        ("af", "à"),
        ("ef", "è"),
        ("if", "ì"),
        ("of", "ò"),
        ("uf", "ù"),
        ("yf", "ỳ"),
        // Modified vowels
        ("aaf", "ầ"),
        ("eef", "ề"),
        ("oof", "ồ"),
        ("owf", "ờ"),
        ("uwf", "ừ"),
        ("awf", "ằ"),
        // Two-vowel: medial+main
        ("oaf", "oà"),
        ("oef", "oè"),
        ("uyf", "uỳ"),
        ("ueef", "uề"),
        // Two-vowel: main+glide
        ("aif", "ài"),
        ("aof", "ào"),
        ("auf", "àu"),
        ("ayf", "ày"),
        ("oif", "òi"),
        ("uif", "ùi"),
        ("eof", "èo"),
        ("iuf", "ìu"),
        // Special patterns
        ("iaf", "ìa"),
        ("uwaf", "ừa"),
        // Compound vowels
        ("uowf", "ườ"),
        ("uoof", "uồ"),
        ("ieef", "iề"),
        // Three-vowel
        ("oaif", "oài"),
        ("oayf", "oày"),
        ("uooif", "uồi"),
        ("uowif", "ười"),
    ];

    telex(patterns);
}
