//! A read through a deferred index while it still owes some documents an
//! update (`proofs/tla/DeferredIndex.tla`): the stored keys of those documents
//! are hidden, and their live keys are merged in, in key order.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rapidhash::{HashSetExt, RapidHashSet};

use crate::corekv::traits::private::Sealed;
use crate::corekv::{IterOptions, Iterator, KvPair, Reader, Result};

/// The keys a read through one deferred index must hide and add.
#[derive(Debug, Default, Clone)]
pub struct IndexOverlay {
    /// The index's entry prefix: no other key is touched.
    prefix: Vec<u8>,
    /// The stored keys of owed documents, from their position records.
    hidden: Arc<RapidHashSet<Vec<u8>>>,
    /// The owed documents' live keys, sorted.
    added: Vec<Vec<u8>>,
}

impl IndexOverlay {
    pub fn new(prefix: Vec<u8>, hidden: Vec<Vec<u8>>, mut added: Vec<Vec<u8>>) -> Self {
        let mut set = RapidHashSet::with_capacity(hidden.len());
        set.extend(hidden);
        added.sort_unstable();
        added.dedup();
        Self {
            prefix,
            hidden: Arc::new(set),
            added,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.hidden.is_empty() && self.added.is_empty()
    }

    /// The added keys `opts` would iterate, in its order.
    fn added_in(&self, opts: &IterOptions) -> Vec<Vec<u8>> {
        let mut keys: Vec<Vec<u8>> = self
            .added
            .iter()
            .filter(|key| {
                opts.prefix().is_none_or(|prefix| key.starts_with(prefix))
                    && opts.start().is_none_or(|start| key.as_slice() >= start)
                    && opts.end().is_none_or(|end| key.as_slice() < end)
            })
            .cloned()
            .collect();
        if opts.reverse() {
            keys.reverse();
        }
        keys
    }

    /// Whether `opts` can reach a key of this index.
    fn covers(&self, opts: &IterOptions) -> bool {
        match opts.prefix() {
            Some(prefix) => prefix.starts_with(&self.prefix) || self.prefix.starts_with(prefix),
            None => true,
        }
    }
}

/// `inner`, read through `overlay`.
pub struct OverlayReader<'a, R>
where
    R: Reader + ?Sized,
{
    inner: &'a R,
    overlay: &'a IndexOverlay,
}

impl<'a, R> OverlayReader<'a, R>
where
    R: Reader + ?Sized,
{
    pub fn new(inner: &'a R, overlay: &'a IndexOverlay) -> Self {
        Self { inner, overlay }
    }
}

impl<R> Sealed for OverlayReader<'_, R> where R: Reader + ?Sized {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<R> Reader for OverlayReader<'_, R>
where
    R: Reader + ?Sized,
{
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        self.inner.get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        if self.overlay.is_empty() || !self.overlay.covers(&opts) {
            return self.inner.iterator(opts).await;
        }
        let added = self.overlay.added_in(&opts);
        let reverse = opts.reverse();
        Ok(Box::new(MergedIterator {
            stored: self.inner.iterator(opts).await?,
            hidden: Arc::clone(&self.overlay.hidden),
            added,
            next_added: 0,
            peeked: None,
            reverse,
        }))
    }
}

/// Stored keys minus the hidden ones, merged with the added ones.
struct MergedIterator {
    stored: Box<dyn Iterator>,
    hidden: Arc<RapidHashSet<Vec<u8>>>,
    /// Already in iteration order.
    added: Vec<Vec<u8>>,
    next_added: usize,
    /// The next stored pair not yet returned.
    peeked: Option<KvPair>,
    reverse: bool,
}

impl MergedIterator {
    async fn peek_stored(&mut self) -> Result<Option<&KvPair>> {
        while self.peeked.is_none() {
            match self.stored.next().await? {
                Some(pair) if self.hidden.contains(&pair.key) => continue,
                Some(pair) => self.peeked = Some(pair),
                None => return Ok(None),
            }
        }
        Ok(self.peeked.as_ref())
    }
}

/// Whether `a` comes before `b` in an iteration that runs `reverse` or not.
fn before(reverse: bool, a: &[u8], b: &[u8]) -> bool {
    if reverse {
        a > b
    } else {
        a < b
    }
}

impl Sealed for MergedIterator {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for MergedIterator {
    async fn next(&mut self) -> Result<Option<KvPair>> {
        let added = self.added.get(self.next_added).cloned();
        let reverse = self.reverse;
        let stored_first = match (self.peek_stored().await?, &added) {
            (Some(stored), Some(added)) => before(reverse, &stored.key, added),
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => return Ok(None),
        };
        if stored_first {
            return Ok(self.peeked.take());
        }
        self.next_added += 1;
        Ok(added.map(|key| KvPair {
            key,
            value: Bytes::new(),
        }))
    }

    async fn close(&mut self) -> Result<()> {
        self.next_added = self.added.len();
        self.peeked = None;
        self.stored.close().await
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        self.peeked = None;
        let stored = self.stored.seek(key).await?;
        self.next_added = self
            .added
            .iter()
            .position(|added| !before(self.reverse, added, key))
            .unwrap_or(self.added.len());
        Ok(stored || self.next_added < self.added.len())
    }

    async fn reset(&mut self) -> Result<()> {
        self.peeked = None;
        self.next_added = 0;
        self.stored.reset().await
    }
}
