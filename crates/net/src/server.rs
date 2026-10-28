//! The accept loop: a tower service served over HTTP/1 and HTTP/2 on a
//! nuclei listener, plain or over TLS.
//!
//! Each connection runs as a task the server owns: when the server future
//! is dropped, its connections end with it, keep-alive ones included.

use std::io;
use std::time::Duration;

use http::{Request, Response};
use hyper::body::Incoming;
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use nuclei::net::{TcpListener, TcpStream};
use nuclei::task::JoinSet;
use tower_service::Service;

use crate::rt::{Executor, Io, Timer};
use crate::tls::TlsAcceptor;
use crate::BoxError;

/// How long a client may take to finish its TLS handshake.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// How long the loop waits after an accept fails for want of resources
/// (descriptors, memory) before it accepts again.
const ACCEPT_BACKOFF: Duration = Duration::from_secs(1);

/// Serves `service` on every connection `listener` accepts until the
/// returned future is dropped. With `tls`, each connection first completes
/// a TLS handshake, and ALPN picks HTTP/2 or HTTP/1.1; without it, the
/// protocol is read from the connection's first bytes.
pub async fn serve<S, B>(
    listener: TcpListener,
    tls: Option<TlsAcceptor>,
    service: S,
) -> io::Result<()>
where
    S: Service<Request<Incoming>, Response = Response<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<BoxError>,
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    let mut connections = JoinSet::new();
    loop {
        let accepted = nuclei::select! {
            accepted = listener.accept() => accepted,
            Some(_) = connections.join_next() => continue,
        };
        let stream = match accepted {
            Ok((stream, _peer)) => stream,
            Err(error) if is_connection_error(&error) => continue,
            Err(error) => {
                tracing::warn!(%error, "accepting a connection failed; retrying shortly");
                nuclei::time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };
        if let Err(error) = stream.get_ref().set_nodelay(true) {
            tracing::debug!(%error, "could not disable Nagle's algorithm on a connection");
        }
        connections.spawn(serve_connection(stream, tls.clone(), service.clone()));
    }
}

async fn serve_connection<S, B>(stream: TcpStream, tls: Option<TlsAcceptor>, service: S)
where
    S: Service<Request<Incoming>, Response = Response<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<BoxError>,
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    let service = TowerToHyperService::new(service);
    let mut builder = auto::Builder::new(Executor);
    builder.http1().timer(Timer);
    builder.http2().timer(Timer);
    let served = match tls {
        None => {
            builder
                .serve_connection_with_upgrades(Io::new(stream), service)
                .await
        }
        Some(acceptor) => match nuclei::time::timeout(HANDSHAKE, acceptor.accept(stream)).await {
            Ok(Ok(tls)) => {
                builder
                    .serve_connection_with_upgrades(Io::new(tls), service)
                    .await
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "TLS handshake failed");
                return;
            }
            Err(_) => {
                tracing::debug!("TLS handshake timed out");
                return;
            }
        },
    };
    if let Err(error) = served {
        tracing::debug!(%error, "HTTP connection ended with an error");
    }
}

/// An accept error that concerns only the connection being accepted.
fn is_connection_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}
