//! Thông số tệp nhạc cho màn Đang phát (định dạng · tần số lấy mẫu · số bit · bitrate · dung lượng) — đọc từ phần đầu tệp đã có
//! trên máy (khúc đầu tải lúc mở bài), không tải thêm. Telegram chỉ ghi tên / ca sĩ / thời lượng.

use serde_json::{json, Value};

use crate::library::Song;

/// Thông số đọc được từ phần đầu tệp.
#[derive(Debug, Default, PartialEq)]
pub struct Info {
    pub format: String,
    pub rate: Option<u32>,
    pub bits: Option<u32>,
    pub channels: Option<u32>,
    /// kbit/s ghi trong tệp (MP3 CBR, Vorbis danh định); None → dùng trung bình cỡ / thời lượng.
    pub kbps: Option<u32>,
    pub vbr: bool,
    pub lossless: bool,
}

/// Đọc thông số từ `head` (phần đầu tệp) theo đuôi tệp.
pub fn parse(ext: &str, head: &[u8]) -> Info {
    let mut i = Info { format: ext.to_uppercase(), ..Default::default() };
    match ext {
        "mp3" => {
            if let Some((rate, kbps, ch, vbr)) = mp3(head) {
                i.rate = Some(rate);
                i.channels = Some(ch);
                i.vbr = vbr;
                i.kbps = (!vbr).then_some(kbps);
            }
        }
        "flac" => {
            i.lossless = true;
            if head.len() >= 22 && &head[0..4] == b"fLaC" && head[4] & 0x7f == 0 {
                let b = &head[8..];
                i.rate = Some((b[10] as u32) << 12 | (b[11] as u32) << 4 | (b[12] as u32) >> 4);
                i.channels = Some(((b[12] as u32 >> 1) & 7) + 1);
                i.bits = Some((((b[12] as u32) & 1) << 4 | (b[13] as u32) >> 4) + 1);
            }
        }
        "wav" => {
            i.lossless = true;
            // `get`: «fmt » nằm sát cuối phần đầu tệp (tệp cụt / tệp lạ ai đó gửi lên nhóm) — cắt thẳng là panic, app sập
            if let Some(f) = find(head, b"fmt ").and_then(|j| head.get(j + 8..)) {
                if f.len() >= 16 {
                    i.channels = Some(u16::from_le_bytes([f[2], f[3]]) as u32);
                    i.rate = Some(u32::from_le_bytes([f[4], f[5], f[6], f[7]]));
                    i.bits = Some(u16::from_le_bytes([f[14], f[15]]) as u32);
                }
            }
        }
        "ogg" | "oga" | "opus" => {
            if let Some(j) = find(head, b"OpusHead") {
                // Opus luôn giải ở 48 kHz (tần số gốc ghi ở byte 12–15 chỉ để tham khảo)
                let v = &head[j..];
                i.format = "OPUS".into();
                if v.len() >= 19 {
                    i.channels = Some(v[9] as u32);
                }
                i.rate = Some(48_000);
            } else if let Some(j) = find(head, b"\x01vorbis") {
                let v = &head[j + 7..];
                if v.len() >= 17 {
                    i.format = "OGG".into();
                    i.channels = Some(v[4] as u32);
                    i.rate = Some(u32::from_le_bytes([v[5], v[6], v[7], v[8]]));
                    let nominal = i32::from_le_bytes([v[13], v[14], v[15], v[16]]);
                    i.kbps = (nominal > 0).then(|| nominal as u32 / 1000);
                }
            }
        }
        "m4a" | "m4b" | "mp4" => {
            // Mục mô tả mẫu trong moov (nếu moov nằm đầu tệp): «alac» = Apple Lossless, «mp4a» = AAC
            let entry = find(head, b"alac").map(|j| (j, true)).or_else(|| find(head, b"mp4a").map(|j| (j, false)));
            if let Some((j, alac)) = entry {
                i.format = if alac { "ALAC".into() } else { "AAC".into() };
                i.lossless = alac;
                let e = &head[j..];
                if e.len() >= 30 {
                    let ch = u16::from_be_bytes([e[20], e[21]]) as u32;
                    let bits = u16::from_be_bytes([e[22], e[23]]) as u32;
                    let rate = u16::from_be_bytes([e[28], e[29]]) as u32;
                    if (1..=8).contains(&ch) && (8000..=384_000).contains(&rate) {
                        i.channels = Some(ch);
                        i.rate = Some(rate);
                        i.bits = alac.then_some(bits);
                    }
                }
            } else {
                i.format = "AAC".into();
            }
        }
        "aif" | "aiff" | "alac" => i.lossless = true,
        _ => {}
    }
    i
}

