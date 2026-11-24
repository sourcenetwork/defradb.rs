//! Which replicated documents a receiving node registers with a remote ACP
//! provider, as its embedder configures it.
//!
//! With a remote provider (Vera) a replicated document merges only once the
//! provider holds it, and a document created on another deployment is held
//! by nobody there. For a collection listed here the receiving node registers
//! such a document as its provider's own actor, with the grants the embedder
//! asks for its creator, before the genesis merges (`acp::replica`). A
//! creator that is one of this deployment's own actors is never pre-empted:
//! its document waits for the creator's own registration. Nothing here
//! applies to an unlisted collection or to a local provider.
//!
//! The model is `proofs/tla/ReplicaRegistration.tla`.

use std::sync::Arc;

use acp::ReplicaGrant;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;
use rapidhash::RapidHashSet;
use schema::CollectionVersion;

/// The embedder's answers about a replicated document's creator.
pub trait ReplicaOwnership: MaybeSendSync {
    /// Whether `actor` is one of this deployment's own actors: a document it
    /// created waits for its own registration and is never registered by a
    /// receiving node.
    fn is_own_actor(&self, collection: &str, actor: &Did) -> bool;

    /// The relationships a receiving node grants on a document it registers.
    /// They must give `creator` the `update` and `delete` permissions of the
    /// collection's policy (for example `writer` for the creator), or the
    /// creator's later changes are refused; read for this deployment's nodes,
    /// when other nodes are to merge the document too, belongs here as well.
    fn creator_grants(&self, collection: &str, creator: &Did, doc_id: &str) -> Vec<ReplicaGrant>;
}

/// The collections whose replicated documents a receiving node registers,
/// and the embedder's answers for them. Off unless installed.
#[derive(Clone)]
pub struct ReplicaRegistration {
    collections: RapidHashSet<String>,
    ownership: Arc<dyn ReplicaOwnership>,
}

impl ReplicaRegistration {
    /// Register replicated documents of the collections named in
    /// `collections`, answered by `ownership`.
    pub fn new(
        collections: impl IntoIterator<Item = impl Into<String>>,
        ownership: Arc<dyn ReplicaOwnership>,
    ) -> Self {
        Self {
            collections: collections.into_iter().map(Into::into).collect(),
            ownership,
        }
    }

    /// Whether `collection` is listed.
    pub fn lists(&self, collection: &CollectionVersion) -> bool {
        self.collections.contains(&collection.name)
    }

    pub fn ownership(&self) -> &Arc<dyn ReplicaOwnership> {
        &self.ownership
    }
}

/// What the merge does about a frame of a listed collection, decided before
/// its transaction opens and carried into it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ReplicaStep {
    /// The usual gates decide.
    #[default]
    Gate,
    /// The genesis of a foreign creator's document nobody holds: it merges,
    /// and its transaction records the unit the node owes for it.
    Register {
        creator: Did,
        registrant: Did,
        grants: Vec<ReplicaGrant>,
    },
    /// The genesis of a foreign creator's document another actor holds while
    /// the creator cannot write yet: the usual gates decide, and its
    /// transaction records that the node waits for the owner's grants.
    Adopt { creator: Did },
    /// A later frame of a document nobody holds yet: the walk goes on to its
    /// genesis, and the frame itself is put back to be tried again.
    AfterGenesis,
}
