//! Vietnamese Sentence Tests - Proverbs, Idioms, and Common Phrases
//!
//! Tests full sentences using Vietnamese thành ngữ (idioms) and tục ngữ (proverbs)

use gonhanh_core::data::keys;
use gonhanh_core::engine::{Action, Engine};

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

fn type_sentence(e: &mut Engine, input: &str) -> String {
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

fn run_telex(cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        let mut e = Engine::new();
        let result = type_sentence(&mut e, input);
        assert_eq!(result, *expected, "\n[Telex] '{}'\n→ '{}'\n(expected '{}')", input, result, expected);
    }
}

fn run_vni(cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        let mut e = Engine::new();
        e.set_method(1);
        let result = type_sentence(&mut e, input);
        assert_eq!(result, *expected, "\n[VNI] '{}'\n→ '{}'\n(expected '{}')", input, result, expected);
    }
}

// ============================================================
// TELEX: GREETINGS & COMMON PHRASES
// ============================================================

#[test]
fn telex_greetings() {
    run_telex(&[
        ("xin chaof", "xin chào"),                      // Hello
        ("tamj bieetj", "tạm biệt"),                    // Goodbye
        ("camr own", "cảm ơn"),                         // Thank you
        ("xin looxix", "xin lỗi"),                      // Sorry (oo=ô, x=ngã, i, x=ngã)
        ("raats vui dduwowcj gaawpj banj", "rất vui được gặp bạn"),  // Nice to meet you
    ]);
}

#[test]
fn telex_introductions() {
    run_telex(&[
        ("tooi teen laf", "tôi tên là"),                // My name is
        ("tooi ddeens tufw", "tôi đến từ"),             // I come from
        ("tooi laf nguwowif vieetj nam", "tôi là người việt nam"),  // I am Vietnamese
        ("banj khoeer khoonng", "bạn khỏe không"),      // How are you? (oo=ô, ng)
        ("tooi khoeer", "tôi khỏe"),                    // I'm fine
    ]);
}

// ============================================================
// TELEX: TỤC NGỮ (Proverbs)
// ============================================================

#[test]
fn telex_proverbs_about_learning() {
    run_telex(&[
        // Học hành
        ("hocj haanhf", "học hành"),                    // study
        ("hocj mootj bieets muwowif", "học một biết mười"),  // Learn one, know ten

        ("ddi mootj ngayf ddangf hocj mootj sangf khoon", "đi một ngày đàng học một sàng khôn"),
        // Travel one day, learn a basketful of wisdom

        ("khoonng thaaayf ddoos mayf laf thawngss", "không thầy đố mày là thắng"),
        // Without a teacher, you can't succeed (khoonng=không, thaaayf=thầy)

        ("hocj awn hocj nois hocj gois hocj mowsr", "học ăn học nói học gói học mở"),
        // Learn to eat, speak, wrap, unwrap
    ]);
}

#[test]
fn telex_proverbs_about_family() {
    run_telex(&[
        // Gia đình
        ("coong cha nhuw nusis thaais sown", "công cha như núi thái sơn"),
        // Father's merit is like Thai Son mountain

        ("nghiax mej nhuw nuwowcs trong nguoonf chayr ra", "nghĩa mẹ như nước trong nguồn chảy ra"),
        // Mother's love is like water from the source

        ("mootj giootj maus ddaof hown ao hoof nuwowcs lax", "một giọt máu đào hơn ao hồ nước lã"),
        // One drop of blood is worth more than a pond of water

        ("anh em nhuw theer chaanf tay", "anh em như thể chân tay"),
        // Siblings are like limbs
    ]);
}

#[test]
fn telex_proverbs_about_work() {
    run_telex(&[
        // Lao động
        ("cos coong maif sawts ngayf", "có công mài sắt ngày"),
        // With effort, iron can be ground

        ("tay lafm hafm nhai", "tay làm hàm nhai"),
        // Hands work, mouth eats

        ("nawwngs gif gawtj luas", "nắng gì gặt lúa"),
        // What sun for harvesting rice

        ("muaf xuaan haasi vuuwn", "mùa xuân hái vườn"),
        // Spring season, harvest the garden
    ]);
}

#[test]
fn telex_proverbs_about_character() {
    run_telex(&[
        // Tính cách
        ("toots goox hown ddepj nguwowif", "tốt gỗ hơn đẹp người"),
        // Good wood is better than beautiful person

        ("owr khoonng hown lafn laf ddaaays", "ở không hơn làn là đấy"),
        // Being idle is better than doing wrong (owr=ở)

        ("uoongs nuwowcs nhowsr nguoonf", "uống nước nhớ nguồn"),
        // When drinking water, remember the source

        ("awn quar nhowsr keer troongf caay", "ăn quả nhớ kẻ trồng cây"),
        // Eating fruit, remember who planted the tree
    ]);
}

