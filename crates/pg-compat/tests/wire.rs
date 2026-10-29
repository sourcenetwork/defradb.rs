//! The Postgres wire loop on a nuclei socket, driven with raw protocol
//! bytes against pgwire's no-op handlers: an empty query answers without
//! a handler, any other query fails with a fatal error.

use std::time::Duration;

use nuclei::io::{AsyncReadExt, AsyncWriteExt};
use nuclei::net::{TcpListener, TcpStream};
use nuclei::time::Instant;
use pgwire::api::PgWireServerHandlers;

struct Defaults;

impl PgWireServerHandlers for Defaults {}

/// A client connected to a listener, and the server's task for that one
/// connection.
async fn connection() -> (TcpStream, nuclei::Task<std::io::Result<()>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.get_ref().local_addr().unwrap();
    let client = TcpStream::connect(address).await.unwrap();
    let (socket, peer) = listener.accept().await.unwrap();
    let server = nuclei::spawn(pg_compat::wire::serve(socket, peer, Defaults));
    (client, server)
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

#[nuclei::test]
async fn ssl_is_refused_then_queries_run_and_a_fatal_error_closes_the_connection() {
    let (mut client, server) = connection().await;

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
    let served = nuclei::time::timeout(Duration::from_secs(5), server.join()).await;
    assert!(matches!(served, Ok(Ok(Ok(())))));
}

#[nuclei::test]
async fn a_client_that_starts_tls_directly_is_closed() {
    let (mut client, server) = connection().await;

    send(&mut client, &[0x16, 0x03, 0x01, 0x00, 0x05]).await;

    let mut rest = Vec::new();
    client.read_to_end(&mut rest).await.unwrap();
    assert!(rest.is_empty());
    let served = nuclei::time::timeout(Duration::from_secs(5), server.join()).await;
    assert!(matches!(served, Ok(Ok(Ok(())))));
}

#[nuclei::test(start_paused = true)]
async fn a_client_that_sends_nothing_is_closed_at_the_startup_deadline() {
    let (mut client, server) = connection().await;
    let connected = Instant::now();

    let served = nuclei::time::timeout(Duration::from_secs(61), server.join()).await;
    assert!(matches!(served, Ok(Ok(Ok(())))));
    assert!(connected.elapsed() >= Duration::from_secs(60));
    let mut rest = Vec::new();
    client.read_to_end(&mut rest).await.unwrap();
    assert!(rest.is_empty());
}
