//! Nguồn PCM cho **Ogg Opus** (.opus, .ogg chứa Opus) — symphonia 0.5 chưa giải Opus → libopus của GramMini (`gm-opus`).
//!
//! - Tự đọc trang Ogg (kiểm CRC), ghép gói qua nhiều trang, chỉ lấy luồng có OpusHead (bỏ luồng khác cùng tệp).
//! - Tổng số mẫu = granule trang cuối − pre-skip (đọc 64–256 KB cuối tệp — cổng phát luôn cho đọc cuối tệp).
//! - Tua: tìm nhị phân theo granule tới trang kết thúc trước điểm cần 80 ms (Opus cần «làm nóng» bộ giải), giải từ gói nối tiếp
//!   trang đó — mẫu đầu = granule trang đó − pre-skip — hàm gọi bỏ phần thừa → đúng từng mẫu.
//! - Luôn giải ở 48 kHz (tần số của Opus), 1–2 kênh (Opus nhiều kênh — bản đồ kênh ≠ 0 — chưa làm).

use std::collections::VecDeque;
use std::io::{self, SeekFrom};
use std::sync::OnceLock;

use symphonia::core::io::MediaSource;

/// Opus cần chừng này mẫu giải trước điểm tua để ra tiếng đúng (RFC 7845: 80 ms).
const PREROLL: i64 = 3840;

fn crc_table() -> &'static [u32; 256] {
    static T: OnceLock<[u32; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04c1_1db7 } else { r << 1 };
            }
            *v = r;
        }
        t
    })
}

/// CRC của trang Ogg (đa thức 0x04c11db7, không đảo bit).
fn crc(data: &[u8]) -> u32 {
    let t = crc_table();
    data.iter().fold(0u32, |c, b| (c << 8) ^ t[((c >> 24) as u8 ^ b) as usize])
}

#[derive(Clone, Debug)]
pub struct Page {
    pub off: u64,
    pub len: u64,
    pub granule: i64,
    pub flags: u8,
    pub serial: u32,
    pub lacing: Vec<u8>,
    pub body: Vec<u8>,
}

impl Page {
    fn continued(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Các đoạn (độ dài) — gói kết thúc ở đoạn có độ dài < 255.
    fn segments(&self) -> impl Iterator<Item = (usize, usize, bool)> + '_ {
        let mut at = 0usize;
        self.lacing.iter().map(move |&l| {
            let s = at;
            at += l as usize;
            (s, l as usize, l < 255)
        })
    }
}

fn read_at(r: &mut dyn MediaSource, off: u64, buf: &mut [u8]) -> io::Result<()> {
    r.seek(SeekFrom::Start(off))?;
    r.read_exact(buf)
}

/// Trang Ogg ở đúng `off` (đúng chữ ký, CRC khớp); None nếu không phải.
fn page_at(r: &mut dyn MediaSource, off: u64, size: u64) -> Option<Page> {
    if off + 27 > size {
        return None;
    }
    let mut h = [0u8; 27];
    read_at(r, off, &mut h).ok()?;
    if &h[0..4] != b"OggS" || h[4] != 0 {
        return None;
    }
    let nseg = h[26] as usize;
    let mut lacing = vec![0u8; nseg];
    r.read_exact(&mut lacing).ok()?;
    let blen: usize = lacing.iter().map(|&l| l as usize).sum();
    let len = 27 + nseg as u64 + blen as u64;
    if off + len > size {
        return None;
    }
    let mut body = vec![0u8; blen];
    r.read_exact(&mut body).ok()?;
    let want = u32::from_le_bytes([h[22], h[23], h[24], h[25]]);
    let mut all = Vec::with_capacity(len as usize);
    all.extend_from_slice(&h[..22]);
    all.extend_from_slice(&[0, 0, 0, 0]);
    all.push(h[26]);
    all.extend_from_slice(&lacing);
    all.extend_from_slice(&body);
    if crc(&all) != want {
        return None;
    }
    Some(Page {
        off,
        len,
        granule: i64::from_le_bytes(h[6..14].try_into().ok()?),
        flags: h[5],
        serial: u32::from_le_bytes([h[14], h[15], h[16], h[17]]),
        lacing,
        body,
    })
}