#[test]
fn telex_proverbs_about_nature() {
    run_telex(&[
        // Thiên nhiên
        ("nuwowcs chayr ddas moonf", "nước chảy đá mòn"),
        // Water flows, stone erodes

        ("troiwf muwaf khi naof mats", "trời mưa khi nào mát"),
        // When it rains, it's cool

        ("las ruwngf veef cooix", "lá rừng về cõi"),
        // Forest leaves return to earth
    ]);
}

// ============================================================
// TELEX: THÀNH NGỮ (Idioms)
// ============================================================

#[test]
fn telex_idioms_4_words() {
    run_telex(&[
        // Thành ngữ 4 chữ
        ("an cuw lacj nghieepj", "an cư lạc nghiệp"),   // Settle down and prosper
        ("ddoongf taanm hieepj luwcj", "đồng tâm hiệp lực"),  // United in heart and strength
        ("cos hoofn phucjs phaans", "có hồn phúc phần"), // Have soul and fortune
    ]);
}

#[test]
fn telex_idioms_about_time() {
    run_telex(&[
        // Thời gian
        ("mootj nawm cos boons muaf", "một năm có bốn mùa"),
        // One year has four seasons

        ("thowif gian laf tieeenf bacj", "thời gian là tiền bạc"),
        // Time is money

        ("soongs mootj ngayf bieets mootj ngayf", "sống một ngày biết một ngày"),
        // Live one day, know one day
    ]);
}

#[test]
fn telex_idioms_about_friendship() {
    run_telex(&[
        // Tình bạn
        ("banj bef boons nguwowif", "bạn bè bốn người"),
        // Four friends

        ("toots goox hown toots banj", "tốt gỗ hơn tốt bạn"),
        // Good wood is better than good friends

        ("gaawns muwcj thif dden gaawns ddenf thif sangs", "gần mực thì đen gần đèn thì sáng"),
        // Near ink becomes black, near lamp becomes bright
    ]);
}

// ============================================================
// TELEX: DAILY CONVERSATIONS
// ============================================================

#[test]
fn telex_daily_conversations() {
    run_telex(&[
        // Hội thoại hàng ngày
        ("hoom nay thowif tieets thees naof", "hôm nay thời tiết thế nào"),
        // How's the weather today?

        ("banj ddi ddaaau vaayj", "bạn đi đâu vậy"),
        // Where are you going?

        ("tooi ddang ddi lafm", "tôi đang đi làm"),
        // I'm going to work

        ("mootj lys caf phee nhes", "một ly cà phê nhé"),
        // One cup of coffee please

        ("bao nhieeu tieeenf", "bao nhiêu tiền"),
        // How much?

        ("camr own banj nhieeuf lawms", "cảm ơn bạn nhiều lắm"),
        // Thank you very much
    ]);
}

#[test]
fn telex_food_ordering() {
    run_telex(&[
        // Đặt món
        ("cho tooi xem thuwcj ddown", "cho tôi xem thực đơn"),
        // Let me see the menu

        ("tooi muoons goij mootj phaanf phowrs", "tôi muốn gọi một phần phở"),
        // I want to order pho

        ("tooi muoons uoongs traf", "tôi muốn uống trà"),
        // I want to drink tea

        ("ddoof awn raats ngon", "đồ ăn rất ngon"),
        // The food is very delicious

        ("tinhs tieeenf nhes", "tính tiền nhé"),
        // Bill please
    ]);
}

// ============================================================
// VNI: PROVERBS
// ============================================================

#[test]
fn vni_proverbs() {
    run_vni(&[
        ("ho5c mo65t bie61t mu8o8i2", "học một biết mười"),
        // Learn one, know ten

        ("uo61ng nu8o81c nho81 nguo62n", "uống nước nhớ nguồn"),
        // When drinking water, remember the source

        ("to61t go64 ho8n d9e5p ngu8o8i2", "tốt gỗ hơn đẹp người"),
        // Good wood is better than beautiful person

        ("nu8o81c cha3y d9a1 mo2n", "nước chảy đá mòn"),
        // Water flows, stone erodes
    ]);
}

