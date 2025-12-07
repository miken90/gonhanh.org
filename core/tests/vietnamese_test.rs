//! Comprehensive Vietnamese IME Tests
//! Covers all typing patterns, mark placement rules, and edge cases

use gonhanh_core::data::keys;
use gonhanh_core::engine::{Action, Engine};

// ============================================================
// Test Infrastructure
// ============================================================

fn char_to_key(c: char) -> u16 {
    match c.to_ascii_lowercase() {
        'a' => keys::A, 'b' => keys::B, 'c' => keys::C, 'd' => keys::D,
        'e' => keys::E, 'f' => keys::F, 'g' => keys::G, 'h' => keys::H,
        'i' => keys::I, 'j' => keys::J, 'k' => keys::K, 'l' => keys::L,
        'm' => keys::M, 'n' => keys::N, 'o' => keys::O, 'p' => keys::P,
        'q' => keys::Q, 'r' => keys::R, 's' => keys::S, 't' => keys::T,
        'u' => keys::U, 'v' => keys::V, 'w' => keys::W, 'x' => keys::X,
        'y' => keys::Y, 'z' => keys::Z,
        '0' => keys::N0, '1' => keys::N1, '2' => keys::N2, '3' => keys::N3,
        '4' => keys::N4, '5' => keys::N5, '6' => keys::N6, '7' => keys::N7,
        '8' => keys::N8, '9' => keys::N9,
        ' ' => keys::SPACE,
        _ => 255,
    }
}

