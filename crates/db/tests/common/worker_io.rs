//! The pool spawned test transactions run on: its workers drive regolith's
//! I/O queues, so their commits take the path a node's do.

use regolith::sync::Lazy;

fn start() -> workers::Pool {
    storage::start_pool(workers::Config::unplaced(4), None).unwrap()
}

static POOL: Lazy<workers::Pool> = Lazy::new(start);

/// The binary's worker pool, started on first use and kept for the process.
pub fn pool() -> &'static workers::Pool {
    &POOL
}
