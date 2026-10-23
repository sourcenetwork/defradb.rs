//! The write path: the mutators that produce document deltas.
pub mod autocommit;
pub(crate) mod create;
pub mod doc;
pub(crate) mod prepare;
pub(crate) mod update;

/// The transaction's views a document write goes through.
pub(crate) struct WriteStores<'a> {
    pub datastore: &'a datastore::NamespaceView,
    pub systemstore: &'a datastore::NamespaceView,
    pub blockstore: &'a datastore::NamespaceView,
    pub headstore: &'a datastore::NamespaceView,
}
