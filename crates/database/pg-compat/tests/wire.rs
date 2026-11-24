//! The Postgres wire loop on a nuclei socket, driven with raw protocol
//! bytes against pgwire's no-op handlers: an empty query answers without
//! a handler, any other query fails with a fatal error. A connection whose
//! edge stops accepting finishes the message in progress, an idle one
//! closes at once.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use edge::{Edge, Task};
use futures::Sink;
use nuclei::io::{AsyncReadExt, AsyncWriteExt};
use nuclei::net::{TcpListener, TcpStream};
use nuclei::time::{timeout, Instant};
use pgwire::api::query::SimpleQueryHandler;
use pgwire::api::results::{Response, Tag};
use pgwire::api::{ClientInfo, ClientPortalStore, PgWireServerHandlers};
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::PgWireBackendMessage;
use query::{
    QueryExecutor, QueryRequest, QueryResponse, StaticCollectionProvider, TransactionError,
    TransactionHandle,
};
use regolith::sync::Event;

const LIMIT: Duration = Duration::from_secs(10);

struct Defaults;

impl PgWireServerHandlers for Defaults {}

/// Handlers whose query waits until `release`, saying on `started` that it
/// began, then completes.
#[derive(Clone)]
struct Gated {
    started: Arc<Event>,
    release: Arc<Event>,
}

impl Gated {
    fn new() -> Self {
        Self {
            started: Arc::new(Event::new()),
            release: Arc::new(Event::new()),
        }
    }
}

#[async_trait]
impl SimpleQueryHandler for Gated {
    async fn do_query<C>(&self, _client: &mut C, _query: &str) -> PgWireResult<Vec<Response>>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        self.started.set();
        self.release.wait().await;
        Ok(vec![Response::Execution(Tag::new("SELECT"))])
    }
}

impl PgWireServerHandlers for Gated {
    fn simple_query_handler(&self) -> Arc<impl SimpleQueryHandler> {
        Arc::new(self.clone())
    }
}

/// A client connected to a listener, and the server's task on `edge` for
/// that one connection.
async fn connection<H>(edge: &Edge, handlers: H) -> (TcpStream, Task<std::io::Result<()>>)
where
    H: PgWireServerHandlers + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.get_ref().local_addr().unwrap();
    let client = TcpStream::connect(address).await.unwrap();
    let (socket, peer) = listener.accept().await.unwrap();
    let server = edge
        .spawn(pg_compat::wire::serve(socket, peer, handlers, edge.clone()))
        .within(Duration::MAX);
    (client, server)
}

/// Completes the startup of `client`.
async fn start_up(client: &mut TcpStream) {
    send(client, &startup()).await;
    until_ready(client).await;
}

fn message(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes = vec![kind];
    bytes.extend_from_slice(&(body.len() as u32 + 4).to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn startup() -> Vec<u8> {
    let mut body = 196_608u32.to_be_bytes().to_vec();
    body.extend_from_slice(b"user\0test\0database\0test\0\0");
    let mut bytes = (body.len() as u32 + 4).to_be_bytes().to_vec();
    bytes.extend_from_slice(&body);
    bytes
}

/// The types of the backend messages up to and including ReadyForQuery,
/// and the body of the first.
async fn until_ready(client: &mut TcpStream) -> (Vec<u8>, Vec<u8>) {
    let (mut kinds, mut first) = (Vec::new(), None);
    loop {
        let mut header = [0u8; 5];
        client.read_exact(&mut header).await.unwrap();
        let length = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
        let mut body = vec![0u8; length - 4];
        client.read_exact(&mut body).await.unwrap();
        first.get_or_insert(body);
        kinds.push(header[0]);
        if header[0] == b'Z' {
            return (kinds, first.unwrap_or_default());
        }
    }
}

async fn send(client: &mut TcpStream, bytes: &[u8]) {
    client.write_all(bytes).await.unwrap();
    client.flush().await.unwrap();
}

#[test]
fn ssl_is_refused_then_queries_run_and_a_fatal_error_closes_the_connection() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let (mut client, server) = connection(&edge, Defaults).await;

        send(&mut client, &[0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f]).await;
        let mut refused = [0u8; 1];
        client.read_exact(&mut refused).await.unwrap();
        assert_eq!(&refused, b"N");

        send(&mut client, &startup()).await;
        let (kinds, authentication) = until_ready(&mut client).await;
        assert_eq!(kinds.first(), Some(&b'R'));
        assert_eq!(authentication, 0u32.to_be_bytes());

        send(&mut client, &message(b'Q', b";\0")).await;
        assert_eq!(until_ready(&mut client).await.0, b"IZ");

        send(&mut client, &message(b'Q', b"SELECT 1\0")).await;
        assert_eq!(until_ready(&mut client).await.0, b"EZ");

        let mut rest = Vec::new();
        client.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
        let served = timeout(Duration::from_secs(5), server).await;
        assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
    })
    .unwrap();
}

#[test]
fn a_client_that_starts_tls_directly_is_closed() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let (mut client, server) = connection(&edge, Defaults).await;

        send(&mut client, &[0x16, 0x03, 0x01, 0x00, 0x05]).await;

        let mut rest = Vec::new();
        client.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
        let served = timeout(Duration::from_secs(5), server).await;
        assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
    })
    .unwrap();
}