/// JSON cho app: bitrate thiếu thì tính trung bình theo cỡ / thời lượng.
pub fn to_json(song: &Song, i: &Info) -> Value {
    let avg = (song.dur > 0).then(|| (song.size * 8 / song.dur as u64 / 1000) as u32);
    json!({
        "format": i.format, "rate": i.rate, "bits": i.bits, "channels": i.channels,
        "kbps": i.kbps.or(avg), "vbr": i.vbr, "lossless": i.lossless, "size": song.size,
        "hires": i.rate.is_some_and(|r| r > 48_000) || i.bits.is_some_and(|b| b > 16),
    })
}

/// Thời lượng (giây) đọc từ phần đầu / cuối tệp — cho tệp trên máy (bài trên Telegram có sẵn thời lượng). `tail`: 64 KB cuối.
pub fn duration(ext: &str, head: &[u8], tail: &[u8], size: u64) -> Option<f64> {
    let be32 = |b: &[u8], i: usize| b.get(i..i + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]));
    let d = match ext {
        "flac" => {
            if head.len() < 26 || &head[0..4] != b"fLaC" || head[4] & 0x7f != 0 {
                return None;
            }
            let b = &head[8..];
            let rate = (b[10] as u64) << 12 | (b[11] as u64) << 4 | (b[12] as u64) >> 4;
            let total = ((b[13] as u64) & 0x0f) << 32 | (b[14] as u64) << 24 | (b[15] as u64) << 16 | (b[16] as u64) << 8 | b[17] as u64;
            (rate > 0 && total > 0).then(|| total as f64 / rate as f64)?
        }
        "wav" => {
            let f = head.get(find(head, b"fmt ")? + 8..)?;
            let byte_rate = u32::from_le_bytes(f.get(8..12)?.try_into().ok()?) as f64;
            let j = find(head, b"data")?;
            let n = u32::from_le_bytes(head.get(j + 4..j + 8)?.try_into().ok()?) as u64;
            let n = if n == 0 || n == u32::MAX as u64 { size.saturating_sub(j as u64 + 8) } else { n };
            (byte_rate > 0.0).then(|| n as f64 / byte_rate)?
        }
        "mp3" => {
            let (i, ver, rate, kbps) = mp3_frame(head)?;
            let spf = if ver == 3 { 1152.0 } else { 576.0 };
            let frame = &head[i..head.len().min(i + 200)];
            let frames = match (find(frame, b"Xing").or_else(|| find(frame, b"Info")), find(frame, b"VBRI")) {
                (Some(x), _) if be32(frame, x + 4).is_some_and(|fl| fl & 1 != 0) => be32(frame, x + 8),
                (_, Some(v)) => be32(frame, v + 14),
                _ => None,
            };
            match frames {
                Some(n) if n > 0 => n as f64 * spf / rate as f64,
                _ => size.saturating_sub(i as u64) as f64 * 8.0 / (kbps as f64 * 1000.0),
            }
        }
        "ogg" | "oga" | "opus" => {
            let (rate, skip) = if let Some(j) = find(head, b"OpusHead") {
                (48_000.0, u16::from_le_bytes(head.get(j + 10..j + 12)?.try_into().ok()?) as f64)
            } else {
                let v = &head[find(head, b"vorbis")? + 7..];
                (u32::from_le_bytes(v.get(5..9)?.try_into().ok()?) as f64, 0.0)
            };
            // Granule của trang cuối = số mẫu
            let mut i = tail.len().checked_sub(14)?;
            let g = loop {
                if &tail[i..i + 4] == b"OggS" {
                    let g = i64::from_le_bytes(tail[i + 6..i + 14].try_into().ok()?);
                    if g > 0 {
                        break g;
                    }
                }
                i = i.checked_sub(1)?;
            };
            (rate > 0.0).then(|| (g as f64 - skip).max(0.0) / rate)?
        }
        "m4a" | "m4b" | "mp4" | "aac" | "alac" => {
            // mvhd: phiên bản 0 (32 bit) / 1 (64 bit) — moov nằm đầu hoặc cuối tệp
            let (b, j) = find(head, b"mvhd").map(|j| (head, j)).or_else(|| find(tail, b"mvhd").map(|j| (tail, j)))?;
            let (scale, dur) = if b.get(j + 4) == Some(&1) {
                (be32(b, j + 24)? as f64, u64::from_be_bytes(b.get(j + 28..j + 36)?.try_into().ok()?) as f64)
            } else {
                (be32(b, j + 16)? as f64, be32(b, j + 20)? as f64)
            };
            (scale > 0.0).then(|| dur / scale)?
        }
        _ => return None,
    };
    (d.is_finite() && d > 0.0).then_some(d)
}

