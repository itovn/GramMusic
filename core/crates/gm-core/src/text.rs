//! Chữ: bỏ dấu để tìm / xếp (tiếng Việt và chữ Latinh có dấu), tách tên bài / ca sĩ từ tên tệp, chữ cái đầu cho thanh chữ cái.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Chữ có dấu → chữ không dấu (đã viết thường). Không dùng bảng Unicode đầy đủ (thêm vài trăm KB vào tệp chạy): phủ tiếng
/// Việt + Latin-1 + Latin mở rộng A thường gặp trong tên bài hát.
const GROUPS: [(&str, char); 14] = [
    ("àáảãạăằắẳẵặâầấẩẫậäåāą", 'a'),
    ("çćč", 'c'),
    ("đď", 'd'),
    ("èéẻẽẹêềếểễệëēėęě", 'e'),
    ("ğ", 'g'),
    ("ìíỉĩịīįï", 'i'),
    ("łľ", 'l'),
    ("ñńň", 'n'),
    ("òóỏõọôồốổỗộơờớởỡợöøōő", 'o'),
    ("ŕř", 'r'),
    ("śšş", 's'),
    ("ť", 't'),
    ("ùúủũụưừứửữựüūůűų", 'u'),
    ("ỳýỷỹỵÿźżž", 'y'),
];

fn table() -> &'static HashMap<char, char> {
    static T: OnceLock<HashMap<char, char>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        for (src, dst) in GROUPS {
            for c in src.chars() {
                // ź ż ž nằm chung dòng «y» cho gọn bảng — sửa lại ở đây
                m.insert(c, if matches!(c, 'ź' | 'ż' | 'ž') { 'z' } else { dst });
            }
        }
        m
    })
}

/// Viết thường + bỏ dấu: «Đêm Trăng Tình Yêu» → «dem trang tinh yeu». Dấu nối âm tách rời (U+0300–U+036F, tiếng Việt
/// gõ kiểu tổ hợp) bị bỏ; ß → ss, æ → ae, œ → oe.
pub fn fold(s: &str) -> String {
    let t = table();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        for l in c.to_lowercase() {
            match l {
                'ß' => out.push_str("ss"),
                'æ' => out.push_str("ae"),
                'œ' => out.push_str("oe"),
                _ => out.push(*t.get(&l).unwrap_or(&l)),
            }
        }
    }
    out
}

/// Các từ cần tìm (đã bỏ dấu), bỏ khoảng trắng thừa.
pub fn query_words(q: &str) -> Vec<String> {
    fold(q).split_whitespace().map(str::to_string).collect()
}

/// `hay` (đã bỏ dấu) chứa đủ mọi từ.
pub fn matches(hay: &str, words: &[String]) -> bool {
    words.iter().all(|w| hay.contains(w.as_str()))
}

/// Khoá xếp theo tên: bỏ dấu, bỏ ký tự đầu không phải chữ / số (dấu ngoặc, «-», «.»), bỏ mạo từ «the » đầu tên tiếng Anh.
pub fn sort_key(s: &str) -> String {
    let f = fold(s);
    let t = f.trim_start_matches(|c: char| !c.is_alphanumeric());
    let t = t.strip_prefix("the ").unwrap_or(t);
    t.to_string()
}

/// Chữ cái cho thanh chữ cái: A–Z theo khoá xếp, còn lại «#».
pub fn letter(key: &str) -> char {
    match key.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        _ => '#',
    }
}

/// Bỏ số thứ tự bài ở đầu tên tệp: «01. », «01 - », «1-», «Track 03 ».
fn strip_track_no(s: &str) -> &str {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    let t = if lower.starts_with("track ") { &t[6..] } else { t };
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return s.trim();
    }
    let rest = &t[digits..];
    let rest2 = rest.trim_start_matches([' ', '.', '-', '_', ')']);
    // «01 Tên» cần ít nhất một dấu cách / dấu chấm / gạch sau số; «2NE1» không phải số thứ tự
    if rest2.len() == rest.len() || rest2.is_empty() {
        return s.trim();
    }
    rest2
}

/// Tên bài + ca sĩ khi tệp không ghi thuộc tính âm thanh: theo tên tệp (bỏ đuôi, số thứ tự, «_» thành dấu cách),
/// dạng «Ca sĩ - Tên bài» thì tách đôi.
pub fn title_from_file(name: &str) -> (String, Option<String>) {
    let stem = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && e.len() <= 5 => s,
        _ => name,
    };
    let clean = stem.replace('_', " ");
    let clean = strip_track_no(&clean).trim().to_string();
    for sep in [" - ", " – ", " — "] {
        if let Some((a, t)) = clean.split_once(sep) {
            let (a, t) = (a.trim(), t.trim());
            if !a.is_empty() && !t.is_empty() {
                return (t.to_string(), Some(a.to_string()));
            }
        }
    }
    (clean, None)
}

