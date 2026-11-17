//! Tệp trên Telegram: vị trí tải (kèm DC — chép từ GramMini), nhận bài hát từ tin nhắn, ảnh bìa.

use grammers_client::media::{Document, Downloadable, Media, PhotoSize};
use grammers_client::message::Message;
use grammers_client::tl;

use crate::library::Song;
use crate::text;

/// Vị trí tệp + DC giữ tệp. `PhotoSize` của grammers không mang DC (tải sẽ hỏi DC chính → tệp ở DC khác bị lỗi), nên bọc lại
/// kèm DC.
#[derive(Clone)]
pub struct Loc {
    pub loc: tl::enums::InputFileLocation,
    pub dc: Option<i32>,
    pub size: Option<usize>,
}

impl Downloadable for Loc {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        Some(self.loc.clone())
    }

    fn dc_id(&self) -> Option<i32> {
        self.dc
    }

    fn size(&self) -> Option<usize> {
        self.size
    }
}

/// Tệp của document.
pub fn document_loc(d: &Document) -> Option<Loc> {
    Some(Loc { loc: d.to_raw_input_location()?, dc: d.dc_id(), size: d.size() })
}

/// Ảnh bìa (ảnh thu nhỏ đính kèm tệp nhạc): cỡ lớn nhất tải được.
pub fn doc_thumb_loc(d: &Document) -> Option<Loc> {
    let thumbs: Vec<PhotoSize> = d.thumbs().into_iter().filter(|t| t.to_raw_input_location().is_some()).collect();
    let t = thumbs.iter().max_by_key(|t| t.size())?;
    Some(Loc { loc: t.to_raw_input_location()?, dc: d.dc_id(), size: Some(t.size()) })
}

/// Ảnh đại diện nhỏ (160 px) của một peer.
pub fn avatar_loc(peer: &grammers_session::types::PeerRef, photo_id: i64, dc: i32) -> Loc {
    Loc {
        loc: tl::enums::InputFileLocation::InputPeerPhotoFileLocation(tl::types::InputPeerPhotoFileLocation {
            big: false,
            peer: peer.into(),
            photo_id,
        }),
        dc: Some(dc),
        size: None,
    }
}

/// Thuộc tính âm thanh của document.
pub fn audio_attr(d: &Document) -> Option<tl::types::DocumentAttributeAudio> {
    let tl::enums::Document::Document(doc) = d.raw.document.as_ref()? else { return None };
    doc.attributes.iter().find_map(|a| match a {
        tl::enums::DocumentAttribute::Audio(x) => Some(x.clone()),
        _ => None,
    })
}

/// Đuôi tệp được coi là bài hát khi gửi dạng «tệp» (không có thuộc tính âm thanh) — như MeloVault.
pub const AUDIO_EXTS: [&str; 11] = ["mp3", "m4a", "flac", "wav", "ogg", "opus", "aac", "aif", "aiff", "alac", "m4b"];

/// Bài hát từ một tin: tin có tệp âm thanh (không phải tin thoại), hoặc tệp có đuôi âm thanh. Tên bài / ca sĩ lấy từ thuộc
/// tính của tệp, thiếu thì đoán từ tên tệp. `peer` = khoá peer của chat chứa tin.
pub fn song_from(m: &Message, peer: &str) -> Option<Song> {
    let Some(Media::Document(d)) = m.media() else { return None };
    let audio = audio_attr(&d);
    if audio.as_ref().is_some_and(|a| a.voice) {
        return None;
    }
    let name = d.name().unwrap_or("").to_string();
    let mime = d.mime_type().unwrap_or("").to_string();
    let ext = text::ext_of(&name, &mime);
    // m3u / pls (danh sách phát), cue, lrc… có khi mang mime audio/* — không phải bài hát
    if matches!(ext.as_str(), "m3u" | "m3u8" | "pls" | "cue" | "xspf" | "lrc" | "txt" | "log" | "nfo" | "jpg" | "png" | "pdf") {
        return None;
    }
    let audio_file = AUDIO_EXTS.contains(&ext.as_str()) || (mime.starts_with("audio/") && mime != "audio/ogg");
    if audio.is_none() && !audio_file {
        return None;
    }
    if audio.is_none() && mime.starts_with("video/") {
        return None;
    }
    let (ftitle, fartist) = text::title_from_file(&name);
    let title = audio
        .as_ref()
        .and_then(|a| a.title.clone())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or(if ftitle.is_empty() { "Bài hát không tên".to_string() } else { ftitle });
    let artist = audio
        .as_ref()
        .and_then(|a| a.performer.clone())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or(fartist)
        .unwrap_or_default();
    let dur = audio.as_ref().map(|a| a.duration.max(0) as u32).unwrap_or(0);
    let size = d.size().unwrap_or(0) as u64;
    if size == 0 {
        return None;
    }
    Some(Song {
        peer: peer.to_string(),
        id: m.id(),
        title,
        artist,
        dur,
        size,
        mime,
        name,
        ext,
        date: m.date().as_second(),
        doc: d.id(),
        thumb: doc_thumb_loc(&d).is_some(),
        topic: topic_of(m),
    })
}

/// Chủ đề của tin trong nhóm diễn đàn (cờ `forum_topic`): có `reply_to_top_id` → đó là chủ đề (tin trả lời một tin trong chủ đề);
/// không có → `reply_to_msg_id` chính là chủ đề. Không thuộc chủ đề nào (General / nhóm thường) → 0. Như GramMini `reply_topic`.
pub fn topic_of(m: &Message) -> i32 {
    let tl::enums::Message::Message(x) = &m.raw else { return 0 };
    let Some(tl::enums::MessageReplyHeader::Header(h)) = &x.reply_to else { return 0 };
    if !h.forum_topic {
        return 0;
    }
    h.reply_to_top_id.or(h.reply_to_msg_id).unwrap_or(0)
}

/// base64 (ảnh mờ thu nhỏ trong JSON).
pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_known() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
