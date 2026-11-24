//! A takeover report as the event carries it.

use super::*;
use crate::vera_rs::follow::effect::LostSubject;

const OWNER: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";

fn token(issuer: &str) -> String {
    use base64::Engine as _;
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::json!({ "iss": issuer }).to_string());
    format!("e30.{claims}.sig")
}

fn cut() -> TermCut {
    TermCut {
        term: 2,
        height: 5,
        block: "0xcut".into(),
    }
}

fn grant() -> Lost {
    Lost::Relationship {
        add: true,
        policy_id: "0xpolicy".into(),
        resource: "users".into(),
        object: "bae-1".into(),
        relation: "reader".into(),
        subject: LostSubject::Wildcard,
        token: token(OWNER),
    }
}

fn registration() -> Lost {
    Lost::Registration {
        policy_id: "0xpolicy".into(),
        resource: "users".into(),
        object: "bae-2".into(),
        token: token(OWNER),
    }
}

#[test]
fn a_lost_policy_creation_is_named_with_its_creator() {
    let report = TakeoverReport {
        applied: vec![Applied::PolicyCreation {
            policy_id: "0xheld".into(),
            creator: OWNER.into(),
        }],
        unrestorable: vec![Unrestorable::PolicyCreation {
            policy_id: "0xlost".into(),
            creator: OWNER.into(),
            reason: "voided".into(),
        }],
        ..TakeoverReport::default()
    };
    let data = takeover_report(&cut(), &report, None);
    assert_eq!(data.applied_again.len(), 1);
    assert_eq!(data.applied_again[0].kind, "policy-creation");
    assert_eq!(data.applied_again[0].policy_id, "0xheld");
    assert_eq!(data.applied_again[0].actor.as_deref(), Some(OWNER));
    assert_eq!(data.unrestorable.len(), 1);
    assert_eq!(data.unrestorable[0].kind, "policy-creation");
    assert_eq!(data.unrestorable[0].policy_id, "0xlost");
    assert_eq!(data.unrestorable[0].actor.as_deref(), Some(OWNER));
    assert_eq!(data.unrestorable[0].reason, "voided");
}

#[test]
fn the_event_keeps_what_is_owed_again_and_the_cut_of_its_term() {
    let report = TakeoverReport {
        applied: vec![Applied::Change(registration())],
        owed: vec![grant(), registration()],
        ..TakeoverReport::default()
    };
    let data = takeover_report(&cut(), &report, None);
    assert_eq!(
        (data.term, data.cut_height, data.cut_block.as_str()),
        (2, 5, "0xcut")
    );
    assert_eq!(data.closed, None);
    let owed: Vec<_> = data
        .owed_again
        .iter()
        .map(|item| (item.kind.as_str(), item.object.as_str(), item.add))
        .collect();
    assert_eq!(
        owed,
        [
            ("relationship", "bae-1", Some(true)),
            ("registration", "bae-2", None)
        ]
    );
    assert_eq!(data.owed_again[0].relation.as_deref(), Some("reader"));
    assert_eq!(data.owed_again[0].actor.as_deref(), Some(OWNER));
    assert_eq!(data.applied_again[0].kind, "registration");
}

#[test]
fn a_check_that_failed_closed_says_why_even_with_nothing_replayed() {
    let data = takeover_report(&cut(), &TakeoverReport::default(), Some("uncovered"));
    assert_eq!(data.closed.as_deref(), Some("uncovered"));
    assert!(data.applied_again.is_empty() && data.owed_again.is_empty());
    assert!(data.unrestorable.is_empty());
}
