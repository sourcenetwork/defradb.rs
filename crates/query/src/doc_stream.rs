//! Pull-based document source for query execution.

use crate::error::Result;
use async_trait::async_trait;
use document::Document;
use storage::corekv::MaybeSend;

/// A pull-based source of documents paired with their deletion status.
///
/// Returned by [`crate::fetcher::DocFetcher::stream_all_with_deleted`]. Each
/// call yields at most one document, so a consumer that stops pulling stops
/// the underlying storage work.
///
/// A stream is owned by the one task that pulls it, through `&mut self`, so
/// it may move between threads but is never shared: no `Sync` bound, and an
/// implementation holds its iterator and read transaction as plain fields.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DocStream: MaybeSend {
    /// Advance the stream. `Ok(None)` means exhausted; further calls keep
    /// returning `Ok(None)`.
    async fn next(&mut self) -> Result<Option<(Document, bool)>>;

    /// Number of index entries consumed by this stream, before filtering.
    fn index_fetches(&self) -> u64 {
        0
    }

    /// Finish with the stream, whether or not it was exhausted.
    ///
    /// A consumer that stops pulling early (a satisfied `LimitNode` above a
    /// `ScanNode`) must call this before dropping the stream: `Drop` cannot
    /// await, so this is the only point at which a stream can flush work it
    /// deferred while yielding documents. The default is a no-op, which is
    /// correct for streams that defer nothing.
    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A [`DocStream`] over an already-materialized vector.
///
/// This is the default fallback for fetchers that have no streaming
/// implementation: correct, but it does not avoid the eager fetch.
pub struct VecStream {
    pairs: std::vec::IntoIter<(Document, bool)>,
}

impl VecStream {
    /// Wrap pre-fetched documents as a stream.
    pub fn new(pairs: Vec<(Document, bool)>) -> Self {
        Self {
            pairs: pairs.into_iter(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocStream for VecStream {
    async fn next(&mut self) -> Result<Option<(Document, bool)>> {
        Ok(self.pairs.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use document::Document;

    #[test]
    fn vec_stream_yields_all_pairs_in_order_then_none() {
        workers::block_on(async {
            let pairs = vec![
                (crate::test_utils::test_doc(&[]), false),
                (crate::test_utils::test_doc(&[]), true),
                (crate::test_utils::test_doc(&[]), false),
            ];
            let mut stream = VecStream::new(pairs);

            assert_eq!(stream.next().await.unwrap().map(|(_, d)| d), Some(false));
            assert_eq!(stream.next().await.unwrap().map(|(_, d)| d), Some(true));
            assert_eq!(stream.next().await.unwrap().map(|(_, d)| d), Some(false));
            assert!(stream.next().await.unwrap().is_none());
            assert!(stream.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn vec_stream_empty_yields_none_immediately() {
        workers::block_on(async {
            let mut stream = VecStream::new(Vec::new());
            assert!(stream.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    /// A stream that is not `Sync` (it counts its pulls in a `Cell`) is still
    /// a `DocStream`, and its box can move to another thread: one task owns
    /// a stream, so nothing needs a lock to make it `Sync`.
    #[test]
    fn a_stream_owned_by_one_task_need_not_be_sync() {
        workers::block_on(async {
            struct Counting {
                pulls: std::cell::Cell<usize>,
            }

            #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
            #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
            impl DocStream for Counting {
                async fn next(&mut self) -> Result<Option<(Document, bool)>> {
                    self.pulls.set(self.pulls.get() + 1);
                    Ok(None)
                }
            }

            fn moves_between_threads<T>(_: &T)
            where
                T: Send,
            {
            }

            let mut stream: Box<dyn DocStream> = Box::new(Counting {
                pulls: std::cell::Cell::new(0),
            });
            moves_between_threads(&stream);
            assert!(stream.next().await.unwrap().is_none());
            assert!(stream.next().await.unwrap().is_none());
        })
        .unwrap()
    }
}
