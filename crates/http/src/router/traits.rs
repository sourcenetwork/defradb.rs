//! Operation traits and supporting types for the HTTP router.

/// The `bind_request` an operations trait provides: the handle that serves
/// one request (see `crate::request_scope`).
macro_rules! bind_request {
    ($operations:ident) => {
        /// This handle bound to the request `context` serves, for an
        /// implementation that reads it (the identity node access control
        /// checks, the creator of what it writes); `None` serves every request
        /// as it is.
        fn bind_request(
            &self,
            _context: &defra_core::RequestContext,
        ) -> Option<std::sync::Arc<dyn $operations>> {
            None
        }
    };
}

/// Trait for ACP (Access Control Policy) operations.
///
/// ACP policies define access permissions for collections and documents,
/// determining which identities can read, write, or manage data.
///
/// Policies should be provided in YAML or JSON format following the ACP
/// policy specification.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AcpOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(AcpOperations);

    /// Add a new policy. Returns the policy ID on success.
    ///
    /// The policy should be valid YAML or JSON. Returns an error string
    /// if the policy is malformed or cannot be added.
    async fn add_policy(&self, policy: &str) -> Result<String, String>;

    /// List all policies.
    async fn list_policies(&self) -> Result<Vec<PolicyInfo>, String>;

    /// Get a policy by ID.
    ///
    /// Returns `Ok(None)` if the policy doesn't exist, `Ok(Some(info))` if found,
    /// or `Err(message)` on internal errors.
    async fn get_policy(&self, id: &str) -> Result<Option<PolicyInfo>, String>;

    /// Validate a collection's `@policy(id:..., resource:...)` directive
    /// against the ACP store. Called at schema-add time.
    ///
    /// Matches Go's `acp.ValidateResourceInterface` in `acp/validation.go`:
    /// 1. The policy with `policy_id` must exist
    /// 2. The resource with `resource_name` must exist on that policy
    /// 3. The resource must declare the DPI-required `read`, `update`,
    ///    and `delete` permissions (Go's `RequiredResourcePermissionsForDocument`)
    ///
    /// Error messages must match Go's format so the behavior is indistinguishable
    /// from the caller's perspective.
    ///
    /// The default impl returns `Ok(())` (permissive) so backends that don't
    /// query a policy store — test mocks, Vera light-client placeholders —
    /// don't break. Production impls (`AcpAdapter`, `VeraAcpAdapter`)
    /// override with real validation.
    async fn validate_resource_interface(
        &self,
        _policy_id: &str,
        _resource_name: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Get ACP light client status when this ACP backend exposes one.
    async fn get_light_client_status(&self) -> Result<AcpLightClientStatus, String> {
        Err("ACP light client status is not available for this ACP backend".to_string())
    }
}

/// Policy information for HTTP responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_time: Option<String>,
}

/// ACP light client status for observability/debugging.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AcpLightClientStatus {
    pub height: u64,
    pub module_state_root: String,
    pub cache_entries: usize,
    pub last_invalidation_height: u64,
    pub connected: bool,
}

/// Trait for index operations.
///
/// Indexes improve query performance for specific fields. Creating unique
/// indexes also enforces uniqueness constraints on the indexed fields.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait IndexOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(IndexOperations);

    /// Create an index on a collection. Returns the created index info.
    ///
    /// `name` of `None` auto-generates one. `vector` makes this a vector index,
    /// in which case `unique` does not apply. Both are parameters rather than a
    /// defaulted second method so an implementor cannot quietly ignore the
    /// vector config and build an ordinary index over the vector field.
    async fn create_index(
        &self,
        collection: &str,
        fields: Vec<String>,
        name: Option<&str>,
        unique: bool,
        vector: Option<schema::VectorIndexDescription>,
    ) -> Result<IndexInfo, String>;

    /// List indexes, optionally filtered by collection.
    ///
    /// If `collection` is `None`, returns indexes from all collections.
    async fn list_indexes(&self, collection: Option<&str>) -> Result<Vec<IndexInfo>, String>;

    /// Delete an index by collection and name.
    async fn delete_index(&self, collection: &str, name: &str) -> Result<(), String>;
}

/// Index information for HTTP responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexInfo {
    #[serde(skip)]
    pub id: u32,
    pub name: String,
    pub collection: String,
    #[serde(skip)]
    pub collection_id: String,
    pub fields: Vec<IndexFieldInfo>,
    #[serde(default)]
    pub unique: bool,
    /// Kind-specific config, absent for an ordinary index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<schema::IndexKind>,
}

/// Index field information.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexFieldInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

/// Trait for encrypted index (searchable encryption) operations.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait EncryptedIndexOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(EncryptedIndexOperations);

    /// Add an encrypted index on a collection field.
    async fn add_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexInfo, String>;

    /// List encrypted indexes. If `collection` is `None`, returns indexes from all collections.
    async fn list_encrypted_indexes(
        &self,
        collection: Option<&str>,
    ) -> Result<Vec<EncryptedIndexInfo>, String>;

    /// Delete an encrypted index from a collection field.
    async fn delete_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<(), String>;
}

