// Dựng libopus 1.4 (core/vendor/opus-1.4 — bản phát hành chính thức của xiph.org, kiểm SHA-256) bằng cc: danh sách tệp lấy
// từ *_sources.mk (phần C chung, dấu chấm động — không SIMD). Dựng cho iOS thì cc dùng clang của Theos
// (CC_armv7_apple_ios6 do tools/rust/cargo-ios6.sh đặt).
use std::path::{Path, PathBuf};

/// Đọc biến `NAME = a.c b.c \ …` (nhiều dòng) trong tệp .mk.
fn mk_list(path: &Path, name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("đọc .mk");
    let mut out = Vec::new();
    let mut on = false;
    for line in text.lines() {
        let l = line.trim();
        if !on {
            if let Some(rest) = l.strip_prefix(&format!("{name} =")) {
                on = true;
                out.extend(rest.split_whitespace().filter(|t| *t != "\\").map(str::to_string));
                if !l.ends_with('\\') {
                    break;
                }
            }
            continue;
        }
        out.extend(l.split_whitespace().filter(|t| *t != "\\").map(str::to_string));
        if !l.ends_with('\\') {
            break;
        }
    }
    out
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/opus-1.4");
    let mut files = Vec::new();
    for (mk, vars) in [
        ("celt_sources.mk", &["CELT_SOURCES"][..]),
        ("silk_sources.mk", &["SILK_SOURCES", "SILK_SOURCES_FLOAT"][..]),
        ("opus_sources.mk", &["OPUS_SOURCES", "OPUS_SOURCES_FLOAT"][..]),
    ] {
        println!("cargo:rerun-if-changed={}", root.join(mk).display());
        for v in vars {
            files.extend(mk_list(&root.join(mk), v));
        }
    }
    assert!(files.len() > 100, "danh sách tệp libopus quá ngắn: {}", files.len());
    let mut b = cc::Build::new();
    for f in &files {
        b.file(root.join(f));
    }
    b.include(root.join("include"))
        .include(root.join("celt"))
        .include(root.join("silk"))
        .include(root.join("silk/float"))
        .include(root.join("src"))
        .define("OPUS_BUILD", None)
        .define("USE_ALLOCA", None)
        .define("HAVE_LRINTF", None)
        .define("HAVE_LRINT", None)
        .warnings(false)
        .opt_level(2);
    if !std::env::var("TARGET").unwrap_or_default().contains("windows") {
        b.define("HAVE_ALLOCA_H", None);
    }
    b.compile("opus");
}
