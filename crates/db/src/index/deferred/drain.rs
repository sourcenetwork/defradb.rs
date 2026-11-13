//! Moving a deferred index's entries to the values its log owes.

use std::sync::Arc;

use rapidhash::RapidHashSet;
use storage::corekv::{IterOptions, RetryAttemptError, RetryAttemptValue, Store};
use storage::keys::{index_pending_log, index_position_key};

use crate::collection::loader::load_collection_from_systemstore;
use crate::database::DB;
use crate::index::error::{Error, Result};
use crate::index::manager::{IndexType, LiveDocs};
use crate::index::IndexManager;

/// The most log entries one drain transaction takes, so a backlog commits in
/// bounded transactions.
const MAX_PLAN: usize = 256;

/// What one drain will apply, read from a snapshot.
struct Plan {
    first: u64,
    through: u64,
    docs: Vec<u64>,
}

impl Plan {
    /// Positions are dense from the head, so this is how many entries it takes.
    fn entries(&self) -> usize {
        usize::try_from(self.through - self.first + 1).unwrap_or(usize::MAX)
    }

    /// Whether carrying all of `carried` would only renumber the plan's
    /// entries. A full plan is still carried, so the entries behind it come forward.
    fn only_renumbers(&self, carried: usize) -> bool {
        carried == self.entries() && self.entries() < MAX_PLAN
    }
}

/// What a drain did with its plan.
#[derive(Default)]
pub(crate) struct Drained {
    /// Documents whose entries now match their live value.
    pub(crate) moved: Vec<u64>,
    /// Documents that failed, each with why. Each is owed again at the log's tail.
    pub(crate) failed: Vec<(u64, Error)>,
    /// Whether some document is owed again at the log's tail.
    pub(crate) carried: bool,
    /// Whether the log shrank, so another plan may find more to do.
    pub(crate) progressed: bool,
}

/// Drains up to [`MAX_PLAN`] entries of the index `index_id` of the
/// collection `name`, or `None` when nothing is owed. A document in `skip`,
/// or one that fails, is owed again at the log's tail, so the rest still drain.
pub(crate) async fn drain_index<S>(
    db: &DB<S>,
    name: &str,
    index_id: u32,
    skip: &RapidHashSet<u64>,
) -> Result<Option<Drained>>
where
    S: Store + 'static,
{
    let Some(collection) = db
        .get_collection(name)
        .map_err(|e| Error::Other(e.to_string()))?
    else {
        return Ok(None);
    };
    let short_id = collection.resolved_root_id();
    let Some(plan) = plan(db, short_id, index_id).await? else {
        return Ok(None);
    };
    if plan.only_renumbers(plan.docs.len()) && plan.docs.iter().all(|doc| skip.contains(doc)) {
        return Ok(Some(Drained {
            carried: true,
            ..Drained::default()
        }));
    }
    let (name, plan, skip) = (name.to_string(), Arc::new(plan), Arc::new(skip.clone()));
    let lens_store = Arc::clone(db.lens_store());
    let drained = datastore::transact(
        db.store().as_ref(),
        db.options().retry_options(),
        move |txn| {
            let (name, plan, skip) = (name.clone(), Arc::clone(&plan), Arc::clone(&skip));
            let lens_store = Arc::clone(&lens_store);
            Box::pin(async move {
                let drained = apply(txn, &name, short_id, index_id, &plan, &skip, &lens_store)
                    .await
                    .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
                Ok(Box::new(drained) as RetryAttemptValue)
            })
        },
    )
    .await
    .map_err(|error| match error.downcast_abort::<Error>() {
        Ok(error) => error,
        Err(error) => Error::Storage(error.into_storage()),
    })?;
    Ok(Some(drained))
}

/// The oldest entries of the log, read on a snapshot. Entries committed after
/// it take later positions, so they stay owed.
async fn plan<S>(db: &DB<S>, short_id: u32, index_id: u32) -> Result<Option<Plan>>
where
    S: Store + 'static,
{
    let snapshot = db
        .new_txn(true)
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let read = async {
        let datastore = snapshot
            .datastore()
            .map_err(|e| Error::Other(e.to_string()))?;
        let log = index_pending_log(short_id, index_id);
        let mut iter = datastore
            .iterator(IterOptions::default().with_prefix(log.entry_prefix().to_vec()))
            .await
            .map_err(Error::Storage)?;
        let mut entries = Vec::new();
        while entries.len() < MAX_PLAN {
            let Some(pair) = iter.next().await.map_err(Error::Storage)? else {
                break;
            };
            entries.push((position(&pair.key)?, doc_short_id(&pair.value)?));
        }
        iter.close().await.map_err(Error::Storage)?;
        Ok::<_, Error>(entries)
    };
    let entries = read.await;
    let _ = snapshot.discard();
    let entries = entries?;
    let (Some(first), Some(last)) = (entries.first(), entries.last()) else {
        return Ok(None);
    };
    let (first, through) = (first.0, last.0);
    let mut docs: Vec<u64> = entries.into_iter().map(|(_, doc)| doc).collect();
    docs.sort_unstable();
    docs.dedup();
    Ok(Some(Plan {
        first,
        through,
        docs,
    }))
}

