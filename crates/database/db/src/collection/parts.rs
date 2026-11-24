//! Persistent local IDs for Regolith's projected document reads, and the
//! decoded map each collection version shares.
//!
//! Schema field IDs are CIDs, not `u32`s. These IDs never cross the wire and
//! are never recycled, including when a schema version is removed and restored.
//!
//! A version's map is written by the transaction that gives the version its
//! IDs and never changes once it names every field of the version. So a
//! complete map is read and parsed once and published in the version's
//! [`PartIdSnapshot`], which every handle of the version shares: clones of a
//! [`Collection`] and handles rebuilt from the same definition. A map that is
//! missing or still incomplete is read in the caller's transaction every time,
//! so a transaction that races the write of a map still conflicts with it.
use std::collections::BTreeMap;
use std::sync::Arc;

use regolith::sync::Atom;

use super::{Collection, Error, NamespaceView, Result};
use schema::CollectionVersion;
use storage::corekv::Store;
use storage::namespace::Namespace;

pub(crate) type PartIds = BTreeMap<String, u32>;

fn key(version: &str) -> Vec<u8> {
    format!("/parts/{version}").into_bytes()
}

fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Storage(storage::Error::Other(format!(
        "invalid document part IDs: {error}"
    )))
}

pub(crate) async fn prepare<S: Store>(
    store: &S,
    systemstore: &NamespaceView,
    schema: &CollectionVersion,
) -> Result<()> {
    let key = key(&schema.version_id);
    let mut ids: PartIds = match systemstore.get(&key).await.map_err(Error::Storage)? {
        Some(bytes) => serde_json::from_slice(&bytes).map_err(invalid)?,
        None => BTreeMap::new(),
    };
    let mut missing: Vec<_> = schema
        .fields
        .iter()
        .filter(|field| !ids.contains_key(&field.name))
        .map(|field| field.name.clone())
        .collect();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Ok(());
    }
    let reserved = store
        .allocate(b"s/seq/document-part", missing.len() as u64)
        .await
        .map_err(Error::Storage)?;
    if reserved.end - 1 > u64::from(u32::MAX) {
        return Err(invalid("part ID space exhausted"));
    }
    for (name, id) in missing.into_iter().zip(reserved) {
        ids.insert(name, id as u32);
    }
    systemstore
        .set(&key, &serde_json::to_vec(&ids).map_err(invalid)?)
        .await
        .map_err(Error::Storage)
}

/// The decoded part-ID map of one collection version, once it is complete.
#[derive(Debug, Default)]
pub(crate) struct PartIdSnapshot {
    map: Atom<Option<Arc<PartIds>>>,
}

impl PartIdSnapshot {
    fn get(&self) -> Option<Arc<PartIds>> {
        self.map.load_clone()
    }

    fn publish(&self, ids: Arc<PartIds>) {
        self.map.store(Some(ids));
    }
}

impl Collection {
    /// This version's part-ID map, or `None` when the version has none.
    pub(crate) async fn part_ids(&self, datastore: &NamespaceView) -> Result<Option<Arc<PartIds>>> {
        if let Some(ids) = self.part_id_snapshot.get() {
            return Ok(Some(ids));
        }
        let system = datastore.sibling(Namespace::Systemstore);
        let Some(bytes) = system
            .get(&key(self.version_id()))
            .await
            .map_err(Error::Storage)?
        else {
            return Ok(None);
        };
        let ids: Arc<PartIds> = Arc::new(serde_json::from_slice(&bytes).map_err(invalid)?);
        if self
            .schema()
            .fields
            .iter()
            .all(|field| ids.contains_key(&field.name))
        {
            self.part_id_snapshot.publish(Arc::clone(&ids));
        }
        Ok(Some(ids))
    }

    /// The sorted part IDs of `names`, or `None` when the version has no map
    /// or the map does not name one of them.
    pub(crate) async fn part_ids_of<'a, I>(
        &self,
        datastore: &NamespaceView,
        names: I,
    ) -> Result<Option<Vec<u32>>>
    where
        I: IntoIterator<Item = &'a str>,
    {
        Ok(self.part_ids(datastore).await?.and_then(|ids| {
            let mut parts = names
                .into_iter()
                .map(|name| ids.get(name).copied())
                .collect::<Option<Vec<u32>>>()?;
            parts.sort_unstable();
            parts.dedup();
            Some(parts)
        }))
    }
}
