//! Lớp vỏ obfuscated2 của MTProto (https://core.telegram.org/mtproto/mtproto-transports#transport-obfuscation), ở mức
//! luồng byte, kèm khung bản ghi TLS khi MTProxy dùng fake-TLS (secret `ee`).
//!
//! 64 byte đầu luồng (khách gửi): ngẫu nhiên, trừ byte 56..60 = thẻ transport (`eeeeeeee` intermediate, `dddddddd` có đệm),
//! 60..62 = số DC (i16 LE). Khoá gửi = byte 8..40, iv = 40..56; khoá nhận / iv lấy từ 8..56 đảo ngược. Có secret MTProxy thì
//! khoá = SHA-256(khoá ‖ secret). Cả 64 byte được mã hoá AES-256-CTR bằng khoá gửi, nhưng chỉ byte 56..64 gửi đi ở dạng mã
//! hoá (56 byte đầu gửi nguyên). Dữ liệu sau đó mã hoá tiếp bằng cùng dòng khoá.
//!
//! Thẻ transport là 4 byte đầu mà transport của grammers ghi ra lần đầu (Intermediate / PaddedIntermediate ghi thẻ trước gói
//! đầu tiên): luồng này lấy 4 byte đó đưa vào phần đầu, không gửi thẳng. Khoá không phụ thuộc thẻ, nên khoá nhận có ngay từ
//! đầu; chỉ byte 56..64 của phần đầu phải chờ thẻ.
//!
//! Ghi: mỗi lần `poll_write` nhận một mẩu (tối đa `MAX_CHUNK`), mã hoá vào hàng chờ rồi đẩy hết xuống luồng dưới mới trả
//! `Ok(n)`. Nếu luồng dưới chưa nhận hết thì trả `Pending` và giữ hàng chờ; lần gọi sau (cùng dữ liệu đầu — grammers luôn
//! gọi lại với phần chưa gửi) đẩy tiếp. Nhờ vậy không có byte nào nằm kẹt trong lớp vỏ khi bên gọi thôi ghi.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use aes::Aes256;
use aes::cipher::{KeyIvInit, StreamCipher};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

type Ctr = ctr::Ctr128BE<Aes256>;

/// Mỗi lần ghi nhận tối đa ngần này byte của bên gọi.
const MAX_CHUNK: usize = 64 * 1024;
/// Cỡ phần dữ liệu tối đa của một bản ghi TLS khách gửi (như tdlib — giống Chrome).
pub(crate) const MAX_TLS_RECORD: usize = 2878;
/// Bản ghi TLS máy chủ gửi: tối đa 2^14 + phần dư cho phép.
const MAX_TLS_IN: usize = 16384 + 2048;

const FORBIDDEN_FIRST_INTS: [[u8; 4]; 7] = [
    *b"HEAD",
    *b"POST",
    *b"GET ",
    *b"OPTI",
    [0x16, 0x03, 0x01, 0x02],
    [0xdd, 0xdd, 0xdd, 0xdd],
    [0xee, 0xee, 0xee, 0xee],
];

pub(crate) fn random_bytes(buf: &mut [u8]) {
    getrandom::fill(buf).expect("getrandom");
}

/// Phần đầu obfuscated2 và hai bộ mã hoá.
pub(crate) struct Init {
    /// 64 byte gửi đi; byte 56..64 còn thiếu tới khi biết thẻ (`finish`).
    head: [u8; 64],
    /// Dòng khoá gửi ứng với byte 56..64 của phần đầu.
    ks_tail: [u8; 8],
    dc_id: i16,
    pub(crate) tx: Ctr,
    pub(crate) rx: Ctr,
}

