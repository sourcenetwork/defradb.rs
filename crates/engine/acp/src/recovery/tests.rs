//! Recovery ledger key and page tests.

use std::sync::Arc;

use identity::Did;
use storage::RegolithStore;

use super::*;

fn owner() -> Did {
    Did::new("did:key:owner").unwrap()
}

fn grant(object: &str, relation: &str, held: bool) -> RecoveryFact {
    let subject = OwedSubject::Actor {
        did: Did::new("did:key:reader").unwrap(),
    };
    RecoveryFact::relationship("policy", "users", object, owner(), relation, subject, held)
}

fn owned(object: &str) -> RecoveryFact {
    RecoveryFact::registered("policy", "users", object, owner())
}

fn facts(page: &LedgerPage) -> Vec<RecoveryFact> {
    page.slots.iter().map(|slot| slot.fact.clone()).collect()
}

#[test]
fn a_slot_key_orders_an_objects_owner_before_its_relationships() {
    let owner = owned("bae-1").key(CONFIRMED).unwrap();
    let relationship = grant("bae-1", "reader", true).key(CONFIRMED).unwrap();
    let next = owned("bae-2").key(CONFIRMED).unwrap();
    assert!(owner < relationship);
    assert!(relationship < next);
    assert_eq!(
        grant("bae-1", "reader", true).slot_key().unwrap(),
        grant("bae-1", "reader", false).slot_key().unwrap(),
        "a grant and its delete share a slot"
    );
}

#[test]
fn the_providers_next_answer_replaces_what_a_stop_left_unanswered() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let txn = datastore::BasicTxn::new(store.as_ref(), 0, false)
            .await
            .unwrap();
        let ledger = RecoveryLedger::new(txn.acpstore());
        ledger
            .prepare(&grant("bae-1", "reader", false))
            .await
            .unwrap();
        ledger
            .prepare(&grant("bae-1", "reader", true))
            .await
            .unwrap();
        let unanswered = ledger.page(None, usize::MAX).await.unwrap();
        assert_eq!(facts(&unanswered), vec![grant("bae-1", "reader", false)]);
        assert!(!unanswered.slots[0].confirmed);

        ledger
            .settle(&grant("bae-1", "reader", true), true)
            .await
            .unwrap();
        let answered = ledger.page(None, usize::MAX).await.unwrap();
        assert_eq!(facts(&answered), vec![grant("bae-1", "reader", true)]);
        assert!(answered.slots[0].confirmed);
    })
    .unwrap()
}

#[test]
fn a_page_never_splits_a_slot_and_says_when_the_ledger_ends() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let txn = datastore::BasicTxn::new(store.as_ref(), 0, false)
            .await
            .unwrap();
        let ledger = RecoveryLedger::new(txn.acpstore());
        ledger.confirm(&owned("bae-1")).await.unwrap();
        ledger
            .confirm(&grant("bae-1", "reader", true))
            .await
            .unwrap();
        ledger
            .prepare(&grant("bae-1", "reader", false))
            .await
            .unwrap();
        ledger.confirm(&owned("bae-2")).await.unwrap();

        let first = ledger.page(None, 1).await.unwrap();
        assert_eq!(facts(&first), vec![owned("bae-1")]);
        assert!(!first.end);
        let second = ledger.page(first.last.as_deref(), 1).await.unwrap();
        assert_eq!(
            facts(&second),
            vec![grant("bae-1", "reader", false)],
            "the unanswered delete is read with its grant"
        );
        assert!(!second.end);
        let third = ledger.page(second.last.as_deref(), 1).await.unwrap();
        assert_eq!(facts(&third), vec![owned("bae-2")]);
        assert!(third.end);

        ledger
            .settle(&grant("bae-1", "reader", false), false)
            .await
            .unwrap();
        let all = ledger.page(None, usize::MAX).await.unwrap();
        assert_eq!(
            facts(&all),
            vec![
                owned("bae-1"),
                grant("bae-1", "reader", true),
                owned("bae-2")
            ],
            "a refused delete leaves the grant as it was confirmed"
        );
    })
    .unwrap()
}
