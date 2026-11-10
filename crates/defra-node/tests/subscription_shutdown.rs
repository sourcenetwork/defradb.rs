//! A GraphQL subscription open over SSE does not hold `Node::shutdown` for
//! the request drain's deadline: its stream ends, with its `complete` event,
//! once the node stops admitting requests (#2779). Modeled by
//! proofs/tla/ServerDrain.tla (INV_DeadlineCutsOnlyStuck).
#![cfg(feature = "http")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use defra_node::{HttpConfig, Node};

/// Well under the request drain's 30-second deadline.
const PROMPT: Duration = Duration::from_secs(10);

const SUBSCRIPTION: &str = r#"{"query":"subscription { User { _docID name } }"}"#;

/// Reads `socket` until the end of the response head and returns the head.
fn read_head(socket: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        assert_eq!(
            socket.read(&mut byte).unwrap(),
            1,
            "the response head ended early"
        );
        head.push(byte[0]);
    }
    String::from_utf8(head).unwrap()
}

#[test]
fn shutdown_ends_an_open_graphql_subscription_without_waiting_out_the_drain() {
    let runtime = edge::testing::edge();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let node: Node = runtime
        .block_on(async {
            let node = Node::builder()
                .with_http(HttpConfig::with_listener(listener).unwrap())
                .build()
                .await
                .unwrap();
            node.add_collection("type User { name: String }")
                .await
                .unwrap();
            node
        })
        .unwrap();

    let mut socket = TcpStream::connect(address).unwrap();
    socket.set_read_timeout(Some(PROMPT * 6)).unwrap();
    write!(
        socket,
        "POST /api/v0/graphql HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Accept: text/event-stream\r\nContent-Length: {}\r\n\r\n{SUBSCRIPTION}",
        SUBSCRIPTION.len()
    )
    .unwrap();
    let head = read_head(&mut socket);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("text/event-stream"), "{head}");

    let client = std::thread::spawn(move || {
        let mut body = Vec::new();
        socket.read_to_end(&mut body).unwrap();
        String::from_utf8(body).unwrap()
    });

    let started = Instant::now();
    runtime.block_on(node.shutdown()).unwrap().unwrap();
    let took = started.elapsed();
    assert!(
        took < PROMPT,
        "shutdown waited {took:?} for an open subscription"
    );

    let body = client.join().unwrap();
    assert!(body.contains("event: complete"), "{body:?}");
    assert!(
        body.ends_with("0\r\n\r\n"),
        "the stream must end with its last chunk, not a cut connection: {body:?}"
    );
}