/// Moves each planned document's entries to its live value and truncates the
/// log through the plan. The position record is the one read that decides: a
/// drain racing this one on a document makes one of them run again. A
/// document that cannot be read or decoded is appended again instead, so it
/// stays owed without holding back the others.
async fn apply(
    txn: &mut datastore::BasicTxn,
    name: &str,
    short_id: u32,
    index_id: u32,
    plan: &Plan,
    skip: &RapidHashSet<u64>,
    lens_store: &Arc<dyn lens::TransformStore>,
) -> Result<Drained> {
    let datastore = txn.datastore();
    let systemstore = txn.systemstore();
    let log = index_pending_log(short_id, index_id);
    // Read inside the transaction, so a drop of the index commits first or
    // makes this run again and find it gone.
    let collection = load_collection_from_systemstore(&systemstore, name)
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let desc = collection.as_ref().and_then(|collection| {
        collection
            .write_indexes()
            .iter()
            .find(|index| index.id == index_id)
            .cloned()
    });
    let (Some(collection), Some(desc)) = (collection, desc) else {
        datastore
            .truncate_log(&log, plan.first..=plan.through)
            .await
            .map_err(Error::Storage)?;
        return Ok(Drained {
            moved: plan.docs.clone(),
            progressed: true,
            ..Drained::default()
        });
    };
    let schema = collection.schema();
    let manager = IndexManager::from_indexes(
        short_id,
        schema,
        std::slice::from_ref(&desc),
        collection.crdts(),
    )?;
    let index = manager
        .get_index(&desc.name)
        .ok_or_else(|| Error::Other(format!("index '{}' not found", desc.name)))?;
    let mut live_docs = LiveDocs::new(&collection, &datastore, &systemstore, Some(lens_store));
    let mut drained = Drained::default();
    let mut carry = Vec::new();
    for &doc in &plan.docs {
        if skip.contains(&doc) {
            carry.push(doc);
            continue;
        }
        let entries = async {
            let new = match live_docs.get(doc).await? {
                Some(live) => IndexManager::entry_keys(
                    index,
                    doc,
                    &manager.extract_index_values(&live, &desc, schema)?,
                )?,
                None => Vec::new(),
            };
            // Read only once the live value is known, so a document carried
            // forward validates nothing.
            let record = datastore
                .get(&index_position_key(short_id, index_id, doc))
                .await
                .map_err(Error::Storage)?;
            Ok::<_, Error>((IndexManager::recorded(record.as_deref())?, new))
        };
        match entries.await {
            Ok((old, new)) => {
                if let Err(source) =
                    move_entries(&datastore, index, short_id, doc, &old, &new).await
                {
                    return Err(named(&systemstore, doc, source).await);
                }
                drained.moved.push(doc);
            }
            // A store failure is no fault of the document's.
            Err(Error::Storage(error)) => {
                return Err(named(&systemstore, doc, Error::Storage(error)).await)
            }
            Err(source) => {
                drained
                    .failed
                    .push((doc, named(&systemstore, doc, source).await));
                carry.push(doc);
            }
        }
    }
    drained.carried = !carry.is_empty();
    if drained.moved.is_empty() && plan.only_renumbers(carry.len()) {
        return Ok(drained);
    }
    let shrinks = !drained.moved.is_empty() || carry.len() < plan.entries();
    for doc in carry {
        datastore
            .append(&log, &doc.to_be_bytes(), None)
            .await
            .map_err(Error::Storage)?;
    }
    datastore
        .truncate_log(&log, plan.first..=plan.through)
        .await
        .map_err(Error::Storage)?;
    drained.progressed = shrinks;
    Ok(drained)
}

