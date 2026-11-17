//! Bắt tay fake-TLS của MTProxy (secret `ee`): khách gửi ClientHello giống Chrome (TLS 1.3, SNI = tên miền trong secret),
//! ô `random` = HMAC-SHA256(secret, ClientHello với random = 0), 4 byte cuối XOR với giờ Unix (LE) — máy chủ dùng để chống
//! gửi lại, nên **giờ của máy phải đúng**. Máy chủ trả ServerHello + ChangeCipherSpec + một bản ghi ApplicationData rác;
//! ô random của ServerHello = HMAC-SHA256(secret, random của khách ‖ cả câu trả lời với ô đó = 0). Sau đó dữ liệu đi trong bản
//! ghi ApplicationData (`obfs.rs`).

use std::io;

use hmac::{Hmac, KeyInit, Mac};
use num_bigint::BigUint;
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::obfs::random_bytes;

/// Cỡ ClientHello (cả phần đầu bản ghi) như Chrome: 517 byte.
const HELLO_LEN: usize = 517;
/// Ô random trong ClientHello / ServerHello: sau 5 byte đầu bản ghi + 4 byte đầu handshake + 2 byte phiên bản.
const RANDOM_AT: usize = 11;

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("hmac key");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

/// GREASE (RFC 8701): 0x?a?a. Chọn 7 giá trị, hai giá trị liền cặp phải khác nhau (như Chrome / tdlib).
fn grease() -> [u16; 7] {
    let mut r = [0u8; 7];
    random_bytes(&mut r);
    let mut g = r.map(|b| {
        let x = (b & 0xf0) | 0x0a;
        u16::from_be_bytes([x, x])
    });
    for i in (1..g.len()).step_by(2) {
        if g[i] == g[i - 1] {
            g[i] ^= 0x1010;
        }
    }
    g
}

/// Khoá công khai X25519 trông như thật: 32 byte ngẫu nhiên là toạ độ u của một điểm trên đường cong
/// (u³ + 486662u² + u là số chính phương mod p) — máy lọc gói không phân biệt được với khoá thật.
fn x25519_like_key() -> [u8; 32] {
    let p = (BigUint::from(1u8) << 255u32) - BigUint::from(19u8);
    let exp = (&p - BigUint::from(1u8)) >> 1u32;
    loop {
        let mut k = [0u8; 32];
        random_bytes(&mut k);
        k[31] &= 0x7f;
        let u = BigUint::from_bytes_le(&k) % &p;
        let y2 = (&u * &u * &u + BigUint::from(486662u32) * &u * &u + &u) % &p;
        if y2 == BigUint::from(0u8) || y2.modpow(&exp, &p) == BigUint::from(1u8) {
            return k;
        }
    }
}

