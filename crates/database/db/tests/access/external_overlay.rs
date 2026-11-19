//! Changes one transaction stages on one object after a revocation it staged
//! there: the owed-delete overlay denies an actor whose authority the
//! revocation may take away, and only such an actor.

use std::sync::Arc;

use acp::{OwedOp, Subject, ZanzibarStore as _};
use query::QueryExecutor as _;
use zanzibar::types::{Policy, Relation, Resource};

use super::external_registration::{alice, bob, mallory, MANAGER, POLICY, RESOURCE};
use super::external_staging::{carol, Change, Staging};

/// A node whose external provider's policy it also keeps, as a node that
/// created or adopted it does: `manager` manages `reader`.
async fn staging() -> Staging {
    let staging = Staging::new().await;
    let mut manager = Relation::direct(MANAGER);
    manager.manages = vec!["reader".into()];
    let policy = Policy::new(POLICY, "remote").with_resource(
        Resource::new(RESOURCE)
            .with_relation(Relation::direct("owner"))
            .with_relation(Relation::direct("reader"))
            .with_relation(manager),
    );
    acp::PersistentZanzibarStore::from_store(Arc::clone(&staging.store))
        .store_policy(&policy)
        .await
        .unwrap();
    staging
}

fn wide(actor: identity::Did, doc: &str, op: OwedOp) -> Change {
    Change {
        other_subject: Some(Subject::Wildcard),
        ..Change::new(actor, doc, op, bob())
    }
}

fn manager_of(doc: &str, who: identity::Did, op: OwedOp) -> Change {
    let mut change = Change::new(alice(), doc, op, who);
    change.relation = MANAGER;
    change
}

#[test]
fn the_owner_changes_an_object_after_revoking_a_tuple_on_it() {
    workers::block_on(async {
        let staging = staging().await;
        let doc = staging.create(alice(), "shared", None).await;
        let handle = staging.begin().await;
        let results = staging
            .statement(
                &handle,
                vec![
                    Change::new(alice(), &doc, OwedOp::Delete, bob()),
                    Change::new(alice(), &doc, OwedOp::Add, carol()),
                    wide(alice(), &doc, OwedOp::Delete),
                    Change::new(alice(), &doc, OwedOp::Add, carol()),
                ],
                true,
            )
            .await;
        for result in &results {
            assert!(result.is_ok(), "{results:?}");
        }
        staging.commit(&handle).await;
    })
    .unwrap()
}

#[test]
fn a_manager_changes_an_object_after_revoking_a_tuple_its_authority_does_not_rest_on() {
    workers::block_on(async {
        let staging = staging().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), manager_of(&doc, mallory(), OwedOp::Add))
            .await;
        let handle = staging.begin().await;
        let results = staging
            .statement(
                &handle,
                vec![
                    Change::new(mallory(), &doc, OwedOp::Delete, bob()),
                    Change::new(mallory(), &doc, OwedOp::Add, carol()),
                    wide(mallory(), &doc, OwedOp::Delete),
                    Change::new(mallory(), &doc, OwedOp::Add, carol()),
                ],
                true,
            )
            .await;
        for result in &results {
            assert!(result.is_ok(), "{results:?}");
        }
        staging.commit(&handle).await;
    })
    .unwrap()
}

#[test]
fn a_manager_whose_managing_tuple_is_revoked_changes_nothing_more() {
    workers::block_on(async {
        let staging = staging().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), manager_of(&doc, mallory(), OwedOp::Add))
            .await;

        let handle = staging.begin().await;
        staging
            .stage(&handle, manager_of(&doc, mallory(), OwedOp::Delete))
            .await
            .unwrap();
        assert!(staging
            .stage(&handle, Change::new(mallory(), &doc, OwedOp::Add, carol()))
            .await
            .is_err());
        staging.node.runner.discard_txn(&handle).await.unwrap();

        let handle = staging.begin().await;
        let mut own = manager_of(&doc, mallory(), OwedOp::Delete);
        own.actor = mallory();
        staging.stage(&handle, own).await.unwrap();
        assert!(staging
            .stage(&handle, Change::new(mallory(), &doc, OwedOp::Add, carol()))
            .await
            .is_err());
        staging.node.runner.discard_txn(&handle).await.unwrap();
    })
    .unwrap()
}
