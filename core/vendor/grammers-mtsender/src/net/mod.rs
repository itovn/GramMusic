// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

mod tcp;

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

pub use tcp::NetStream;

/// A bidirectional byte stream (GramMini patch).
pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadWrite for T {}

/// Boxed bidirectional byte stream returned by a [`Connector`] (GramMini patch).
pub type BoxedStream = Box<dyn AsyncReadWrite>;

/// Future returned by [`Connector::connect`] (GramMini patch).
pub type ConnectFuture<'a> = Pin<Box<dyn Future<Output = std::io::Result<BoxedStream>> + Send + 'a>>;

/// Opens the byte stream used to talk to a datacenter (GramMini patch).
///
/// Lets the application decide how to reach `dc_id` (whose address is `address`): directly,
/// through a SOCKS5 / Shadowsocks proxy, or through an MTProxy with its obfuscation / fake-TLS
/// layers. The returned stream carries the bytes produced by the configured transport as-is.
pub trait Connector: Send + Sync + std::fmt::Debug {
    fn connect(&self, dc_id: i32, address: SocketAddr) -> ConnectFuture<'_>;
}

/// Represents a socket address which may be proxied.
#[derive(Debug, Clone)]
pub enum ServerAddr {
    /// Socket address whose connection should be proxied.
    #[cfg(feature = "proxy")]
    Proxied {
        address: std::net::SocketAddr,
        proxy: String,
    },
    /// Proxy address for direct connection.
    Tcp { address: std::net::SocketAddr },
    /// Connection opened by an application-provided [`Connector`] (GramMini patch).
    Custom {
        dc_id: i32,
        address: std::net::SocketAddr,
        connector: Arc<dyn Connector>,
    },
}