fn find(h: &[u8], pat: &[u8]) -> Option<usize> {
    h.windows(pat.len()).position(|w| w == pat)
}

/// Khung MP3 đầu tiên (bỏ thẻ ID3v2): (tần số, kbit/s, số kênh, VBR — có thẻ Xing / VBRI).
fn mp3(b: &[u8]) -> Option<(u32, u32, u32, bool)> {
    let (i, _, rate, kbps) = mp3_frame(b)?;
    let ch = if b[i + 3] >> 6 == 3 { 1 } else { 2 };
    let frame = &b[i..b.len().min(i + 200)];
    let vbr = find(frame, b"Xing").is_some() || find(frame, b"VBRI").is_some();
    Some((rate, kbps, ch, vbr))
}

/// Đầu khung MP3 (Layer III) đầu tiên sau thẻ ID3v2: (vị trí, phiên bản — 3 MPEG1 / 2 MPEG2 / 0 MPEG2.5, tần số, kbit/s).
fn mp3_frame(b: &[u8]) -> Option<(usize, u8, u32, u32)> {
    let mut i = 0usize;
    if b.len() > 10 && &b[0..3] == b"ID3" {
        let sz = (b[6] as usize & 0x7f) << 21 | (b[7] as usize & 0x7f) << 14 | (b[8] as usize & 0x7f) << 7 | (b[9] as usize & 0x7f);
        i = 10 + sz + if b[5] & 0x10 != 0 { 10 } else { 0 };
    }
    const BR1: [u32; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
    const BR2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    while i + 4 <= b.len() {
        if b[i] == 0xFF && b[i + 1] & 0xE0 == 0xE0 {
            let ver = (b[i + 1] >> 3) & 3; // 0 MPEG2.5, 2 MPEG2, 3 MPEG1
            let layer = (b[i + 1] >> 1) & 3; // 1 = Layer III
            let bi = (b[i + 2] >> 4) as usize;
            let si = ((b[i + 2] >> 2) & 3) as usize;
            if ver != 1 && layer == 1 && (1..15).contains(&bi) && si < 3 {
                let rates = match ver {
                    3 => [44_100, 48_000, 32_000],
                    2 => [22_050, 24_000, 16_000],
                    _ => [11_025, 12_000, 8_000],
                };
                let kbps = if ver == 3 { BR1[bi] } else { BR2[bi] };
                return Some((i, ver, rates[si], kbps));
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_cbr_with_id3() {
        let mut h = b"ID3\x04\x00\x00\x00\x00\x00\x0a".to_vec();
        h.extend_from_slice(&[0u8; 10]);
        // MPEG1 Layer III, 320 kbit/s (14), 44,1 kHz (0), stereo
        h.extend_from_slice(&[0xFF, 0xFB, 0xE0, 0x00]);
        h.extend_from_slice(&[0u8; 300]);
        let i = parse("mp3", &h);
        assert_eq!((i.rate, i.kbps, i.channels, i.vbr), (Some(44_100), Some(320), Some(2), false));
    }

    #[test]
    fn durations_from_headers() {
        let rd = |f: &str| std::fs::read(format!("{}/tests/data/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let ht = |d: &[u8]| (d[..d.len().min(65536)].to_vec(), d[d.len().saturating_sub(65536)..].to_vec(), d.len() as u64);
        let (h, t, n) = ht(&rd("noise44.flac"));
        assert!((duration("flac", &h, &t, n).unwrap() - 4.0).abs() < 0.01);
        let (h, t, n) = ht(&rd("tone.opus"));
        assert!((duration("opus", &h, &t, n).unwrap() - 20.0).abs() < 0.01);
        let (h, t, n) = ht(&rd("tone.ogg"));
        let d = duration("ogg", &h, &t, n).unwrap();
        assert!((1.8..2.2).contains(&d), "{d}");
        // MP3 CBR 320 kbit/s: 40 000 byte sau khung đầu ≈ 1 giây
        let mut m = vec![0xFF, 0xFB, 0xE0, 0x00];
        m.resize(40_000, 0);
        assert!((duration("mp3", &m, &[], 40_000).unwrap() - 1.0).abs() < 0.01);
        assert_eq!(duration("mp3", &[0u8; 100], &[], 100), None);
    }

    #[test]
    fn flac_and_tests_data() {
        let d = std::fs::read(format!("{}/tests/data/hires96.flac", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let i = parse("flac", &d[..64]);
        assert_eq!((i.rate, i.bits, i.channels, i.lossless), (Some(96_000), Some(24), Some(2), true));
        let d = std::fs::read(format!("{}/tests/data/noise44.flac", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(parse("flac", &d[..64]).bits, Some(16));
        let d = std::fs::read(format!("{}/tests/data/tone.ogg", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let i = parse("ogg", &d[..d.len().min(4096)]);
        assert_eq!((i.rate, i.channels, i.format.as_str()), (Some(44_100), Some(2), "OGG"));
        let song = Song { size: 25 << 20, dur: 200, ..Default::default() };
        let v = to_json(&song, &parse("flac", &std::fs::read(format!("{}/tests/data/hires96.flac", env!("CARGO_MANIFEST_DIR"))).unwrap()[..64]));
        assert_eq!((v["kbps"].as_u64(), v["hires"].as_bool()), (Some(1048), Some(true)));
    }

    #[test]
    fn truncated_headers_do_not_panic() {
        // «fmt » sát cuối: trước đây head[j + 8..] panic (bản phát hành panic = abort → app sập)
        let h = b"RIFF\0\0\0\0WAVEfmt ";
        assert_eq!(parse("wav", h).rate, None);
        assert_eq!(duration("wav", h, &[], 16), None);
        assert_eq!(duration("ogg", b"\x01vorbis", &[], 7), None);
        assert_eq!(parse("mp3", b"ID3\x04\0\0\x7f\x7f\x7f\x7f").rate, None);
    }

    #[test]
    fn wav_header() {
        let p = crate::decode::Params { rate_in: 48_000, k: 1, ch_in: 2, ch_out: 2, frames_out: 10 };
        let h = crate::decode::wav_header(&p);
        let i = parse("wav", &h);
        assert_eq!((i.rate, i.bits, i.channels), (Some(48_000), Some(16), Some(2)));
    }
}
