//! Where an import's writes and store jobs run: the request path of the edge
//! the import came in by, handed in by that edge.

use std::sync::Arc;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSendSync};
use query::{QueryExecutor, QueryRequest, QueryResponse};

/// A store job an import sends to a database worker: an access check, an
/// alias check or an alias registration, built where it is sent.
#[cfg(not(target_arch = "wasm32"))]
pub type DbJob = Box<dyn FnOnce() -> MaybeBoxFuture<'static, Result<(), String>> + Send>;

/// A store job an import runs where it is: wasm has one thread.
#[cfg(target_arch = "wasm32")]
pub type DbJob = Box<dyn FnOnce() -> MaybeBoxFuture<'static, Result<(), String>>>;

/// The request path of the edge an import came in by.
///
/// Every document an import creates, and every relation it patches, is a
/// request sent through [`run`](Self::run), so it is admitted, signed, owned
/// and broadcast exactly as a create sent through the same edge would be
/// (`proofs/tla/BackupImportWritePath.tla`). The import's driver runs on the
/// edge's own task; only [`run`](Self::run) and [`on_db`](Self::on_db)
/// place work on the database's workers.
pub trait ImportHost: MaybeSendSync {
    /// Runs `request` as this edge runs a write it receives, as the caller
    /// the import came from.
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse>;

    /// Runs `job` on a database worker of this edge's node.
    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>>;
}

/// The host of an import with no edge of its own
/// ([`import_database`](super::import_database)): writes go through
/// `runner`, store jobs run where the import runs.
pub struct RunnerHost {
    runner: Arc<dyn QueryExecutor>,
}

impl RunnerHost {
    /// A host that sends every write to `runner`.
    pub fn new(runner: Arc<dyn QueryExecutor>) -> Self {
        Self { runner }
    }
}

impl ImportHost for RunnerHost {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        self.runner.execute(request)
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        job()
    }
}
