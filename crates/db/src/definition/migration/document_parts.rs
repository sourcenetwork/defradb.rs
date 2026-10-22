//! Field operands for documents written before part IDs existed.
//!
//! A collection version gets local part IDs when it is created (see
//! `collection::parts`), and its documents are then stored in the field format
//! (`crdt::stored`), which merges field operands and answers projected
//! reads. A version created by an earlier release has no part IDs, so its
//! documents stay one CBOR value forever: every update rewrites the whole
//! value and every read validates all of it.
//!
//! At open this assigns part IDs to every stored version that lacks them, then
//! rewrites each document of every collection's current version into the field
//! format, a page at a time: each page is one engine-retried transaction,
//! bounded by bytes, that also records how far it got, so a crash resumes
//! where the last committed page ended. A converted document is read back in
//! the same transaction and must read exactly as before; one that would not is
//! left whole and counted. Documents of other versions stay whole: a version
//! change is a whole-value put anyway. A stamp then makes later opens one read.

use std::sync::Arc;

use crdt::stored::StoredDocument;
use datastore::BasicTxn;
use document::Document;
use schema::CollectionVersion;
use storage::corekv::{
    IterOptions, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryOptions, RetryTxn, Store,
};
use storage::keys::doc_id_index::decode_doc_short_id;
use storage::keys::systemstore::CollectionKey;
use tracing::{info, warn};

use crate::collection::Collection;
use crate::error::{Error, Result};
use crate::DB;

/// The document format the store's documents are in: 1 means every stored
/// collection version has part IDs and every document of a current version
/// that can be is in the field format.
pub const CURRENT_DOCUMENT_FORMAT: u32 = 1;

/// Systemstore key holding [`CURRENT_DOCUMENT_FORMAT`] once the store is in it.
pub const DOCUMENT_FORMAT_KEY: &[u8] = b"/system/document_format";

/// Where a collection's conversion has reached: its last converted key.
const CURSOR_PREFIX: &str = "/system/document_format/cursor/";

/// Bytes of documents one page reads, and so about what it writes.
const PAGE_BYTES: usize = 1 << 20;

impl<S: Store> DB<S> {
    /// Brings documents written before part IDs existed into the field format.
    pub(crate) async fn migrate_document_parts(&self) -> Result<()>
    where
        S: 'static,
    {
        if self.stored_document_format().await? == Some(CURRENT_DOCUMENT_FORMAT) {
            return Ok(());
        }
        self.run_parts_attempt(Work::PartIds).await?;
        let collections: Vec<Collection> = self.collections.peek(|map| {
            map.values()
                .filter(|collection| !collection.schema().is_placeholder)
                .cloned()
                .collect()
        });
        let mut total = Page::default();
        for collection in collections {
            loop {
                let page = self
                    .run_parts_attempt(Work::Page(collection.clone()))
                    .await?;
                total.converted += page.converted;
                total.kept_whole += page.kept_whole;
                if page.done {
                    break;
                }
            }
        }
        self.run_parts_attempt(Work::Stamp).await?;
        if total.converted > 0 {
            info!(
                documents = total.converted,
                "stored documents written before part IDs in the field format"
            );
        }
        if total.kept_whole > 0 {
            warn!(
                documents = total.kept_whole,
                "documents that would not read the same in the field format stay whole values"
            );
        }
        Ok(())
    }

    /// The document format stamped on disk, or `None` before the first open
    /// of this release.
    pub async fn stored_document_format(&self) -> Result<Option<u32>> {
        let txn = self.new_txn(true).await?;
        let raw = txn
            .systemstore()?
            .get(DOCUMENT_FORMAT_KEY)
            .await
            .map_err(Error::Storage)?;
        let _ = txn.discard();
        Ok(raw
            .and_then(|bytes| <[u8; 4]>::try_from(bytes.as_ref()).ok())
            .map(u32::from_be_bytes))
    }

    async fn run_parts_attempt(&self, work: Work) -> Result<Page>
    where
        S: 'static,
    {
        let attempt = PartsAttempt {
            store: Arc::clone(self.store()),
            work,
        };
        match self
            .store()
            .transact_retry(
                RetryOptions::new(self.options().max_txn_retries()),
                Box::new(attempt),
            )
            .await
        {
            Ok(value) => value
                .downcast::<Page>()
                .map(|page| *page)
                .map_err(|_| Error::Other("invalid document format attempt result".into())),
            Err(error) => match error.downcast_abort::<Error>() {
                Ok(error) => Err(error),
                Err(error) => Err(Error::Storage(error.into_storage())),
            },
        }
    }
}

/// One transaction of the migration.
enum Work {
    /// Part IDs for every stored collection version that has none.
    PartIds,
    /// The next page of one collection's documents.
    Page(Collection),
    /// The stamp that ends the migration.
    Stamp,
}

#[derive(Default)]
struct Page {
    converted: u64,
    kept_whole: u64,
    done: bool,
}

struct PartsAttempt<S> {
    store: Arc<S>,
    work: Work,
}