/// Moves a document's entries from the keys its record names to `new`.
async fn move_entries(
    datastore: &datastore::NamespaceView,
    index: &IndexType,
    short_id: u32,
    doc: u64,
    old: &[Vec<u8>],
    new: &[Vec<u8>],
) -> Result<()> {
    if old == new {
        return Ok(());
    }
    for key in old.iter().filter(|key| !new.contains(key)) {
        datastore.delete(key).await.map_err(Error::Storage)?;
    }
    for key in new.iter().filter(|key| !old.contains(key)) {
        datastore.set(key, &[]).await.map_err(Error::Storage)?;
    }
    let record = index_position_key(short_id, index.description().id, doc);
    if new.is_empty() {
        datastore.delete(&record).await.map_err(Error::Storage)
    } else {
        datastore
            .set(&record, &super::encode_record(new)?)
            .await
            .map_err(Error::Storage)
    }
}

/// `source`, naming the document it failed on.
async fn named(systemstore: &datastore::NamespaceView, doc: u64, source: Error) -> Error {
    Error::DeferredDrainDocument {
        doc: document_name(systemstore, doc).await,
        source: Box::new(source),
    }
}

/// The document's DocID for an error, or its short id when that is not to be had.
async fn document_name(systemstore: &datastore::NamespaceView, doc: u64) -> String {
    match crate::docid::map::get_doc_id(systemstore, doc).await {
        Ok(Some(id)) => id,
        _ => format!("with short id {doc}"),
    }
}

/// A log entry key ends in its position as 20 decimal digits.
pub(super) fn position(key: &[u8]) -> Result<u64> {
    key.len()
        .checked_sub(20)
        .and_then(|start| std::str::from_utf8(&key[start..]).ok())
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| Error::Other("deferred index log entry without a position".into()))
}