/// Trang hợp lệ đầu tiên bắt đầu từ `from` trở đi, dò trong `max` byte.
fn find_page(r: &mut dyn MediaSource, from: u64, size: u64, max: u64) -> Option<Page> {
    let end = (from + max).min(size);
    let mut at = from;
    while at + 27 <= end {
        let n = (64 * 1024).min((end - at) as usize);
        let mut buf = vec![0u8; n];
        read_at(r, at, &mut buf).ok()?;
        let mut i = 0;
        while i + 4 <= n {
            if &buf[i..i + 4] == b"OggS" {
                if let Some(p) = page_at(r, at + i as u64, size) {
                    return Some(p);
                }
            }
            i += 1;
        }
        if n < 4 {
            break;
        }
        at += (n - 3) as u64;
    }
    None
}

pub struct OpusSrc {
    r: Box<dyn MediaSource>,
    size: u64,
    serial: u32,
    pub channels: usize,
    pre_skip: i64,
    data_start: u64,
    next_off: u64,
    pending: Vec<u8>,
    packets: VecDeque<Vec<u8>>,
    /// Mẫu đầu (đã trừ pre-skip) của gói kế tiếp sẽ giải.
    cur_ts: i64,
    dec: gm_opus::Decoder,
    /// Tổng số mẫu (48 kHz) của bài.
    pub total: u64,
}

impl OpusSrc {
    /// Đọc OpusHead / OpusTags, tìm trang cuối để biết tổng số mẫu.
    pub fn open(mut r: Box<dyn MediaSource>) -> Result<OpusSrc, String> {
        let size = r.byte_len().ok_or("không biết cỡ tệp")?;
        let p0 = page_at(&mut *r, 0, size).ok_or("không phải tệp Ogg")?;
        if p0.body.len() < 19 || &p0.body[0..8] != b"OpusHead" {
            return Err("không phải Ogg Opus".into());
        }
        let channels = p0.body[9] as usize;
        let pre_skip = u16::from_le_bytes([p0.body[10], p0.body[11]]) as i64;
        let family = p0.body[18];
        if channels == 0 || (family != 0 && channels > 2) {
            return Err(format!("Opus {channels} kênh chưa hỗ trợ"));
        }
        let channels = channels.min(2);
        let serial = p0.serial;
        // OpusTags (có thể dài nhiều trang) — trang âm thanh đầu tiên bắt đầu sau trang kết thúc gói tags
        let mut off = p0.off + p0.len;
        let data_start = loop {
            let p = find_page(&mut *r, off, size, 1 << 20).ok_or("thiếu OpusTags")?;
            off = p.off + p.len;
            if p.serial == serial && p.segments().any(|(_, _, end)| end) {
                break off;
            }
        };
        // Trang cuối (granule lớn nhất của luồng)
        let mut total = 0i64;
        for back in [64u64 << 10, 256 << 10, 1 << 20] {
            let from = size.saturating_sub(back).max(data_start);
            let mut at = from;
            while let Some(p) = find_page(&mut *r, at, size, size - at) {
                if p.serial == serial && p.granule > 0 {
                    total = total.max(p.granule);
                }
                at = p.off + p.len;
            }
            if total > 0 || from == data_start {
                break;
            }
        }
        let dec = gm_opus::Decoder::new(48_000, channels).map_err(|e| e.to_string())?;
        Ok(OpusSrc {
            r,
            size,
            serial,
            channels,
            pre_skip,
            data_start,
            next_off: data_start,
            pending: Vec::new(),
            packets: VecDeque::new(),
            cur_ts: -pre_skip,
            dec,
            total: (total - pre_skip).max(0) as u64,
        })
    }

