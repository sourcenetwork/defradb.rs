//! A collection that declares a governance root and also carries `@policy`,
//! on a node whose merge validator did not claim it by name. ACP takes
//! precedence: the collection is not judged, and a replicated document goes
//! through the ACP merge hook exactly as in a collection with no root, its
//! registration wait and its replica registration included.

use std::sync::atomic::{AtomicUsize, Ordering};

use defra_core::merge::MergeOutcome;

use super::{
    async_trait, Arc, MergeCandidate, MergeGovernance, MergeValidator, MergeVerdict, MergeView,
};
use crate::replica_support::{key, Chain, Embedder, Receiver, Source, POLICY, RESOURCE};

/// `Users` under a governance root, with the policy the receivers share.
const ROOTED_USERS_SDL: &str = r#"type Users @governed(root: "root-a") @policy(id: "replica-policy", resource: "users") {
    name: String
}"#;

/// Accepts every composite it is asked about, and counts the asking.
#[derive(Default)]
struct Counting {
    asked: AtomicUsize,
}

#[async_trait]
impl MergeValidator for Counting {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(MergeVerdict::Accept)
    }
}

/// A receiver under strict access on the shared provider `chain`, holding the
/// rooted, policied `Users`, with a merge validator that claims no name.
async fn receiver(
    chain: &Arc<Chain>,
    actor: &identity::Did,
    replicas: Option<Arc<Embedder>>,
) -> (Receiver, Arc<Counting>) {
    let acp = chain.as_node(actor);
    let replicas = replicas.map(|embedder| embedder as Arc<dyn db::merge::ReplicaOwnership>);
    let receiver = Receiver::open_collection(ROOTED_USERS_SDL, move |_| acp, replicas, true).await;
    let validator = Arc::new(Counting::default());
    receiver.db.set_merge_governance(
        MergeGovernance::new(Vec::<String>::new()).with_validator(validator.clone()),
    );
    (receiver, validator)
}

/// The ACP gate still holds an unregistered protected document: a validator
/// that would accept it is not what decides.
#[test]
fn a_rooted_policied_document_waits_for_its_registration() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let (receiver, validator) = receiver(&chain, &node, None).await;
        let source = Source::with_sdl(ROOTED_USERS_SDL).await;
        let creator = key();

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::retryable_skip(
                "replicated protected document is not yet registered in local ACP"
            )
        );
        assert_eq!(chain.owner(&doc), None);

        // The creator's own node registers it; the replica then merges.
        chain
            .as_node(&creator.did)
            .register_doc_object(&creator.did, POLICY, RESOURCE, &doc, &[])
            .await
            .unwrap();
        chain.grant(&doc, "reader", &node);
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert_eq!(chain.owner(&doc), Some(creator.did));
        assert_eq!(
            validator.asked.load(Ordering::SeqCst),
            0,
            "the validator did not claim the collection"
        );
    })
    .unwrap()
}

/// Replica registration still runs: a foreign creator's document in a listed
/// collection is registered by the receiver as its provider's actor.
#[test]
fn a_rooted_policied_replica_is_still_registered_by_its_receiver() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let embedder = Arc::new(Embedder {
            own: vec![],
            nodes: vec![],
        });
        let (receiver, validator) = receiver(&chain, &node, Some(embedder)).await;
        let source = Source::with_sdl(ROOTED_USERS_SDL).await;
        let creator = key();

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        receiver.settle(&doc).await.unwrap();
        assert_eq!(
            chain.owner(&doc),
            Some(node),
            "the receiver registers as its actor"
        );
        assert!(chain.holds(&doc, "writer", &creator.did));
        assert_eq!(receiver.owed(&doc).await, acp::ReplicaOwed::default());
        assert_eq!(
            validator.asked.load(Ordering::SeqCst),
            0,
            "the validator did not claim the collection"
        );
    })
    .unwrap()
}
