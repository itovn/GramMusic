// Đóng api_id / api_hash (keys/telegram.txt — không có trên git; hoặc tệp ở biến GM_TELEGRAM_KEYS) vào thư viện.
use std::path::PathBuf;

fn main() {
    let path = std::env::var("GM_TELEGRAM_KEYS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../keys/telegram.txt"));
    println!("cargo:rerun-if-env-changed=GM_TELEGRAM_KEYS");
    println!("cargo:rerun-if-changed={}", path.display());
    if let Ok(s) = std::fs::read_to_string(&path) {
        for line in s.lines() {
            if let Some(v) = line.trim().strip_prefix("api_id=") {
                println!("cargo:rustc-env=GM_API_ID={}", v.trim());
            }
            if let Some(v) = line.trim().strip_prefix("api_hash=") {
                println!("cargo:rustc-env=GM_API_HASH={}", v.trim());
            }
        }
    }
}
