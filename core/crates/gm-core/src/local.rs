//! Nhạc trên máy — **tệp** trong máy (Documents, Tải về… — khoá `file:<đường dẫn>`, DECISIONS M15). Nhạc của app Nhạc (thư viện
//! iPod, khoá `ipod:<mã>`) do app tự đọc và phát (MediaPlayer), lõi không đụng.
//!
//! - Tên bài / ca sĩ theo tên tệp (như tệp gửi lên Telegram không ghi thuộc tính); thời lượng đọc từ đầu / cuối tệp (`info::duration`).
//! - mp3, m4a, wav…: app phát thẳng `file://`. FLAC / OGG / Opus: lõi giải từ tệp trên máy thành WAV qua cổng phát (như bài trên
//!   Telegram, chỉ khác nguồn đọc) — khoá nội bộ `local_<băm>` (đường dẫn + cỡ + giờ sửa: tệp đổi → bản giải mới).

use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::{json, Value};

use crate::library::Song;
use crate::text;

pub const PREFIX: &str = "file:";

/// Đọc tối đa `n` byte ở đầu và cuối tệp.
fn head_tail(path: &str, size: u64, n: usize) -> (Vec<u8>, Vec<u8>) {
    let Ok(mut f) = std::fs::File::open(path) else { return (Vec::new(), Vec::new()) };
    let mut head = Vec::new();
    let _ = (&mut f).take(n as u64).read_to_end(&mut head);
    let mut tail = Vec::new();
    if size > n as u64 && f.seek(SeekFrom::Start(size - n as u64)).is_ok() {
        let _ = f.take(n as u64).read_to_end(&mut tail);
    } else {
        tail = head.clone();
    }
    (head, tail)
}

/// (mã băm từ đường dẫn + cỡ + giờ sửa, giờ sửa) của tệp.
fn id_of(path: &str, md: &std::fs::Metadata) -> (i32, i64) {
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (path, md.len(), mtime).hash(&mut h);
    ((h.finish() & 0x7fff_ffff) as i32, mtime)
}

/// Khoá nội bộ (`local_<băm>`) của tệp — chỉ đọc thông tin tệp (gọi mỗi giây khi báo nhịp nghe).
pub fn inner_key(path: &str) -> Option<String> {
    let md = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some(format!("local_{}", id_of(path, &md).0))
}

/// Bài từ tệp trên máy (None: không phải tệp / không đọc được).
pub fn song(path: &str) -> Option<Song> {
    let md = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let name = Path::new(path).file_name()?.to_string_lossy().into_owned();
    let (title, artist) = text::title_from_file(&name);
    let (id, mtime) = id_of(path, &md);
    let ext = text::ext_of(&name, "");
    let (head, tail) = head_tail(path, md.len(), 64 * 1024);
    let dur = crate::info::duration(&ext, &head, &tail, md.len()).map(|d| d.round() as u32).unwrap_or(0);
    Some(Song {
        peer: "local".into(),
        id,
        title,
        artist: artist.unwrap_or_default(),
        dur,
        size: md.len(),
        name,
        ext,
        date: mtime,
        ..Default::default()
    })
}

/// JSON như `Song` của thư viện (khoá `file:<đường dẫn>`, `local`: true).
pub fn to_json(path: &str, s: &Song) -> Value {
    json!({
        "key": format!("{PREFIX}{path}"), "peer": "local", "id": s.id, "title": s.title, "artist": s.artist, "dur": s.dur,
        "size": s.size, "ext": s.ext, "date": s.date, "thumb": false, "ok": s.playable(), "src": "Tệp trên máy", "fav": false,
        "dl": Value::Null, "topic": 0, "local": true, "path": path,
    })
}

/// `file://` cho AVPlayer (mã hoá phần trăm mọi byte ngoài chữ / số / `-._~/`).
pub fn file_url(path: &str) -> String {
    let mut u = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            u.push(b as char);
        } else {
            u.push_str(&format!("%{b:02X}"));
        }
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_song_and_url() {
        let p = format!("{}/tests/data/noise44.flac", env!("CARGO_MANIFEST_DIR")).replace('\\', "/");
        let s = song(&p).unwrap();
        assert_eq!((s.ext.as_str(), s.dur, s.peer.as_str()), ("flac", 4, "local"));
        assert!(s.key().starts_with("local_"));
        assert_eq!(inner_key(&p), Some(s.key()));
        let v = to_json(&p, &s);
        assert_eq!(v["ok"], true);
        assert_eq!(v["key"], format!("file:{p}"));
        assert_eq!(file_url("/var/mobile/Nhạc hay/a b.mp3"), "file:///var/mobile/Nh%E1%BA%A1c%20hay/a%20b.mp3");
        assert!(song("/khong/co/tep.mp3").is_none());
    }
}
