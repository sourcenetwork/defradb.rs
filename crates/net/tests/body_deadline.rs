//! A body read past the request's time limit: reading it fails as a timeout,
//! whether the body stalled or had a piece ready, and the limit still holds
//! for every read after that one.

use std::time::Duration;

use futures::{AsyncReadExt, AsyncWriteExt, FutureExt, StreamExt};
use net::http::{Client, Response};
use nuclei::net::TcpListener;
use nuclei::Task;

const LIMIT: Duration = Duration::from_millis(500);

/// A server for one request: it sends `answer`, then holds the connection
/// until the client leaves.
async fn serving(answer: &'static [u8]) -> (String, Task<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.get_ref().local_addr().expect("an address");
    let server = nuclei::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("a connection");
        let (mut head, mut byte) = (Vec::new(), [0]);
        while !head.ends_with(b"\r\n\r\n") {
            socket
                .read_exact(&mut byte)
                .await
                .expect("the request head");
            head.push(byte[0]);
        }
        socket.write_all(answer).await.expect("the answer");
        let _ = socket.read(&mut byte).await;
    });
    (format!("http://{address}/"), server)
}

/// The head of a 1024-byte body and none of its bytes.
async fn stalling() -> (String, Task<()>) {
    serving(b"HTTP/1.1 200 OK\r\ncontent-length: 1024\r\n\r\n").await
}

async fn stalled_response(url: String) -> Response {
    Client::builder(edge::testing::edge())
        .no_proxy()
        .build()
        .expect("a client")
        .get(url)
        .timeout(LIMIT)
        .send()
        .await
        .expect("the head before the limit")
}

#[nuclei::test]
async fn a_stalled_body_stream_gives_one_timeout_then_ends() {
    let (url, _server) = stalling().await;
    let mut stream = stalled_response(url).await.bytes_stream();

    let first = stream.next().await;

    assert!(
        matches!(&first, Some(Err(error)) if error.is_timeout()),
        "{first:?}"
    );
    for _ in 0..2 {
        let next = stream.next().now_or_never();
        assert!(matches!(next, Some(None)), "{next:?}");
    }
}

#[nuclei::test]
async fn every_chunk_read_past_the_limit_times_out_at_once() {
    let (url, _server) = stalling().await;
    let mut response = stalled_response(url).await;

    let first = response.chunk().await;
    let second = response.chunk().now_or_never();

    assert!(first.is_err_and(|error| error.is_timeout()));
    assert!(
        second.is_some_and(|read| read.is_err_and(|error| error.is_timeout())),
        "a read past the limit waits for nothing"
    );
}

#[nuclei::test]
async fn a_piece_ready_past_the_limit_gives_the_timeout() {
    let (url, _server) = serving(b"HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\nbody").await;
    let mut stream = stalled_response(url).await.bytes_stream();
    nuclei::time::sleep(LIMIT * 2).await;

    let first = stream.next().await;

    assert!(
        matches!(&first, Some(Err(error)) if error.is_timeout()),
        "{first:?}"
    );
    assert!(matches!(stream.next().now_or_never(), Some(None)));
}
