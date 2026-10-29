//! A connect to a name with an IPv6 and an IPv4 address, as hyper's
//! connector makes one: the first address's family alone for 300 ms, then
//! the other family's beside it, and the other family at once when the
//! first fails.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use futures::{AsyncReadExt, AsyncWriteExt};
use http::Uri;
#[cfg(target_os = "linux")]
use nuclei::net::TcpStream;
use nuclei::net::{TcpListener, TcpSocket};
use nuclei::time::timeout;
#[cfg(target_os = "linux")]
use nuclei::time::Instant;

#[cfg(target_os = "linux")]
const FALLBACK_DELAY: Duration = Duration::from_millis(300);
const LIMIT: Duration = Duration::from_secs(10);

/// `localhost`'s first address, as a connect tries them, and the first of
/// the other family; `None`, said on stderr, where `localhost` names one
/// family only or the other loopback address cannot be bound.
async fn both_families() -> Option<(IpAddr, IpAddr)> {
    let names: Vec<SocketAddr> = nuclei::net::lookup_host(("localhost", 0))
        .await
        .expect("localhost resolves")
        .collect();
    let first = *names.first().expect("localhost has an address");
    let both = names
        .iter()
        .find(|name| name.is_ipv6() != first.is_ipv6())
        .filter(|other| bound(**other).is_ok() && bound(first).is_ok())
        .map(|other| (first.ip(), other.ip()));
    if both.is_none() {
        eprintln!("skipping: localhost does not name a usable IPv6 and IPv4 loopback address");
    }
    both
}

fn bound(address: SocketAddr) -> io::Result<TcpSocket> {
    let socket = if address.is_ipv6() {
        TcpSocket::new_v6()
    } else {
        TcpSocket::new_v4()
    }?;
    socket.bind(address)?;
    Ok(socket)
}

/// Sockets bound to one port on `first` and on `other`, not listening yet.
fn one_port(first: IpAddr, other: IpAddr) -> (TcpSocket, TcpSocket, u16) {
    for _ in 0..16 {
        let on_other = bound(SocketAddr::new(other, 0)).expect("a free port");
        let port = on_other.local_addr().expect("a bound address").port();
        if let Ok(on_first) = bound(SocketAddr::new(first, port)) {
            return (on_first, on_other, port);
        }
    }
    panic!("no port was free on both loopback addresses");
}

fn localhost(port: u16) -> Uri {
    format!("http://localhost:{port}/")
        .parse()
        .expect("a valid URL")
}

/// Whether `listener` takes a connection that sends `byte`.
async fn takes(listener: &TcpListener, byte: u8) -> bool {
    let (mut accepted, _) = timeout(LIMIT, listener.accept())
        .await
        .expect("a connection before the limit")
        .expect("a connection");
    let mut got = [0];
    accepted.read_exact(&mut got).await.expect("a byte");
    got[0] == byte
}

// A listener with a full queue drops a new SYN only on Linux; macOS takes
// the connection, so the first family is never silent there.
#[cfg(target_os = "linux")]
#[nuclei::test]
async fn the_other_family_connects_when_the_first_is_silent_past_the_delay() {
    let Some((first, other)) = both_families().await else {
        return;
    };
    let (on_first, on_other, port) = one_port(first, other);
    // A listener whose queue is full drops a new connection's SYN, so a
    // connect to it waits as one to a dropping route does.
    let silent = on_first.listen(0).expect("listens");
    let _queued = TcpStream::connect(SocketAddr::new(first, port))
        .await
        .expect("the queue's one place");
    let answering = on_other.listen(16).expect("listens");
    let started = Instant::now();

    let mut stream = timeout(LIMIT, net::connect(&localhost(port)))
        .await
        .expect("a connection before the limit")
        .expect("a connection");
    let waited = started.elapsed();
    stream.write_all(b"!").await.expect("a byte sent");

    assert!(takes(&answering, b'!').await);
    assert!(
        waited >= FALLBACK_DELAY,
        "the other family started after {waited:?}"
    );
    drop(silent);
}

#[nuclei::test]
async fn the_other_family_is_tried_when_the_first_refuses() {
    let Some((first, other)) = both_families().await else {
        return;
    };
    let (_refusing, on_other, port) = one_port(first, other);
    let answering = on_other.listen(16).expect("listens");

    let mut stream = timeout(LIMIT, net::connect(&localhost(port)))
        .await
        .expect("a connection before the limit")
        .expect("a connection");
    stream.write_all(b"!").await.expect("a byte sent");

    assert!(takes(&answering, b'!').await);
}

#[nuclei::test]
async fn when_both_families_refuse_the_last_refusal_comes_back() {
    let Some((first, other)) = both_families().await else {
        return;
    };
    // Closed, not just bound: a bound socket that does not listen refuses
    // only on Linux.
    let (_, _, port) = one_port(first, other);

    let connected = timeout(LIMIT, net::connect(&localhost(port)))
        .await
        .expect("an answer before the limit");

    let Err(error) = connected else {
        panic!("nothing listens on either address");
    };
    let error = error.downcast::<io::Error>().expect("an I/O error");
    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
}
