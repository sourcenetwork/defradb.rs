//! The node's side of a node in its own process. It starts a Rust node on
//! the directory it is given, over HTTP, prints its URL, and then takes one
//! command per line on stdin:
//!
//! - `arm <point>`: arm a [`CrashPoint`], answered `armed`;
//! - `stop`, or the end of stdin: stop the node in order and exit.

use std::io::BufRead;
use std::sync::Arc;

use defra_node::{HttpConfig, Node};

use super::{CrashEnv, CrashPoint};

/// The variable that names the node's data directory and makes
/// [`HOST_TEST`](super::HOST_TEST) a node's process.
pub const DIR_VAR: &str = "DEFRA_CRASH_HOST_DIR";

/// What the host writes for the test follows this on its line, after
/// anything the test harness prints ahead of it.
pub const PREFIX: &str = "crash-host: ";

/// Run a node until told to stop, when [`DIR_VAR`] names its directory;
/// otherwise do nothing.
pub fn serve() {
    let Some(dir) = std::env::var_os(DIR_VAR) else {
        return;
    };
    let env = Arc::new(CrashEnv::default());
    let mut options = storage::RegolithStoreOptions::default();
    options.engine = options.engine.env(env.clone());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a port");
    let url = format!("http://{}", listener.local_addr().expect("local address"));
    let http = HttpConfig::with_listener(listener)
        .expect("http config")
        .with_dev_mode(true)
        .with_collection_management(true);
    let node = workers::block_on(
        Node::builder()
            .data_path(dir)
            .with_regolith_options(options)
            .with_http(http)
            .build(),
    )
    .expect("run the builder")
    .expect("start the node");
    println!("{PREFIX}url {url}");
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("read a command");
        match line.split_once(' ') {
            Some(("arm", name)) => {
                let point = CrashPoint::parse(name)
                    .unwrap_or_else(|| panic!("unknown crash point {name:?}"));
                env.arm(point);
                println!("{PREFIX}armed");
            }
            _ if line == "stop" => break,
            _ => panic!("unknown command {line:?}"),
        }
    }
    workers::block_on(node.shutdown())
        .expect("run the shutdown")
        .expect("stop the node");
    println!("{PREFIX}stopped");
}
