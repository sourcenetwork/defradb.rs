//! Opens a client connection for the pool: a name looked up, a nuclei
//! socket connected, TLS negotiated (HTTP/2 or HTTP/1.1 by ALPN), through
//! the system's proxy when one applies.

use std::future::{poll_fn, Future};
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use http::Uri;
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::client::proxy::matcher::Matcher;
use rustls::pki_types::ServerName;
use tower_service::Service;

use crate::stream::{ClientStream, Conn, Stream};
use crate::tls::{ClientTls, TlsConnector};
use crate::BoxError;

/// A connection to `uri`'s host on a nuclei socket, for a protocol spoken
/// over it directly (a WebSocket): TLS with the system's root certificates
/// for `https` and `wss`, HTTP/1.1 offered by ALPN, no proxy.
pub async fn connect(uri: &Uri) -> Result<ClientStream, BoxError> {
    let tls = ClientTls {
        alpn: vec![b"http/1.1".to_vec()],
        ..ClientTls::default()
    }
    .config()?;
    let connector = Connector {
        tls: Ok(TlsConnector::from(tls)),
        connect_timeout: None,
        proxies: None,
    };
    Ok(ClientStream(
        connector.connect(uri.clone()).await?.into_stream(),
    ))
}

#[derive(Clone)]
pub(crate) struct Connector {
    /// The TLS client, or why it could not be built: HTTPS fails with it.
    pub(crate) tls: Result<TlsConnector, Arc<str>>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) proxies: Option<Arc<Matcher>>,
}

impl Service<Uri> for Connector {
    type Response = Conn;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Conn, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        let connector = self.clone();
        Box::pin(async move { connector.connect(dst).await })
    }
}

impl Connector {
    async fn connect(self, dst: Uri) -> Result<Conn, BoxError> {
        let https = is_https(&dst);
        let intercept = self
            .proxies
            .as_ref()
            .and_then(|proxies| proxies.intercept(&dst));
        let Some(intercept) = intercept else {
            let stream = self.socket(&dst).await?;
            return if https {
                self.tls(stream, &dst).await
            } else {
                Ok(Conn::new(stream, false, false))
            };
        };
        let direct = Self {
            proxies: None,
            ..self.clone()
        };
        if !https {
            // A plain request goes to the proxy whole, in absolute form.
            let proxy = Box::pin(direct.connect(intercept.uri().clone())).await?;
            return Ok(Conn::new(proxy.into_stream(), false, true));
        }
        let mut tunnel = Tunnel::new(intercept.uri().clone(), direct);
        if let Some(auth) = intercept.basic_auth() {
            tunnel = tunnel.with_auth(auth.clone());
        }
        poll_fn(|cx| tunnel.poll_ready(cx)).await?;
        let tunneled = tunnel.call(dst.clone()).await?;
        self.tls(tunneled.into_stream(), &dst).await
    }

    /// A nuclei socket to `dst`'s host, trying each address it resolves to.
    async fn socket(&self, dst: &Uri) -> io::Result<Stream> {
        let host = host(dst)?;
        let port = dst
            .port_u16()
            .unwrap_or(if is_https(dst) { 443 } else { 80 });
        let connecting = async {
            let addresses: Vec<SocketAddr> = match host.parse::<IpAddr>() {
                Ok(ip) => vec![SocketAddr::new(ip, port)],
                Err(_) => nuclei::net::lookup_host((host.to_owned(), port))
                    .await?
                    .collect(),
            };
            let mut last = None;
            for address in addresses {
                match nuclei::net::TcpStream::connect(address).await {
                    Ok(socket) => {
                        socket.get_ref().set_nodelay(true)?;
                        return Ok(Stream::Plain(socket));
                    }
                    Err(error) => last = Some(error),
                }
            }
            Err(last.unwrap_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("{host} has no address"))
            }))
        };
        match self.connect_timeout {
            Some(limit) => nuclei::time::timeout(limit, connecting)
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connect timed out"))?,
            None => connecting.await,
        }
    }

    async fn tls(&self, stream: Stream, dst: &Uri) -> Result<Conn, BoxError> {
        let connector = self
            .tls
            .clone()
            .map_err(|reason| -> BoxError { reason.as_ref().into() })?;
        let name = ServerName::try_from(host(dst)?.to_owned())?;
        let tls = connector.connect(name, stream).await?;
        let h2 = tls.get_ref().1.alpn_protocol() == Some(b"h2");
        Ok(Conn::new(Stream::Tls(Box::new(tls)), h2, false))
    }
}

fn is_https(uri: &Uri) -> bool {
    matches!(uri.scheme_str(), Some("https" | "wss"))
}

/// The host of `uri`, without an IPv6 literal's brackets.
fn host(uri: &Uri) -> io::Result<&str> {
    uri.host()
        .map(|host| host.trim_start_matches('[').trim_end_matches(']'))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the URL has no host"))
}
