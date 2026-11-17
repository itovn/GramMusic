//! Transport intermediate của MTProto (https://core.telegram.org/mtproto/mtproto-transports#intermediate), có hoặc không
//! có đệm, cho grammers.
//!
//! Tự viết thay cho `grammers_mtproto::transport::Intermediate` vì bản đó (0.12) kiểm thiếu độ dài: khi bộ đệm có từ `len`
//! tới `len + 3` byte nó vẫn trả gói và cắt lát ra ngoài bộ đệm → panic (trên iOS `panic = abort` là tắt app).
//!
//! Có đệm (thẻ `dddddddd`, bắt buộc với MTProxy secret `dd` / `ee`): khách thêm 0–3 byte ngẫu nhiên sau mỗi gói (độ dài
//! gói tính cả đệm, nên không còn chia hết cho 4). MTProxy bỏ đệm bằng cách làm tròn xuống bội 4; máy chủ có thể đệm
//! tới 15 byte — khách bỏ đệm theo nội dung: gói đã mã hoá có độ dài ≡ 8 (mod 16), gói chưa mã hoá (lúc tạo auth key)
//! ghi sẵn độ dài dữ liệu.

use grammers_mtproto::DequeBuffer;
use grammers_mtproto::transport::{Error, Transport, UnpackedOffset};

use crate::obfs::random_bytes;

/// Gói lớn nhất nhận (MTProto giới hạn ~1 MB; chừa rộng).
const MAX_PACKET: usize = 16 * 1024 * 1024;

pub struct Intermediate {
    padded: bool,
    tag_sent: bool,
}

impl Intermediate {
    /// Không đệm, thẻ `eeeeeeee` (đi thẳng, SOCKS5, MTProxy secret thường).
    pub fn plain() -> Self {
        Self { padded: false, tag_sent: false }
    }

    /// Có đệm, thẻ `dddddddd` (MTProxy secret `dd` / `ee`).
    pub fn padded() -> Self {
        Self { padded: true, tag_sent: false }
    }

    fn tag(&self) -> [u8; 4] {
        if self.padded { [0xdd; 4] } else { [0xee; 4] }
    }
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

impl Transport for Intermediate {
    fn pack(&mut self, buffer: &mut DequeBuffer<u8>) {
        assert_eq!(buffer.len() % 4, 0);
        if self.padded {
            let mut r = [0u8; 4];
            random_bytes(&mut r);
            let n = (r[0] & 3) as usize;
            buffer.extend_back(&r[1..1 + n]);
        }
        let len = buffer.len() as u32;
        buffer.extend_front(&len.to_le_bytes());
        if !self.tag_sent {
            buffer.extend_front(&self.tag());
            self.tag_sent = true;
        }
    }

    fn unpack(&mut self, buffer: &mut [u8]) -> Result<UnpackedOffset, Error> {
        if buffer.len() < 4 {
            return Err(Error::MissingBytes);
        }
        let raw = i32_at(buffer, 0);
        if raw < 0 || raw as usize > MAX_PACKET {
            return Err(Error::BadLen { got: raw });
        }
        let len = raw as usize;
        if buffer.len() < 4 + len {
            return Err(Error::MissingBytes);
        }
        let p = &buffer[4..4 + len];
        let first = if len >= 4 { i32_at(p, 0) } else { 0 };
        // Lỗi tầng transport: 4 byte số âm (-404, -429…), có thể kèm đệm
        if len < 24 && first < 0 {
            return Err(Error::BadStatus { status: first.unsigned_abs() });
        }
        let real = if len >= 20 && p[..8] == [0; 8] {
            // Gói chưa mã hoá: auth_key_id = 0, msg_id (8), độ dài dữ liệu (4), dữ liệu
            let n = i32_at(p, 16);
            if n < 0 || 20 + n as usize > len {
                return Err(Error::BadLen { got: raw });
            }
            if self.padded { 20 + n as usize } else { len }
        } else if len >= 24 {
            if self.padded { len - (len - 8) % 16 } else { len }
        } else {
            return Err(Error::BadLen { got: raw });
        };
        Ok(UnpackedOffset { data_range: 4..4 + real, next_offset: 4 + len })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(t: &mut Intermediate, data: &[u8]) -> Vec<u8> {
        let mut b = DequeBuffer::with_capacity(data.len(), 16);
        b.extend_back(data);
        t.pack(&mut b);
        b.as_ref().to_vec()
    }

    #[test]
    fn pack_tags_and_padding() {
        let mut t = Intermediate::plain();
        let a = packed(&mut t, &[1u8; 8]);
        assert_eq!(a[..8], [0xee, 0xee, 0xee, 0xee, 8, 0, 0, 0]);
        let b = packed(&mut t, &[2u8; 8]);
        assert_eq!(b[..4], [8, 0, 0, 0]); // thẻ chỉ một lần

        let mut t = Intermediate::padded();
        for i in 0..50 {
            let p = packed(&mut t, &[3u8; 24]);
            let p = if i == 0 { &p[4..] } else { &p[..] };
            let len = u32::from_le_bytes(p[..4].try_into().unwrap()) as usize;
            assert!((24..=27).contains(&len));
            assert_eq!(p.len(), 4 + len);
        }
    }

    #[test]
    fn unpack_partial_never_panics() {
        // Gói 40 byte: thử mọi độ dài cắt dở — phải báo thiếu byte, không panic (lỗi của bản grammers)
        let mut full = 40u32.to_le_bytes().to_vec();
        full.extend_from_slice(&[0x11; 40]);
        let mut t = Intermediate::plain();
        for cut in 0..full.len() {
            let mut b = full[..cut].to_vec();
            assert_eq!(t.unpack(&mut b), Err(Error::MissingBytes), "cut {cut}");
        }
        let mut b = full.clone();
        let o = t.unpack(&mut b).unwrap();
        assert_eq!((o.data_range, o.next_offset), (4..44, 44));
    }

    #[test]
    fn unpack_strips_server_padding() {
        let mut t = Intermediate::padded();
        // Gói mã hoá 8 + 16 + 32 = 56 byte, máy chủ đệm 11 byte
        let mut b = 67u32.to_le_bytes().to_vec();
        b.extend_from_slice(&[0x42; 56]);
        b.extend_from_slice(&[0x99; 11]);
        let o = t.unpack(&mut b).unwrap();
        assert_eq!((o.data_range, o.next_offset), (4..60, 71));
        // Gói chưa mã hoá: auth_key_id 0, msg_id, độ dài 12, dữ liệu 12 byte, đệm 3
        let mut p = vec![0u8; 8];
        p.extend_from_slice(&[0x55; 8]);
        p.extend_from_slice(&12i32.to_le_bytes());
        p.extend_from_slice(&[0x66; 12]);
        p.extend_from_slice(&[0x77; 3]);
        let mut b = (p.len() as u32).to_le_bytes().to_vec();
        b.extend_from_slice(&p);
        let o = t.unpack(&mut b).unwrap();
        assert_eq!((o.data_range, o.next_offset), (4..36, 39));
    }

    #[test]
    fn unpack_status() {
        let mut t = Intermediate::padded();
        let mut b = 4u32.to_le_bytes().to_vec();
        b.extend_from_slice(&(-404i32).to_le_bytes());
        assert_eq!(t.unpack(&mut b), Err(Error::BadStatus { status: 404 }));
        let mut b = 7u32.to_le_bytes().to_vec();
        b.extend_from_slice(&(-429i32).to_le_bytes());
        b.extend_from_slice(&[1, 2, 3]);
        assert_eq!(t.unpack(&mut b), Err(Error::BadStatus { status: 429 }));
    }
}
