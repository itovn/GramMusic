//! Đường ra mạng của lõi GramMini: đi thẳng / SOCKS5 / MTProxy (secret thường, `dd`, `ee` fake-TLS), luôn bọc obfuscated2.
//! Cắm vào grammers bằng `GmConnector::connection_params`.

pub mod clock;
pub mod config;
pub mod connector;
pub mod dns;
pub mod faketls;
pub mod link;
pub mod obfs;
pub mod socks5;
pub mod transport;

pub use config::{MtProxy, ProxyConfig, Secret, Socks5};
pub use connector::{ClientInfo, GmConnector, open};
pub use link::{parse_link, parse_secret};
