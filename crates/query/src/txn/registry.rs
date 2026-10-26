//! Transaction registry trait and no-op implementation.

use std::time::Duration;

use async_trait::async_trait;
use defra_core::RequestContext;
use storage::corekv::{MaybeSend, MaybeSendSync};

use crate::error::TransactionError;

use super::handle::TransactionHandle;
use super::turn::TxnJob;

/// Registry for managing active transactions.
///
/// The database layer implements this. A transaction lives where the registry
/// keeps it, and every request for it runs there, one at a time, in the
/// order the requests arrived ([`run`](Self::run)); nothing outside the
/// registry ever holds the transaction. A transaction belongs to the
/// identity of the context that began it, and only a request carrying that
/// identity reaches it again.
///
/// Each call comes twice: `*_as` takes the [`RequestContext`] of the request
/// it serves, and the plain form runs in [`RequestContext::ambient`]. An
/// implementation that tracks ownership overrides both forms; the `*_as`
/// defaults run the plain form inside [`RequestContext::mirror`].
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait TransactionRegistry: MaybeSendSync + 'static {
    /// Begin a new transaction.
    ///
    /// Returns a handle that names it to [`run`](Self::run),
    /// [`commit`](Self::commit) and [`rollback`](Self::discard).
    async fn begin(
        &self,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError>;

    /// [`Self::begin`] for the request `context` serves.
    async fn begin_as(
        &self,
        context: &RequestContext,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        context.mirror(self.begin(readonly)).await
    }

    /// Runs `job` on the transaction `handle` names, where it lives: after
    /// every request for it that arrived earlier and before every one that
    /// arrives later. A job still running after `timeout` is stopped, its
    /// statement undone, and the call fails with an execution error. A
    /// handle the caller does not own is not found.
    async fn run<T>(
        &self,
        handle: &TransactionHandle,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        let _ = (handle, timeout, job);
        Err(TransactionError::not_supported(
            "transactions are not supported in this configuration",
        ))
    }

    /// [`Self::run`] for the request `context` serves.
    async fn run_as<T>(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        context.mirror(self.run(handle, timeout, job)).await
    }

    /// Runs `job` against a read-only snapshot of its own, which no other
    /// request sees and nothing registers: a standalone query's implicit
    /// read. `timeout` stops it as [`run`](Self::run) does.
    async fn read<T>(
        &self,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        let _ = (timeout, job);
        Err(TransactionError::not_supported(
            "transactions are not supported in this configuration",
        ))
    }

    /// [`Self::read`] for the request `context` serves.
    async fn read_as<T>(
        &self,
        context: &RequestContext,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        context.mirror(self.read(timeout, job)).await
    }

    /// Commit a transaction, after every request for it that arrived earlier.
    ///
    /// After commit, the handle names nothing.
    async fn commit(&self, handle: &TransactionHandle)
        -> std::result::Result<(), TransactionError>;

    /// [`Self::commit`] for the request `context` serves.
    async fn commit_as(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        context.mirror(self.commit(handle)).await
    }

    /// Rollback a transaction, after every request for it that arrived
    /// earlier.
    ///
    /// After rollback, the handle names nothing.
    async fn discard(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError>;

    /// [`Self::discard`] for the request `context` serves.
    async fn discard_as(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        context.mirror(self.discard(handle)).await
    }

    /// Let go of a transaction without committing its writes, at once and
    /// without waiting: no later request finds it, and it ends after the
    /// requests already queued for it. Must be idempotent and work without
    /// an async runtime (a guard's destructor calls it).
    fn abandon(&self, handle: &TransactionHandle);
}

/// A no-op transaction registry that doesn't support transactions.
///
/// This is used when transactions aren't needed or available.
#[derive(Debug, Clone, Default)]
pub struct NoOpTransactionRegistry;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl TransactionRegistry for NoOpTransactionRegistry {
    fn abandon(&self, _handle: &TransactionHandle) {}

    async fn begin(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        Err(TransactionError::not_supported(
            "transactions are not supported in this configuration",
        ))
    }

    async fn commit(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported(
            "transactions are not supported in this configuration",
        ))
    }

    async fn discard(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported(
            "transactions are not supported in this configuration",
        ))
    }
}
