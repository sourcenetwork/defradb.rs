//! A test server on a nuclei listener, plain or over TLS.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use net::tls::{ServerConfig, TlsAcceptor};

/// What the test server answers, by path:
/// - `/echo`: the request body, with its method in `x-method`;
/// - `/query`: the request's query string;
/// - `/status/<code>`: that status;
/// - `/slow`: an answer after two seconds;
/// - `/redirect/<code>`: a redirect to `/method` with that status;
/// - `/method`: the request's method.
async fn answer(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = request.uri().path().to_owned();
    let method = request.method().to_string();
    let query = request.uri().query().unwrap_or("").to_owned();
    let body = request
        .into_body()
        .collect()
        .await
        .map(|body| body.to_bytes())
        .unwrap_or_default();
    let response = match path.as_str() {
        "/echo" => Response::builder()
            .header("x-method", method)
            .body(Full::new(body)),
        "/query" => Response::builder().body(Full::new(Bytes::from(query))),
        "/slow" => {
            nuclei::time::sleep(Duration::from_secs(2)).await;
            Response::builder().body(Full::new(Bytes::from_static(b"late")))
        }
        "/method" => Response::builder().body(Full::new(Bytes::from(method))),
        other => {
            if let Some(code) = other.strip_prefix("/status/") {
                Response::builder()
                    .status(code.parse::<u16>().unwrap_or(500))
                    .body(Full::new(Bytes::new()))
            } else if let Some(code) = other.strip_prefix("/redirect/") {
                Response::builder()
                    .status(code.parse::<u16>().unwrap_or(302))
                    .header("location", "/method")
                    .body(Full::new(Bytes::new()))
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::new()))
            }
        }
    };
    Ok(response.expect("a valid test response"))
}

/// Starts the test server; it runs until the test's runtime ends.
pub async fn start(tls: Option<Arc<ServerConfig>>) -> SocketAddr {
    let listener = nuclei::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the test listener binds");
    let address = listener.get_ref().local_addr().expect("a local address");
    let acceptor = tls.map(TlsAcceptor::from);
    nuclei::spawn(net::server::serve(
        listener,
        acceptor,
        tower::service_fn(answer),
    ))
    .detach();
    address
}

/// A self-signed certificate for `localhost`, the server configuration
/// that presents it with ALPN `alpn`, and the certificate as a client root.
#[allow(dead_code, reason = "only the client tests use TLS")]
pub fn localhost_tls(alpn: &[&[u8]]) -> (Arc<ServerConfig>, net::http::tls::Certificate) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(certified.key_pair.serialize_der().into());
    let mut config = ServerConfig::builder_with_provider(net::tls::provider())
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_no_client_auth()
        .with_single_cert(vec![certified.cert.der().clone()], key)
        .expect("a server certificate");
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    let root = net::http::tls::Certificate::from_der(certified.cert.der()).expect("a root");
    (Arc::new(config), root)
}
