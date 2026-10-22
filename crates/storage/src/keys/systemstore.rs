/// Systemstore keys for metadata and configuration
///
/// These keys are prefixed with 's' at the store level and handle:
/// - Collection metadata
/// - Field metadata
/// - Sequence counters
/// - P2P tracking
/// - Access control policies
use crate::corekv::Key;
use defra_core::Action;

/// Systemstore key holding the format the indexes on disk were written with, a
/// four-byte big-endian version. Every store a database has opened holds it.
pub const INDEX_FORMAT_KEY: &[u8] = b"/system/index_format";

/// Systemstore key the one-time at-rest rewrite writes into the store it
/// produced, once that store has been checked against the one it replaces.
/// Its value is the number of entries copied, eight bytes big-endian.
pub const AT_REST_REWRITE_KEY: &[u8] = b"/system/at_rest_rewrite";

const ACTION_STATUS_PREFIX: &str = "/a/s";
const ACTION_REASON_PREFIX: &str = "/a/r";
const ACTION_PROGRESS_PREFIX: &str = "/a/p";

fn action_key(prefix: &str, collection_id: &str, action: Action, subject: &str) -> Vec<u8> {
    if subject.is_empty() {
        format!("{prefix}/{collection_id}/{}", action.value()).into_bytes()
    } else {
        format!("{prefix}/{collection_id}/{}/{subject}", action.value()).into_bytes()
    }
}

/// Current status of a long-running action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionStatusKey {
    pub collection_id: String,
    pub action: Action,
    pub subject: String,
}

impl ActionStatusKey {
    pub fn new(collection_id: impl Into<String>, action: Action) -> Self {
        Self {
            collection_id: collection_id.into(),
            action,
            subject: String::new(),
        }
    }

    pub fn with_subject(
        collection_id: impl Into<String>,
        action: Action,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            collection_id: collection_id.into(),
            action,
            subject: subject.into(),
        }
    }

    pub fn prefix() -> Vec<u8> {
        format!("{ACTION_STATUS_PREFIX}/").into_bytes()
    }

    pub fn collection_prefix(collection_id: &str) -> Vec<u8> {
        format!("{ACTION_STATUS_PREFIX}/{collection_id}/").into_bytes()
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let value = std::str::from_utf8(bytes)
            .ok()?
            .strip_prefix(&format!("{ACTION_STATUS_PREFIX}/"))?;
        let mut parts = value.split('/');
        let collection_id = parts.next()?.to_string();
        let action = Action::new(parts.next()?.parse().ok()?);
        let subject = parts.next().unwrap_or_default().to_string();
        if collection_id.is_empty() || parts.next().is_some() {
            return None;
        }
        Some(Self {
            collection_id,
            action,
            subject,
        })
    }
}

impl Key for ActionStatusKey {
    fn bytes(&self) -> Vec<u8> {
        action_key(
            ACTION_STATUS_PREFIX,
            &self.collection_id,
            self.action,
            &self.subject,
        )
    }
}

/// Error reason retained for an action that did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionReasonKey {
    collection_id: String,
    action: Action,
    subject: String,
}

impl ActionReasonKey {
    pub fn new(
        collection_id: impl Into<String>,
        action: Action,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            collection_id: collection_id.into(),
            action,
            subject: subject.into(),
        }
    }
}

impl Key for ActionReasonKey {
    fn bytes(&self) -> Vec<u8> {
        action_key(
            ACTION_REASON_PREFIX,
            &self.collection_id,
            self.action,
            &self.subject,
        )
    }
}

/// How far an action that runs in batches has got, so a restart resumes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionProgressKey {
    collection_id: String,
    action: Action,
    subject: String,
}

impl ActionProgressKey {
    pub fn new(
        collection_id: impl Into<String>,
        action: Action,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            collection_id: collection_id.into(),
            action,
            subject: subject.into(),
        }
    }
}

