//! Lõi GramMusic (phần không dính giao diện): phiên đăng nhập (`session`, chép từ GramMini), thư viện nhạc (`library`,
//! `playlists`), phát qua cổng nội bộ (`stream`), dịch vụ cho app (`service`).

pub mod cache;
pub mod decode;
pub mod info;
pub mod library;
pub mod local;
pub mod lyrics;
pub mod media;
pub mod oggopus;
pub mod playlists;
pub mod session;
pub mod stream;
pub mod text;

pub use session::FileSession;
pub mod service;
pub mod sync;

pub use service::{Config, Core, CoreError, Emit};

/// Nhật ký lõi cho các module không giữ `Core` (stream, decode): service.rs gắn hàm ghi `core.log` lúc mở lõi.
static LOG: std::sync::OnceLock<std::sync::Arc<dyn Fn(&str) + Send + Sync>> = std::sync::OnceLock::new();

pub fn set_logger(f: std::sync::Arc<dyn Fn(&str) + Send + Sync>) {
    let _ = LOG.set(f);
}

pub fn dlog(msg: &str) {
    if let Some(f) = LOG.get() {
        f(msg);
    }
}
