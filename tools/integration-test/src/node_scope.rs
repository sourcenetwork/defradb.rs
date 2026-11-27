//! The scope an in-process node call acts in: the caller, the key it signs
//! with and the explicit transaction it runs in, as a scenario action names
//! them.

use defra_core::signing::SigningConfig;
use defra_core::TxnId;
use defra_node::{Node, ScopedNode, TxnNode};

/// Who an in-process node call acts as, the key it signs with, and the
/// transaction it runs in.
#[derive(Clone)]
pub struct NodeScope {
    pub did: Option<::identity::Did>,
    pub bearer: Option<String>,
    pub signing: Option<SigningConfig>,
    pub txn: Option<TxnId>,
}

impl NodeScope {
    /// `node` acting as the scope's caller, outside any transaction: for the
    /// operations that run in none.
    pub fn scoped<'n>(&self, node: &'n Node) -> ScopedNode<'n> {
        let scoped = match &self.did {
            Some(did) => node.with_identity(did.clone()),
            None => node.anonymous(),
        };
        scoped
            .with_bearer(self.bearer.clone())
            .with_signing(self.signing.clone())
    }

    /// `node` acting as the scope's caller, in its transaction when it names
    /// one.
    pub fn on<'n>(&self, node: &'n Node) -> Scoped<'n> {
        let scoped = self.scoped(node);
        match self.txn {
            Some(id) => Scoped::Txn(scoped.with_transaction(id)),
            None => Scoped::Node(scoped),
        }
    }
}

/// A node scoped to an action's caller, in the action's transaction or in
/// none.
pub enum Scoped<'n> {
    Node(ScopedNode<'n>),
    Txn(TxnNode<'n>),
}

/// Forward each listed operation to whichever scope this is.
macro_rules! dispatch {
    ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty;)*) => {$(
        pub async fn $name(&self, $($arg: $ty),*) -> $ret {
            match self {
                Self::Node(scoped) => scoped.$name($($arg),*).await,
                Self::Txn(scoped) => scoped.$name($($arg),*).await,
            }
        }
    )*};
}

impl Scoped<'_> {
    dispatch! {
        fn execute_request(request: query::QueryRequest) -> query::QueryResponse;
        fn add_collection(sdl: &str) -> defra_node::Result<Vec<schema::CollectionVersion>>;
        fn add_view(
            source_query: &str,
            target_sdl: &str,
            transform: Option<&str>,
        ) -> defra_node::Result<Vec<schema::CollectionVersion>>;
        fn refresh_views(selector: schema::CollectionSelector) -> defra_node::Result<()>;
        fn patch_collection(
            patch: &str,
            migration: Option<&serde_json::Value>,
        ) -> defra_node::Result<()>;
        fn set_active_collection_version(version_id: &str) -> defra_node::Result<()>;
        fn truncate_collection(
            name: &str,
            filter: Option<&query::documents::Filter>,
        ) -> defra_node::Result<()>;
        fn set_migration(config: &serde_json::Value) -> defra_node::Result<String>;
        fn add_lens(lens: &serde_json::Value) -> defra_node::Result<String>;
        fn list_lenses() -> defra_node::Result<serde_json::Map<String, serde_json::Value>>;
        fn get_collections(
            selector: &schema::CollectionSelector,
        ) -> defra_node::Result<Vec<schema::CollectionVersion>>;
        fn add_docs(
            collection: &str,
            docs: &[serde_json::Value],
            options: &query::documents::AddOptions,
        ) -> defra_node::Result<Vec<String>>;
        fn update_doc(
            collection: &str,
            doc_id: &str,
            patch: &serde_json::Value,
        ) -> defra_node::Result<()>;
        fn save_doc(
            collection: &str,
            doc_id: Option<&str>,
            doc: &serde_json::Value,
            options: &query::documents::AddOptions,
        ) -> defra_node::Result<String>;
        fn save_docs(
            collection: &str,
            docs: &[serde_json::Value],
            options: &query::documents::AddOptions,
        ) -> defra_node::Result<Vec<String>>;
        fn delete_doc(collection: &str, doc_id: &str) -> defra_node::Result<()>;
        fn update_with_filter(
            collection: &str,
            filter: &query::documents::Filter,
            updater: &str,
        ) -> defra_node::Result<query::documents::DocumentsResult>;
        fn delete_with_filter(
            collection: &str,
            filter: &query::documents::Filter,
        ) -> defra_node::Result<query::documents::DocumentsResult>;
        fn create_index(
            collection: &str,
            name: Option<&str>,
            fields: Vec<schema::IndexedFieldDescription>,
            unique: bool,
            vector: Option<schema::VectorIndexDescription>,
        ) -> defra_node::Result<schema::IndexDescription>;
        fn start_index(
            collection: &str,
            name: Option<&str>,
            fields: Vec<schema::IndexedFieldDescription>,
            unique: bool,
            vector: Option<schema::VectorIndexDescription>,
        ) -> defra_node::Result<schema::IndexDescription>;
        fn list_indexes(collection: &str) -> defra_node::Result<Vec<defra_node::IndexListing>>;
        fn delete_index(collection: &str, name: &str) -> defra_node::Result<()>;
        fn add_encrypted_index(
            collection: &str,
            field_name: &str,
        ) -> defra_node::Result<defra_node::EncryptedIndexListing>;
        fn list_encrypted_indexes(
            collection: &str,
        ) -> defra_node::Result<Vec<defra_node::EncryptedIndexListing>>;
        fn list_all_encrypted_indexes(
        ) -> defra_node::Result<Vec<(String, Vec<defra_node::EncryptedIndexListing>)>>;
        fn delete_encrypted_index(collection: &str, field_name: &str) -> defra_node::Result<()>;
        fn verify_block_signature(
            cid: &str,
            public_key: &str,
            key_type: crypto::KeyType,
        ) -> defra_node::Result<()>;
    }
}
