//! Mã hồ sơ NextDNS của app (DNS mã hoá, xem src/dns.rs): đọc keys/nextdns.txt ở gốc dự án lúc dựng và đóng vào tệp chạy (biến
//! GM_NEXTDNS_ID, đọc bằng option_env!) — như ChimToTV. Tệp không có trên git: không có thì bản dựng bỏ NextDNS (DNS mã hoá đi
//! thẳng Cloudflare → Google). Không in mã ra đâu cả.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let file = root.join("keys/nextdns.txt");
    let dir = root.join("keys");
    if file.exists() {
        println!("cargo:rerun-if-changed={}", file.display());
        if let Ok(text) = std::fs::read_to_string(&file) {
            let id = text.trim();
            // Chỉ chữ thường + số — tệp sai thì bỏ NextDNS
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
                println!("cargo:rustc-env=GM_NEXTDNS_ID={id}");
            }
        }
    } else if dir.is_dir() {
        // Có thư mục keys mà chưa có tệp: thêm tệp vào là dựng lại
        println!("cargo:rerun-if-changed={}", dir.display());
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
}
