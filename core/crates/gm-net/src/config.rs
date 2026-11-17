//! Cấu hình đường ra mạng của lõi.

/// Đường tới máy chủ Telegram.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProxyConfig {
    /// Đi thẳng tới DC (vẫn bọc obfuscated2 để máy lọc gói khó nhận ra MTProto).
    #[default]
    Direct,
    MtProxy(MtProxy),
    Socks5(Socks5),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtProxy {
    pub host: String,
    pub port: u16,
    pub secret: Secret,
}

/// Secret MTProxy (xem `link.rs`).
#[derive(Clone, PartialEq, Eq)]
pub enum Secret {
    Simple([u8; 16]),
    Padded([u8; 16]),
    FakeTls { key: [u8; 16], domain: String },
}

impl Secret {
    pub fn key(&self) -> &[u8; 16] {
        match self {
            Secret::Simple(k) | Secret::Padded(k) | Secret::FakeTls { key: k, .. } => k,
        }
    }

    /// Dùng intermediate có đệm (thẻ `dddddddd`) — bắt buộc với secret `dd` / `ee`.
    pub fn padded(&self) -> bool {
        !matches!(self, Secret::Simple(_))
    }
}

// Không in khoá ra log.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Secret::Simple(_) => write!(f, "Simple(…)"),
            Secret::Padded(_) => write!(f, "Padded(…)"),
            Secret::FakeTls { domain, .. } => write!(f, "FakeTls(…, {domain})"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Socks5 {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

impl std::fmt::Debug for Socks5 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Socks5({}:{}{})", self.host, self.port, if self.user.is_empty() { "" } else { ", có đăng nhập" })
    }
}
