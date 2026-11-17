//! Lời bài hát từ **LRCLIB** (lrclib.net — kho lời miễn phí, không cần khoá, có lời chạy theo thời gian dạng LRC). Chỉ chạy khi
//! anh bật trong Cài đặt (tên bài / ca sĩ / thời lượng được gửi tới LRCLIB — DECISIONS M14).
//!
//! - Tìm: `api/get` (tên bài + ca sĩ + thời lượng, khớp ±2 giây) với tên gốc, rồi tên đã bỏ phần trong ngoặc; không thấy thì
//!   `api/search` và chọn bản có lời chạy, thời lượng gần nhất (lệch ≤ 5 giây).
//! - Lời chạy (LRC) tách sẵn thành dòng có mốc giây — app chỉ việc tô dòng đang hát.
//! - Lưu `lyrics/<khoá>.json`: lời chạy / bài không lời giữ mãi; lời thường / không thấy thì 7 ngày sau hỏi lại.

use std::time::Duration;

use serde_json::{json, Value};

use crate::library::Song;

const API: &str = "https://lrclib.net/api";
/// Không thấy lời / chỉ có lời thường: chừng này lâu mới hỏi lại.
pub const MISS_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// Phiên bản bản lưu `lyrics/<khoá>.json` (2: đã tìm thêm bản có lời chạy).
pub const CACHE_V: u64 = 2;

/// Tra tên miền cho reqwest qua DNS mã hoá của gm-net (DoH → DoT, NextDNS → Cloudflare → Google; tắt / lỗi hết thì DNS của máy —
/// DECISIONS M17).
pub(crate) struct DohResolve;

impl reqwest::dns::Resolve for DohResolve {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = gm_net::dns::lookup_host(&host, 0).await?;
            let it: reqwest::dns::Addrs = Box::new(addrs.into_iter());
            Ok(it)
        })
    }
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .dns_resolver(std::sync::Arc::new(DohResolve))
            .user_agent(concat!("GramMusic/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            // Kết nối nghỉ lâu dễ đã chết khi mạng máy đổi (Wi-Fi chập chờn) — dùng lại thì chờ tới hết giờ (đo: 21 giây)
            .pool_idle_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap_or_default()
    })
}

/// Mốc `mm:ss.xx` (hoặc `mm:ss`, `mm:ss:xx`) → giây.
fn parse_time(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    let m: u32 = m.trim().parse().ok()?;
    let s = s.trim();
    // `mm:ss:xx` (một số tệp LRC cũ) → `mm:ss.xx`
    let s = match s.split_once(':') {
        Some((a, b)) => format!("{a}.{b}"),
        None => s.to_string(),
    };
    let s: f64 = s.parse().ok()?;
    (0.0..61.0).contains(&s).then_some(m as f64 * 60.0 + s)
}