impl Key for ActionProgressKey {
    fn bytes(&self) -> Vec<u8> {
        action_key(
            ACTION_PROGRESS_PREFIX,
            &self.collection_id,
            self.action,
            &self.subject,
        )
    }
}

/// CollectionKey: Maps collection ID to full collection definition (JSON)
///
/// Structure: /collection/id/[CollectionID]
/// Example: /collection/id/user_profiles_v1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionKey {
    /// Full collection identifier
    pub collection_id: String,
}

impl CollectionKey {
    /// Create a new CollectionKey
    pub fn new(collection_id: impl Into<String>) -> Self {
        Self {
            collection_id: collection_id.into(),
        }
    }

    /// Create a prefix for all collection keys
    pub fn collection_prefix() -> Vec<u8> {
        b"/collection/id/".to_vec()
    }
}

impl Key for CollectionKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/collection/id/{}", self.collection_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/collection/id/{}", self.collection_id)
    }
}

/// CollectionID: Maps full collection ID to short ID (reverse index)
///
/// Structure: /collection/shortID/[CollectionID]
/// Example: /collection/shortID/user_profiles_v1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionID {
    /// Full collection identifier
    pub collection_id: String,
}

impl CollectionID {
    /// Create a new CollectionID key
    pub fn new(collection_id: impl Into<String>) -> Self {
        Self {
            collection_id: collection_id.into(),
        }
    }

    /// Create a prefix for all collection short ID mappings
    pub fn short_id_prefix() -> Vec<u8> {
        b"/collection/shortID/".to_vec()
    }
}

impl Key for CollectionID {
    fn bytes(&self) -> Vec<u8> {
        format!("/collection/shortID/{}", self.collection_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/collection/shortID/{}", self.collection_id)
    }
}

/// CollectionNameKey: Maps collection name to collection ID
///
/// Structure: /collection/name/[CollectionName]
/// Example: /collection/name/users
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionNameKey {
    /// Collection name
    pub name: String,
}

impl CollectionNameKey {
    /// Create a new CollectionNameKey
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Create a prefix for all collection name mappings
    pub fn name_prefix() -> Vec<u8> {
        b"/collection/name/".to_vec()
    }
}

impl Key for CollectionNameKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/collection/name/{}", self.name).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/collection/name/{}", self.name)
    }
}

/// CollectionVersionKey: Tracks all versions of a collection
///
/// Structure: /collection/version/[CollectionID]/[VersionID]
/// Example: /collection/version/user_profiles/v1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionVersionKey {
    /// Full collection identifier
    pub collection_id: String,
    /// Version identifier
    pub version_id: String,
}

impl CollectionVersionKey {
    /// Create a new CollectionVersionKey
    pub fn new(collection_id: impl Into<String>, version_id: impl Into<String>) -> Self {
        Self {
            collection_id: collection_id.into(),
            version_id: version_id.into(),
        }
    }

    /// Create a prefix for all collection versions
    pub fn version_prefix() -> Vec<u8> {
        b"/collection/version/".to_vec()
    }

    /// Create a prefix for a specific collection's versions
    pub fn collection_prefix(collection_id: impl Into<String>) -> Vec<u8> {
        let collection_id = collection_id.into();
        format!("/collection/version/{}/", collection_id).into_bytes()
    }
}

impl Key for CollectionVersionKey {
    fn bytes(&self) -> Vec<u8> {
        format!(
            "/collection/version/{}/{}",
            self.collection_id, self.version_id
        )
        .into_bytes()
    }

    fn to_string(&self) -> String {
        format!(
            "/collection/version/{}/{}",
            self.collection_id, self.version_id
        )
    }
}

/// FieldID: Maps full field ID to short field ID (reverse index)
///
/// Structure: /field/shortID/[CollectionShortID]/[FieldID]
/// Example: /field/shortID/1/user_email
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldID {
    /// Collection short ID (decimal format)
    pub collection_short_id: u32,
    /// Full field identifier
    pub field_id: String,
}

