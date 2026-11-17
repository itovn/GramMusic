//! Phía khách SOCKS5 (RFC 1928, đăng nhập RFC 1929) — chép từ lõi chung ChimToTV (`linux/core/src/net.rs`), đổi lỗi sang
//! `io::Error` (câu báo tiếng Việt cho người dùng). Đích luôn là địa chỉ IP (địa chỉ DC của Telegram).

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::config::Socks5;

fn err(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionRefused, msg)
}

fn reply_text(rep: u8) -> String {
    match rep {
        1 => "lỗi chung".into(),
        2 => "proxy không cho phép".into(),
        3 => "mạng không tới được".into(),
        4 => "máy đích không tới được".into(),
        5 => "máy đích từ chối".into(),
        6 => "hết thời gian".into(),
        7 => "lệnh không hỗ trợ".into(),
        8 => "kiểu địa chỉ không hỗ trợ".into(),
        n => format!("mã {n}"),
    }
}

/// Mở kết nối tới `target` qua proxy SOCKS5 `c`.
pub async fn connect(c: &Socks5, target: SocketAddr) -> io::Result<TcpStream> {
    let mut s = match tokio::time::timeout(Duration::from_secs(10), crate::dns::connect(&c.host, c.port)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) if e.kind() == io::ErrorKind::NotFound || e.to_string().to_lowercase().contains("lookup") => {
            return Err(err(format!("không tìm thấy proxy {}", c.host)));
        }
        _ => return Err(err(format!("không kết nối được proxy {}:{}", c.host, c.port))),
    };
    let _ = s.set_nodelay(true);
    match tokio::time::timeout(Duration::from_secs(15), handshake(&mut s, c, target)).await {
        Ok(Ok(())) => Ok(s),
        Ok(Err(e)) if e.kind() == io::ErrorKind::ConnectionRefused => Err(e),
        Ok(Err(e)) => Err(err(format!("proxy {}:{} ngắt kết nối ({e})", c.host, c.port))),
        Err(_) => Err(err(format!("proxy {}:{} ngắt kết nối (hết thời gian)", c.host, c.port))),
    }
}

async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(st: &mut S, c: &Socks5, target: SocketAddr) -> io::Result<()> {
    let login = !c.user.is_empty();
    st.write_all(if login { &[5, 2, 0, 2] } else { &[5, 1, 0] }).await?;
    let mut g = [0u8; 2];
    st.read_exact(&mut g).await?;
    if g[0] != 5 {
        return Err(err(format!("{}:{} không phải proxy SOCKS5", c.host, c.port)));
    }
    match g[1] {
        0 => {}
        2 => {
            if !login {
                return Err(err("proxy cần tên đăng nhập / mật khẩu".into()));
            }
            let (u, p) = (c.user.as_bytes(), c.pass.as_bytes());
            if u.len() > 255 || p.len() > 255 {
                return Err(err("tên đăng nhập / mật khẩu quá dài (tối đa 255 byte)".into()));
            }
            let mut m = vec![1, u.len() as u8];
            m.extend_from_slice(u);
            m.push(p.len() as u8);
            m.extend_from_slice(p);
            st.write_all(&m).await?;
            let mut a = [0u8; 2];
            st.read_exact(&mut a).await?;
            if a[1] != 0 {
                return Err(err("proxy báo sai tên đăng nhập / mật khẩu".into()));
            }
        }
        _ => {
            return Err(err(
                if login { "proxy không nhận cách đăng nhập này" } else { "proxy cần tên đăng nhập / mật khẩu" }.into(),
            ));
        }
    }
    let mut req = vec![5, 1, 0];
    match target.ip() {
        IpAddr::V4(v) => {
            req.push(1);
            req.extend_from_slice(&v.octets());
        }
        IpAddr::V6(v) => {
            req.push(4);
            req.extend_from_slice(&v.octets());
        }
    }
    req.extend_from_slice(&target.port().to_be_bytes());
    st.write_all(&req).await?;
    let mut r = [0u8; 4];
    st.read_exact(&mut r).await?;
    if r[0] != 5 {
        return Err(err("proxy trả lời sai giao thức".into()));
    }
    let rep = r[1];
    let skip = match r[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut l = [0u8; 1];
            st.read_exact(&mut l).await?;
            l[0] as usize
        }
        _ => 0,
    };
    let mut rest = vec![0u8; skip + 2];
    st.read_exact(&mut rest).await?;
    // Mã 2 khi có đăng nhập: nhiều proxy nhận mọi mật khẩu ở bước đăng nhập rồi mới từ chối ở đây
    if rep == 2 && login {
        return Err(err("proxy từ chối (thường do sai tên đăng nhập / mật khẩu)".into()));
    }
    if rep != 0 {
        return Err(err(format!("proxy báo lỗi: {} ({target})", reply_text(rep))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handshake_with_login() {
        let (mut cl, mut sv) = tokio::io::duplex(4096);
        let cfg = Socks5 { host: "p".into(), port: 1, user: "u".into(), pass: "pw".into() };
        let target: SocketAddr = "149.154.167.41:443".parse().unwrap();
        let server = tokio::spawn(async move {
            let mut b = [0u8; 4];
            sv.read_exact(&mut b).await.unwrap();
            assert_eq!(b, [5, 2, 0, 2]);
            sv.write_all(&[5, 2]).await.unwrap();
            let mut a = [0u8; 6];
            sv.read_exact(&mut a).await.unwrap();
            assert_eq!(a, [1, 1, b'u', 2, b'p', b'w']);
            sv.write_all(&[1, 0]).await.unwrap();
            let mut r = [0u8; 10];
            sv.read_exact(&mut r).await.unwrap();
            assert_eq!(r, [5, 1, 0, 1, 149, 154, 167, 41, 1, 187]);
            sv.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
        });
        handshake(&mut cl, &cfg, target).await.unwrap();
        server.await.unwrap();
    }
}
