//! Replicated documents a receiving node registers with a remote ACP
//! provider (`proofs/tla/ReplicaRegistration.tla`): a creator on another
//! deployment, whose document nobody would register, and the creator's later
//! changes, merged through the full ACP merge handler under strict access.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::replica_support::{is_retry, key, Chain, Embedder, Receiver, Source, POLICY, RESOURCE};
use defra_core::merge::MergeOutcome;

fn embedder(own: Vec<identity::Did>, nodes: Vec<identity::Did>) -> Arc<Embedder> {
    Arc::new(Embedder { own, nodes })
}

#[test]
fn a_foreign_creators_row_merges_once_the_receiver_registers_it_and_its_changes_follow() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let receiver = Receiver::open(&chain, &node, Some(embedder(vec![], vec![]))).await;
        assert_ne!(
            receiver.identity, node,
            "the node's identity is not its provider's actor"
        );
        let source = Source::open().await;
        let creator = key();

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        receiver.settle(&doc).await.unwrap();
        assert_eq!(
            chain.owner(&doc),
            Some(node.clone()),
            "the receiver registers as its actor"
        );
        assert!(chain.holds(&doc, "writer", &creator.did));
        assert_eq!(
            receiver.owed(&doc).await,
            acp::ReplicaOwed::default(),
            "the marker goes"
        );

        let update = source.update(&creator, &doc, "Alicia").await;
        assert_eq!(
            receiver.receive(&source, &doc, &update).await,
            MergeOutcome::Merged
        );
        let delete = source.delete(&creator, &doc).await;
        assert_eq!(
            receiver.receive(&source, &doc, &delete).await,
            MergeOutcome::Merged
        );
        assert_eq!(
            chain.owner(&doc),
            Some(node),
            "a merged delete never unregisters"
        );
        assert!(receiver
            .acp()
            .is_doc_registered(POLICY, RESOURCE, &doc)
            .await
            .unwrap());
    })
    .unwrap()
}

#[test]
fn the_creators_update_while_the_unit_is_owed_is_put_back_and_then_merges() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let receiver = Receiver::open(&chain, &node, Some(embedder(vec![], vec![]))).await;
        let source = Source::open().await;
        let creator = key();
        chain.hold_registrations.store(true, Ordering::SeqCst);

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert_eq!(chain.owner(&doc), None);
        let owed = receiver.owed(&doc).await;
        assert_eq!(owed.registration, Some(node.clone()));
        assert_eq!(owed.creator, Some(creator.did.clone()));
        assert!(owed.creator_grants);

        let update = source.update(&creator, &doc, "Alicia").await;
        let outcome = receiver.receive(&source, &doc, &update).await;
        assert!(
            is_retry(&outcome),
            "a change while the unit is owed waits: {outcome:?}"
        );

        // The registration lands; the creator's write grant does not yet.
        chain.hold("writer");
        chain.hold_registrations.store(false, Ordering::SeqCst);
        let outcome = receiver.receive(&source, &doc, &update).await;
        assert!(
            is_retry(&outcome),
            "the grants are part of the unit: {outcome:?}"
        );
        assert_eq!(chain.owner(&doc), Some(node));
        assert!(receiver.owed(&doc).await.creator_grants);

        chain.release("writer");
        let mut outcome = receiver.receive(&source, &doc, &update).await;
        if is_retry(&outcome) {
            outcome = receiver.receive(&source, &doc, &update).await;
        }
        assert_eq!(outcome, MergeOutcome::Merged);
    })
    .unwrap()
}

#[test]
fn another_signers_change_waits_while_the_receivers_registration_is_owed() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let receiver = Receiver::open(&chain, &node, Some(embedder(vec![], vec![]))).await;
        let source = Source::open().await;
        let (creator, writer) = (key(), key());
        chain.hold_registrations.store(true, Ordering::SeqCst);

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        let update = source.update(&writer, &doc, "Mallory").await;
        let outcome = receiver.receive(&source, &doc, &update).await;
        assert!(
            is_retry(&outcome),
            "the node's stand-in decides nobody: {outcome:?}"
        );
    })
    .unwrap()
}

#[test]
fn a_same_deployment_creators_row_is_never_registered_by_the_receiver() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let creator = key();
        let receiver = Receiver::open(
            &chain,
            &node,
            Some(embedder(vec![creator.did.clone()], vec![])),
        )
        .await;
        let source = Source::open().await;

        let (doc, genesis) = source.create(&creator, "Alice").await;
        let outcome = receiver.receive(&source, &doc, &genesis).await;
        assert!(
            is_retry(&outcome),
            "it waits for its creator's own registration: {outcome:?}"
        );
        assert_eq!(chain.owner(&doc), None);
        assert_eq!(receiver.owed(&doc).await, acp::ReplicaOwed::default());

        // The creator's own node registers it; the replica then merges.
        chain
            .as_node(&creator.did)
            .register_doc_object(&creator.did, POLICY, RESOURCE, &doc)
            .await
            .unwrap();
        chain.grant(&doc, "reader", &node);
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert_eq!(chain.owner(&doc), Some(creator.did));
    })
    .unwrap()
}

