//! A transform run over documents its caller reads from a source it owns.
//!
//! A lensed read hands the lens its source one document at a time, only when
//! the lens asks for the next one, so at most one source document waits
//! between them and neither side holds the whole input. Both sides are polled
//! by the caller's task: the lens never runs ahead of a source the caller
//! cannot lend it, and the caller never reads ahead of what the lens needs.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};
use regolith::sync::ArrayQueue;

use crate::{Error, LensDoc, LensDocResultStream, Result, TransformId, TransformStore};

/// A running transform its caller feeds and reads through [`LensFeed::step`].
pub struct LensFeed {
    slot: Arc<Slot>,
    output: OnlyMut<LensDocResultStream>,
}

/// A `Send` value in a `Sync` holder: nothing reaches the value through a
/// shared reference, so sharing the holder shares nothing. A lensed read can
/// then live in a plan node, which must be `Sync`, though a stream is only
/// `Send`.
struct OnlyMut<T>(T);

// SAFETY: `OnlyMut` has no method, its field is private to this module, and
// this module reaches the field only through `&mut LensFeed`, which is
// exclusive: no two threads can reach the value at once.
unsafe impl<T> Sync for OnlyMut<T> where T: Send {}

/// The one document on its way from the caller to the lens, and the two
/// signals between them.
struct Slot {
    document: ArrayQueue<LensDoc>,
    /// The lens found the slot empty and waits for a document.
    wanted: AtomicBool,
    /// The source has ended.
    closed: AtomicBool,
}

/// The input stream the lens reads: the slot, until the source ends.
struct SlotStream(Arc<Slot>);

impl Stream for SlotStream {
    type Item = LensDoc;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<LensDoc>> {
        let slot = &self.0;
        if let Some(document) = slot.document.pop() {
            return Poll::Ready(Some(document));
        }
        if slot.closed.load(Ordering::Acquire) {
            return Poll::Ready(None);
        }
        // No waker: the feed checks `wanted` right after every poll of the
        // lens, within the same poll of its task.
        slot.wanted.store(true, Ordering::Release);
        Poll::Pending
    }
}

/// What a feed's caller does next.
pub enum FeedStep {
    /// Hand the caller what the lens yielded: a document, its failure, or
    /// the end of its output (`None`).
    Output(Option<Result<LensDoc>>),
    /// Read the next source document and [`push`](LensFeed::push) it: the lens
    /// waits for it.
    Input,
}

impl LensFeed {
    /// Starts transform `id` of `store` over documents the caller pushes.
    pub fn transform(store: &dyn TransformStore, id: &TransformId) -> Result<Self> {
        let slot = Arc::new(Slot {
            document: ArrayQueue::new(1),
            wanted: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        });
        let output = store.transform(id, Box::pin(SlotStream(Arc::clone(&slot))))?;
        Ok(Self {
            slot,
            output: OnlyMut(output),
        })
    }

    /// Runs the lens until it yields something or waits for a source
    /// document.
    pub async fn step(&mut self) -> FeedStep {
        loop {
            if let Poll::Ready(item) = futures::poll!(self.output.0.next()) {
                return FeedStep::Output(item);
            }
            if self.slot.wanted.swap(false, Ordering::AcqRel) {
                return FeedStep::Input;
            }
            // The lens is running or waiting on its own channels, whose wakers
            // it registered in the poll above; one of them resumes this task.
            futures::pending!();
        }
    }

    /// Hands the lens the source document it asked for, or the end of the
    /// source (`None`).
    pub fn push(&mut self, document: Option<LensDoc>) -> Result<()> {
        match document {
            Some(document) => self.slot.document.push(document).map_err(|_| {
                Error::Pipeline("a lens was fed a document while one still waited".to_string())
            }),
            None => {
                self.slot.closed.store(true, Ordering::Release);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{LensConfig, LensModule, MemoryTransformStore};

    pub(crate) fn doc(n: u64) -> LensDoc {
        let mut doc = LensDoc::new();
        doc.insert("n".to_string(), serde_json::json!(n));
        doc
    }

    /// The next thing `feed` yields, pushing from `source` each time the lens
    /// asks, and counting the asks in `pulls`.
    pub(crate) fn next_from(
        feed: &mut LensFeed,
        source: &mut impl Iterator<Item = LensDoc>,
        pulls: &mut usize,
    ) -> Option<Result<LensDoc>> {
        loop {
            match block_on(feed.step()) {
                FeedStep::Output(output) => return output,
                FeedStep::Input => {
                    *pulls += 1;
                    feed.push(source.next()).expect("the lens asked for it");
                }
            }
        }
    }

    fn feed() -> LensFeed {
        let store = MemoryTransformStore::new();
        let id = TransformId::new("passthrough");
        block_on(store.add_with_id(
            id.clone(),
            LensConfig::new("v1", "v2", LensModule::default()),
        ))
        .unwrap();
        LensFeed::transform(&store, &id).unwrap()
    }

    #[test]
    fn a_feed_reads_its_source_one_document_per_ask() {
        let mut feed = feed();
        let mut source = (0..3).map(doc);
        let mut pulls = 0;
        let mut outputs = Vec::new();
        while let Some(output) = next_from(&mut feed, &mut source, &mut pulls) {
            outputs.push(output.unwrap());
            assert_eq!(pulls, outputs.len(), "nothing is read ahead");
        }
        assert_eq!(outputs, (0..3).map(doc).collect::<Vec<_>>());
        assert_eq!(pulls, 4, "three documents and the end");
    }

    #[test]
    fn a_document_pushed_unasked_onto_a_waiting_one_fails() {
        let mut feed = feed();
        feed.push(Some(doc(1))).unwrap();
        assert!(matches!(feed.push(Some(doc(2))), Err(Error::Pipeline(_))));
    }

    #[test]
    fn a_missing_transform_fails_at_the_start() {
        let store = MemoryTransformStore::new();
        assert!(matches!(
            LensFeed::transform(&store, &TransformId::new("missing")),
            Err(Error::TransformNotFound(_))
        ));
    }
}