/// Bỏ mốc từng chữ của LRC mở rộng (`<00:12.34>`).
fn strip_word_times(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        match rest[i..].find('>') {
            Some(j) if parse_time(&rest[i + 1..i + j]).is_some() => {
                out.push_str(&rest[..i]);
                rest = &rest[i + j + 1..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// LRC → các dòng (giây, chữ) xếp theo thời gian. Một dòng nhiều mốc (điệp khúc) → nhiều dòng; thẻ `[offset:±ms]` được áp dụng.
pub fn parse_lrc(lrc: &str) -> Vec<(f64, String)> {
    let mut offset = 0.0;
    let mut out: Vec<(f64, String)> = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(t) = parse_time(tag) {
                times.push(t);
            } else if let Some(v) = tag.strip_prefix("offset:") {
                if let Ok(ms) = v.trim().parse::<f64>() {
                    offset = ms / 1000.0;
                }
            }
            rest = &rest[end + 1..];
        }
        let text = strip_word_times(rest);
        for t in times {
            out.push((t, text.clone()));
        }
    }
    // offset dương = lời hiện sớm hơn
    for l in &mut out {
        l.0 = (l.0 - offset).max(0.0);
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Tên để tìm: bỏ phần trong ngoặc («(Official MV)», «[FLAC 24bit]»…), số thứ tự đầu dạng «01.» / «01 -», gộp dấu cách.
pub fn clean_name(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' | '【' | '（' => depth += 1,
            ')' | ']' | '}' | '】' | '）' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let t = out.trim();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    let t = if (1..=3).contains(&digits) {
        let after = &t[digits..];
        let a = after.trim_start();
        if a.starts_with('.') || a.starts_with('-') {
            a[1..].trim_start()
        } else {
            t
        }
    } else {
        t
    };
    t.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ca sĩ chính (bỏ «feat.», «ft.», «&», «,», «x» phía sau) — LRCLIB thường ghi một ca sĩ.
fn main_artist(a: &str) -> String {
    let low = a.to_lowercase();
    let mut cut = a.len();
    for sep in [" feat", " ft.", " ft ", " & ", ", ", " x ", " và ", ";"] {
        if let Some(i) = low.find(sep) {
            cut = cut.min(i);
        }
    }
    a[..cut].trim().to_string()
}

async fn get_json(url: &str, q: &[(&str, String)]) -> Result<Option<Value>, String> {
    let resp = client().get(url).query(q).send().await.map_err(|e| format!("Không kết nối được LRCLIB: {e}"))?;
    let code = resp.status().as_u16();
    if code == 404 {
        return Ok(None);
    }
    if code != 200 {
        return Err(format!("LRCLIB trả HTTP {code}"));
    }
    let text = resp.text().await.map_err(|e| format!("Không đọc được trả lời của LRCLIB: {e}"))?;
    serde_json::from_str::<Value>(&text).map(Some).map_err(|e| format!("LRCLIB trả dữ liệu lạ: {e}"))
}

/// Chọn bản hợp nhất trong kết quả tìm: lệch thời lượng ≤ 5 giây (nếu biết), ưu tiên có lời chạy, rồi lệch ít.
fn pick(items: &[Value], dur: u32, title: &str) -> Option<Value> {
    let want = crate::text::fold(title);
    items
        .iter()
        .filter(|v| v.get("plainLyrics").is_some_and(|x| x.is_string()) || v.get("syncedLyrics").is_some_and(|x| x.is_string())
            || v.get("instrumental").and_then(Value::as_bool) == Some(true))
        .filter_map(|v| {
            let d = v.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
            let diff = if dur > 0 && d > 0.0 { (d - dur as f64).abs() } else { 0.0 };
            if dur > 0 && d > 0.0 && diff > 5.0 {
                return None;
            }
            // Không biết thời lượng: chỉ nhận khi tên khớp hẳn
            if dur == 0 && crate::text::fold(v.get("trackName").and_then(Value::as_str).unwrap_or("")) != want {
                return None;
            }
            let synced = v.get("syncedLyrics").is_some_and(|x| x.as_str().is_some_and(|s| !s.is_empty()));
            Some((!synced, (diff * 10.0) as i64, v.clone()))
        })
        .min_by_key(|(s, d, _)| (*s, *d))
        .map(|(_, _, v)| v)
}

fn has_synced(v: &Value) -> bool {
    v.get("syncedLyrics").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty())
}

fn instrumental(v: &Value) -> bool {
    v.get("instrumental").and_then(Value::as_bool) == Some(true)
}

/// Hỏi LRCLIB lời của bài. Ok(None) = không có. Ưu tiên **lời chạy**: LRCLIB thường có nhiều bản cho một bài — bản khớp `api/get`
/// có khi chỉ có lời thường (vd. «All The Way / One For My Baby — Kenny G»), thì tìm thêm bản có lời chạy cùng
/// thời lượng; không có mới dùng lời thường.
pub async fn fetch(song: &Song) -> Result<Option<Value>, String> {
    let title_raw = song.title.trim().to_string();
    let title = clean_name(&title_raw);
    let artist = main_artist(&clean_name(&song.artist));
    if title.is_empty() {
        return Ok(None);
    }
    let mut best: Option<Value> = None;
    if !artist.is_empty() && song.dur > 0 {
        let mut tries = vec![title_raw.clone()];
        if title != title_raw {
            tries.push(title.clone());
        }
        for t in tries {
            let q = [("track_name", t), ("artist_name", artist.clone()), ("duration", song.dur.to_string())];
            if let Some(v) = get_json(&format!("{API}/get"), &q).await? {
                if has_synced(&v) || instrumental(&v) {
                    return Ok(Some(v));
                }
                best = Some(v);
                break;
            }
        }
    }
    let mut searches: Vec<Vec<(&str, String)>> = Vec::new();
    if artist.is_empty() {
        searches.push(vec![("q", title.clone())]);
    } else {
        searches.push(vec![("track_name", title.clone()), ("artist_name", artist.clone())]);
        // LRCLIB tìm theo cặp chặt; «tên bài ca sĩ» mờ hơn
        searches.push(vec![("q", format!("{title} {artist}"))]);
    }
    for q in searches {
        let found = get_json(&format!("{API}/search"), &q).await?;
        let items = found.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
        if let Some(v) = pick(&items, song.dur, &title) {
            if has_synced(&v) {
                return Ok(Some(v));
            }
            if best.is_none() {
                best = Some(v);
            }
        }
    }
    Ok(best)
}

/// Bản ghi LRCLIB → kết quả cho app: `{found, synced, lines: [{t, s}], plain, instrumental, title, artist}`.
pub fn to_result(rec: Option<&Value>) -> Value {
    let Some(r) = rec else {
        return json!({"found": false, "synced": false, "lines": [], "plain": "", "instrumental": false, "v": CACHE_V});
    };
    let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let lines: Vec<Value> = parse_lrc(&s("syncedLyrics"))
        .into_iter()
        .map(|(t, s)| json!({"t": (t * 100.0).round() / 100.0, "s": s}))
        .collect();
    let mut plain = s("plainLyrics");
    if plain.trim().is_empty() && !lines.is_empty() {
        plain = lines.iter().filter_map(|l| l["s"].as_str()).collect::<Vec<_>>().join("\n");
    }
    json!({
        "found": true,
        "synced": !lines.is_empty(),
        "lines": lines,
        "plain": plain,
        "instrumental": r.get("instrumental").and_then(Value::as_bool).unwrap_or(false),
        "title": s("trackName"),
        "artist": s("artistName"),
        "v": CACHE_V,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_lines() {
        let lrc = "[ar:Ai đó]\n[offset:+500]\n[00:12.50]Dòng một\n[00:05.00][01:00.00]Điệp <00:05.20>khúc\n[00:20.00]\n[bad]x";
        let l = parse_lrc(lrc);
        assert_eq!(l.len(), 4);
        assert_eq!(l[0], (4.5, "Điệp khúc".to_string()));
        assert_eq!(l[1], (12.0, "Dòng một".to_string()));
        assert_eq!(l[2], (19.5, String::new()));
        assert_eq!(l[3], (59.5, "Điệp khúc".to_string()));
        assert_eq!(parse_time("03:07:25"), Some(187.25));
    }

    #[test]
    fn names() {
        assert_eq!(clean_name("01. Em Của Ngày Hôm Qua (Official MV) [FLAC 24bit]"), "Em Của Ngày Hôm Qua");
        assert_eq!(clean_name("7 Years"), "7 Years");
        assert_eq!(clean_name("99 Problems"), "99 Problems");
        assert_eq!(clean_name("03 - Hello_World"), "Hello World");
        assert_eq!(main_artist("Sơn Tùng M-TP feat. Snoop Dogg"), "Sơn Tùng M-TP");
        assert_eq!(main_artist("Adele"), "Adele");
    }

    #[test]
    fn picks_synced_close_duration() {
        let items = vec![
            json!({"trackName": "A", "duration": 200.0, "plainLyrics": "x", "syncedLyrics": null}),
            json!({"trackName": "A", "duration": 203.0, "plainLyrics": "x", "syncedLyrics": "[00:01.00]y"}),
            json!({"trackName": "A", "duration": 260.0, "plainLyrics": "x", "syncedLyrics": "[00:01.00]z"}),
        ];
        let v = pick(&items, 201, "A").unwrap();
        assert_eq!(v["duration"], 203.0);
        assert!(pick(&items[2..], 201, "A").is_none());
        let r = to_result(Some(&v));
        assert_eq!(r["synced"], true);
        assert_eq!(r["lines"][0]["s"], "y");
    }

    /// Gọi thật LRCLIB (cần mạng): `cargo test -p gm-core lyrics -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn live_lrclib() {
        let song = Song { title: "Hello (Official Video)".into(), artist: "Adele".into(), dur: 295, ..Default::default() };
        let t = std::time::Instant::now();
        let v = to_result(fetch(&song).await.unwrap().as_ref());
        println!("{} ms, synced {}, {} dòng, {} / {}", t.elapsed().as_millis(), v["synced"], v["lines"].as_array().unwrap().len(), v["title"], v["artist"]);
        assert_eq!(v["found"], true);
        assert_eq!(v["synced"], true);
        // `api/get` của bài này chỉ có lời thường → phải tìm ra bản có lời chạy nếu LRCLIB có
        let song = Song { title: "All The Way / One For My Baby".into(), artist: "Kenny G".into(), dur: 368, ..Default::default() };
        let v = to_result(fetch(&song).await.unwrap().as_ref());
        println!("Kenny G: found {}, synced {}, {} dòng, {} / {}", v["found"], v["synced"], v["lines"].as_array().unwrap().len(), v["title"], v["artist"]);
    }
}
