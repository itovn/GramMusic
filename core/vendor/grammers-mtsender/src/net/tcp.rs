// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::info;

use super::ServerAddr;

/// GramMini patch: the stream is kept as two boxed halves so that any byte stream
/// (plain TCP, SOCKS5, or one opened by a [`super::Connector`]) can be used.
pub struct NetStream {
    reader: Box<dyn AsyncRead + Send + Unpin>,
    writer: Box<dyn AsyncWrite + Send + Unpin>,
}

impl NetStream {
    fn from_tcp(stream: TcpStream) -> Self {
        let _ = stream.set_nodelay(true);
        let (reader, writer) = stream.into_split();
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }

    pub(crate) fn split(
        &mut self,
    ) -> (
        &mut (dyn AsyncRead + Send + Unpin),
        &mut (dyn AsyncWrite + Send + Unpin),
    ) {
        (&mut *self.reader, &mut *self.writer)
    }

    pub(crate) async fn connect(addr: &ServerAddr) -> Result<Self, std::io::Error> {
        info!("connecting...");
        match addr {
            ServerAddr::Tcp { address } => Ok(Self::from_tcp(TcpStream::connect(address).await?)),
            ServerAddr::Custom {
                dc_id,
                address,
                connector,
            } => {
                let stream = connector.connect(*dc_id, *address).await?;
                let (reader, writer) = tokio::io::split(stream);
                Ok(Self {
                    reader: Box::new(reader),
                    writer: Box::new(writer),
                })
            }
            #[cfg(feature = "proxy")]
            ServerAddr::Proxied { address, proxy } => {
                Self::connect_proxy_stream(address, proxy).await
            }
        }
    }

    #[cfg(feature = "proxy")]
    async fn connect_proxy_stream(
        addr: &std::net::SocketAddr,
        proxy_url: &str,
    ) -> Result<NetStream, std::io::Error> {
        use std::{
            io::{self, ErrorKind},
            net::{IpAddr, SocketAddr},
        };

        use hickory_resolver::Resolver;
        use url::Host;

        let proxy = url::Url::parse(proxy_url)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
        let scheme = proxy.scheme();
        let host = proxy.host().ok_or(io::Error::new(
            ErrorKind::NotFound,
            format!("proxy host is missing from url: {}", proxy_url),
        ))?;
        let port = proxy.port().ok_or(io::Error::new(
            ErrorKind::NotFound,
            format!("proxy port is missing from url: {}", proxy_url),
        ))?;
        let username = proxy.username();
        let password = proxy.password().unwrap_or("");
        let socks_addr = match host {
            Host::Domain(domain) => {
                let resolver = Resolver::builder_tokio().unwrap().build().unwrap();
                let response = resolver
                    .lookup_ip(domain)
                    .await
                    .map_err(|err| io::Error::new(ErrorKind::Other, err))?;
                let socks_ip_addr = response.iter().next().ok_or(io::Error::new(
                    ErrorKind::NotFound,
                    format!("proxy host did not return any ip address: {}", domain),
                ))?;
                SocketAddr::new(socks_ip_addr, port)
            }
            Host::Ipv4(v4) => SocketAddr::new(IpAddr::from(v4), port),
            Host::Ipv6(v6) => SocketAddr::new(IpAddr::from(v6), port),
        };

        let stream = match scheme {
            "socks5" => {
                if username.is_empty() {
                    tokio_socks::tcp::Socks5Stream::connect(socks_addr, addr)
                        .await
                        .map_err(|err| io::Error::new(ErrorKind::ConnectionAborted, err))?
                } else {
                    tokio_socks::tcp::Socks5Stream::connect_with_password(
                        socks_addr, addr, username, password,
                    )
                    .await
                    .map_err(|err| io::Error::new(ErrorKind::ConnectionAborted, err))?
                }
            }
            scheme => {
                return Err(io::Error::new(
                    ErrorKind::ConnectionAborted,
                    format!("proxy scheme not supported: {}", scheme),
                ));
            }
        };
        Ok(Self::from_tcp(stream.into_inner()))
    }

    pub(crate) async fn disconnect(&mut self) -> std::io::Result<()> {
        self.writer.shutdown().await
    }
}
