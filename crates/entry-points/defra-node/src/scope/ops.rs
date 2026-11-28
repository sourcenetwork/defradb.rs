//! The operations every scope offers, listed once. Each scope type takes the
//! lists that fit it and forwards each operation to the node's core with the
//! context it curries (`forward`): [`Node`](crate::Node) and
//! [`ScopedNode`](super::ScopedNode) take both, [`TxnNode`](super::TxnNode)
//! only the operations that run in a transaction.

/// Forward each listed operation to the core, in the context the scope's
/// `in_scope` builds.
macro_rules! forward {
    ($(
        $(#[$meta:meta])*
        fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty;
    )*) => {$(
        $(#[$meta])*
        pub async fn $name(&self, $($arg: $ty),*) -> $ret {
            let (core, context) = self.in_scope();
            core.$name(&context, $($arg),*).await
        }
    )*};
}

/// Pass the operations that run in an explicit transaction, the one the
/// scope names if it names one, to `$forward`.
macro_rules! transactional_ops {
    ($forward:ident) => {
        $forward! {
            /// Add the collections the GraphQL SDL `sdl` defines. With no
            /// identity named, or outside a transaction, it acts as the node,
            /// as Go's in-process client does.
            fn add_collection(sdl: &str) -> crate::Result<Vec<schema::CollectionVersion>>;

            /// Create the views `target_sdl` defines over `source_query`
            /// (`"SourceType { field1 field2 ... }"`), their documents run
            /// through the stored lens `transform` when it names one; the
            /// view collections created. In a transaction, a type the
            /// transaction created is known to `target_sdl`. Go's `AddView`.
            fn add_view(
                source_query: &str,
                target_sdl: &str,
                transform: Option<&str>,
            ) -> crate::Result<Vec<schema::CollectionVersion>>;

            /// Refresh the materialized views `selector` names, every one
            /// when it names none. Go's `RefreshViews`.
            fn refresh_views(selector: db::CollectionSelector) -> crate::Result<()>;

            /// Apply the JSON patch `patch` to the collection definitions,
            /// with `migration`, a lens (Go's `model.Lens`), as the migration
            /// to the versions it adds. Go's `PatchCollection`.
            fn patch_collection(
                patch: &str,
                migration: Option<&serde_json::Value>,
            ) -> crate::Result<()>;

            /// Make collection version `version_id` its collection's active
            /// one. Go's `SetActiveCollectionVersion`.
            fn set_active_collection_version(version_id: &str) -> crate::Result<()>;

            /// Remove the documents of collection `name` that `filter`
            /// matches, or all of them without one, keeping the collection.
            /// Go's `Collection.Truncate`.
            fn truncate_collection(
                name: &str,
                filter: Option<&query::documents::Filter>,
            ) -> crate::Result<()>;

            /// Set the lens migration `config` (Go's `LensConfig`) between
            /// two collection versions and return its lens ID. Go's
            /// `SetMigration`; without the `wasmtime-runtime` feature it is
            /// refused.
            fn set_migration(config: &serde_json::Value) -> crate::Result<String>;

            /// Store `lens` (Go's `model.Lens`) and return its content ID.
            /// Go's `AddLens`.
            fn add_lens(lens: &serde_json::Value) -> crate::Result<String>;

            /// Every stored lens, by content ID. Go's `ListLenses`.
            fn list_lenses() -> crate::Result<serde_json::Map<String, serde_json::Value>>;

            /// The collection versions `selector` picks, ordered by name then
            /// version ID. Go's `GetCollections`.
            fn get_collections(
                selector: &schema::CollectionSelector,
            ) -> crate::Result<Vec<schema::CollectionVersion>>;

            /// Add `docs` to `collection`, all or none, and return their IDs
            /// in order. Go's `AddDocument` and `AddManyDocuments`.
            fn add_docs(
                collection: &str,
                docs: &[serde_json::Value],
                options: &query::documents::AddOptions,
            ) -> crate::Result<Vec<String>>;

            /// Set the fields `patch` names on document `doc_id`. Go's
            /// `UpdateDocument`.
            fn update_doc(
                collection: &str,
                doc_id: &str,
                patch: &serde_json::Value,
            ) -> crate::Result<()>;

            /// Refuse document `doc_id` when it has been deleted, else write
            /// `doc` as it when it exists, else add it, and return its ID.
            /// Go's `SaveDocument`: one transaction, the scope's when it
            /// names one.
            fn save_doc(
                collection: &str,
                doc_id: Option<&str>,
                doc: &serde_json::Value,
                options: &query::documents::AddOptions,
            ) -> crate::Result<String>;

            /// Save every document of `docs` in one transaction, each updated
            /// when its `_docID` names a stored document and added otherwise,
            /// and return their IDs in order.
            fn save_docs(
                collection: &str,
                docs: &[serde_json::Value],
                options: &query::documents::AddOptions,
            ) -> crate::Result<Vec<String>>;

            /// Delete document `doc_id`. Go's `DeleteDocument`.
            fn delete_doc(collection: &str, doc_id: &str) -> crate::Result<()>;

            /// Apply `updater`, a JSON object of fields, to every document
            /// `filter` matches. Go's `UpdateDocumentsWithFilter`.
            fn update_with_filter(
                collection: &str,
                filter: &query::documents::Filter,
                updater: &str,
            ) -> crate::Result<query::documents::DocumentsResult>;

            /// Delete every document `filter` matches. Go's
            /// `DeleteDocumentsWithFilter`.
            fn delete_with_filter(
                collection: &str,
                filter: &query::documents::Filter,
            ) -> crate::Result<query::documents::DocumentsResult>;

            /// Create an index named `name` (generated when `None`) over
            /// `fields` of `collection`, `unique` or not, and index the
            /// documents it already holds: a vector index when `vector`
            /// describes one, an ordered one otherwise. Go's
            /// `Collection.CreateIndex`.
            fn create_index(
                collection: &str,
                name: Option<&str>,
                fields: Vec<schema::IndexedFieldDescription>,
                unique: bool,
                vector: Option<schema::VectorIndexDescription>,
                fulltext: Option<schema::FullTextIndexDescription>,
            ) -> crate::Result<schema::IndexDescription>;

            /// Create an index as `create_index` does, returning once it is
            /// defined: it is listed as building until its backfill
            /// completes. Go's `Collection.NewIndex`.
            fn start_index(
                collection: &str,
                name: Option<&str>,
                fields: Vec<schema::IndexedFieldDescription>,
                unique: bool,
                vector: Option<schema::VectorIndexDescription>,
                fulltext: Option<schema::FullTextIndexDescription>,
            ) -> crate::Result<schema::IndexDescription>;

            /// The indexes of `collection`, each with the state of the action
            /// that builds it. Go's `Collection.GetIndexes`.
            fn list_indexes(collection: &str) -> crate::Result<Vec<crate::IndexListing>>;

            /// Delete index `name` of `collection`. Go's `Collection.DropIndex`.
            fn delete_index(collection: &str, name: &str) -> crate::Result<()>;

            /// Add an equality encrypted (searchable-encryption) index on
            /// `field_name` of `collection`. Go's `AddEncryptedIndex`.
            fn add_encrypted_index(
                collection: &str,
                field_name: &str,
            ) -> crate::Result<crate::EncryptedIndexListing>;

            /// The encrypted indexes of `collection`. Go's
            /// `ListEncryptedIndexes`.
            fn list_encrypted_indexes(
                collection: &str,
            ) -> crate::Result<Vec<crate::EncryptedIndexListing>>;

            /// The encrypted indexes of every collection, by collection name.
            /// Go's `ListAllEncryptedIndexes`.
            fn list_all_encrypted_indexes(
            ) -> crate::Result<Vec<(String, Vec<crate::EncryptedIndexListing>)>>;

            /// Delete the encrypted index on `field_name` of `collection`.
            /// Go's `DeleteEncryptedIndex`.
            fn delete_encrypted_index(collection: &str, field_name: &str) -> crate::Result<()>;

            /// Check that block `cid` was signed by `public_key` (hex, of
            /// `key_type`). Go's `VerifySignature`.
            fn verify_block_signature(
                cid: &str,
                public_key: &str,
                key_type: crypto::KeyType,
            ) -> crate::Result<()>;

            /// Cryptographically verify block `cid` and return its signer
            /// DID. Unsigned, malformed, missing, or invalid blocks fail
            /// closed. In a transaction it sees the transaction's uncommitted
            /// blocks, read as the node.
            fn verified_block_signer_did(cid: &str) -> crate::Result<String>;

            /// Give `target` `relation` on document `doc_id`; whether it
            /// existed already. Go's `AddDACActorRelationship`.
            /// An anonymous caller is refused by node and document access
            /// control, with Go's errors.
            fn add_dac_actor_relationship(
                collection: &str,
                doc_id: &str,
                relation: &str,
                target: &str,
            ) -> crate::Result<bool>;

            /// Take `relation` on document `doc_id` from `target`; whether it
            /// was there. Go's `DeleteDACActorRelationship`.
            /// An anonymous caller is refused by node and document access
            /// control, with Go's errors.
            fn delete_dac_actor_relationship(
                collection: &str,
                doc_id: &str,
                relation: &str,
                target: &str,
            ) -> crate::Result<bool>;

            /// Add, or `delete`, `relation` on document `doc_id` for
            /// `target`, and say what the change did.
            /// An anonymous caller is refused by node and document access
            /// control, with Go's errors.
            fn change_dac_actor_relationship(
                collection: &str,
                doc_id: &str,
                relation: &str,
                target: &str,
                delete: bool,
            ) -> crate::Result<crate::RelationshipChange>;

            /// The relationships of document `doc_id`. An anonymous caller is
            /// refused with "identity required for document ACP", as
            /// `GET /api/v0/acp/document/relationships` refuses one.
            fn list_dac_relationships(
                collection: &str,
                doc_id: &str,
            ) -> crate::Result<Vec<acp::DocumentRelationship>>;

            /// Whether `target` holds `relation` on document `doc_id`. An
            /// anonymous caller is refused with "identity required for
            /// document ACP".
            fn observe_dac_actor_relationship(
                collection: &str,
                doc_id: &str,
                relation: &str,
                target: &str,
            ) -> crate::Result<bool>;
        }
    };
}

/// Pass the operations that run outside any transaction to `$forward`.
macro_rules! standalone_ops {
    ($forward:ident) => {
        $forward! {
            /// Eagerly migrate and cache every known-version document in a
            /// collection; the number of documents advanced to the active
            /// version. A datastore-only operation that creates and
            /// broadcasts no commits.
            fn materialize_collection(collection_name: &str) -> crate::Result<usize>;

            /// Add the YAML `policy` with the scope's identity as its creator,
            /// and return its ID. Go's `AddDACPolicy`: without an identity
            /// there is no creator. With a `nonce` the ID is the one the
            /// remote ACP (vera.rs) derives from the definition, the creator
            /// and the nonce, so sending it again returns the same policy; a
            /// node on local ACP refuses a nonce.
            fn add_dac_policy(
                policy: &str,
                nonce: Option<acp::PolicyNonce>,
            ) -> crate::Result<String>;

            /// Add the YAML `policy` with the local policy ID built from
            /// `counter`, so peers that join a policy-bound collection
            /// reproduce the creator's ID. Local ACP only.
            fn add_dac_policy_with_counter(policy: &str, counter: u64) -> crate::Result<String>;

            /// Every DAC policy the node holds.
            fn list_dac_policies() -> crate::Result<Vec<acp::Policy>>;

            /// The DAC policy `policy_id`; `None` when no ACP has it.
            fn get_dac_policy(policy_id: &str) -> crate::Result<Option<acp::Policy>>;

            /// The remote ACP's light client status.
            fn acp_light_client_status() -> crate::Result<acp::LightClientStatus>;

            /// The certified assignments `query` selects.
            fn certified_assignments(
                query: acp::AssignmentQuery,
            ) -> crate::Result<acp::CertifiedAssignments>;

            /// The node's NAC status. Go's `GetNACStatus`.
            fn nac_status() -> crate::Result<acp::nac::NacStatus>;

            /// Every way `subject` (a DID) holds each node permission: owner,
            /// admin, a direct grant, or the shortest chain of sources.
            fn explain_nac_subject(
                subject: &str,
            ) -> crate::Result<Vec<acp::nac::PermissionProvenance>>;

            /// Turn NAC off, keeping its relationships. Go's `DisableNAC`.
            fn disable_nac() -> crate::Result<()>;

            /// Turn a disabled NAC back on. Go's `ReEnableNAC`.
            fn re_enable_nac() -> crate::Result<()>;

            /// Give `target_actor` (a DID, or `*`) `relation` on the node;
            /// whether it existed already. Go's `AddNACActorRelationship`.
            fn add_nac_actor_relationship(
                relation: &str,
                target_actor: &str,
            ) -> crate::Result<bool>;

            /// Take `relation` on the node from `target_actor`; whether it
            /// was there. Go's `DeleteNACActorRelationship`.
            fn delete_nac_actor_relationship(
                relation: &str,
                target_actor: &str,
            ) -> crate::Result<bool>;

            /// Every action the node runs or has run, such as index builds.
            fn list_actions() -> crate::Result<Vec<defra_core::ActionExecution>>;

            /// The documents of `collections` (every collection when empty)
            /// as the JSON a backup file holds. Go's `BasicExport`.
            fn export_backup(collections: &[String], pretty: bool) -> crate::Result<String>;

            /// Import the documents of backup `data`; they get new document
            /// IDs, as Go's import gives them. One admitted request, run to
            /// its end if its caller stops waiting. Go's `BasicImport`.
            fn import_backup(data: &str) -> crate::Result<()>;

            /// What `actor` still owes the remote document ACP provider, for
            /// that actor or the node's principal.
            fn owed_acp(actor: &str) -> crate::Result<Vec<acp::OwedEntry>>;

            /// Wait, for at most `deadline`, until what the committed
            /// transaction `txn` owes the provider is settled or has come to
            /// rest, and say which.
            fn await_owed_acp(
                txn: defra_core::TxnId,
                deadline: std::time::Duration,
            ) -> crate::Result<acp::OwedWait>;

            /// Hand in a fresh `credential` for the owed `entry`, and say what
            /// the provider made of it.
            fn reauthorize_owed_acp(
                entry: &acp::OwedRef,
                credential: crate::SignerCredential,
            ) -> crate::Result<acp::OwedOutcome>;

            /// Abandon the refused owed change `seq` the scope's identity
            /// staged.
            fn abandon_owed_acp_change(seq: u64) -> crate::Result<()>;

            /// Load canonical signed-block material after applying document
            /// ACP for the scope's identity. The caller must independently
            /// verify both CIDs and the detached signature before trusting
            /// the returned signer.
            fn authorized_signed_block_bytes(cid: &str) -> crate::Result<(Vec<u8>, Vec<u8>)>;

            /// Restore the shared transport collection `sdl` defines on the
            /// device `selection` proves, acting as that device.
            fn restore_shared_transport_collection(
                sdl: &str,
                selection: acp::TransportSentinelQuery,
            ) -> crate::Result<Vec<schema::CollectionVersion>>;

            /// The node's P2P addresses; none without P2P.
            #[cfg(feature = "managed-p2p")]
            fn peer_info() -> crate::Result<Vec<String>>;

            /// The peers the node is connected to.
            #[cfg(feature = "managed-p2p")]
            fn active_peers() -> crate::Result<Vec<String>>;

            /// Connect to the peers `addresses` name.
            #[cfg(feature = "managed-p2p")]
            fn connect(addresses: &[String]) -> crate::Result<()>;

            /// Disconnect from the peers `addresses` name.
            #[cfg(feature = "managed-p2p")]
            fn disconnect(addresses: &[String]) -> crate::Result<()>;

            /// Replicate `collections` to the peer `addresses` name, through
            /// `filters`. Go's `SetReplicator`.
            #[cfg(feature = "managed-p2p")]
            fn add_replicator(
                addresses: &[String],
                collections: &[String],
                filters: crate::ReplicationFilters,
            ) -> crate::Result<()>;

            /// Stop replicating `collections` (all of them when empty) to peer
            /// `id`. Go's `DeleteReplicator`.
            #[cfg(feature = "managed-p2p")]
            fn delete_replicator(id: &str, collections: &[String]) -> crate::Result<()>;

            /// The node's replicators. Go's `GetAllReplicators`.
            #[cfg(feature = "managed-p2p")]
            fn list_replicators() -> crate::Result<Vec<defra_p2p_adapter::peer::Replicator>>;

            /// Subscribe to the collections `names` over P2P.
            #[cfg(feature = "managed-p2p")]
            fn add_p2p_collections(names: &[String]) -> crate::Result<()>;

            /// Unsubscribe from the collections `names`.
            #[cfg(feature = "managed-p2p")]
            fn delete_p2p_collections(names: &[String]) -> crate::Result<()>;

            /// The collections the node subscribes to over P2P.
            #[cfg(feature = "managed-p2p")]
            fn list_p2p_collections() -> crate::Result<Vec<String>>;

            /// Subscribe to the documents `doc_ids` over P2P.
            #[cfg(feature = "managed-p2p")]
            fn add_p2p_documents(doc_ids: &[String]) -> crate::Result<()>;

            /// Unsubscribe from the documents `doc_ids`.
            #[cfg(feature = "managed-p2p")]
            fn delete_p2p_documents(doc_ids: &[String]) -> crate::Result<()>;

            /// The documents the node subscribes to over P2P.
            #[cfg(feature = "managed-p2p")]
            fn list_p2p_documents() -> crate::Result<Vec<String>>;

            /// Fetch documents `doc_ids` of `collection` from peers.
            #[cfg(feature = "managed-p2p")]
            fn sync_documents(collection: &str, doc_ids: &[String]) -> crate::Result<()>;

            /// Fetch collection versions `version_ids` from peers.
            #[cfg(feature = "managed-p2p")]
            fn sync_collection_versions(version_ids: &[String]) -> crate::Result<()>;

            /// Fetch the branchable collection `collection_id` from peers.
            #[cfg(feature = "managed-p2p")]
            fn sync_branchable_collection(collection_id: &str) -> crate::Result<()>;
        }
    };
}
