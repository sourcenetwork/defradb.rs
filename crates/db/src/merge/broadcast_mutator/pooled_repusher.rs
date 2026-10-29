//! The SE re-push, its storage reads on the database's worker pool: the retry
//! sweep calls it from the network edge, which reads no storage.

use std::sync::Arc;

use super::SeArtifactRepusher;

/// `inner` as the retry sweep and the merge path hold it: on `pool` when the
/// database has one, as it is otherwise.
pub fn pooled_se_repusher(
    inner: Arc<dyn SeArtifactRepusher>,
    pool: Option<&workers::Pool>,
) -> Arc<dyn SeArtifactRepusher> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(pool) = pool {
        return Arc::new(PooledSeRepusher {
            inner,
            pool: pool.clone(),
        });
    }
    // A browser's one worker is the page's thread: a call runs where it is
    // made.
    #[cfg(target_arch = "wasm32")]
    let _ = pool;
    inner
}

/// `inner`'s re-push, run on a pool's workers.
#[cfg(not(target_arch = "wasm32"))]
struct PooledSeRepusher {
    inner: Arc<dyn SeArtifactRepusher>,
    pool: workers::Pool,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl SeArtifactRepusher for PooledSeRepusher {
    async fn regenerate_and_push_se_artifacts(&self, collection_id: &str, doc_id: &str) {
        let inner = Arc::clone(&self.inner);
        let (collection_id, doc_id) = (collection_id.to_owned(), doc_id.to_owned());
        let repushed = workers::run_on(
            Some(&self.pool),
            workers::Target::LeastLoaded,
            move || async move {
                inner
                    .regenerate_and_push_se_artifacts(&collection_id, &doc_id)
                    .await;
            },
        )
        .await;
        if let Err(error) = repushed {
            tracing::warn!(error = %error.report("SE artifact re-push"), "SE retry: the re-push did not run");
        }
    }
}
