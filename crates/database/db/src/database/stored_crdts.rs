//! The custom CRDT types a store holds values of. A node without one of them
//! could not fold or keep those values, so it refuses to open the store.

use crdt::stored::ValueKind;
use datastore::NamespaceView;
use schema::CrdtName;
use storage::corekv::{IterOptions, Key, Store};
use storage::keys::StoredCrdtTypeKey;
use storage::namespace::Namespace;

use crate::error::{Error, Result};
use crate::DB;

/// The custom types among `kinds`, the part kinds a write stores.
pub(crate) fn custom<I>(kinds: I) -> Vec<CrdtName>
where
    I: IntoIterator<Item = ValueKind>,
{
    kinds
        .into_iter()
        .filter_map(|kind| match kind {
            ValueKind::Custom(crdt) => Some(crdt),
            _ => None,
        })
        .collect()
}

/// Record, by blind puts in the writing transaction, the custom types a
/// write stores values of.
pub(crate) async fn record(
    datastore: &NamespaceView,
    crdts: Vec<CrdtName>,
) -> std::result::Result<(), storage::corekv::Error> {
    if crdts.is_empty() {
        return Ok(());
    }
    let systemstore = datastore.sibling(Namespace::Systemstore);
    for crdt in crdts {
        systemstore
            .set(&StoredCrdtTypeKey::new(crdt.as_str()).bytes(), &[])
            .await?;
    }
    Ok(())
}

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Fail when the store holds values of a custom type this node lacks.
    pub(crate) async fn check_stored_crdts(&self) -> Result<()> {
        let txn = self.new_txn(true).await?;
        let systemstore = txn.systemstore()?;
        let prefix = StoredCrdtTypeKey::PREFIX.as_bytes().to_vec();
        let mut iter = systemstore
            .iterator(IterOptions::new().with_prefix(prefix.clone()))
            .await?;
        let mut missing = None;
        while let Some(pair) = iter.next().await? {
            let crdt = String::from_utf8_lossy(&pair.key[prefix.len()..]).into_owned();
            let registered = schema::CrdtName::new(&crdt)
                .ok()
                .is_some_and(|name| self.crdts().registered(name).is_some());
            if !registered {
                missing = Some(crdt);
                break;
            }
        }
        iter.close().await?;
        drop(systemstore);
        let _ = txn.discard();
        match missing {
            Some(crdt) => Err(Error::UnregisteredStoredCrdt { crdt }),
            None => Ok(()),
        }
    }
}