impl Init {
    pub(crate) fn new(dc_id: i16, secret: Option<&[u8; 16]>) -> Self {
        let mut init = [0u8; 64];
        loop {
            random_bytes(&mut init);
            let first: [u8; 4] = init[..4].try_into().unwrap();
            if init[0] != 0xef && init[4..8] != [0; 4] && !FORBIDDEN_FIRST_INTS.contains(&first) {
                break;
            }
        }
        let rev: Vec<u8> = init[8..56].iter().rev().copied().collect();
        let (mut tx_key, tx_iv) = (init[8..40].to_vec(), &init[40..56]);
        let (mut rx_key, rx_iv) = (rev[..32].to_vec(), &rev[32..48]);
        if let Some(s) = secret {
            tx_key = Sha256::new().chain_update(&tx_key).chain_update(s).finalize().to_vec();
            rx_key = Sha256::new().chain_update(&rx_key).chain_update(s).finalize().to_vec();
        }
        let mut tx = Ctr::new_from_slices(&tx_key, tx_iv).unwrap();
        let rx = Ctr::new_from_slices(&rx_key, rx_iv).unwrap();
        // Dòng khoá của 64 byte đầu (mã hoá phần đầu dùng hết 64 byte dòng khoá đầu tiên)
        let mut ks = [0u8; 64];
        tx.apply_keystream(&mut ks);
        Self { head: init, ks_tail: ks[56..64].try_into().unwrap(), dc_id, tx, rx }
    }

    /// Phần đầu hoàn chỉnh với thẻ transport `tag`.
    pub(crate) fn finish(&self, tag: [u8; 4]) -> [u8; 64] {
        let mut h = self.head;
        h[56..60].copy_from_slice(&tag);
        h[60..62].copy_from_slice(&self.dc_id.to_le_bytes());
        for i in 0..8 {
            h[56 + i] ^= self.ks_tail[i];
        }
        h
    }
}

/// Luồng obfuscated2 (và fake-TLS nếu `tls`) bọc luồng `S`.
pub struct ObfsStream<S> {
    inner: S,
    init: Init,
    tls: bool,
    /// Đã gửi phần đầu chưa (lần ghi đầu).
    started: bool,
    /// Byte đã mã hoá chờ đẩy xuống `inner`, vị trí đã đẩy, và số byte của bên gọi mà hàng chờ này ứng với.
    out: Vec<u8>,
    out_pos: usize,
    out_accepted: usize,
    /// fake-TLS: byte thô đọc từ `inner` chưa đủ một bản ghi; dữ liệu đã giải mã chưa trả cho bên gọi.
    raw: Vec<u8>,
    plain: Vec<u8>,
    plain_pos: usize,
}

impl<S> ObfsStream<S> {
    /// `dc_id`: số DC (MTProxy dùng để chọn DC); `secret`: khoá MTProxy (None khi đi thẳng / qua SOCKS5);
    /// `tls`: đã bắt tay fake-TLS xong trên `inner`, dữ liệu đi trong bản ghi TLS.
    pub fn new(inner: S, dc_id: i16, secret: Option<&[u8; 16]>, tls: bool) -> Self {
        Self {
            inner,
            init: Init::new(dc_id, secret),
            tls,
            started: false,
            out: Vec::new(),
            out_pos: 0,
            out_accepted: 0,
            raw: Vec::new(),
            plain: Vec::new(),
            plain_pos: 0,
        }
    }

    /// Mã hoá `data` (byte của bên gọi) vào hàng chờ.
    fn encode(&mut self, data: &[u8]) -> io::Result<usize> {
        let (head, body) = if self.started {
            (None, data)
        } else {
            if data.len() < 4 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "lần ghi đầu phải có thẻ transport (4 byte)"));
            }
            (Some(self.init.finish(data[..4].try_into().unwrap())), &data[4..])
        };
        let mut payload = Vec::with_capacity(64 + body.len());
        if let Some(h) = head {
            payload.extend_from_slice(&h);
        }
        let at = payload.len();
        payload.extend_from_slice(body);
        self.init.tx.apply_keystream(&mut payload[at..]);
        self.out.clear();
        self.out_pos = 0;
        if self.tls {
            if !self.started {
                // ChangeCipherSpec như trình duyệt, rồi mới tới dữ liệu
                self.out.extend_from_slice(&[0x14, 0x03, 0x03, 0x00, 0x01, 0x01]);
            }
            for rec in payload.chunks(MAX_TLS_RECORD) {
                self.out.extend_from_slice(&[0x17, 0x03, 0x03]);
                self.out.extend_from_slice(&(rec.len() as u16).to_be_bytes());
                self.out.extend_from_slice(rec);
            }
        } else {
            self.out = payload;
        }
        self.started = true;
        Ok(data.len())
    }
}