#[test]
fn an_unlisted_collection_waits_for_a_registration_as_before() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let receiver = Receiver::open(&chain, &node, None).await;
        let source = Source::open().await;
        let creator = key();

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::retryable_skip(
                "replicated protected document is not yet registered in local ACP"
            )
        );
        assert_eq!(chain.owner(&doc), None);
        assert_eq!(receiver.owed(&doc).await, acp::ReplicaOwed::default());
    })
    .unwrap()
}

#[test]
fn an_update_reaching_the_receiver_before_its_genesis_registers_through_the_genesis() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let receiver = Receiver::open(&chain, &node, Some(embedder(vec![], vec![]))).await;
        let source = Source::open().await;
        let creator = key();

        let (doc, _) = source.create(&creator, "Alice").await;
        let update = source.update(&creator, &doc, "Alicia").await;
        let outcome = receiver.receive(&source, &doc, &update).await;
        assert!(
            is_retry(&outcome),
            "the update waits for its genesis' unit: {outcome:?}"
        );
        receiver.settle(&doc).await.unwrap();
        assert_eq!(
            chain.owner(&doc),
            Some(node),
            "the genesis merged and registered"
        );
        assert_eq!(
            receiver.receive(&source, &doc, &update).await,
            MergeOutcome::Merged
        );
    })
    .unwrap()
}

/// The refusal parks the change rather than rejecting it for good
/// (`proofs/tla/ParkedMerge.tla`; `parked_refusals.rs` covers the re-drive).
#[test]
fn a_revoked_writers_later_update_is_parked() {
    workers::block_on(async {
        let chain = Chain::new();
        let node = key().did;
        let receiver = Receiver::open(&chain, &node, Some(embedder(vec![], vec![]))).await;
        let source = Source::open().await;
        let (creator, writer) = (key(), key());

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        receiver.settle(&doc).await.unwrap();
        chain.grant(&doc, "writer", &writer.did);
        let first = source.update(&writer, &doc, "Bob").await;
        assert_eq!(
            receiver.receive(&source, &doc, &first).await,
            MergeOutcome::Merged
        );

        chain.revoke(&doc, "writer", &writer.did);
        let second = source.update(&writer, &doc, "Bobby").await;
        let outcome = receiver.receive(&source, &doc, &second).await;
        assert!(crate::replica_support::is_parked(&outcome), "{outcome:?}");
    })
    .unwrap()
}

/// Two receivers of one deployment both register the same foreign document.
/// The provider takes the first; the second's grants, which it cannot give,
/// are discarded with a note, and it waits for the owner's: the creator's
/// update there is put back, never refused, until the owner's grant lands.
#[test]
fn a_second_receiver_waits_for_the_owners_grants_and_never_refuses_the_creator() {
    workers::block_on(async {
        let chain = Chain::new();
        let (first_actor, second_actor) = (key().did, key().did);
        let nodes = vec![first_actor.clone(), second_actor.clone()];
        let first =
            Receiver::open(&chain, &first_actor, Some(embedder(vec![], nodes.clone()))).await;
        let second = Receiver::open(&chain, &second_actor, Some(embedder(vec![], nodes))).await;
        let source = Source::open().await;
        let creator = key();
        chain.hold_registrations.store(true, Ordering::SeqCst);
        chain.hold("writer");

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            first.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert_eq!(
            second.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );

        chain.hold_registrations.store(false, Ordering::SeqCst);
        // The creator's write grant cannot land yet: the settlement says so.
        assert!(first.settle(&doc).await.is_err());
        second.settle(&doc).await.unwrap();
        assert_eq!(chain.owner(&doc), Some(first_actor.clone()));
        let owed = second.owed(&doc).await;
        assert!(
            owed.adopted && owed.registration.is_none() && !owed.creator_grants,
            "{owed:?}"
        );
        let discarded = second
            .acp()
            .owed_entries(acp::outbox::OwedFilter::Actor(&second_actor))
            .await
            .unwrap();
        assert!(
            discarded
                .iter()
                .any(|entry| format!("{entry:?}").contains("another actor holds")),
            "{discarded:?}"
        );

        let update = source.update(&creator, &doc, "Alicia").await;
        let outcome = second.receive(&source, &doc, &update).await;
        assert!(
            is_retry(&outcome),
            "the creator waits for the owner's grant: {outcome:?}"
        );

        chain.release("writer");
        first.settle(&doc).await.unwrap();
        assert!(chain.holds(&doc, "writer", &creator.did));
        assert_eq!(
            second.receive(&source, &doc, &update).await,
            MergeOutcome::Merged
        );
        second.settle(&doc).await.unwrap();
        assert_eq!(second.owed(&doc).await, acp::ReplicaOwed::default());
    })
    .unwrap()
}