#[test]
fn a_client_that_sends_nothing_is_closed_at_the_startup_deadline() {
    edge::testing::paused(|edge| async move {
        let (mut client, server) = connection(&edge, Defaults).await;
        let connected = Instant::now();

        let served = timeout(Duration::from_secs(61), server).await;
        assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
        assert!(connected.elapsed() >= Duration::from_secs(60));
        let mut rest = Vec::new();
        client.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
    });
}

#[test]
fn a_query_in_flight_when_the_edge_starts_closing_completes_and_the_connection_closes() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let scope = edge.scope();
        let handlers = Gated::new();
        let (started, release) = (handlers.started.clone(), handlers.release.clone());
        let (mut client, server) = connection(scope.edge(), handlers).await;
        start_up(&mut client).await;
        send(&mut client, &message(b'Q', b"SELECT 1\0")).await;
        timeout(LIMIT, started.wait())
            .await
            .expect("the query reaches the handler");

        scope.close();
        release.set();

        assert_eq!(until_ready(&mut client).await.0, b"CZ");
        let mut rest = Vec::new();
        client.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
        let served = timeout(LIMIT, server).await;
        assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
    })
    .unwrap();
}

#[test]
fn an_idle_connection_closes_at_once_when_the_edge_starts_closing() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let scope = edge.scope();
        let (mut client, server) = connection(scope.edge(), Defaults).await;
        start_up(&mut client).await;

        scope.close();

        let mut rest = Vec::new();
        timeout(LIMIT, client.read_to_end(&mut rest))
            .await
            .expect("the connection closes")
            .unwrap();
        assert!(rest.is_empty());
        let served = timeout(LIMIT, server).await;
        assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
    })
    .unwrap();
}

/// The real server's query executor holds a request until the test releases it.
struct GatedExecutor {
    started: Arc<Event>,
    release: Arc<Event>,
}

#[async_trait]
impl QueryExecutor for GatedExecutor {
    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        self.started.set();
        self.release.wait().await;
        QueryResponse::success(serde_json::json!({"Users": []}))
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(&self, _readonly: bool) -> Result<TransactionHandle, TransactionError> {
        unreachable!("the test uses an implicit transaction")
    }

    async fn commit_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        unreachable!("the test uses an implicit transaction")
    }

    async fn discard_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        unreachable!("the test uses an implicit transaction")
    }

    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn schema(&self) -> query::Result<String> {
        Ok(String::new())
    }
}

#[test]
fn the_real_pg_server_drains_an_in_flight_query() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        // Reserve an ephemeral address for the server, whose listener is
        // created inside PgServer::run rather than handed to the caller.
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);

        let scope = edge.scope();
        let (started, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
        let executor = Arc::new(GatedExecutor {
            started: started.clone(),
            release: release.clone(),
        });
        let server = pg_compat::PgServer::new(
            address,
            executor,
            Arc::new(StaticCollectionProvider::new(vec![
                schema::CollectionVersion::new(
                    "Users",
                    "version",
                    "collection",
                    vec![schema::FieldDescription::new(
                        "field",
                        "name",
                        schema::FieldKind::Scalar(schema::ScalarKind::String),
                        schema::CType::LwwRegister,
                    )],
                ),
            ])),
            None,
            scope.edge().clone(),
        );
        let finished = Arc::new(AtomicBool::new(false));
        let finished_server = finished.clone();
        let server_task = scope.edge().spawn(async move {
            let result = server.run().await.map_err(|error| error.to_string());
            finished_server.store(true, Ordering::SeqCst);
            result
        });

        let mut client = timeout(LIMIT, async {
            loop {
                if let Ok(client) = TcpStream::connect(address).await {
                    break client;
                }
                edge.sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the server starts listening");
        // The real server treats a non-anonymous username as a DID and
        // requests a JWT; use its anonymous startup path here.
        let mut startup_body = 196_608u32.to_be_bytes().to_vec();
        startup_body.extend_from_slice(b"user\0anonymous\0database\0test\0\0");
        let mut startup = (startup_body.len() as u32 + 4).to_be_bytes().to_vec();
        startup.extend_from_slice(&startup_body);
        send(&mut client, &startup).await;
        timeout(LIMIT, until_ready(&mut client))
            .await
            .expect("the server completes startup");
        send(&mut client, &message(b'Q', b"SELECT name FROM Users\0")).await;
        timeout(LIMIT, started.wait())
            .await
            .expect("the query reaches the executor");

        scope.close();
        edge.sleep(Duration::from_millis(50)).await;
        assert!(
            !finished.load(Ordering::SeqCst),
            "the server returned while a query was in flight"
        );
        release.set();

        assert_eq!(
            timeout(LIMIT, until_ready(&mut client)).await.unwrap().0,
            b"TCZ"
        );
        let mut rest = Vec::new();
        timeout(LIMIT, client.read_to_end(&mut rest))
            .await
            .unwrap()
            .unwrap();
        assert!(rest.is_empty());
        assert!(scope.drain(LIMIT).await.is_complete());
        assert!(matches!(timeout(LIMIT, server_task).await, Ok(Ok(Ok(())))));
    })
    .unwrap();
}