impl FieldID {
    /// Create a new FieldID key
    pub fn new(collection_short_id: u32, field_id: impl Into<String>) -> Self {
        Self {
            collection_short_id,
            field_id: field_id.into(),
        }
    }

    /// Create a prefix for all field short ID mappings
    pub fn short_id_prefix() -> Vec<u8> {
        b"/field/shortID/".to_vec()
    }

    /// Create a prefix for a specific collection's field mappings
    pub fn collection_prefix(collection_short_id: u32) -> Vec<u8> {
        format!("/field/shortID/{}/", collection_short_id).into_bytes()
    }
}

impl Key for FieldID {
    fn bytes(&self) -> Vec<u8> {
        format!(
            "/field/shortID/{}/{}",
            self.collection_short_id, self.field_id
        )
        .into_bytes()
    }

    fn to_string(&self) -> String {
        format!(
            "/field/shortID/{}/{}",
            self.collection_short_id, self.field_id
        )
    }
}

/// NodeACPKey: Stores node-level Access Control Policy
///
/// Structure: nac (singleton key)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeACPKey;

impl NodeACPKey {
    /// Create the singleton NodeACPKey
    pub fn new() -> Self {
        Self
    }
}

impl Default for NodeACPKey {
    fn default() -> Self {
        Self::new()
    }
}

impl Key for NodeACPKey {
    fn bytes(&self) -> Vec<u8> {
        b"nac".to_vec()
    }

    fn to_string(&self) -> String {
        "nac".to_string()
    }
}

/// P2PCollectionKey: Tracks P2P replication metadata for collections
///
/// Structure: /p2p/collection/[CollectionID]
/// Example: /p2p/collection/users
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2PCollectionKey {
    /// Collection identifier
    pub collection_id: String,
}

impl P2PCollectionKey {
    /// Create a new P2PCollectionKey
    pub fn new(collection_id: impl Into<String>) -> Self {
        Self {
            collection_id: collection_id.into(),
        }
    }

    /// Create a prefix for all P2P collection metadata
    pub fn p2p_collection_prefix() -> Vec<u8> {
        b"/p2p/collection/".to_vec()
    }
}

impl Key for P2PCollectionKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/p2p/collection/{}", self.collection_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/p2p/collection/{}", self.collection_id)
    }
}

/// P2PDocumentKey: Tracks P2P replication metadata for documents
///
/// Structure: /p2p/document/[DocID]
/// Example: /p2p/document/bae123456789abcdef0123456789abcdef012345
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2PDocumentKey {
    /// Document identifier
    pub doc_id: String,
}

impl P2PDocumentKey {
    /// Create a new P2PDocumentKey
    pub fn new(doc_id: impl Into<String>) -> Self {
        Self {
            doc_id: doc_id.into(),
        }
    }

    /// Create a prefix for all P2P document metadata
    pub fn p2p_document_prefix() -> Vec<u8> {
        b"/p2p/document/".to_vec()
    }
}

impl Key for P2PDocumentKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/p2p/document/{}", self.doc_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/p2p/document/{}", self.doc_id)
    }
}

/// P2PPendingDagKey: Persisted pending-DAG registration awaiting missing links
///
/// Structure: /p2p/pending_dag/[RootCID]
/// Example: /p2p/pending_dag/bafyreib2rxk3rybk3aobmv5cjuql3bm2twh4jo5uxgf5kpqrsgxbyqx54
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2PPendingDagKey {
    /// Root CID of the pending DAG (canonical string form)
    pub root_cid: String,
}

impl P2PPendingDagKey {
    /// Create a new P2PPendingDagKey
    pub fn new(root_cid: impl Into<String>) -> Self {
        Self {
            root_cid: root_cid.into(),
        }
    }

    /// Create a prefix for all persisted pending-DAG registrations
    pub fn p2p_pending_dag_prefix() -> Vec<u8> {
        b"/p2p/pending_dag/".to_vec()
    }
}