    /// Tua tới mẫu `ts` (48 kHz, đã trừ pre-skip).
    pub fn seek(&mut self, ts: u64) -> Result<(), String> {
        self.dec = gm_opus::Decoder::new(48_000, self.channels).map_err(|e| e.to_string())?;
        self.packets.clear();
        self.pending.clear();
        let target = ts as i64 + self.pre_skip - PREROLL;
        let mut best: Option<Page> = None;
        if target > self.pre_skip {
            let (mut lo, mut hi) = (self.data_start, self.size);
            for _ in 0..48 {
                if hi <= lo || hi - lo < 64 * 1024 {
                    break;
                }
                let mid = lo + (hi - lo) / 2;
                match find_page(&mut *self.r, mid, self.size, hi - mid) {
                    Some(p) if p.off < hi => {
                        if p.serial != self.serial || p.granule < 0 || p.granule <= target {
                            if p.serial == self.serial && p.granule >= 0 {
                                best = Some(p.clone());
                            }
                            lo = p.off + p.len;
                        } else {
                            hi = mid;
                        }
                    }
                    _ => hi = mid,
                }
            }
            // Đi tiếp từng trang trong đoạn còn lại tới trang cuối cùng kết thúc trước điểm cần
            let mut at = best.as_ref().map(|b| b.off + b.len).unwrap_or(self.data_start).max(lo.min(self.size));
            if let Some(b) = &best {
                at = b.off + b.len;
            }
            while let Some(p) = find_page(&mut *self.r, at, self.size, 128 * 1024) {
                if p.serial == self.serial && p.granule >= 0 {
                    if p.granule > target {
                        break;
                    }
                    best = Some(p.clone());
                }
                at = p.off + p.len;
            }
        }
        match best {
            None => {
                self.next_off = self.data_start;
                self.cur_ts = -self.pre_skip;
            }
            Some(b) => {
                // Gói dở cuối trang b (nối sang trang sau) — giải được vì có đủ đầu gói
                let mut tail_start = None;
                for (s, _, end) in b.segments() {
                    if end {
                        tail_start = None;
                    } else if tail_start.is_none() {
                        tail_start = Some(s);
                    }
                }
                if let Some(s) = tail_start {
                    self.pending = b.body[s..].to_vec();
                }
                self.next_off = b.off + b.len;
                self.cur_ts = b.granule - self.pre_skip;
            }
        }
        Ok(())
    }

    /// Gói kế tiếp → mẫu xen kẽ vào `out`; trả mẫu đầu của gói. None = hết tệp.
    pub fn next(&mut self, out: &mut Vec<i16>) -> Result<Option<i64>, String> {
        loop {
            if let Some(pk) = self.packets.pop_front() {
                out.clear();
                out.resize(5760 * self.channels, 0);
                match self.dec.decode(&pk, out) {
                    Ok(n) => {
                        out.truncate(n * self.channels);
                        let ts = self.cur_ts;
                        self.cur_ts += n as i64;
                        return Ok(Some(ts));
                    }
                    Err(_) => continue, // gói hỏng: bỏ qua
                }
            }
            if self.next_off + 27 > self.size {
                return Ok(None);
            }
            let p = match page_at(&mut *self.r, self.next_off, self.size) {
                Some(p) => p,
                None => match find_page(&mut *self.r, self.next_off, self.size, 1 << 20) {
                    Some(p) => p,
                    None => return Ok(None),
                },
            };
            self.next_off = p.off + p.len;
            if p.serial != self.serial {
                continue;
            }
            // Trang nối mà không có phần đầu gói (vừa nhảy tới) → bỏ đoạn nối
            let mut drop_first = p.continued() && self.pending.is_empty();
            for (s, l, end) in p.segments() {
                if drop_first {
                    if end {
                        drop_first = false;
                    }
                    continue;
                }
                self.pending.extend_from_slice(&p.body[s..s + l]);
                if end {
                    self.packets.push_back(std::mem::take(&mut self.pending));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known() {
        // Trang Ogg thật trong tệp mẫu: CRC tính lại phải khớp (page_at kiểm)
        let d = std::fs::read(format!("{}/tests/data/tone.opus", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let size = d.len() as u64;
        let mut r: Box<dyn MediaSource> = Box::new(std::io::Cursor::new(d));
        let p = page_at(&mut *r, 0, size).expect("trang đầu hợp lệ");
        assert_eq!(&p.body[0..8], b"OpusHead");
    }
}