fn doc_short_id(value: &[u8]) -> Result<u64> {
    value
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| Error::Other("deferred index log entry is not a document short id".into()))
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;

    use document::{DocID, Document, FieldOp, NormalValue};
    use query::DocMutator;
    use rapidhash::{HashSetExt, RapidHashSet};
    use schema::{
        CType, CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
        OrderedIndexDescription,
    };
    use storage::RegolithStore;

    use super::*;
    use crate::AutoCommitMutator;

    pub(crate) const NAME: &str = "Tallies";

    pub(crate) async fn indexed() -> (Arc<DB<RegolithStore>>, u32, u32) {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(CollectionVersion::new(
            NAME,
            "v1",
            "col-drain-tallies",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
                FieldDescription::new("2", "count", FieldKind::int(), CType::PCounter),
            ],
        ))
        .await
        .unwrap();
        let index = db
            .create_index(
                NAME,
                Some("by_count"),
                vec![IndexedFieldDescription {
                    name: "count".to_string(),
                    descending: false,
                }],
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
            )
            .await
            .unwrap();
        let short_id = db.require_collection(NAME).unwrap().resolved_root_id();
        (db, short_id, index.id)
    }

    pub(crate) async fn create(db: &Arc<DB<RegolithStore>>, count: i64) -> DocID {
        let mut doc = Document::new(db.require_collection(NAME).unwrap().schema_arc());
        doc.set("count", NormalValue::Int(count)).unwrap();
        AutoCommitMutator::new(Arc::clone(db))
            .create(NAME, doc)
            .await
            .unwrap()
            .doc_id
    }

    pub(crate) async fn increment(db: &Arc<DB<RegolithStore>>, id: &DocID) {
        let mut doc = Document::new(db.require_collection(NAME).unwrap().schema_arc());
        doc.set_id(id.clone());
        doc.edit("count", FieldOp::Increment(NormalValue::Int(1)))
            .unwrap();
        let mut changed = RapidHashSet::new();
        changed.insert("count".to_string());
        AutoCommitMutator::new(Arc::clone(db))
            .update(NAME, doc, changed)
            .await
            .unwrap();
    }

    async fn apply_committed(db: &DB<RegolithStore>, short_id: u32, index_id: u32, plan: Plan) {
        let plan = Arc::new(plan);
        let lens_store = Arc::clone(db.lens_store());
        datastore::transact(
            db.store().as_ref(),
            db.options().retry_options(),
            move |txn| {
                let plan = Arc::clone(&plan);
                let lens_store = Arc::clone(&lens_store);
                Box::pin(async move {
                    apply(
                        txn,
                        NAME,
                        short_id,
                        index_id,
                        &plan,
                        &RapidHashSet::new(),
                        &lens_store,
                    )
                    .await
                    .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
                    Ok(Box::new(()) as RetryAttemptValue)
                })
            },
        )
        .await
        .map(|_: ()| ())
        .unwrap();
    }

    /// An increment that commits after the plan stays owed: the drain
    /// truncates only through the head it planned.
    #[test]
    fn an_increment_after_the_plan_stays_owed() {
        workers::block_on(async {
            let (db, short_id, index_id) = indexed().await;
            let id = create(&db, 1).await;
            increment(&db, &id).await;
            let planned = plan(&db, short_id, index_id).await.unwrap().unwrap();
            increment(&db, &id).await;

            apply_committed(&db, short_id, index_id, planned).await;

            let left = plan(&db, short_id, index_id).await.unwrap();
            assert_eq!(left.map(|plan| plan.docs.len()), Some(1));
        })
        .unwrap();
    }

    /// Two drains of one document: the one that read the position record
    /// before the other moved it runs again instead of leaving an entry behind.
    #[test]
    fn a_drain_that_read_a_moved_record_does_not_commit() {
        workers::block_on(async {
            let (db, short_id, index_id) = indexed().await;
            let id = create(&db, 1).await;
            increment(&db, &id).await;
            let planned = plan(&db, short_id, index_id).await.unwrap().unwrap();
            let mut first = datastore::BasicTxn::new(db.store().as_ref(), 0, false)
                .await
                .unwrap();
            apply(
                &mut first,
                NAME,
                short_id,
                index_id,
                &planned,
                &RapidHashSet::new(),
                db.lens_store(),
            )
            .await
            .unwrap();
            increment(&db, &id).await;

            while drain_index(&db, NAME, index_id, &RapidHashSet::new())
                .await
                .unwrap()
                .is_some_and(|drained| drained.progressed)
            {}

            assert!(first.commit().await.is_err());
        })
        .unwrap();
    }

    /// A drain that fails on one document names it.
    #[test]
    fn a_failed_drain_names_the_document() {
        workers::block_on(async {
            let (db, short_id, index_id) = indexed().await;
            let id = create(&db, 1).await;
            increment(&db, &id).await;
            let doc = plan(&db, short_id, index_id).await.unwrap().unwrap().docs[0];
            let txn = datastore::BasicTxn::new(db.store().as_ref(), 0, false)
                .await
                .unwrap();
            txn.datastore()
                .set(&index_position_key(short_id, index_id, doc), b"bad")
                .await
                .unwrap();
            txn.commit().await.unwrap();

            let error = db.drain_deferred_indexes().await.unwrap_err().to_string();

            assert!(
                error.contains(NAME) && error.contains(&id.to_string()),
                "{error}"
            );
        })
        .unwrap();
    }

    /// A document at an older schema version takes whole writes that leave
    /// its version alone, as a peer's merge does. The drain reads its value
    /// without validating it, so such a write is no conflict.
    #[test]
    fn a_write_to_an_old_version_document_does_not_abort_its_drain() {
        workers::block_on(async {
            let (db, short_id, index_id) = indexed().await;
            let id = create(&db, 1).await;
            increment(&db, &id).await;
            db.patch_collection(
                NAME,
                r#"[{"op":"add","path":"/Tallies/Fields/-","value":{"Name":"note","Kind":"String"}}]"#,
                None,
            )
            .await
            .unwrap();
            let planned = plan(&db, short_id, index_id).await.unwrap().unwrap();
            let doc_short_id = planned.docs[0];
            let mut first = datastore::BasicTxn::new(db.store().as_ref(), 0, false)
                .await
                .unwrap();
            apply(
                &mut first,
                NAME,
                short_id,
                index_id,
                &planned,
                &RapidHashSet::new(),
                db.lens_store(),
            )
            .await
            .unwrap();

            let collection = db.require_collection(NAME).unwrap();
            let write = datastore::BasicTxn::new(db.store().as_ref(), 0, false)
                .await
                .unwrap();
            let datastore = write.datastore();
            let mut doc = collection
                .read_document_unvalidated(&datastore, doc_short_id)
                .await
                .unwrap()
                .unwrap();
            doc.set("count", NormalValue::Int(7)).unwrap();
            collection
                .put_document_value(&datastore, doc_short_id, &doc, false, None)
                .await
                .unwrap();
            drop(datastore);
            write.commit().await.unwrap();

            first
                .commit()
                .await
                .expect("the drain decided nothing on the document's value");
        })
        .unwrap();
    }
}