impl Key for P2PPendingDagKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/p2p/pending_dag/{}", self.root_cid).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/p2p/pending_dag/{}", self.root_cid)
    }
}

/// P2PQuarantinedDagKey: Terminally-rejected pending-DAG record, retained for forensics
///
/// Deliberately a distinct prefix from `/p2p/pending_dag/` (not a subpath of it):
/// the live-record resync sweep prefix-scans `/p2p/pending_dag/` via `load_all` and
/// must never observe a quarantined root, or it would re-drive a merge that is
/// known to fail deterministically on every replay.
///
/// Structure: /p2p/quarantined_dag/[RootCID]
/// Example: /p2p/quarantined_dag/bafyreib2rxk3rybk3aobmv5cjuql3bm2twh4jo5uxgf5kpqrsgxbyqx54
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2PQuarantinedDagKey {
    /// Root CID of the quarantined DAG (canonical string form)
    pub root_cid: String,
}

impl P2PQuarantinedDagKey {
    /// Create a new P2PQuarantinedDagKey
    pub fn new(root_cid: impl Into<String>) -> Self {
        Self {
            root_cid: root_cid.into(),
        }
    }

    /// Create a prefix for all quarantined pending-DAG records
    pub fn p2p_quarantined_dag_prefix() -> Vec<u8> {
        b"/p2p/quarantined_dag/".to_vec()
    }
}

impl Key for P2PQuarantinedDagKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/p2p/quarantined_dag/{}", self.root_cid).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/p2p/quarantined_dag/{}", self.root_cid)
    }
}

/// StoredCrdtTypeKey: a custom CRDT type some document part in this store was
/// written by. A node opens the store only if it registers every such type.
///
/// Structure: /crdt/stored/[CrdtName]
/// Example: /crdt/stored/maxreg
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCrdtTypeKey {
    /// The custom CRDT type's name.
    pub crdt: String,
}

impl StoredCrdtTypeKey {
    pub const PREFIX: &'static str = "/crdt/stored/";

    pub fn new(crdt: impl Into<String>) -> Self {
        Self { crdt: crdt.into() }
    }
}

impl Key for StoredCrdtTypeKey {
    fn bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }

    fn to_string(&self) -> String {
        format!("{}{}", Self::PREFIX, self.crdt)
    }
}

/// ParkedCrdtBlockKey: a field block of a custom CRDT type this node does not
/// have, kept until a build of the node registers the type and merges it. One
/// key per collection, document, field and block, so parking is a blind put;
/// the block's priority, fixed-width, orders a field's blocks oldest first.
/// A parked block that cannot merge moves under [`Self::FAILED_PREFIX`], with
/// the rest of its key unchanged.
///
/// Structure: /crdt/parked/[CrdtName]/[CollectionID]/[DocShortID]/[Field]/[Priority]/[CID]
/// Example: /crdt/parked/maxreg/col-1/7/best/00000000000000000003/bafyreib2rxk3rybk3aobmv5cjuql3bm2twh4jo5uxgf5kpqrsgxbyqx54
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParkedCrdtBlockKey {
    /// The custom CRDT type the block was written by.
    pub crdt: String,
    /// The collection the document belongs to.
    pub collection: String,
    /// The document's node-local short ID.
    pub doc_short_id: u64,
    /// The field the block writes.
    pub field: String,
    /// The block's priority (its height in the field's DAG).
    pub priority: u64,
    /// CID of the field block (canonical string form).
    pub cid: String,
}

impl ParkedCrdtBlockKey {
    pub const PREFIX: &'static str = "/crdt/parked/";
    pub const FAILED_PREFIX: &'static str = "/crdt/failed/";

    pub fn new(
        crdt: impl Into<String>,
        collection: impl Into<String>,
        doc_short_id: u64,
        field: impl Into<String>,
        priority: u64,
        cid: impl Into<String>,
    ) -> Self {
        Self {
            crdt: crdt.into(),
            collection: collection.into(),
            doc_short_id,
            field: field.into(),
            priority,
            cid: cid.into(),
        }
    }

