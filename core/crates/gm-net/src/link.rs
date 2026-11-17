//! Đọc cấu hình proxy từ link người dùng dán: `tg://proxy?server=…&port=…&secret=…`, `https://t.me/proxy?…` (MTProxy),
//! `tg://socks?server=…&port=…&user=…&pass=…`, `https://t.me/socks?…` (SOCKS5).
//!
//! Secret MTProxy dạng hex hoặc base64 (thường / url-safe, có hay không có dấu `=`):
//! - 16 byte: obfuscated2 thường;
//! - `dd` + 16 byte: obfuscated2 + intermediate có đệm;
//! - `ee` + 16 byte + tên miền: fake-TLS (giả bắt tay TLS tới tên miền đó) + intermediate có đệm.

use crate::config::{MtProxy, ProxyConfig, Secret, Socks5};

/// Đọc link proxy. Lỗi: câu báo tiếng Việt cho người dùng.
pub fn parse_link(link: &str) -> Result<ProxyConfig, String> {
    let link = link.trim();
    let lower = link.to_ascii_lowercase();
    let rest = ["tg://", "https://t.me/", "http://t.me/", "t.me/", "https://telegram.me/", "http://telegram.me/"]
        .iter()
        .find_map(|p| lower.starts_with(p).then(|| &link[p.len()..]))
        .ok_or_else(|| "Link không phải link proxy Telegram (tg://proxy… hoặc t.me/proxy…).".to_string())?;
    let (kind, query) = rest.split_once('?').ok_or_else(|| "Link proxy thiếu thông tin (không có phần sau dấu ?).".to_string())?;
    let kind = kind.trim_end_matches('/').to_ascii_lowercase();
    let params = parse_query(query);
    let get = |k: &str| params.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str());
    let server = get("server").filter(|s| !s.is_empty()).ok_or_else(|| "Link proxy thiếu máy chủ (server).".to_string())?;
    let port: u16 = get("port")
        .and_then(|p| p.parse().ok())
        .filter(|p| *p != 0)
        .ok_or_else(|| "Link proxy thiếu cổng (port) hoặc cổng sai.".to_string())?;
    match kind.as_str() {
        "proxy" => {
            let secret = get("secret").ok_or_else(|| "Link MTProxy thiếu secret.".to_string())?;
            Ok(ProxyConfig::MtProxy(MtProxy { host: server.to_string(), port, secret: parse_secret(secret)? }))
        }
        "socks" => Ok(ProxyConfig::Socks5(Socks5 {
            host: server.to_string(),
            port,
            user: get("user").unwrap_or("").to_string(),
            pass: get("pass").unwrap_or("").to_string(),
        })),
        _ => Err("Link không phải link proxy Telegram (tg://proxy… hoặc t.me/proxy…).".into()),
    }
}

/// Đọc secret MTProxy (hex hoặc base64).
pub fn parse_secret(s: &str) -> Result<Secret, String> {
    let s = s.trim();
    let bytes = if s.len() % 2 == 0 && !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        hex_decode(s)
    } else {
        base64_decode(s)
    }
    .ok_or_else(|| "Secret MTProxy không đọc được (không phải hex / base64).".to_string())?;
    let key = |b: &[u8]| -> [u8; 16] { b.try_into().unwrap() };
    match bytes.len() {
        16 => Ok(Secret::Simple(key(&bytes))),
        17 if bytes[0] == 0xdd => Ok(Secret::Padded(key(&bytes[1..17]))),
        n if n > 17 && bytes[0] == 0xee => {
            let domain = String::from_utf8(bytes[17..].to_vec())
                .ok()
                .filter(|d| d.bytes().all(|c| c.is_ascii_graphic()))
                .ok_or_else(|| "Secret fake-TLS (ee…) có tên miền không hợp lệ.".to_string())?;
            Ok(Secret::FakeTls { key: key(&bytes[1..17]), domain })
        }
        _ => Err("Secret MTProxy sai độ dài (cần 16 byte, dd + 16 byte, hoặc ee + 16 byte + tên miền).".into()),
    }
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split(['&', ';'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    b.chunks(2).map(|p| Some(hex_val(p[0])? << 4 | hex_val(p[1])?)).collect()
}

/// base64 thường hoặc url-safe, dấu `=` cuối tuỳ ý.
pub(crate) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter(|c| *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 16] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00];

    #[test]
    fn secrets_hex() {
        assert_eq!(parse_secret("112233445566778899aabbccddeeff00").unwrap(), Secret::Simple(KEY));
        assert_eq!(parse_secret("dd112233445566778899aabbccddeeff00").unwrap(), Secret::Padded(KEY));
        assert_eq!(
            parse_secret("ee112233445566778899aabbccddeeff00676f6f676c652e636f6d").unwrap(),
            Secret::FakeTls { key: KEY, domain: "google.com".into() }
        );
        assert!(parse_secret("ab112233445566778899aabbccddeeff00").is_err());
    }

    #[test]
    fn secrets_base64() {
        // ee + KEY + "google.com" dạng base64url không có '=' và base64 thường có '='
        let raw: Vec<u8> = [&[0xee][..], &KEY, b"google.com"].concat();
        let want = Secret::FakeTls { key: KEY, domain: "google.com".into() };
        assert_eq!(parse_secret("7hEiM0RVZneImaq7zN3u_wBnb29nbGUuY29t").unwrap(), want);
        assert_eq!(parse_secret("7hEiM0RVZneImaq7zN3u/wBnb29nbGUuY29t").unwrap(), want);
        assert_eq!(base64_decode("7hEiM0RVZneImaq7zN3u_wBnb29nbGUuY29t").unwrap(), raw);
    }

    #[test]
    fn links() {
        let l = "tg://proxy?server=proxy.example.org&port=443&secret=dd112233445566778899aabbccddeeff00";
        assert_eq!(
            parse_link(l).unwrap(),
            ProxyConfig::MtProxy(MtProxy { host: "proxy.example.org".into(), port: 443, secret: Secret::Padded(KEY) })
        );
        let l = "https://t.me/proxy?server=1.2.3.4&port=8443&secret=ee112233445566778899aabbccddeeff00676f6f676c652e636f6d";
        assert!(matches!(parse_link(l).unwrap(), ProxyConfig::MtProxy(MtProxy { port: 8443, secret: Secret::FakeTls { .. }, .. })));
        let l = "tg://socks?server=10.0.0.1&port=1080&user=a%40b&pass=x%20y";
        assert_eq!(
            parse_link(l).unwrap(),
            ProxyConfig::Socks5(Socks5 { host: "10.0.0.1".into(), port: 1080, user: "a@b".into(), pass: "x y".into() })
        );
        assert!(parse_link("https://example.com/proxy?server=a&port=1&secret=00").is_err());
        assert!(parse_link("tg://proxy?server=a&secret=dd112233445566778899aabbccddeeff00").is_err());
    }
}
