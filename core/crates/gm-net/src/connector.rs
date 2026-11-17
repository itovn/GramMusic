//! Mở kết nối tới DC theo cấu hình proxy, cắm vào grammers qua `ConnectionParams::connector` (bản vá
//! `patches/0001-grammers-mtsender-connector.patch`).
//!
//! | Cấu hình | Đường đi | Vỏ | Transport |
//! | --- | --- | --- | --- |
//! | Đi thẳng | TCP tới DC | obfuscated2 | intermediate |
//! | SOCKS5 | SOCKS5 tới DC | obfuscated2 | intermediate |
//! | MTProxy secret thường | TCP tới MTProxy | obfuscated2 + secret | intermediate |
//! | MTProxy `dd` | TCP tới MTProxy | obfuscated2 + secret | intermediate có đệm |
//! | MTProxy `ee` | TCP tới MTProxy, bắt tay fake-TLS | bản ghi TLS + obfuscated2 + secret | intermediate có đệm |

use std::io;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use grammers_mtproto::transport::Transport;
use grammers_mtsender::{AutoSleep, BoxedStream, ConnectFuture, ConnectionParams, Connector};
use tokio::net::TcpStream;

use crate::config::{ProxyConfig, Secret};
use crate::obfs::ObfsStream;
use crate::transport::Intermediate;
use crate::{faketls, socks5};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Hàm báo mỗi socket TCP vừa mở xong (fd, DC) — app iOS bọc socket thành «VoIP» để iOS đánh thức app khi có tin về.
pub type SocketHook = Arc<dyn Fn(i32, i32) + Send + Sync>;

/// Bộ mở kết nối dùng chung cho mọi kết nối của một phiên. Đổi proxy bằng `set` (kết nối mới theo cấu hình mới; bên gọi tự
/// ngắt kết nối cũ).
#[derive(Default)]
pub struct GmConnector {
    cfg: RwLock<ProxyConfig>,
    hook: RwLock<Option<SocketHook>>,
}

impl std::fmt::Debug for GmConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GmConnector").field("cfg", &self.config()).finish_non_exhaustive()
    }
}

impl GmConnector {
    pub fn new(cfg: ProxyConfig) -> Arc<Self> {
        Arc::new(Self { cfg: RwLock::new(cfg), hook: RwLock::new(None) })
    }

    /// Đặt hàm báo socket mới (xem [`SocketHook`]).
    pub fn set_socket_hook(&self, hook: SocketHook) {
        *self.hook.write().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    pub fn config(&self) -> ProxyConfig {
        self.cfg.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, cfg: ProxyConfig) {
        *self.cfg.write().unwrap_or_else(|e| e.into_inner()) = cfg;
    }

    /// Transport hợp với cấu hình hiện tại.
    pub fn transport(&self) -> Box<dyn Transport + Send> {
        match &*self.cfg.read().unwrap_or_else(|e| e.into_inner()) {
            ProxyConfig::MtProxy(m) if m.secret.padded() => Box::new(Intermediate::padded()),
            _ => Box::new(Intermediate::plain()),
        }
    }

    /// `ConnectionParams` của grammers dùng bộ mở kết nối này.
    ///
    /// Dựng đủ từng trường, không qua `ConnectionParams::default()`: hàm đó hỏi ngôn ngữ hệ thống bằng crate
    /// `locate-locale` (gọi Objective-C trên Apple) và `os_info` — trên iOS kéo theo ký hiệu objc, mà lõi không cần (app tự
    /// báo tên máy, phiên bản, ngôn ngữ).
    pub fn connection_params(self: &Arc<Self>, info: ClientInfo) -> ConnectionParams {
        let me = Arc::clone(self);
        ConnectionParams {
            device_model: info.device_model,
            system_version: info.system_version,
            app_version: info.app_version,
            system_lang_code: info.lang_code.clone(),
            lang_code: info.lang_code,
            use_ipv6: false,
            retry_policy: Box::new(AutoSleep::default()),
            updates_channel_capacity: NonZeroUsize::new(100).unwrap(),
            transport: Box::new(move || me.transport()),
            connector: Some(Arc::clone(self) as Arc<dyn Connector>),
            __non_exhaustive: (),
        }
    }
}

/// Thông tin báo cho Telegram trong `initConnection` (hiện ở mục «Thiết bị» của tài khoản).
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// vd «iPhone5,2».
    pub device_model: String,
    /// vd «iOS 6.1.4».
    pub system_version: String,
    /// vd «GramMini 0.1.0».
    pub app_version: String,
    /// Mã ngôn ngữ ISO 639-1, vd «vi».
    pub lang_code: String,
}

