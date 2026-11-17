//! Trạng thái quét thư viện (`sync.json`). Quét **từng chat** của tài khoản (`messages.search` trong chat, hai lượt: tệp âm thanh
//! `InputMessagesFilterMusic`, rồi tài liệu có đuôi nhạc `InputMessagesFilterDocument` — bài gửi dạng «tệp», hay gặp với FLAC),
//! mỗi trang 100 tin. Tìm toàn cục `messages.searchGlobal` với từ khoá rỗng Telegram trả gần như không có gì (DECISIONS M7).
//!
//! - **Quét toàn bộ** (lần đầu, hoặc người dùng bấm): mọi chat; chat xong ghi vào `done` → tắt app giữa chừng thì lần sau quét
//!   tiếp từ chat chưa xong.
//! - **Lấy bài mới** (các lần sau): chỉ chat có tin mới từ lần quét trước (và chat chưa quét bao giờ); trong chat dừng khi gặp liền
//!   10 bài đã có hoặc tới tin cũ hơn lần quét trước của chat đó.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Các lượt quét theo thứ tự.
pub const PHASES: [&str; 2] = ["music", "document"];
/// Lấy bài mới: gặp liền chừng này bài đã có thì coi như đã bắt kịp.
pub const OVERLAP: u32 = 10;
/// Mỗi trang.
pub const PAGE: i32 = 100;
/// Phiên bản dữ liệu quét hiện tại (2: bài có chủ đề của nhóm diễn đàn).
pub const VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Cursor {
    pub offset_rate: i32,
    /// Khoá peer của tin cuối trang trước ("" = từ đầu).
    pub offset_peer: String,
    pub offset_id: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SyncState {
    /// Lượt đang quét dở của lần quét toàn bộ ("" = không có lần nào dở).
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub cursor: Cursor,
    /// Đã quét toàn bộ xong ít nhất một lần.
    #[serde(default)]
    pub full_done: bool,
    /// Lần quét xong gần nhất (Unix giây).
    #[serde(default)]
    pub last_sync: i64,
    /// Số trang đã đọc của lần quét toàn bộ đang dở (để báo tiến độ).
    #[serde(default)]
    pub pages: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Chat đã quét (khoá peer → lần quét xong gần nhất, Unix giây) — chat có trong đây thì lần sau chỉ lấy tin mới.
    #[serde(default)]
    pub chats: HashMap<String, i64>,
    /// Chat đã xong của lần quét toàn bộ đang dở (tắt app giữa chừng thì lần sau bỏ qua các chat này).
    #[serde(default)]
    pub done: Vec<String>,
    /// Phiên bản dữ liệu quét: thấp hơn `VERSION` (bản cũ chưa ghi chủ đề của bài…) → quét toàn bộ lại một lần.
    #[serde(default)]
    pub version: u32,
    /// Đang quét chat thứ mấy / tổng số chat (báo tiến độ, không lưu).
    #[serde(skip)]
    pub chat_i: usize,
    #[serde(skip)]
    pub chat_n: usize,
}

impl SyncState {
    pub fn load(dir: &std::path::Path) -> SyncState {
        std::fs::read(dir.join("sync.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &std::path::Path) -> std::io::Result<()> {
        crate::library::write_atomic(&dir.join("sync.json"), &serde_json::to_vec(self).unwrap_or_default())
    }

    /// Lần quét toàn bộ mới (hoặc tiếp lần dở): (lượt bắt đầu, con trỏ).
    pub fn resume_point(&self) -> (usize, Cursor) {
        match PHASES.iter().position(|p| *p == self.phase) {
            Some(i) => (i, self.cursor.clone()),
            None => (0, Cursor::default()),
        }
    }
}

/// Đếm bài đã có liên tiếp (chế độ lấy bài mới).
#[derive(Default)]
pub struct Overlap {
    run: u32,
}

impl Overlap {
    /// Ghi nhận một bài (đã có hay mới); true = đủ liền `OVERLAP` bài đã có → dừng lượt.
    pub fn see(&mut self, known: bool) -> bool {
        if known {
            self.run += 1;
        } else {
            self.run = 0;
        }
        self.run >= OVERLAP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_and_overlap() {
        let mut s = SyncState::default();
        assert_eq!(s.resume_point(), (0, Cursor::default()));
        s.phase = "document".into();
        s.cursor = Cursor { offset_rate: 5, offset_peer: "-1001".into(), offset_id: 77 };
        assert_eq!(s.resume_point(), (1, s.cursor.clone()));
        let mut o = Overlap::default();
        for _ in 0..9 {
            assert!(!o.see(true));
        }
        assert!(!o.see(false));
        for _ in 0..9 {
            assert!(!o.see(true));
        }
        assert!(o.see(true));
    }
}