/// Encrypted index information for HTTP responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedIndexInfo {
    /// Collection name (used for grouping in list-all responses, not serialized in per-collection responses).
    #[serde(skip)]
    pub collection: String,
    #[serde(rename = "FieldName")]
    pub field_name: String,
    #[serde(rename = "Type")]
    pub index_type: String,
}

/// Trait for block operations.
///
/// Block operations provide signature verification capabilities for
/// DAG-CBOR blocks stored in the blockstore.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait BlockOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(BlockOperations);

    /// Return canonical bytes for an authorized signed block and its detached
    /// signature so clients can verify content addressing and authorship
    /// locally.
    async fn signed_block_bytes(
        &self,
        cid: &str,
        caller_did: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>), String>;

    /// Verify the signature of a block.
    ///
    /// Loads a block by CID, checks its signature, and verifies using the
    /// provided public key. `key_type` defaults to "secp256k1" if `None`.
    /// `caller_did` is the DID string of the caller for document-level ACP checks.
    async fn verify_signature(
        &self,
        cid: &str,
        public_key: &str,
        key_type: Option<&str>,
        caller_did: Option<&str>,
    ) -> Result<(), String>;
}

/// Result of a backup import operation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ImportResult {
    /// Number of documents successfully imported.
    pub documents_imported: u64,
    /// Number of documents skipped (e.g., duplicates).
    pub documents_skipped: u64,
    /// Collections that were affected by the import.
    pub collections_affected: Vec<String>,
    /// Errors encountered during import (non-fatal).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Re-export NAC types from the acp crate for convenience.
pub use acp::nac::{NacStatus, NodePermission};

/// Trait for Node Access Control (NAC) operations.
///
/// NAC provides node-level access control using the Zanzibar permission model.
/// When enabled, node operations require authentication and authorization.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait NodeAcpOperations: defra_core::thread_bounds::MaybeSendSync {
    /// Check if an identity has a specific node permission.
    ///
    /// Returns `true` if:
    /// - NAC is not enabled (all operations allowed)
    /// - The identity has the required permission
    async fn check_permission(
        &self,
        identity: &identity::Did,
        permission: NodePermission,
    ) -> Result<bool, String>;

    /// Get the current NAC status.
    async fn get_status(&self) -> NacStatus;

    /// Get the owner identity.
    async fn owner(&self) -> Option<identity::Did>;

    /// Check if an identity is an admin.
    async fn is_admin(&self, identity: &identity::Did) -> Result<bool, String>;

    /// Add an admin relationship.
    async fn add_admin(
        &self,
        requestor: &identity::Did,
        target: &identity::Did,
    ) -> Result<bool, String>;

    /// Remove an admin relationship.
    async fn remove_admin(
        &self,
        requestor: &identity::Did,
        target: &identity::Did,
    ) -> Result<bool, String>;

    /// Temporarily disable NAC on this node.
    ///
    /// The requestor must be an admin.
    async fn disable(&self, requestor: &identity::Did) -> Result<(), String>;

    /// Re-enable NAC after it was temporarily disabled.
    ///
    /// The requestor must be an admin (uses persisted check).
    async fn re_enable(&self, requestor: &identity::Did) -> Result<(), String>;

    /// Enable NAC with the given owner identity.
    ///
    /// Initializes NAC and sets the owner. Can only be called when NAC
    /// is not already configured.
    async fn enable(&self, owner: &identity::Did) -> Result<(), String>;

    /// Add a NAC relationship (admin or permission grant).
    ///
    /// Routes to the appropriate operation based on the relation name:
    /// - "admin" → add admin
    /// - valid permission name → add permission grant
    /// - "owner" or invalid → error
    async fn add_relationship(
        &self,
        requestor: &identity::Did,
        target: &identity::Did,
        relation: &str,
    ) -> Result<bool, String>;

    /// Remove a NAC relationship (admin or permission grant).
    ///
    /// Routes to the appropriate operation based on the relation name:
    /// - "admin" → remove admin
    /// - valid permission name → remove permission grant
    /// - "owner" or invalid → error
    async fn remove_relationship(
        &self,
        requestor: &identity::Did,
        target: &identity::Did,
        relation: &str,
    ) -> Result<bool, String>;

    /// Get full NAC status info including all FFI-compatible fields.
    async fn info(&self) -> NacStatusInfo;
}

/// NAC status information for HTTP responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NacStatusInfo {
    pub status: String,
    pub configured_enabled: bool,
    pub dev_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// Trait for schema operations.
///
/// Enables adding and managing collection schemas via SDL.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait CollectionOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(CollectionOperations);

    /// Add a schema from SDL string.
    ///
    /// Parses the SDL and creates collections for each type defined.
    /// Returns the created collection versions.
    async fn add_collection(&self, sdl: &str) -> Result<Vec<schema::CollectionVersion>, String>;
}