impl<S> RetryAttemptFn for PartsAttempt<S>
where
    S: Store + 'static,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let mut txn = BasicTxn::from_retry_attempt_io(io, 0);
            let page = match &self.work {
                Work::PartIds => assign_part_ids(self.store.as_ref(), &mut txn).await,
                Work::Page(collection) => convert_page(collection, &mut txn).await,
                Work::Stamp => txn
                    .systemstore()
                    .set(DOCUMENT_FORMAT_KEY, &CURRENT_DOCUMENT_FORMAT.to_be_bytes())
                    .await
                    .map(|()| Page::default())
                    .map_err(Error::Storage),
            }
            .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(page) as RetryAttemptValue)
        })
    }
}

/// Gives every stored collection version its part IDs, as creating it in
/// this release would have. Versions that have them are left alone.
async fn assign_part_ids<S>(store: &S, txn: &mut BasicTxn) -> Result<Page>
where
    S: Store,
{
    let systemstore = txn.systemstore();
    let mut iter = systemstore
        .iterator(IterOptions::new().with_prefix(CollectionKey::collection_prefix()))
        .await
        .map_err(Error::Storage)?;
    let mut versions = Vec::new();
    while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
        let version: CollectionVersion = serde_json::from_slice(&pair.value)
            .map_err(|error| Error::collection_schema_json("invalid stored collection", error))?;
        versions.push(version);
    }
    iter.close().await.map_err(Error::Storage)?;
    for version in &versions {
        crate::collection::parts::prepare(store, &systemstore, version).await?;
    }
    Ok(Page::default())
}

fn cursor_key(collection: &Collection) -> Vec<u8> {
    format!("{CURSOR_PREFIX}{}", collection.collection_id()).into_bytes()
}

/// Converts the next page of `collection`'s documents after its cursor and
/// moves the cursor, or deletes it once the collection is done.
async fn convert_page(collection: &Collection, txn: &mut BasicTxn) -> Result<Page> {
    let datastore = txn.datastore();
    let systemstore = txn.systemstore();
    let cursor = cursor_key(collection);
    let prefix = collection.collection_key_prefix();
    let start = match systemstore.get(&cursor).await.map_err(Error::Storage)? {
        Some(last) => [last.as_ref(), &[0]].concat(),
        None => prefix.clone(),
    };
    // A validated scan: the page decides on each whole value it reads.
    let mut iter = datastore
        .iterator(
            IterOptions::new()
                .with_prefix(prefix.clone())
                .with_start(start)
                .with_validation(true),
        )
        .await
        .map_err(Error::Storage)?;
    let mut documents = Vec::new();
    let mut bytes = 0;
    let mut more = false;
    while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
        if bytes >= PAGE_BYTES {
            more = true;
            break;
        }
        bytes += pair.key.len() + pair.value.len();
        documents.push(pair);
    }
    iter.close().await.map_err(Error::Storage)?;

    let mut page = Page::default();
    for pair in &documents {
        let Ok(doc_short_id) = decode_doc_short_id(&pair.key[prefix.len()..]) else {
            continue;
        };
        if StoredDocument::is_field_format(&pair.value)
            || collection
                .load_version(&datastore, doc_short_id)
                .await?
                .as_deref()
                != Some(collection.version_id())
        {
            continue;
        }
        let before = crdt::stored::document_from_storage(&pair.value, collection.schema_arc())
            .map_err(|error| Error::document_at_key(&pair.key, error))?;
        match collection
            .put_document_value(&datastore, doc_short_id, &before, false, None)
            .await
        {
            Ok(()) => {}
            Err(Error::Storage(error)) => return Err(Error::Storage(error)),
            // Not representable in the field format (nothing was written):
            // it stays a whole value, which reads exactly as before.
            Err(_) => {
                page.kept_whole += 1;
                continue;
            }
        }
        let written = datastore
            .get(&pair.key)
            .await
            .map_err(Error::Storage)?
            .ok_or_else(|| Error::Other("a converted document disappeared".into()))?;
        let after = crdt::stored::document_from_storage(&written, collection.schema_arc())
            .map_err(|error| Error::document_at_key(&pair.key, error))?;
        if reads_equal(&before, &after) {
            page.converted += 1;
        } else {
            datastore
                .set(&pair.key, &pair.value)
                .await
                .map_err(Error::Storage)?;
            page.kept_whole += 1;
        }
    }
    match documents.last() {
        Some(last) if more => systemstore.set(&cursor, &last.key).await,
        _ => {
            page.done = true;
            systemstore.delete(&cursor).await
        }
    }
    .map_err(Error::Storage)?;
    Ok(page)
}

/// Whether two documents answer every field read alike. A field holding null
/// reads as an absent one.
fn reads_equal(before: &Document, after: &Document) -> bool {
    let present = |doc: &Document| {
        doc.values()
            .keys()
            .filter(|name| doc.get(name).is_some_and(|value| !value.is_nil()))
            .count()
    };
    present(before) == present(after)
        && before
            .values()
            .keys()
            .all(|name| match (before.get(name), after.get(name)) {
                (Some(value), other) if !value.is_nil() => other == Some(value),
                (_, other) => other.is_none_or(|value| value.is_nil()),
            })
}