#[test]
fn vni_greetings() {
    run_vni(&[
        ("xin cha2o", "xin chào"),
        ("ta5m bie65t", "tạm biệt"),
        ("ca3m o8n", "cảm ơn"),
        ("to6i la2 ngu8o8i2 vie65t nam", "tôi là người việt nam"),
    ]);
}

#[test]
fn vni_daily_phrases() {
    run_vni(&[
        ("ho6m nay tho8i2 tie61t the61 na2o", "hôm nay thời tiết thế nào"),
        ("ba5n d9i d9a6u va65y", "bạn đi đâu vậy"),
        ("to6i d9ang d9i la2m", "tôi đang đi làm"),
        ("bao nhie6u tie62n", "bao nhiêu tiền"),
    ]);
}

// ============================================================
// MIXED CASE SENTENCES
// ============================================================

#[test]
fn telex_mixed_case_sentences() {
    run_telex(&[
        ("Xin chaof", "Xin chào"),
        ("Xin Chaof", "Xin Chào"),
        ("Vieetj Nam", "Việt Nam"),
        ("VIEETJ NAM", "VIỆT NAM"),
        ("Thanhf phoos Hoof Chis Minh", "Thành phố Hồ Chí Minh"),
        // Ho Chi Minh City
    ]);
}

#[test]
fn vni_mixed_case_sentences() {
    run_vni(&[
        ("Xin cha2o", "Xin chào"),
        ("Vie65t Nam", "Việt Nam"),
        ("Tha2nh pho61 Ho62 Chi1 Minh", "Thành phố Hồ Chí Minh"),
    ]);
}

// ============================================================
// LONG SENTENCES
// ============================================================

#[test]
fn telex_long_sentences() {
    run_telex(&[
        (
            "vieetj nam laf mootj quoocs gia nawmf owr ddoong nam as",
            "việt nam là một quốc gia nằm ở đông nam á"
        ),
        // Vietnam is a country located in Southeast Asia

        (
            "thur ddoo cuar vieetj nam laf thanhf phoos haf nooij",
            "thủ đô của việt nam là thành phố hà nội"
        ),
        // The capital of Vietnam is Hanoi city

        (
            "nguwowif vieetj nam raats thaanf thieenj vaf hieeuws khachs",
            "người việt nam rất thân thiện và hiếu khách"
        ),
        // Vietnamese people are very friendly and hospitable
    ]);
}

#[test]
fn vni_long_sentences() {
    run_vni(&[
        (
            "vie65t nam la2 mo65t quo61c gia na72m o83 d9o6ng nam a1",
            "việt nam là một quốc gia nằm ở đông nam á"
        ),

        (
            "thu3 d9o6 cu3a vie65t nam la2 tha2nh pho61 ha2 no65i",
            "thủ đô của việt nam là thành phố hà nội"
        ),
    ]);
}

// ============================================================
// SPECIAL VIETNAMESE EXPRESSIONS
// ============================================================

#[test]
fn telex_exclamations() {
    run_telex(&[
        ("ooi troiwf oiws", "ôi trời ơi"),              // Oh my god (ooi=ôi, troiwf=trời, oiws=ơi)
        ("cheets thaatj", "chết thật"),                 // Really dead
        ("tuyeetj vowif", "tuyệt vời"),                 // Wonderful
        ("kinh khungrr", "kinh khủng"),                 // Terrible/Amazing
        ("hay quas", "hay quá"),                        // So good
        ("ddepj quas", "đẹp quá"),                      // So beautiful
    ]);
}

#[test]
fn telex_common_expressions() {
    run_telex(&[
        ("khoonng sao", "không sao"),                   // No problem
        ("dduwowcj roofif", "được rồi"),                // OK/Alright
        ("binhf thuwowngf", "bình thường"),             // Normal
        ("char bieets", "chả biết"),                    // Don't know (casual)
        ("ai maf bieets", "ai mà biết"),                // Who knows
        ("sao cuungx dduwowcj", "sao cũng được"),       // Whatever works
    ]);
}

// ============================================================
// POETRY & LITERATURE
// ============================================================

#[test]
fn telex_poetry() {
    run_telex(&[
        // Truyện Kiều - Nguyễn Du
        (
            "trawm nawm trong cooix nguwowif ta",
            "trăm năm trong cõi người ta"
        ),
        (
            "chuwx taif chuwx meenhj kheoos laf ghets nhau",
            "chữ tài chữ mệnh khéo là ghét nhau"
        ),

        // Đoàn thuyền đánh cá - Huy Cận
        (
            "mawtj troiwf ddor hawts xuoongs bieenr",
            "mặt trời đỏ hắt xuống biển"
        ),
    ]);
}