    /// The prefix of every parked block of the type `crdt`.
    pub fn crdt_prefix(crdt: &str) -> Vec<u8> {
        format!("{}{crdt}/", Self::PREFIX).into_bytes()
    }

    /// Where the parked block at `key` moves when it cannot merge.
    pub fn failed(key: &[u8]) -> Option<Vec<u8>> {
        let rest = key.strip_prefix(Self::PREFIX.as_bytes())?;
        Some([Self::FAILED_PREFIX.as_bytes(), rest].concat())
    }
}

impl Key for ParkedCrdtBlockKey {
    fn bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }

    fn to_string(&self) -> String {
        format!(
            "{}{}/{}/{}/{}/{:020}/{}",
            Self::PREFIX,
            self.crdt,
            self.collection,
            self.doc_short_id,
            self.field,
            self.priority,
            self.cid
        )
    }
}

/// LensConfigKey: Stores a serialized LensConfig for persistence across restarts.
///
/// Structure: /lens/config/[TransformID]
/// Example: /lens/config/bafe98e8334bf1605a4ddd88cc11a20b330
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LensConfigKey {
    pub transform_id: String,
}

impl LensConfigKey {
    pub fn new(transform_id: impl Into<String>) -> Self {
        Self {
            transform_id: transform_id.into(),
        }
    }

    pub fn prefix() -> Vec<u8> {
        b"/lens/config/".to_vec()
    }
}

impl Key for LensConfigKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/lens/config/{}", self.transform_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/lens/config/{}", self.transform_id)
    }
}

/// CollectionIDSequenceKey: Monotonic sequence counter for generating collection IDs
///
/// Structure: /seq/collection (singleton key with value as counter)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionIDSequenceKey;

impl CollectionIDSequenceKey {
    /// Create the singleton CollectionIDSequenceKey
    pub fn new() -> Self {
        Self
    }
}

impl Default for CollectionIDSequenceKey {
    fn default() -> Self {
        Self::new()
    }
}

impl Key for CollectionIDSequenceKey {
    fn bytes(&self) -> Vec<u8> {
        b"/seq/collection".to_vec()
    }

    fn to_string(&self) -> String {
        "/seq/collection".to_string()
    }
}

/// FieldIDSequenceKey: Monotonic sequence counter for field IDs (per collection)
///
/// Structure: /seq/field/[CollectionShortID]
/// Example: /seq/field/1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIDSequenceKey {
    /// Collection short ID (decimal format)
    pub collection_short_id: u32,
}

impl FieldIDSequenceKey {
    /// Create a new FieldIDSequenceKey
    pub fn new(collection_short_id: u32) -> Self {
        Self {
            collection_short_id,
        }
    }

    /// Create a prefix for all field sequence keys
    pub fn sequence_prefix() -> Vec<u8> {
        b"/seq/field/".to_vec()
    }
}

impl Key for FieldIDSequenceKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/seq/field/{}", self.collection_short_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/seq/field/{}", self.collection_short_id)
    }
}

/// IndexIDSequenceKey: Monotonic sequence counter for index IDs (per collection version)
///
/// Structure: /seq/index/[CollectionID]
/// Example: /seq/index/user_profiles_v1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexIDSequenceKey {
    /// Full collection identifier
    pub collection_id: String,
}

impl IndexIDSequenceKey {
    /// Create a new IndexIDSequenceKey
    pub fn new(collection_id: impl Into<String>) -> Self {
        Self {
            collection_id: collection_id.into(),
        }
    }

    /// Create a prefix for all index sequence keys
    pub fn sequence_prefix() -> Vec<u8> {
        b"/seq/index/".to_vec()
    }
}

impl Key for IndexIDSequenceKey {
    fn bytes(&self) -> Vec<u8> {
        format!("/seq/index/{}", self.collection_id).into_bytes()
    }

