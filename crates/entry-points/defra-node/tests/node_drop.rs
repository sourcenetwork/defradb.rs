//! A node dropped without `shutdown` stops its own background work: its
//! worker pool's threads exit and its files close. Modeled by
//! proofs/tla/NodeDrop.tla.
//!
//! One test in its own binary, because it counts the process's threads and
//! descriptors.
#![cfg(feature = "http")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use defra_node::{HttpConfig, Node};

/// The process's threads now.
fn threads() -> usize {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc/self/task").unwrap().count()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let listing = std::process::Command::new("ps")
            .args(["-M", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        String::from_utf8(listing.stdout).unwrap().lines().count() - 1
    }
}

/// The process's open descriptors now.
fn descriptors() -> usize {
    std::fs::read_dir("/dev/fd").unwrap().count()
}

/// An HTTP-serving node on an ephemeral port, its transaction sweep running.
async fn http_node() -> Node {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    Node::builder()
        .with_http(HttpConfig::with_listener(listener).unwrap())
        .build()
        .await
        .unwrap()
}

#[test]
fn a_dropped_http_node_releases_its_threads_and_descriptors() {
    let runtime = edge::testing::edge();
    // Process-wide state (the network runtime, lazy registries) settles with
    // a first node that is shut down.
    runtime
        .block_on(async { http_node().await.shutdown().await.unwrap() })
        .unwrap();
    let (base_threads, base_fds) = (threads(), descriptors());

    let database = runtime
        .block_on(async {
            let node = http_node().await;
            let database = Arc::downgrade(node.database());
            drop(node);
            database
        })
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while (database.strong_count() > 0 || threads() > base_threads || descriptors() > base_fds)
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    let (after_threads, after_fds) = (threads(), descriptors());
    eprintln!("threads {base_threads} -> {after_threads}, fds {base_fds} -> {after_fds}");
    assert_eq!(
        database.strong_count(),
        0,
        "the dropped node's background work still holds its database"
    );
    assert!(
        after_threads <= base_threads,
        "the dropped node left {} threads running",
        after_threads - base_threads
    );
    assert!(
        after_fds <= base_fds,
        "the dropped node left {} descriptors open",
        after_fds - base_fds
    );
}