impl Connector for GmConnector {
    fn connect(&self, dc_id: i32, address: SocketAddr) -> ConnectFuture<'_> {
        let cfg = self.config();
        let hook = self.hook.read().unwrap_or_else(|e| e.into_inner()).clone();
        Box::pin(async move { open_with(&cfg, dc_id, address, hook.as_ref()).await })
    }
}

/// TCP tới `target` — tên miền (MTProxy ghi bằng tên) tra qua DNS mã hoá nếu bật (dns.rs), không được thì DNS của máy.
async fn tcp(target: (&str, u16)) -> io::Result<TcpStream> {
    let s = tokio::time::timeout(CONNECT_TIMEOUT, crate::dns::connect(target.0, target.1))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, format!("kết nối {}:{} quá lâu", target.0, target.1)))??;
    let _ = s.set_nodelay(true);
    Ok(s)
}

/// Mở luồng tới DC `dc_id` (địa chỉ `addr`) theo `cfg`.
pub async fn open(cfg: &ProxyConfig, dc_id: i32, addr: SocketAddr) -> io::Result<BoxedStream> {
    open_with(cfg, dc_id, addr, None).await
}

/// Báo socket vừa mở (chỉ Unix — fd).
fn announce(s: &TcpStream, dc_id: i32, hook: Option<&SocketHook>) {
    #[cfg(unix)]
    if let Some(h) = hook {
        use std::os::unix::io::AsRawFd;
        h(s.as_raw_fd(), dc_id);
    }
    #[cfg(not(unix))]
    let _ = (s, dc_id, hook);
}

async fn open_with(cfg: &ProxyConfig, dc_id: i32, addr: SocketAddr, hook: Option<&SocketHook>) -> io::Result<BoxedStream> {
    let dc = dc_id as i16;
    match cfg {
        ProxyConfig::Direct => {
            let s = tcp((&addr.ip().to_string(), addr.port())).await?;
            announce(&s, dc_id, hook);
            Ok(Box::new(ObfsStream::new(s, dc, None, false)))
        }
        ProxyConfig::Socks5(c) => {
            let s = socks5::connect(c, addr).await?;
            announce(&s, dc_id, hook);
            Ok(Box::new(ObfsStream::new(s, dc, None, false)))
        }
        ProxyConfig::MtProxy(m) => {
            let mut s = tcp((&m.host, m.port)).await.map_err(|e| {
                io::Error::new(e.kind(), format!("không kết nối được MTProxy {}:{} ({e})", m.host, m.port))
            })?;
            let tls = if let Secret::FakeTls { key, domain } = &m.secret {
                // Ký ClientHello bằng giờ đúng (MTProxy chỉ cho lệch vài giây — D11)
                crate::clock::ensure_synced().await;
                tokio::time::timeout(HANDSHAKE_TIMEOUT, faketls::handshake(&mut s, key, domain))
                    .await
                    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MTProxy không trả lời bắt tay fake-TLS"))??;
                true
            } else {
                false
            };
            announce(&s, dc_id, hook);
            Ok(Box::new(ObfsStream::new(s, dc, Some(m.secret.key()), tls)))
        }
    }
}
