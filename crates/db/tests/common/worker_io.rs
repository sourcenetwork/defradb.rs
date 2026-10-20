//! The pool spawned test transactions run on: its workers drive regolith's
//! I/O queues, so their commits take the path a node's do.

use regolith::sync::OnceCell;

static POOL: OnceCell<workers::Pool> = OnceCell::new();

/// The binary's worker pool, started on first use and kept for the process.
pub fn pool() -> &'static workers::Pool {
    POOL.get_or_init(|| storage::start_pool(workers::Config::unplaced(4), None).unwrap())
}