fn u16be(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn ext(v: &mut Vec<u8>, kind: u16, body: &[u8]) {
    u16be(v, kind);
    u16be(v, body.len() as u16);
    v.extend_from_slice(body);
}

/// ClientHello với ô random = 0.
pub(crate) fn client_hello(domain: &str) -> Vec<u8> {
    let g = grease();
    let mut b = Vec::with_capacity(HELLO_LEN);
    b.extend_from_slice(&[0x16, 0x03, 0x01, 0, 0]); // bản ghi Handshake, độ dài điền sau
    b.extend_from_slice(&[0x01, 0, 0, 0]); // ClientHello, độ dài điền sau
    b.extend_from_slice(&[0x03, 0x03]);
    b.extend_from_slice(&[0u8; 32]); // random
    let mut sid = [0u8; 32];
    random_bytes(&mut sid);
    b.push(32);
    b.extend_from_slice(&sid);
    let suites = [
        g[0], 0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f, 0xc02c, 0xc030, 0xcca9, 0xcca8, 0xc013, 0xc014, 0x009c, 0x009d,
        0x002f, 0x0035,
    ];
    u16be(&mut b, (suites.len() * 2) as u16);
    for s in suites {
        u16be(&mut b, s);
    }
    b.extend_from_slice(&[0x01, 0x00]); // nén: không

    let ext_at = b.len();
    b.extend_from_slice(&[0, 0]);
    ext(&mut b, g[2], &[]);
    let name = domain.as_bytes();
    let mut sni = Vec::new();
    u16be(&mut sni, (name.len() + 3) as u16);
    sni.push(0);
    u16be(&mut sni, name.len() as u16);
    sni.extend_from_slice(name);
    ext(&mut b, 0x0000, &sni);
    ext(&mut b, 0x0017, &[]); // extended_master_secret
    ext(&mut b, 0xff01, &[0x00]); // renegotiation_info
    let mut groups = Vec::new();
    u16be(&mut groups, 8);
    for x in [g[4], 0x001d, 0x0017, 0x0018] {
        u16be(&mut groups, x);
    }
    ext(&mut b, 0x000a, &groups); // supported_groups
    ext(&mut b, 0x000b, &[0x01, 0x00]); // ec_point_formats
    ext(&mut b, 0x0023, &[]); // session_ticket
    ext(&mut b, 0x0010, b"\x00\x0c\x02h2\x08http/1.1"); // ALPN
    ext(&mut b, 0x0005, &[0x01, 0, 0, 0, 0]); // status_request
    let mut sig = Vec::new();
    u16be(&mut sig, 16);
    for x in [0x0403u16, 0x0804, 0x0401, 0x0503, 0x0805, 0x0501, 0x0806, 0x0601] {
        u16be(&mut sig, x);
    }
    ext(&mut b, 0x000d, &sig); // signature_algorithms
    ext(&mut b, 0x0012, &[]); // signed_certificate_timestamp
    let mut ks = Vec::new();
    u16be(&mut ks, 5 + 36);
    u16be(&mut ks, g[4]);
    u16be(&mut ks, 1);
    ks.push(0);
    u16be(&mut ks, 0x001d);
    u16be(&mut ks, 32);
    ks.extend_from_slice(&x25519_like_key());
    ext(&mut b, 0x0033, &ks); // key_share
    ext(&mut b, 0x002d, &[0x01, 0x01]); // psk_key_exchange_modes
    let mut ver = vec![6];
    for x in [g[5], 0x0304, 0x0303] {
        u16be(&mut ver, x);
    }
    ext(&mut b, 0x002b, &ver); // supported_versions
    ext(&mut b, 0x001b, &[0x02, 0x00, 0x02]); // compress_certificate (brotli)
    ext(&mut b, g[3], &[0x00]);
    if b.len() + 4 <= HELLO_LEN {
        let pad = vec![0u8; HELLO_LEN - b.len() - 4];
        ext(&mut b, 0x0015, &pad); // padding tới 517 byte
    }

    let ext_len = (b.len() - ext_at - 2) as u16;
    b[ext_at..ext_at + 2].copy_from_slice(&ext_len.to_be_bytes());
    let hs_len = (b.len() - 9) as u32;
    b[6..9].copy_from_slice(&hs_len.to_be_bytes()[1..]);
    let rec_len = (b.len() - 5) as u16;
    b[3..5].copy_from_slice(&rec_len.to_be_bytes());
    b
}


/// Ký ClientHello: điền ô random. Trả random đã điền (cần để kiểm câu trả lời).
pub(crate) fn sign_hello(hello: &mut [u8], key: &[u8; 16], now: u32) -> [u8; 32] {
    hello[RANDOM_AT..RANDOM_AT + 32].fill(0);
    let mut r = hmac(key, &[hello]);
    for (i, t) in now.to_le_bytes().iter().enumerate() {
        r[28 + i] ^= t;
    }
    hello[RANDOM_AT..RANDOM_AT + 32].copy_from_slice(&r);
    r
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

async fn read_record<S: AsyncRead + Unpin>(s: &mut S, out: &mut Vec<u8>, kind: u8) -> io::Result<()> {
    let mut h = [0u8; 5];
    s.read_exact(&mut h).await?;
    if h[0] != kind || h[1..3] != [0x03, 0x03] {
        return Err(bad("MTProxy không trả lời kiểu fake-TLS (sai địa chỉ / cổng, hoặc không phải MTProxy)"));
    }
    let len = u16::from_be_bytes([h[3], h[4]]) as usize;
    if len == 0 || len > 16384 + 2048 {
        return Err(bad("MTProxy trả bản ghi TLS sai độ dài"));
    }
    out.extend_from_slice(&h);
    let at = out.len();
    out.resize(at + len, 0);
    s.read_exact(&mut out[at..]).await?;
    Ok(())
}

/// Bắt tay fake-TLS trên `s` (đã kết nối tới MTProxy). Xong thì dữ liệu đi bằng `ObfsStream { tls: true }`.
pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(s: &mut S, key: &[u8; 16], domain: &str) -> io::Result<()> {
    let mut hello = client_hello(domain);
    let client_random = sign_hello(&mut hello, key, crate::clock::now_unix());
    s.write_all(&hello).await?;
    s.flush().await?;

    let mut resp = Vec::new();
    read_record(s, &mut resp, 0x16).await?; // ServerHello
    read_record(s, &mut resp, 0x14).await?; // ChangeCipherSpec
    read_record(s, &mut resp, 0x17).await?; // ApplicationData rác
    if resp.len() < RANDOM_AT + 32 {
        return Err(bad("MTProxy trả ServerHello quá ngắn"));
    }
    verify_server(&resp, key, &client_random)
}

pub(crate) fn verify_server(resp: &[u8], key: &[u8; 16], client_random: &[u8; 32]) -> io::Result<()> {
    let mut zeroed = resp.to_vec();
    zeroed[RANDOM_AT..RANDOM_AT + 32].fill(0);
    if hmac(key, &[client_random, &zeroed]) != resp[RANDOM_AT..RANDOM_AT + 32] {
        let clock = match crate::clock::offset_ms() {
            Some(ms) => format!("giờ máy lệch {:+.1} s, đã chỉnh theo NTP", ms as f64 / 1000.0),
            None => "chưa đo được giờ chuẩn (NTP) — kiểm tra Ngày & Giờ của máy".into(),
        };
        return Err(bad(&format!("MTProxy từ chối: secret sai, hoặc giờ lệch ({clock})")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Đọc các phần mở rộng của ClientHello (kiểm cấu trúc đúng chuẩn, độ dài khớp).
    fn parse_hello(b: &[u8]) -> Vec<(u16, Vec<u8>)> {
        assert_eq!(b[0..3], [0x16, 0x03, 0x01]);
        assert_eq!(u16::from_be_bytes([b[3], b[4]]) as usize, b.len() - 5);
        assert_eq!(b[5], 0x01);
        assert_eq!(u32::from_be_bytes([0, b[6], b[7], b[8]]) as usize, b.len() - 9);
        let mut i = 9 + 2 + 32;
        let sid = b[i] as usize;
        assert_eq!(sid, 32);
        i += 1 + sid;
        let cs = u16::from_be_bytes([b[i], b[i + 1]]) as usize;
        i += 2 + cs;
        let comp = b[i] as usize;
        i += 1 + comp;
        let el = u16::from_be_bytes([b[i], b[i + 1]]) as usize;
        i += 2;
        assert_eq!(i + el, b.len());
        let mut exts = Vec::new();
        while i < b.len() {
            let k = u16::from_be_bytes([b[i], b[i + 1]]);
            let l = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            exts.push((k, b[i + 4..i + 4 + l].to_vec()));
            i += 4 + l;
        }
        assert_eq!(i, b.len());
        exts
    }

    #[test]
    fn hello_shape() {
        for domain in ["google.com", "www.cloudflare.com", "a.b"] {
            let h = client_hello(domain);
            assert_eq!(h.len(), HELLO_LEN);
            let exts = parse_hello(&h);
            let sni = &exts.iter().find(|(k, _)| *k == 0).unwrap().1;
            assert_eq!(&sni[5..], domain.as_bytes());
            let ks = &exts.iter().find(|(k, _)| *k == 0x33).unwrap().1;
            assert_eq!(ks.len(), 2 + 5 + 36);
            assert!(exts.iter().any(|(k, b)| *k == 0x2b && b.windows(2).any(|w| w == [0x03, 0x04])));
        }
    }

    #[test]
    fn hmac_both_ways() {
        let key = [3u8; 16];
        let now = 1_763_270_000u32;
        let mut hello = client_hello("google.com");
        let r = sign_hello(&mut hello, &key, now);

        // Phía MTProxy: tính lại HMAC với random = 0, XOR ra giờ
        let mut z = hello.clone();
        z[RANDOM_AT..RANDOM_AT + 32].fill(0);
        let mac = hmac(&key, &[&z]);
        assert_eq!(mac[..28], r[..28]);
        let ts: Vec<u8> = (0..4).map(|i| mac[28 + i] ^ r[28 + i]).collect();
        assert_eq!(u32::from_le_bytes(ts.try_into().unwrap()), now);

        // Câu trả lời giả của máy chủ, ký đúng thì qua, sai khoá thì báo lỗi
        let mut resp = vec![0x16, 0x03, 0x03, 0x00, 0x30, 0x02, 0x00, 0x00, 0x2c, 0x03, 0x03];
        resp.extend_from_slice(&[0u8; 32]);
        resp.extend_from_slice(&[0x20; 5]);
        resp.extend_from_slice(&[0x14, 0x03, 0x03, 0x00, 0x01, 0x01, 0x17, 0x03, 0x03, 0x00, 0x04, 1, 2, 3, 4]);
        let sr = hmac(&key, &[&r, &resp]);
        resp[RANDOM_AT..RANDOM_AT + 32].copy_from_slice(&sr);
        assert!(verify_server(&resp, &key, &r).is_ok());
        assert!(verify_server(&resp, &[4u8; 16], &r).is_err());
    }

    #[test]
    fn key_on_curve() {
        for _ in 0..20 {
            let k = x25519_like_key();
            assert_eq!(k[31] & 0x80, 0);
        }
    }
}