/// A receiver that sees the genesis only after another registered the
/// document, while the creator's write grant is still owed there, waits for
/// it too.
#[test]
fn a_receiver_that_sees_the_genesis_after_another_registered_it_waits_for_the_owners_grant() {
    workers::block_on(async {
        let chain = Chain::new();
        let (first_actor, second_actor) = (key().did, key().did);
        let nodes = vec![first_actor.clone(), second_actor.clone()];
        let first =
            Receiver::open(&chain, &first_actor, Some(embedder(vec![], nodes.clone()))).await;
        let second = Receiver::open(&chain, &second_actor, Some(embedder(vec![], nodes))).await;
        let source = Source::open().await;
        let creator = key();
        chain.hold("writer");

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            first.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert!(first.settle(&doc).await.is_err(), "the write grant is held");
        assert!(chain.holds(&doc, "reader", &second_actor));

        assert_eq!(
            second.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert!(second.owed(&doc).await.adopted);
        let update = source.update(&creator, &doc, "Alicia").await;
        assert!(is_retry(&second.receive(&source, &doc, &update).await));

        chain.release("writer");
        first.settle(&doc).await.unwrap();
        assert_eq!(
            second.receive(&source, &doc, &update).await,
            MergeOutcome::Merged
        );
    })
    .unwrap()
}

/// With a local provider a listed collection behaves as before: the
/// replicated genesis registers its signer inside the merge.
#[test]
fn a_local_provider_registers_the_genesis_signer_as_before() {
    workers::block_on(async {
        let receiver = Receiver::open_with(
            |store| {
                Arc::new(acp::ZanzibarDocumentACP::new(Arc::new(
                    acp::PersistentZanzibarStore::from_store(store),
                )))
            },
            Some(embedder(vec![], vec![])),
            false,
        )
        .await;
        let source = Source::open().await;
        let creator = key();

        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        assert_eq!(
            receiver
                .acp()
                .get_doc_owner(POLICY, RESOURCE, &doc)
                .await
                .unwrap(),
            Some(creator.did)
        );
        assert_eq!(receiver.owed(&doc).await, acp::ReplicaOwed::default());
    })
    .unwrap()
}

#[test]
fn signed_genesis_registers_only_under_a_parent_that_admits_its_signer() {
    workers::block_on(async {
        use zanzibar::ZanzibarStore as _;

        let mut collection = crate::replica_support::users();
        collection.policy.as_mut().unwrap().parent_field = Some("name".into());
        let receiver = Receiver::open_collection(collection, |store| {
            Arc::new(acp::ZanzibarDocumentACP::new(Arc::new(
                acp::PersistentZanzibarStore::from_store(store),
            )))
        }, None, false).await;
        let store = acp::PersistentZanzibarStore::from_store(receiver.db.store().clone());
        let yaml = "name: admissions\nresources:\n- name: nodes\n- name: users\n  relations: [{name: parent, types: [nodes]}]\n  create: {parent: parent, requires: owner}\n";
        let mut policy = acp::policy_yaml::build_policy(&acp::policy_yaml::parse_policy_yaml(yaml).unwrap(), 1).unwrap();
        policy.id = POLICY.into();
        store.store_policy(&policy).await.unwrap();
        let creator = key();
        let provider = receiver.acp();
        provider.register_doc_object(&creator.did, POLICY, "nodes", "allowed").await.unwrap();
        provider.register_doc_object(&receiver.identity, POLICY, "nodes", "other").await.unwrap();

        let source = Source::open().await;
        let (rejected, block) = source.create(&creator, "other").await;
        let error = receiver.try_receive(&source, &rejected, &block).await.unwrap_err();
        assert!(error.to_string().contains("parent requires 'owner'"), "{error}");
        assert!(!provider.is_doc_registered(POLICY, RESOURCE, &rejected).await.unwrap());

        let (accepted, block) = source.create(&creator, "allowed").await;
        assert_eq!(receiver.receive(&source, &accepted, &block).await, MergeOutcome::Merged);
        assert_eq!(provider.get_doc_owner(POLICY, RESOURCE, &accepted).await.unwrap(), Some(creator.did));
        assert!(store.has_relationship(POLICY, RESOURCE, &accepted, "parent", &zanzibar::Subject::entity_set("nodes", "allowed", "")).await.unwrap());
    }).unwrap()
}