    fn to_string(&self) -> String {
        format!("/seq/index/{}", self.collection_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_status_key_round_trips_collection_and_subject_actions() {
        let collection = ActionStatusKey::new("collection", Action::TRUNCATE);
        assert_eq!(collection.bytes(), b"/a/s/collection/1");
        assert_eq!(
            ActionStatusKey::parse(&collection.bytes()),
            Some(collection)
        );

        let subject =
            ActionStatusKey::with_subject("collection", Action::BACKFILL_INDEX, "index_name");
        assert_eq!(subject.bytes(), b"/a/s/collection/3/index_name");
        assert_eq!(ActionStatusKey::parse(&subject.bytes()), Some(subject));
    }

    #[test]
    fn action_progress_key_sits_beside_the_status_and_reason() {
        let progress = ActionProgressKey::new("collection", Action::BACKFILL_INDEX, "7");
        assert_eq!(progress.bytes(), b"/a/p/collection/3/7");
        assert_eq!(
            ActionReasonKey::new("collection", Action::BACKFILL_INDEX, "7").bytes(),
            b"/a/r/collection/3/7"
        );
    }

    #[test]
    fn test_collection_key() {
        let key = CollectionKey::new("user_profiles_v1");
        assert_eq!(key.to_string(), "/collection/id/user_profiles_v1");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_collection_id() {
        let key = CollectionID::new("user_profiles_v1");
        assert_eq!(key.to_string(), "/collection/shortID/user_profiles_v1");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_collection_name_key() {
        let key = CollectionNameKey::new("users");
        assert_eq!(key.to_string(), "/collection/name/users");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_collection_version_key() {
        let key = CollectionVersionKey::new("user_profiles", "v1");
        assert_eq!(key.to_string(), "/collection/version/user_profiles/v1");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_field_id() {
        let key = FieldID::new(1, "user_email");
        assert_eq!(key.to_string(), "/field/shortID/1/user_email");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_node_acp_key() {
        let key = NodeACPKey::new();
        assert_eq!(key.to_string(), "nac");
        assert_eq!(key.bytes(), b"nac");
    }

    #[test]
    fn test_p2p_collection_key() {
        let key = P2PCollectionKey::new("users");
        assert_eq!(key.to_string(), "/p2p/collection/users");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_p2p_document_key() {
        let key = P2PDocumentKey::new("bae123456789abcdef0123456789abcdef012345");
        assert_eq!(
            key.to_string(),
            "/p2p/document/bae123456789abcdef0123456789abcdef012345"
        );
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_p2p_quarantined_dag_key() {
        let key =
            P2PQuarantinedDagKey::new("bafyreib2rxk3rybk3aobmv5cjuql3bm2twh4jo5uxgf5kpqrsgxbyqx54");
        assert_eq!(
            key.to_string(),
            "/p2p/quarantined_dag/bafyreib2rxk3rybk3aobmv5cjuql3bm2twh4jo5uxgf5kpqrsgxbyqx54"
        );
        assert_eq!(key.bytes(), key.to_string().as_bytes());
        assert_eq!(
            P2PQuarantinedDagKey::p2p_quarantined_dag_prefix(),
            b"/p2p/quarantined_dag/".to_vec()
        );
    }

    #[test]
    fn test_collection_id_sequence_key() {
        let key = CollectionIDSequenceKey::new();
        assert_eq!(key.to_string(), "/seq/collection");
        assert_eq!(key.bytes(), b"/seq/collection");
    }

    #[test]
    fn test_field_id_sequence_key() {
        let key = FieldIDSequenceKey::new(1);
        assert_eq!(key.to_string(), "/seq/field/1");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }

    #[test]
    fn test_index_id_sequence_key() {
        let key = IndexIDSequenceKey::new("user_profiles_v1");
        assert_eq!(key.to_string(), "/seq/index/user_profiles_v1");
        assert_eq!(key.bytes(), key.to_string().as_bytes());
    }
}
