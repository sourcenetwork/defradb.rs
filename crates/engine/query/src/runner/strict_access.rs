//! A failed document access check, as a strict read reports it.
//!
//! A request with `RequestContext::with_strict_access_reads` fails every
//! access check that had no answer with
//! [`QueryError::AccessCheckUnavailable`], naming nothing about the document,
//! wherever the check ran: the permission filter, the authorization of a
//! mutation, a `cid` read, commit history, or the edge answering a prepared
//! input. Other requests keep each path's own error
//! (`proofs/tla/StrictAccessRead.tla`).

use super::{DocFetcher, QueryRunner};
use crate::error::QueryError;
use crate::txn::TransactionRegistry;

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    /// `error`, the failure of an access check, as this request reports it:
    /// a missing prepared input, or a failure already reported so, stays as
    /// it is; a strict request's is
    /// [`QueryError::AccessCheckUnavailable`], retryable when the provider was
    /// unavailable; any other is `otherwise(error)`.
    pub(super) fn access_check_failure(
        &self,
        error: QueryError,
        otherwise: impl FnOnce(QueryError) -> QueryError,
    ) -> QueryError {
        match error {
            error @ (QueryError::NeedsInput(_) | QueryError::AccessCheckUnavailable { .. }) => {
                error
            }
            error if self.context().strict_access_reads() => {
                tracing::warn!(%error, "Access check failed, failing the strict request");
                QueryError::AccessCheckUnavailable {
                    retryable: matches!(error, QueryError::AcpProviderUnavailable(_)),
                }
            }
            error => otherwise(error),
        }
    }
}
