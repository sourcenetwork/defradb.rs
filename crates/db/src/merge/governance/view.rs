use async_lock::Mutex;
use async_trait::async_trait;
use cid::Cid;
use defra_core::block::{Block, CrdtDelta};
use defra_core::thread_bounds::MaybeSendSync;
use document::NormalValue;
use schema::CType;
use storage::corekv::Store;

use crate::merge::merge_handler::DbMergeHandler;
use crate::txn::DbTxn;

/// A field a composite links to, as its field block carries it.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Value(NormalValue),
    /// The field block is encrypted; its value is not a verdict input.
    Encrypted,
    /// This node does not hold the field block.
    NotHeld(Cid),
    /// The field block is held but its value does not decode.
    Undecodable,
}

/// The reads a [`super::MergeValidator`] may base a verdict on.
///
/// Every read is addressed by content, or scoped to one snapshot taken for the
/// verdict. `Ok(None)` means the input is not held: a validator defers on it,
/// naming the CID it waited for.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MergeView: MaybeSendSync {
    async fn block(&self, cid: &Cid) -> Result<Option<Block>, String>;

    /// The fields a composite links to, by field name.
    async fn composite_fields(
        &self,
        cid: &Cid,
    ) -> Result<Option<Vec<(String, FieldValue)>>, String>;

    /// The genesis composite reached by following first heads from `cid`.
    /// `None` when an ancestor on the way is not held.
    async fn genesis(&self, cid: &Cid) -> Result<Option<Cid>, String>;

    /// Document IDs in `collection` whose merged `field` equals `value`, read
    /// from one snapshot shared by every call during this verdict.
    ///
    /// Only an `@immutable` scalar LWW field may be looked up; any other field
    /// is an `Err`. An immutable field is set once, so a document that matches
    /// on one replica matches on every replica holding it, whatever order its
    /// updates merged in. A mutable field could match on one replica and not
    /// another. Documents still arrive over time, so an empty result must
    /// defer, never reject.
    async fn find_documents(
        &self,
        collection: &str,
        field: &str,
        value: &NormalValue,
    ) -> Result<Vec<String>, String>;
}

pub(crate) struct DbMergeView<'a, S: Store, B: blockstore::Blockstore> {
    handler: &'a DbMergeHandler<S, B>,
    snapshot: Mutex<Option<DbTxn<S>>>,
}

impl<'a, S: Store, B: blockstore::Blockstore> DbMergeView<'a, S, B> {
    pub(crate) fn new(handler: &'a DbMergeHandler<S, B>) -> Self {
        Self {
            handler,
            snapshot: Mutex::new(None),
        }
    }

    pub(crate) async fn finish(self) {
        if let Some(txn) = self.snapshot.into_inner() {
            let _ = txn.discard();
        }
    }

    async fn load(&self, cid: &Cid) -> Result<Option<Block>, String> {
        match self.handler.blockstore.get(cid).await {
            Ok(Some(data)) => Block::from_dag_cbor(&data)
                .map(Some)
                .map_err(|error| error.to_string()),
            Ok(None) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, B> MergeView for DbMergeView<'_, S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    async fn block(&self, cid: &Cid) -> Result<Option<Block>, String> {
        self.load(cid).await
    }

    async fn composite_fields(
        &self,
        cid: &Cid,
    ) -> Result<Option<Vec<(String, FieldValue)>>, String> {
        let Some(composite) = self.load(cid).await? else {
            return Ok(None);
        };
        let mut fields = Vec::new();
        for link in composite.links.iter().flatten() {
            let value = match self.load(&link.link).await? {
                None => FieldValue::NotHeld(link.link),
                Some(field) if field.encryption.is_some() => FieldValue::Encrypted,
                Some(field) => match &field.delta {
                    CrdtDelta::Lww(payload) => decode(&payload.data),
                    CrdtDelta::Counter(payload) => decode(&payload.data),
                    _ => FieldValue::Undecodable,
                },
            };
            fields.push((link.name.clone(), value));
        }
        Ok(Some(fields))
    }

    async fn genesis(&self, cid: &Cid) -> Result<Option<Cid>, String> {
        let mut current = *cid;
        for depth in 0.. {
            self.handler
                .ensure_merge_depth(&current, depth)
                .map_err(|error| error.to_string())?;
            let Some(block) = self.load(&current).await? else {
                return Ok(None);
            };
            match block.heads.as_deref().and_then(<[Cid]>::first) {
                Some(parent) => current = *parent,
                None => return Ok(Some(current)),
            }
        }
        unreachable!("the depth check ends the walk")
    }

    async fn find_documents(
        &self,
        collection: &str,
        field: &str,
        value: &NormalValue,
    ) -> Result<Vec<String>, String> {
        let db = &self.handler.db;
        let Some(collection) = db
            .get_collection(collection)
            .map_err(|error| error.to_string())?
        else {
            return Ok(Vec::new());
        };
        let lookup_field = collection
            .schema()
            .fields
            .iter()
            .find(|candidate| candidate.name == field);
        if !lookup_field.is_some_and(|field| {
            field.immutable && field.crdt_type == CType::LwwRegister && field.kind.is_scalar()
        }) {
            return Err(format!(
                "find_documents field '{field}' in collection '{}' must be an @immutable scalar LWW field",
                collection.name()
            ));
        }
        let mut snapshot = self.snapshot.lock().await;
        if snapshot.is_none() {
            *snapshot = Some(db.new_txn(true).await.map_err(|error| error.to_string())?);
        }
        let txn = snapshot.as_ref().expect("snapshot opened above");
        let datastore = txn.datastore().map_err(|error| error.to_string())?;
        let systemstore = txn.systemstore().map_err(|error| error.to_string())?;
        let documents = collection
            .get_all_with_datastore(&datastore, &systemstore)
            .await
            .map_err(|error| error.to_string())?;
        Ok(documents
            .into_iter()
            .filter(|document| document.get(field) == Some(value))
            .filter_map(|document| document.id().map(|id| id.to_string()))
            .collect())
    }
}

fn decode(data: &[u8]) -> FieldValue {
    ciborium::from_reader::<NormalValue, _>(data)
        .map(FieldValue::Value)
        .unwrap_or(FieldValue::Undecodable)
}