/// Trait for collection management operations beyond basic CRUD.
///
/// Provides action listing, purge, collection lookups and collection drops,
/// that operate at the collection level rather than the document level.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait CollectionManagementOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(CollectionManagementOperations);

    /// List actions that have not completed successfully.
    async fn list_actions(&self) -> Result<Vec<defra_core::ActionExecution>, String>;

    /// Purge all data from all collections.
    async fn purge(&self) -> Result<(), String>;

    /// Get a collection by name.
    async fn get_collection_by_name(
        &self,
        name: &str,
    ) -> Result<Option<schema::CollectionVersion>, String>;

    /// Check if a collection exists.
    async fn has_collection(&self, name: &str) -> Result<bool, String>;

    /// Find a collection by its collection ID.
    async fn find_collection_by_id(
        &self,
        collection_id: &str,
    ) -> Result<Option<schema::CollectionVersion>, String>;

    /// Get a collection by version ID, searching both cache and storage.
    async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<schema::CollectionVersion>, String>;

    /// Delete multiple collection versions in a batch.
    async fn delete_collection_versions(&self, version_ids: Vec<String>) -> Result<(), String>;

    /// Delete a collection by name.
    async fn delete_collection(&self, name: &str) -> Result<(), String>;

    /// Delete one or more collections by name (Go #4688 parity).
    ///
    /// When `active_only` is true, only the active head version of each named
    /// collection is removed. When false (Go's default), every version of each
    /// named collection is removed.
    async fn delete_collections(&self, names: Vec<String>, active_only: bool)
        -> Result<(), String>;
}

/// Trait for document-level ACP operations.
///
/// Manages per-document access control relationships (e.g., granting a user
/// read or write access to a specific document).
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait DocumentAcpOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(DocumentAcpOperations);

    /// Check whether an actor has a document permission.
    ///
    /// This is a read-only preview of the current document ACP decision and
    /// does not create or remove relationships.
    async fn check_doc_access(
        &self,
        actor: &identity::Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool, String>;

    /// Add an actor relationship to a document.
    ///
    /// Grants the `target_actor` the specified `relation` on the document
    /// identified by `collection` and `doc_id`.
    ///
    /// Returns `true` if a new relationship was created, `false` if it already existed.
    async fn add_doc_relationship(
        &self,
        requestor: &identity::Did,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String>;

    /// Delete an actor relationship from a document.
    ///
    /// Revokes the `target_actor`'s `relation` on the document
    /// identified by `collection` and `doc_id`.
    ///
    /// Returns `true` if the relationship was removed, `false` if it didn't exist.
    async fn delete_doc_relationship(
        &self,
        requestor: &identity::Did,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String>;
}

/// Trait for backup operations.
///
/// Enables exporting and importing database state as JSON. Export produces
/// a JSON representation of documents that can be reimported to restore state.
///
/// For export, the JSON includes document metadata and relationships.
/// For import, the JSON must match the expected structure with valid document
/// IDs and collection references.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait BackupOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(BackupOperations);

    /// Export database to JSON.
    ///
    /// If `collections` is `None`, exports all collections.
    /// If `pretty` is true, the JSON output is formatted with indentation.
    async fn export(
        &self,
        collections: Option<Vec<String>>,
        pretty: bool,
    ) -> Result<String, String>;

    /// Import database from JSON.
    ///
    /// The `data` parameter should be valid JSON matching the export format.
    /// Returns `ImportResult` with details about what was imported, skipped, and any errors.
    /// A fatal error (e.g., completely malformed data) returns `Err(message)`.
    async fn import(&self, data: &str) -> Result<ImportResult, String>;
}

/// Trait for view operations.
///
/// Views are virtual collections backed by a GQL query. Materialized views
/// cache their results and may also expose explicit maintenance operations.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ViewOperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(ViewOperations);

    /// Add a view from a GQL query and SDL schema.
    ///
    /// Returns the created collection versions for the view.
    async fn add_view(
        &self,
        gql_query: &str,
        sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<schema::CollectionVersion>, String>;

    /// Refresh materialized view caches.
    ///
    /// The options select which views to refresh, mirroring Go's collection
    /// lookup. Default options refresh every materialized view.
    async fn refresh_views(&self, options: db::CollectionSelector) -> Result<(), String>;

    /// Run manual downsample history GC.
    ///
    /// If `names` is provided, only those downsample targets are processed.
    /// Otherwise all downsample targets are processed.
    async fn gc_downsample_histories(&self, names: Option<Vec<String>>) -> Result<(), String>;
}

/// Trait for debug dump operations.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait DumpOperations: defra_core::thread_bounds::MaybeSendSync {
    /// Dump all database key/value pairs as a list of human-readable strings.
    async fn print_dump(&self) -> Result<Vec<String>, String>;
}