impl<S: AsyncWrite + Unpin> ObfsStream<S> {
    /// Đẩy hàng chờ xuống `inner`.
    fn poll_drain(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.out_pos < self.out.len() {
            let n = ready!(Pin::new(&mut self.inner).poll_write(cx, &self.out[self.out_pos..]))?;
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.out_pos += n;
        }
        self.out.clear();
        self.out_pos = 0;
        Poll::Ready(Ok(()))
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for ObfsStream<S> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.out_accepted == 0 {
            if buf.is_empty() {
                return Poll::Ready(Ok(0));
            }
            let n = buf.len().min(MAX_CHUNK);
            this.out_accepted = this.encode(&buf[..n])?;
        }
        ready!(this.poll_drain(cx))?;
        Poll::Ready(Ok(std::mem::take(&mut this.out_accepted)))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_drain(cx))?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_drain(cx))?;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for ObfsStream<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.tls {
            let before = buf.filled().len();
            ready!(Pin::new(&mut this.inner).poll_read(cx, buf))?;
            this.init.rx.apply_keystream(&mut buf.filled_mut()[before..]);
            return Poll::Ready(Ok(()));
        }
        loop {
            if this.plain_pos < this.plain.len() {
                let n = buf.remaining().min(this.plain.len() - this.plain_pos);
                buf.put_slice(&this.plain[this.plain_pos..this.plain_pos + n]);
                this.plain_pos += n;
                return Poll::Ready(Ok(()));
            }
            // Đủ một bản ghi trong `raw` thì tách ra
            if this.raw.len() >= 5 {
                let len = u16::from_be_bytes([this.raw[3], this.raw[4]]) as usize;
                if this.raw[1..3] != [0x03, 0x03] || len > MAX_TLS_IN {
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidData, "MTProxy gửi bản ghi TLS sai")));
                }
                if this.raw.len() >= 5 + len {
                    let kind = this.raw[0];
                    let rec: Vec<u8> = this.raw.drain(..5 + len).skip(5).collect();
                    match kind {
                        0x17 => {
                            this.plain = rec;
                            this.plain_pos = 0;
                            this.init.rx.apply_keystream(&mut this.plain);
                        }
                        0x14 => {} // ChangeCipherSpec: bỏ qua
                        _ => {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("MTProxy gửi bản ghi TLS lạ (kiểu {kind:#04x})"),
                            )));
                        }
                    }
                    continue;
                }
            }
            let mut tmp = [0u8; 8192];
            let mut rb = ReadBuf::new(&mut tmp);
            ready!(Pin::new(&mut this.inner).poll_read(cx, &mut rb))?;
            if rb.filled().is_empty() {
                if this.raw.is_empty() {
                    return Poll::Ready(Ok(())); // hết luồng
                }
                return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
            }
            this.raw.extend_from_slice(rb.filled());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Phía máy chủ (MTProxy / DC) theo đặc tả: đọc phần đầu, suy khoá, giải mã. Trả (thẻ, dc, bộ giải mã, bộ mã hoá).
    pub(crate) fn server_side(head: &[u8; 64], secret: Option<&[u8; 16]>) -> ([u8; 4], i16, Ctr, Ctr) {
        let rev: Vec<u8> = head[8..56].iter().rev().copied().collect();
        let (mut dk, div) = (head[8..40].to_vec(), &head[40..56]);
        let (mut ek, eiv) = (rev[..32].to_vec(), &rev[32..48]);
        if let Some(s) = secret {
            dk = Sha256::new().chain_update(&dk).chain_update(s).finalize().to_vec();
            ek = Sha256::new().chain_update(&ek).chain_update(s).finalize().to_vec();
        }
        let mut dec = Ctr::new_from_slices(&dk, div).unwrap();
        let enc = Ctr::new_from_slices(&ek, eiv).unwrap();
        let mut plain = *head;
        dec.apply_keystream(&mut plain);
        (plain[56..60].try_into().unwrap(), i16::from_le_bytes([plain[60], plain[61]]), dec, enc)
    }

    #[test]
    fn header_rules() {
        for _ in 0..200 {
            let i = Init::new(-2, None);
            let h = i.finish([0xdd; 4]);
            assert_ne!(h[0], 0xef);
            assert_ne!(h[4..8], [0; 4]);
            let (tag, dc, _, _) = server_side(&h, None);
            assert_eq!(tag, [0xdd; 4]);
            assert_eq!(dc, -2);
        }
    }

    async fn roundtrip(secret: Option<[u8; 16]>) {
        let (client, mut server) = tokio::io::duplex(1 << 20);
        let mut c = ObfsStream::new(client, 4, secret.as_ref(), false);
        // grammers Intermediate: thẻ eeeeeeee rồi gói (độ dài + dữ liệu)
        let mut first = vec![0xee; 4];
        first.extend_from_slice(&8u32.to_le_bytes());
        first.extend_from_slice(b"12345678");
        c.write_all(&first).await.unwrap();
        c.write_all(b"\x04\x00\x00\x00abcd").await.unwrap();
        c.flush().await.unwrap();

        let mut head = [0u8; 64];
        server.read_exact(&mut head).await.unwrap();
        let (tag, dc, mut dec, mut enc) = server_side(&head, secret.as_ref());
        assert_eq!(tag, [0xee; 4]);
        assert_eq!(dc, 4);
        let mut body = vec![0u8; 12 + 8];
        server.read_exact(&mut body).await.unwrap();
        dec.apply_keystream(&mut body);
        assert_eq!(&body, b"\x08\x00\x00\x0012345678\x04\x00\x00\x00abcd");

        let mut reply = b"\x04\x00\x00\x00pong".to_vec();
        enc.apply_keystream(&mut reply);
        server.write_all(&reply).await.unwrap();
        let mut got = [0u8; 8];
        c.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"\x04\x00\x00\x00pong");
    }

    #[tokio::test]
    async fn obfs2_direct() {
        roundtrip(None).await;
    }

    #[tokio::test]
    async fn obfs2_with_secret() {
        roundtrip(Some([7u8; 16])).await;
    }

    #[tokio::test]
    async fn tls_records() {
        let key = [9u8; 16];
        let (client, mut server) = tokio::io::duplex(1 << 20);
        let mut c = ObfsStream::new(client, 2, Some(&key), true);
        let mut first = vec![0xdd; 4];
        let big = vec![0x5a; 6000]; // > 2 bản ghi
        first.extend_from_slice(&(big.len() as u32).to_le_bytes());
        first.extend_from_slice(&big);
        c.write_all(&first).await.unwrap();
        c.flush().await.unwrap();

        let mut ccs = [0u8; 6];
        server.read_exact(&mut ccs).await.unwrap();
        assert_eq!(ccs, [0x14, 0x03, 0x03, 0x00, 0x01, 0x01]);
        let mut payload = Vec::new();
        while payload.len() < 64 + 4 + big.len() {
            let mut h = [0u8; 5];
            server.read_exact(&mut h).await.unwrap();
            assert_eq!(h[..3], [0x17, 0x03, 0x03]);
            let len = u16::from_be_bytes([h[3], h[4]]) as usize;
            assert!(len <= MAX_TLS_RECORD);
            let mut r = vec![0u8; len];
            server.read_exact(&mut r).await.unwrap();
            payload.extend_from_slice(&r);
        }
        let head: [u8; 64] = payload[..64].try_into().unwrap();
        let (tag, dc, mut dec, mut enc) = server_side(&head, Some(&key));
        assert_eq!((tag, dc), ([0xdd; 4], 2));
        let mut body = payload[64..].to_vec();
        dec.apply_keystream(&mut body);
        assert_eq!(&body[4..], &big[..]);

        // Máy chủ trả hai bản ghi, một ChangeCipherSpec chen giữa
        let mut reply = b"\x08\x00\x00\x00ABCDEFGH".to_vec();
        enc.apply_keystream(&mut reply);
        let mut wire = vec![0x17, 0x03, 0x03, 0x00, 0x05];
        wire.extend_from_slice(&reply[..5]);
        wire.extend_from_slice(&[0x14, 0x03, 0x03, 0x00, 0x01, 0x01, 0x17, 0x03, 0x03, 0x00, 0x07]);
        wire.extend_from_slice(&reply[5..]);
        server.write_all(&wire).await.unwrap();
        let mut got = [0u8; 12];
        c.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"\x08\x00\x00\x00ABCDEFGH");
    }
}