/// Đuôi tệp viết thường (≤ 5 ký tự chữ / số), hoặc theo mime.
pub fn ext_of(name: &str, mime: &str) -> String {
    let from_name = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()));
    from_name.unwrap_or_else(|| {
        match mime {
            "audio/mpeg" | "audio/mp3" | "audio/mpeg3" => "mp3",
            "audio/mp4" | "audio/x-m4a" | "audio/m4a" | "audio/aac" => "m4a",
            "audio/flac" | "audio/x-flac" => "flac",
            "audio/ogg" | "audio/vorbis" => "ogg",
            "audio/opus" => "opus",
            "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
            "audio/aiff" | "audio/x-aiff" => "aiff",
            "video/mp4" => "mp4",
            _ => "bin",
        }
        .to_string()
    })
}

/// Phát được không: AVPlayer của iOS 5 / 6 đọc thẳng (mp3, AAC / ALAC trong m4a / mp4, ADTS, WAV, AIFF, CAF), hoặc lõi giải mã thành
/// WAV (FLAC, OGG Vorbis, Opus — decode.rs).
pub fn playable_ext(ext: &str) -> bool {
    matches!(ext, "mp3" | "m4a" | "m4b" | "mp4" | "aac" | "wav" | "aif" | "aiff" | "caf" | "3gp" | "m4r" | "mov")
        || crate::decode::needs_decode(ext)
}

/// Content-Type cho AVPlayer (cổng phát nội bộ).
pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" | "m4r" => "audio/x-m4a",
        "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/x-wav",
        "aif" | "aiff" => "audio/x-aiff",
        "caf" => "audio/x-caf",
        "3gp" => "audio/3gpp",
        "mov" => "video/quicktime",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_vietnamese() {
        assert_eq!(fold("Đêm Trăng Tình Yêu"), "dem trang tinh yeu");
        assert_eq!(fold("Người Lạ Ơi"), "nguoi la oi");
        assert_eq!(fold("Beyoncé – Crazy in Love"), "beyonce – crazy in love");
        // tổ hợp: «e» + dấu sắc rời
        assert_eq!(fold("Cafe\u{301}"), "cafe");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("ÀÁẢÃẠĂẰẮẲẴẶÂẦẤẨẪẬ"), "a".repeat(17));
        assert_eq!(fold("ÒÓỎÕỌÔỒỐỔỖỘƠỜỚỞỠỢ"), "o".repeat(17));
        assert_eq!(fold("ÙÚỦŨỤƯỪỨỬỮỰ ỲÝỶỸỴ Đ"), format!("{} {} d", "u".repeat(11), "y".repeat(5)));
        assert_eq!(fold("Žluťoučký"), "zlutoucky");
    }

    #[test]
    fn search_words() {
        let hay = fold("Lạc Trôi Sơn Tùng M-TP");
        assert!(matches(&hay, &query_words("lac troi")));
        assert!(matches(&hay, &query_words("SƠN TÙNG")));
        assert!(!matches(&hay, &query_words("hay trao cho anh")));
    }

    #[test]
    fn sort_keys_and_letters() {
        assert_eq!(sort_key("The Beatles"), "beatles");
        assert_eq!(sort_key("(Live) Ánh Nắng"), "live) anh nang");
        assert_eq!(letter(&sort_key("Ánh Nắng")), 'A');
        assert_eq!(letter(&sort_key("Đi Về Nhà")), 'D');
        assert_eq!(letter(&sort_key("2 Phút Hơn")), '#');
        assert_eq!(letter(""), '#');
    }

    #[test]
    fn titles_from_files() {
        assert_eq!(title_from_file("01. Sơn Tùng M-TP - Lạc Trôi.mp3"), ("Lạc Trôi".into(), Some("Sơn Tùng M-TP".into())));
        assert_eq!(title_from_file("Track 03 Hello.flac"), ("Hello".into(), None));
        assert_eq!(title_from_file("2NE1 - I Am The Best.m4a"), ("I Am The Best".into(), Some("2NE1".into())));
        assert_eq!(title_from_file("song_name_here.mp3"), ("song name here".into(), None));
        assert_eq!(title_from_file("noext"), ("noext".into(), None));
    }

    #[test]
    fn exts() {
        assert_eq!(ext_of("A.MP3", ""), "mp3");
        assert_eq!(ext_of("", "audio/flac"), "flac");
        assert_eq!(ext_of("weird.name.with.dots.toolongext", "audio/mpeg"), "mp3");
        assert!(playable_ext("m4a") && playable_ext("flac") && playable_ext("opus") && !playable_ext("dts"));
    }
}
