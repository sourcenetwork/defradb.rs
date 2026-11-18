//! A test server on a nuclei listener, plain or over TLS.

use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::header::AUTHORIZATION;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use net::tls::{ServerConfig, TlsAcceptor};

/// How long a test waits for the server to start on a request.
const START_LIMIT: Duration = Duration::from_secs(10);

/// What the test server answers, by path:
/// - `/echo`: the request body, with its method in `x-method`;
/// - `/query`: the request's query string;
/// - `/status/<code>`: that status;
/// - `/slow`: an answer after two seconds;
/// - `/redirect/<code>`: a redirect with that status to the URL after `to=`
///   in the query, or to `/method`;
/// - `/method`: the request's method;
/// - `/authorization`: the request's `Authorization` header, or nothing.
async fn answer(
    request: Request<Incoming>,
    slow_started: Arc<AtomicUsize>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = request.uri().path().to_owned();
    let method = request.method().to_string();
    let query = request.uri().query().unwrap_or("").to_owned();
    let authorization = request
        .headers()
        .get(AUTHORIZATION)
        .map(|value| Bytes::copy_from_slice(value.as_bytes()))
        .unwrap_or_default();
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
            slow_started.fetch_add(1, Ordering::SeqCst);
            nuclei::time::sleep(Duration::from_secs(2)).await;
            Response::builder().body(Full::new(Bytes::from_static(b"late")))
        }
        "/method" => Response::builder().body(Full::new(Bytes::from(method))),
        "/authorization" => Response::builder().body(Full::new(authorization)),
        other => {
            if let Some(code) = other.strip_prefix("/status/") {
                Response::builder()
                    .status(code.parse::<u16>().unwrap_or(500))
                    .body(Full::new(Bytes::new()))
            } else if let Some(code) = other.strip_prefix("/redirect/") {
                Response::builder()
                    .status(code.parse::<u16>().unwrap_or(302))
                    .header("location", query.strip_prefix("to=").unwrap_or("/method"))
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

/// Starts the test server on the shared test runtime, where it runs until
/// the process ends.
#[allow(dead_code, reason = "the server tests start their own")]
pub async fn start(tls: Option<Arc<ServerConfig>>) -> SocketAddr {
    let served = serve_on(tls, edge::testing::edge()).await;
    drop(served.task);
    served.address
}

/// A test server started by [`serve_on`].
pub struct Served {
    pub address: SocketAddr,
    /// Serves until its edge stops accepting and its connections end.
    pub task: edge::Task<io::Result<()>>,
    slow_started: Arc<AtomicUsize>,
}

impl Served {
    /// Completes once the server started answering a `/slow` request.
    #[allow(dead_code, reason = "only the server tests wait on it")]
    pub async fn slow_request_started(&self) {
        nuclei::time::timeout(START_LIMIT, async {
            let mut wait = Duration::from_millis(1);
            while self.slow_started.load(Ordering::SeqCst) == 0 {
                nuclei::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_millis(50));
            }
        })
        .await
        .expect("the slow request reaches the server");
    }
}

/// Starts the test server as a task of `edge`, whose connections run there
/// too.
pub async fn serve_on(tls: Option<Arc<ServerConfig>>, edge: edge::Edge) -> Served {
    let listener = nuclei::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the test listener binds");
    let address = listener.get_ref().local_addr().expect("a local address");
    let acceptor = tls.map(TlsAcceptor::from);
    let slow_started = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&slow_started);
    let service = tower::service_fn(move |request| answer(request, Arc::clone(&counter)));
    let task = edge
        .spawn(net::server::serve(
            listener,
            acceptor,
            service,
            edge.clone(),
        ))
        .within(Duration::MAX);
    Served {
        address,
        task,
        slow_started,
    }
}

/// A self-signed certificate for `localhost`, the server configuration
/// that presents it with ALPN `alpn`, and the certificate as a client root.
#[allow(dead_code, reason = "only the client tests use TLS")]
pub fn localhost_tls(alpn: &[&[u8]]) -> (Arc<ServerConfig>, net::http::tls::Certificate) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
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
