//! Đọc / ghi OGG tối giản (một luồng logic — đủ cho tin thoại Opus của Telegram).

use std::io;

fn err(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Tách các gói (packet) từ tệp OGG, nối các đoạn qua ranh giới trang.
pub fn packets(data: &[u8]) -> io::Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut i = 0;
    while i + 27 <= data.len() {
        if &data[i..i + 4] != b"OggS" {
            return Err(err("tệp OGG hỏng (thiếu OggS)"));
        }
        let nseg = data[i + 26] as usize;
        let table = data.get(i + 27..i + 27 + nseg).ok_or_else(|| err("tệp OGG cụt"))?;
        let mut p = i + 27 + nseg;
        for &len in table {
            let len = len as usize;
            let seg = data.get(p..p + len).ok_or_else(|| err("tệp OGG cụt"))?;
            cur.extend_from_slice(seg);
            p += len;
            if len < 255 {
                out.push(std::mem::take(&mut cur));
            }
        }
        i = p;
    }
    Ok(out)
}

/// CRC-32 của OGG (đa thức 0x04C11DB7, không đảo bit).
fn crc(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04C1_1DB7 } else { r << 1 };
            }
            *e = r;
        }
        t
    });
    data.iter().fold(0u32, |c, b| (c << 8) ^ t[((c >> 24) as u8 ^ b) as usize])
}

/// Ghi OGG: gom gói vào trang, `flush` = kết thúc trang sau gói này.
pub struct Writer {
    serial: u32,
    seq: u32,
    out: Vec<u8>,
    segs: Vec<u8>,
    body: Vec<u8>,
    granule: u64,
    first: bool,
}

impl Writer {
    pub fn new(serial: u32) -> Self {
        Self { serial, seq: 0, out: Vec::new(), segs: Vec::new(), body: Vec::new(), granule: 0, first: true }
    }

    pub fn packet(&mut self, data: &[u8], granule: u64, flush: bool) {
        let mut left = data.len();
        loop {
            let n = left.min(255);
            self.segs.push(n as u8);
            left -= n;
            if n < 255 {
                break;
            }
        }
        self.body.extend_from_slice(data);
        self.granule = granule;
        if flush || self.segs.len() > 200 {
            self.page(false);
        }
    }

    fn page(&mut self, last: bool) {
        if self.segs.is_empty() && !last {
            return;
        }
        let mut h = Vec::with_capacity(27 + self.segs.len());
        h.extend_from_slice(b"OggS");
        h.push(0);
        h.push(if self.first { 2 } else if last { 4 } else { 0 });
        h.extend_from_slice(&self.granule.to_le_bytes());
        h.extend_from_slice(&self.serial.to_le_bytes());
        h.extend_from_slice(&self.seq.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.push(self.segs.len() as u8);
        h.extend_from_slice(&self.segs);
        let start = self.out.len();
        self.out.extend_from_slice(&h);
        self.out.extend_from_slice(&self.body);
        let c = crc(&self.out[start..]);
        self.out[start + 22..start + 26].copy_from_slice(&c.to_le_bytes());
        self.seq += 1;
        self.first = false;
        self.segs.clear();
        self.body.clear();
    }

    /// Kết thúc luồng (trang cuối có cờ EOS).
    pub fn finish(mut self) -> Vec<u8> {
        if !self.segs.is_empty() {
            self.page(true);
        } else if let Some(pos) = self.last_page_start() {
            // Trang cuối đã ghi: bật cờ EOS rồi tính lại CRC
            self.out[pos + 5] |= 4;
            self.out[pos + 22..pos + 26].fill(0);
            let c = crc(&self.out[pos..]);
            self.out[pos + 22..pos + 26].copy_from_slice(&c.to_le_bytes());
        }
        self.out
    }

    fn last_page_start(&self) -> Option<usize> {
        (0..self.out.len().saturating_sub(3)).rev().find(|&i| &self.out[i..i + 4] == b"OggS")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read() {
        let mut w = Writer::new(7);
        let big = vec![9u8; 600]; // > 255: nhiều đoạn
        w.packet(b"OpusHead-xxxxxxxxxxx", 0, true);
        w.packet(&big, 960, false);
        w.packet(b"short", 1920, true);
        let data = w.finish();
        let p = packets(&data).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[1], big);
        assert_eq!(p[2], b"short");
    }
}