fn type_word(e: &mut Engine, input: &str) -> String {
    let mut screen = String::new();
    for c in input.chars() {
        let key = char_to_key(c);
        if key == keys::SPACE {
            screen.push(' ');
            e.on_key(key, false, false);
            continue;
        }
        let is_caps = c.is_uppercase();
        let r = e.on_key(key, is_caps, false);
        if r.action == Action::Send as u8 {
            for _ in 0..r.backspace { screen.pop(); }
            for i in 0..r.count as usize {
                if let Some(ch) = char::from_u32(r.chars[i]) { screen.push(ch); }
            }
        } else if keys::is_letter(key) {
            screen.push(if is_caps { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
        }
    }
    screen
}

fn run_tests(method: u8, cases: &[(&str, &str)]) {
    let method_name = if method == 0 { "Telex" } else { "VNI" };
    for (input, expected) in cases {
        let mut e = Engine::new();
        e.set_method(method);
        let result = type_word(&mut e, input);
        assert_eq!(
            result, *expected,
            "\n[{}] '{}' → '{}' (expected '{}')",
            method_name, input, result, expected
        );
    }
}

// ============================================================
// TELEX TESTS
// ============================================================

#[test]
fn telex_compound_vowels_with_marks() {
    // Complex vowel combinations + tone marks + diacritics
    run_tests(0, &[
        // ươ patterns (người, mười, trường)
        ("nguwowif", "người"),      // ng + ư + ơ + i + huyền
        ("nguwowis", "ngưới"),      // ng + ư + ơ + i + sắc
        ("nguwowir", "ngưởi"),      // ng + ư + ơ + i + hỏi
        ("nguwowix", "ngưỡi"),      // ng + ư + ơ + i + ngã
        ("nguwowij", "ngượi"),      // ng + ư + ơ + i + nặng
        ("muwowif", "mười"),        // huyền on ơ
        ("truwowngf", "trường"),
        ("luwowix", "lưỡi"),
        ("dduwowngf", "đường"),     // dd + ư + ơ + ng + huyền

        // iê patterns (việt, tiếng, biển)
        ("vieetj", "việt"),         // v + iê + nặng
        ("tieengs", "tiếng"),       // t + iê + ng + sắc
        ("bieenr", "biển"),
        ("ddieeuf", "điều"),
        ("nhieeux", "nhiễu"),
        ("chieenf", "chiền"),

        // uô patterns (muốn, cuộc, buồn)
        ("muoons", "muốn"),
        ("cuoocj", "cuộc"),
        ("buoonf", "buồn"),
        ("thuoocj", "thuộc"),
        ("chuoongs", "chuống"),

        // oa, oe, uy (modern: mark on 2nd vowel)
        ("hoaf", "hoà"),
        ("hoas", "hoá"),
        ("hoax", "hoã"),
        ("hoej", "hoẹ"),
        ("khoes", "khoé"),
        ("huyx", "huỹ"),
        ("quyf", "quỳ"),
        ("tuyeenr", "tuyển"),

        // ai, ao, au (mark on 1st vowel)
        ("mais", "mái"),
        ("maif", "mài"),
        ("caor", "cảo"),
        ("sauj", "sạu"),
        ("ddaus", "đáu"),

        // ưa patterns
        ("suwas", "sứa"),          // sứa: mark on ư
        ("muwaf", "mừa"),
        ("thuwaf", "thừa"),
        ("cuwaj", "cựa"),

        // ơi patterns
        ("ddowif", "đời"),
        ("mowis", "mới"),
        ("rowif", "rời"),

        // Three vowels: ươi, uyê, oai
        ("khuyeenr", "khuyển"),
        ("nguyeenx", "nguyễn"),
        ("ngoais", "ngoái"),
        ("khoair", "khoải"),

        // ươu pattern
        ("ruwowuj", "rượu"),
    ]);
}

#[test]
fn telex_tone_before_mark() {
    // Gõ dấu tone (^, ˘) trước mark (sắc, huyền...)
    run_tests(0, &[
        ("aas", "ấ"),              // â + sắc
        ("aaf", "ầ"),
        ("aar", "ẩ"),
        ("aax", "ẫ"),
        ("aaj", "ậ"),
        ("ees", "ế"),
        ("eef", "ề"),
        ("oos", "ố"),
        ("oof", "ồ"),
        ("aws", "ắ"),              // ă + sắc
        ("awf", "ằ"),
        ("ows", "ớ"),              // ơ + sắc
        ("owf", "ờ"),
        ("uws", "ứ"),              // ư + sắc
        ("uwf", "ừ"),
    ]);
}

#[test]
fn telex_mark_order_variations() {
    // Different orders of typing marks
    run_tests(0, &[
        // tone + consonant + mark
        ("toons", "tốn"),          // t + oo(ô) + n + s(sắc)
        ("beenf", "bền"),          // b + ee(ê) + n + f(huyền)
        ("ddaatj", "đật"),         // dd(đ) + aa(â) + t + j(nặng)

        // Multiple vowels then mark
        ("toans", "toán"),         // t + o + a + n + s → mark on 'a'
        ("hoanf", "hoàn"),
    ]);
}

#[test]
fn telex_dd_combinations() {
    // đ with various patterns
    run_tests(0, &[
        ("ddi", "đi"),
        ("ddeens", "đến"),
        ("dduwowngf", "đường"),
        ("ddangf", "đàng"),
        ("ddieeuf", "điều"),
        ("ddoongf", "đồng"),
        ("ddaatj", "đật"),
        ("ddeemf", "đềm"),
        ("ddays", "đáy"),
        ("ddoois", "đối"),
    ]);
}

#[test]
fn telex_double_key_revert() {
    // Double key cancels transform
    run_tests(0, &[
        // Mark revert
        ("ass", "as"),
        ("aff", "af"),
        ("arr", "ar"),
        ("axx", "ax"),
        ("ajj", "aj"),

        // Tone revert
        ("aaa", "aa"),
        ("eee", "ee"),
        ("ooo", "oo"),
        ("aww", "aw"),
        ("oww", "ow"),
        ("uww", "uw"),

        // Complex revert
        ("toons", "tốn"),
        ("toonss", "tôns"),        // tốn + s = tôns
    ]);
}

#[test]
fn telex_mixed_case() {
    // Uppercase handling
    run_tests(0, &[
        ("Chaof", "Chào"),
        ("CHAOF", "CHÀO"),
        ("Nguwowif", "Người"),
        ("NGUWOWIF", "NGƯỜI"),
        ("Vieetj", "Việt"),
        ("DDaats", "Đất"),
        ("DDAATS", "ĐẤT"),
    ]);
}

#[test]
fn telex_sentences() {
    // Full sentences with spaces
    run_tests(0, &[
        ("xinf chaof", "xìn chào"),
        ("tooi laf nguwowif vieetj nam", "tôi là người việt nam"),
        ("hocj tieengs vieetj", "họct tiếng việt"),
        ("chuscs muwngf nawm mowis", "chúcs mừng năm mới"),
    ]);
}

// ============================================================
// VNI TESTS
// ============================================================

#[test]
fn vni_compound_vowels_with_marks() {
    run_tests(1, &[
        // ươ patterns
        ("ngu8o8i2", "người"),     // u8=ư, o8=ơ, 2=huyền
        ("ngu8o8i1", "ngưới"),
        ("mu8o8i2", "mười"),
        ("tru8o8ng2", "trường"),
        ("lu8o8i4", "lưỡi"),
        ("d9u8o8ng2", "đường"),

        // iê patterns
        ("vie65t", "việt"),        // e6=ê, 5=nặng
        ("tie61ng", "tiếng"),
        ("bie63n", "biển"),
        ("d9ie62u", "điều"),
        ("nhie64u", "nhiễu"),

        // uô patterns
        ("muo61n", "muốn"),
        ("cuo65c", "cuộc"),
        ("buo62n", "buồn"),
        ("thuo65c", "thuộc"),

        // oa, oe, uy
        ("hoa2", "hoà"),
        ("hoa1", "hoá"),
        ("khoe1", "khoé"),
        ("huy4", "huỹ"),
        ("quy2", "quỳ"),
        ("tuye63n", "tuyển"),

        // ai, ao, au
        ("ma1i", "mái"),
        ("ma2i", "mài"),
        ("ca3o", "cảo"),
        ("sa5u", "sạu"),

        // ưa patterns
        ("su81a", "sứa"),          // u8=ư, 1=sắc
        ("mu82a", "mừa"),
        ("thu82a", "thừa"),

        // ơi patterns
        ("d9o8i2", "đời"),
        ("mo8i1", "mới"),

        // Three vowels
        ("ngu8o8i1", "ngưới"),
        ("khuye63n", "khuyển"),
        ("nguye64n", "nguyễn"),
        ("ngoa1i", "ngoái"),

        // ươu
        ("ru8o8u5", "rượu"),
    ]);
}

#[test]
fn vni_delayed_input() {
    // VNI cho phép gõ tone/mark sau nhiều ký tự
    run_tests(1, &[
        ("toi6", "tôi"),           // 6 tìm 'o' (không phải 'i')
        ("toi61", "tối"),
        ("nguoi8", "nguơi"),       // 8 tìm 'o' cuối
        ("nguoi82", "nguời"),
        ("duong8", "duơng"),
        ("duong82", "duờng"),
        ("muon6", "muôn"),
        ("muon61", "muốn"),
    ]);
}

#[test]
fn vni_double_key_revert() {
    run_tests(1, &[
        // Mark revert
        ("a11", "a1"),
        ("a22", "a2"),
        ("a33", "a3"),
        ("a44", "a4"),
        ("a55", "a5"),
        ("to6i11", "tôi1"),        // tối + 1 = tôi1

        // Tone revert
        ("a66", "a6"),
        ("e66", "e6"),
        ("o66", "o6"),
        ("a77", "a7"),
        ("o88", "o8"),
        ("u88", "u8"),
    ]);
}

#[test]
fn vni_mixed_case() {
    run_tests(1, &[
        ("Cha2o", "Chào"),
        ("CHA2O", "CHÀO"),
        ("Ngu8o8i2", "Người"),
        ("Vie65t", "Việt"),
        ("D9a61t", "Đất"),
    ]);
}

#[test]
fn vni_sentences() {
    run_tests(1, &[
        ("xi2n cha2o", "xìn chào"),
        ("to6i la2 ngu8o8i2 vie65t nam", "tôi là người việt nam"),
        ("ho5c tie61ng vie65t", "học tiếng việt"),
    ]);
}

// ============================================================
// EDGE CASES & REGRESSION
// ============================================================

#[test]
fn edge_empty_and_single() {
    run_tests(0, &[
        ("a", "a"),
        ("s", "s"),
        ("1", ""),                 // số không phải letter, không output
    ]);
    run_tests(1, &[
        ("a", "a"),
        ("1", ""),
    ]);
}

#[test]
fn edge_no_vowel() {
    // Mark/tone without vowel = pass through
    run_tests(0, &[
        ("bcs", "bcs"),            // no vowel to mark
        ("ddf", "đf"),             // dd→đ, then f pass through
    ]);
}

#[test]
fn edge_consonant_clusters() {
    run_tests(0, &[
        ("nguyeenx", "nguyễn"),    // ng + uyễn
        ("nhuwngx", "những"),
        ("thruwowngf", "thrường"), // thr + ường (invalid Vietnamese but tests buffer)
        ("khoongf", "không"),
        ("phaatj", "phật"),
        ("chauj", "chạu"),
    ]);
}

#[test]
fn edge_word_break() {
    // Space clears buffer
    run_tests(0, &[
        ("aa bb", "â bb"),         // â, space, bb
        ("as af", "á à"),
    ]);
}

#[test]
fn regression_chaof() {
    // Bug: "chaof" was producing "chaò" instead of "chào"
    let mut e = Engine::new();
    let result = type_word(&mut e, "chaof");
    assert_eq!(result, "chào");

    // Verify engine output directly
    e.clear();
    for c in "chao".chars() {
        e.on_key(char_to_key(c), false, false);
    }
    let r = e.on_key(keys::F, false, false);
    assert_eq!(r.action, Action::Send as u8);
    assert_eq!(r.backspace, 2, "Should delete 'ao'");
    assert_eq!(r.count, 2, "Should output 2 chars");
    assert_eq!(r.chars[0], 0x00E0, "First char should be à");
    assert_eq!(r.chars[1], 0x006F, "Second char should be o");
}

#[test]
fn regression_nguoi() {
    // nguowif → nguời (u stays as u)
    // nguwowif → người (uw=ư, ow=ơ)
    let mut e = Engine::new();

    let result1 = type_word(&mut e, "nguowif");
    assert_eq!(result1, "nguời");

    e.clear();
    let result2 = type_word(&mut e, "nguwowif");
    assert_eq!(result2, "người");
}

#[test]
fn regression_vni_toi61() {
    let mut e = Engine::new();
    e.set_method(1);
    let result = type_word(&mut e, "toi61");
    assert_eq!(result, "tối");
}

// ============================================================
// COMPREHENSIVE WORD LIST
// ============================================================

#[test]
fn telex_common_vietnamese_words() {
    run_tests(0, &[
        // Greetings & basics
        ("xinf chaof", "xìn chào"),
        ("camr own", "cảm ơn"),
        ("tamj bieetj", "tạm biệt"),

        // Pronouns
        ("tooi", "tôi"),
        ("banj", "bạn"),
        ("anhj", "anh"),
        ("chij", "chị"),
        ("emj", "em"),
        ("noj", "nó"),
        ("chungss tooi", "chúngs tôi"),  // Note: chúngs with revert

        // Common verbs
        ("laf", "là"),
        ("cos", "có"),
        ("ddi", "đi"),
        ("ddeens", "đến"),
        ("veef", "về"),
        ("awn", "ăn"),
        ("uoongs", "uống"),
        ("nguwr", "ngủ"),
        ("laafm", "làm"),          // Note: double a
        ("noois", "nói"),
        ("nghix", "nghĩ"),
        ("bieets", "biết"),
        ("hieeur", "hiểu"),
        ("yeeu", "yêu"),

        // Common nouns
        ("nhaf", "nhà"),
        ("truwowngf", "trường"),
        ("beenhj vieenj", "bệnh viện"),
        ("coong ty", "công ty"),
        ("nuwosc", "nước"),
        ("thoiwf gian", "thời gian"),
        ("tieenf", "tiền"),
        ("ddooof awn", "đồ ăn"),

        // Numbers context
        ("mootj", "một"),
        ("hai", "hai"),
        ("ba", "ba"),
        ("boons", "bốn"),
        ("nawm", "năm"),
        ("saus", "sáu"),
        ("bayr", "bảy"),
        ("tams", "tám"),
        ("chins", "chín"),
        ("muwowif", "mười"),

        // Common adjectives
        ("tootss", "tốts"),        // revert
        ("xauus", "xấus"),
        ("lownss", "lớns"),
        ("nhor", "nhỏ"),
        ("ddepj", "đẹp"),
        ("xaus", "xấu"),

        // Question words
        ("gif", "gì"),
        ("ddaau", "đâu"),
        ("naofo", "nàoo"),         // error test
        ("naof", "nào"),
        ("tai sao", "tai sao"),
        ("taij sao", "tại sao"),
        ("bao nhieeu", "bao nhiêu"),
        ("thees naof", "thế nào"),
    ]);
}

#[test]
fn vni_common_vietnamese_words() {
    run_tests(1, &[
        // Greetings
        ("xi2n cha2o", "xìn chào"),
        ("ca3m o8n", "cảm ơn"),
        ("ta5m bie65t", "tạm biệt"),

        // Pronouns
        ("to6i", "tôi"),
        ("ba5n", "bạn"),
        ("anh5", "ạnh"),           // Note: 5 on 'a' not 'nh'

        // Common verbs
        ("la2", "là"),
        ("co1", "có"),
        ("d9i", "đi"),
        ("d9e61n", "đến"),
        ("ve62", "về"),
        ("a7n", "ăn"),
        ("uo61ng", "uống"),
        ("ngu3", "ngủ"),
        ("la2m", "làm"),
        ("no1i", "nói"),
        ("nghi4", "nghĩ"),
        ("bie61t", "biết"),
        ("hie63u", "hiểu"),
        ("ye6u", "yêu"),

        // Common nouns
        ("nha2", "nhà"),
        ("tru8o8ng2", "trường"),
        ("be65nh vie65n", "bệnh viện"),
        ("co6ng ty", "công ty"),
        ("nu8o81c", "nước"),
    ]);
}

// ============================================================
// MARK PLACEMENT RULES
// ============================================================

#[test]
fn mark_placement_single_vowel() {
    run_tests(0, &[
        ("as", "á"),
        ("es", "é"),
        ("is", "í"),
        ("os", "ó"),
        ("us", "ú"),
        ("ys", "ý"),
        ("aas", "ấ"),              // â + sắc
        ("ees", "ế"),
        ("oos", "ố"),
        ("aws", "ắ"),
        ("ows", "ớ"),
        ("uws", "ứ"),
    ]);
}

#[test]
fn mark_placement_two_vowels_closed() {
    // With final consonant: mark on 2nd vowel
    run_tests(0, &[
        ("toans", "toán"),         // mark on 'a'
        ("hoangf", "hoàng"),
        ("tieens", "tiến"),
        ("muoons", "muốn"),
        ("nguoif", "nguồi"),       // Wait, this has no final consonant
        // Let me fix:
        ("nguoins", "nguoín"),     // invalid Vietnamese but tests rule
    ]);
}

#[test]
fn mark_placement_two_vowels_open() {
    // Open syllable (no final consonant)
    // oa, oe, uy: modern = 2nd vowel
    // ai, ao, au: 1st vowel
    run_tests(0, &[
        // Glide + vowel (mark on 2nd = modern)
        ("hoaf", "hoà"),
        ("hoas", "hoá"),
        ("hoer", "hoẻ"),
        ("huyf", "huỳ"),
        ("quys", "quý"),

        // Vowel + glide (mark on 1st)
        ("mais", "mái"),
        ("caof", "cào"),
        ("daus", "dáu"),
        ("dois", "dói"),           // đói without dd
        ("ddois", "đói"),
        ("tuis", "túi"),
    ]);
}

#[test]
fn mark_placement_three_vowels() {
    // ươi: mark on ơ (middle)
    // uyê: mark on ê
    // oai: mark on a
    run_tests(0, &[
        ("nguwowif", "người"),     // mark on ơ
        ("muwowif", "mười"),
        ("khuyeens", "khuyến"),    // mark on ê
        ("nguyeenx", "nguyễn"),
        ("ngoais", "ngoái"),       // mark on a
        ("khoair", "khoải"),
        ("ruwowuj", "rượu"),       // ươu: mark on ơ
    ]);
}

#[test]
fn mark_placement_vowel_with_diacritic() {
    // If vowel already has diacritic (ư, ơ, ê, ô, â, ă), mark goes there
    run_tests(0, &[
        ("suwas", "sứa"),          // ưa: mark on ư (has diacritic)
        ("thuwaf", "thừa"),
        ("ddowif", "đời"),         // ơi: mark on ơ
        ("mowis", "mới"),
        ("toons", "tốn"),          // ô alone
        ("beenf", "bền"),          // ê alone
    ]);
}
