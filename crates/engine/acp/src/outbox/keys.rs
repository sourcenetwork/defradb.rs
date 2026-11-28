//! Where owed relationship changes live in the ACP namespace.
//!
//! - `/acp-owed2/q/<seq>`: the queue. `seq` is eight big-endian bytes, so the
//!   queue reads in sequence order.
//! - `/acp-owed2/l/<policy><resource><object><relation><subject><seq>`: the
//!   sequences still owed on one tuple, each with whether it is a delete, so
//!   its oldest and latest are the first and last of its keys and its counts
//!   are theirs.
//! - `/acp-owed2/h/<policy><resource><object><relation><subject>`: the latest
//!   sequence staged on one tuple, read and written only by staging, which it
//!   keeps ordered.
//! - `/acp-owed2/t/<policy><resource><object>`: the sequence of the latest
//!   transfer staged on one object, written by a transfer's staging and read,
//!   validated, by every other staging on the object, so a change and a
//!   transfer staged on one object at once cannot both commit.
//! - `/acp-owed2/g/<policy><resource><object>`: a marker every staging on one
//!   object blind-merges and a transfer reads validated, so a change and a
//!   transfer staged on one object at once cannot both commit. Settles leave
//!   it alone.
//! - `/acp-owed2/o/<policy><resource><object>`: the deletes owed on one
//!   object whose subject is wider than one actor.
//! - `/acp-owed2/s/<policy><resource><object><subject>`: what is owed on one
//!   object of deletes naming one actor, all of them and those of the owner
//!   relation.
//! - `/acp-owed2/n`: what is owed on the node, changes.
//! - `/acp-owed2/a/<actor>`: how many changes are unsettled that one acting
//!   DID staged.
//! - `/acp-owed2/x/<seq>`: what is kept of a change whose record was deleted
//!   without the provider holding it, until its actor was told.
//! - `/acp-owed2/seq`: the counter sequences are allocated from.
//!
//! The `o`, `s` and `n` counts are owed values ([`storage::merge::owed`]),
//! written only by merges (see `ops.rs`). Each named part is preceded by its
//! length as eight big-endian bytes, as an owed registration's key parts are,
//! so no two tuples, objects or subjects share a key. A subject is one part:
//! a kind byte, then the actor's DID, or the length-prefixed parts of a set,
//! or a resource.
//!
//! The layout before the counts were split lived under `/acp-owed/`.
//! Nothing reads it as owed state: a store whose queue
//! there still holds a record ([`LEGACY_QUEUE_PREFIX`]) is refused, so a
//! revocation it owes can never stop denying unseen.

use identity::Did;

use super::record::{OwedSubject, OwedTuple};
use crate::error::{Error, Result};

const QUEUE_PREFIX: &[u8] = b"/acp-owed2/q/";
const LIVE_PREFIX: &[u8] = b"/acp-owed2/l/";
const HEAD_PREFIX: &[u8] = b"/acp-owed2/h/";
const OBJECT_PREFIX: &[u8] = b"/acp-owed2/o/";
const TRANSFER_PREFIX: &[u8] = b"/acp-owed2/t/";
const STAGED_PREFIX: &[u8] = b"/acp-owed2/g/";
const SUBJECT_PREFIX: &[u8] = b"/acp-owed2/s/";
const ACTOR_PREFIX: &[u8] = b"/acp-owed2/a/";
const DISCARD_PREFIX: &[u8] = b"/acp-owed2/x/";
pub(super) const UNSETTLED_KEY: &[u8] = b"/acp-owed2/n";
pub(super) const SEQUENCE_KEY: &[u8] = b"/acp-owed2/seq";

/// The queue of the layout before the counts were split, under
/// `/acp-owed/`: one record per change still owed.
pub(super) const LEGACY_QUEUE_PREFIX: &[u8] = b"/acp-owed/q/";

const SUBJECT_WILDCARD: u8 = 0;
const SUBJECT_ACTOR: u8 = 1;
const SUBJECT_ENTITY_SET: u8 = 2;
const SUBJECT_TYPED_WILDCARD: u8 = 3;

fn push_part(key: &mut Vec<u8>, part: &[u8]) {
    key.extend_from_slice(&(part.len() as u64).to_be_bytes());
    key.extend_from_slice(part);
}

fn subject_part(subject: &OwedSubject) -> Vec<u8> {
    match subject {
        OwedSubject::Wildcard => vec![SUBJECT_WILDCARD],
        OwedSubject::Actor { did } => [&[SUBJECT_ACTOR], did.as_str().as_bytes()].concat(),
        OwedSubject::EntitySet {
            resource,
            object,
            relation,
        } => {
            let mut part = vec![SUBJECT_ENTITY_SET];
            for text in [resource, object, relation] {
                push_part(&mut part, text.as_bytes());
            }
            part
        }
        OwedSubject::TypedWildcard { resource } => {
            [&[SUBJECT_TYPED_WILDCARD], resource.as_bytes()].concat()
        }
    }
}

pub(super) fn queue_prefix() -> Vec<u8> {
    QUEUE_PREFIX.to_vec()
}

pub(super) fn queue_key(seq: u64) -> Vec<u8> {
    [QUEUE_PREFIX, &seq.to_be_bytes()].concat()
}

pub(super) fn discard_prefix() -> Vec<u8> {
    DISCARD_PREFIX.to_vec()
}

pub(super) fn discard_key(seq: u64) -> Vec<u8> {
    [DISCARD_PREFIX, &seq.to_be_bytes()].concat()
}

/// The parts that name one object, each length-prefixed.
fn object_parts(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    let mut parts = Vec::new();
    for part in [policy_id, resource, object] {
        push_part(&mut parts, part.as_bytes());
    }
    parts
}

/// The parts that name one tuple, each length-prefixed.
fn tuple_parts(tuple: &OwedTuple) -> Vec<u8> {
    let mut parts = object_parts(&tuple.policy_id, &tuple.resource, &tuple.object);
    push_part(&mut parts, tuple.relation.as_bytes());
    push_part(&mut parts, &subject_part(&tuple.subject));
    parts
}

/// The latest sequence staged on one tuple, which only staging reads and
/// writes.
pub(super) fn head_key(tuple: &OwedTuple) -> Vec<u8> {
    [HEAD_PREFIX, &tuple_parts(tuple)].concat()
}

/// Every live sequence of one object's tuples is a key under this.
pub(super) fn object_live_prefix(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    [LIVE_PREFIX, &object_parts(policy_id, resource, object)].concat()
}

/// Every live sequence of one tuple is a key under this.
pub(super) fn live_prefix(tuple: &OwedTuple) -> Vec<u8> {
    [LIVE_PREFIX, &tuple_parts(tuple)].concat()
}

pub(super) fn live_key(tuple: &OwedTuple, seq: u64) -> Vec<u8> {
    [&live_prefix(tuple)[..], &seq.to_be_bytes()].concat()
}

/// The sequence a live key under `prefix` names.
pub(super) fn live_seq(key: &[u8], prefix: &[u8]) -> Result<u64> {
    key.strip_prefix(prefix)
        .and_then(|rest| <[u8; 8]>::try_from(rest).ok())
        .map(u64::from_be_bytes)
        .ok_or_else(|| Error::Serialization("owed ACP live key is malformed".into()))
}

/// The count of deletes naming the actor `did` on one object.
pub(super) fn subject_key(policy_id: &str, resource: &str, object: &str, did: &Did) -> Vec<u8> {
    let mut key = [SUBJECT_PREFIX, &object_parts(policy_id, resource, object)].concat();
    push_part(
        &mut key,
        &subject_part(&OwedSubject::Actor { did: did.clone() }),
    );
    key
}

/// The latest transfer staged on one object, which only staging reads and
/// writes.
pub(super) fn transfer_key(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    [TRANSFER_PREFIX, &object_parts(policy_id, resource, object)].concat()
}

/// The marker every staging on one object writes and a transfer reads.
pub(super) fn staged_key(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    [STAGED_PREFIX, &object_parts(policy_id, resource, object)].concat()
}

pub(super) fn object_key(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    [OBJECT_PREFIX, &object_parts(policy_id, resource, object)].concat()
}

pub(super) fn actor_key(actor: &Did) -> Vec<u8> {
    let mut key = ACTOR_PREFIX.to_vec();
    push_part(&mut key, actor.as_str().as_bytes());
    key
}

/// What is unsettled on one tuple. A change is unsettled while it has a
/// record, whatever its state: one the provider refused still counts. Read
/// from the tuple's count and its live sequences; a tuple nothing is owed on
/// has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwedTupleState {
    /// The lowest sequence unsettled on the tuple: a change above it has an
    /// earlier one of its tuple still owed.
    pub oldest: u64,
    /// The highest sequence unsettled on the tuple.
    pub latest: u64,
    /// How many changes of the tuple are unsettled.
    pub unsettled: u32,
    /// How many of them are deletes. While any is, the tuple's subject is
    /// denied, whatever was staged after it.
    pub deletes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuple(parts: [&str; 4], subject: OwedSubject) -> OwedTuple {
        OwedTuple {
            policy_id: parts[0].into(),
            resource: parts[1].into(),
            object: parts[2].into(),
            relation: parts[3].into(),
            subject,
        }
    }

    fn actor(name: &str) -> OwedSubject {
        OwedSubject::Actor {
            did: Did::new(format!("did:key:{name}")).unwrap(),
        }
    }

    fn set(resource: &str, object: &str, relation: &str) -> OwedSubject {
        OwedSubject::EntitySet {
            resource: resource.into(),
            object: object.into(),
            relation: relation.into(),
        }
    }

    /// Tuples that differ only in where one part ends and the next begins, in
    /// empty parts, in separators, and in subjects that print alike.
    fn adversarial_tuples() -> Vec<OwedTuple> {
        let long = "x".repeat(70_000);
        let length_like = "\0\0\0\0\0\0\0\u{1}a";
        vec![
            tuple(["ab", "c", "d", "e"], actor("z")),
            tuple(["a", "bc", "d", "e"], actor("z")),
            tuple(["a", "b", "cd", "e"], actor("z")),
            tuple(["a", "b", "c", "de"], actor("z")),
            tuple(["", "abc", "d", "e"], actor("z")),
            tuple(["abc", "", "d", "e"], actor("z")),
            tuple(["abc", "d", "", "e"], actor("z")),
            tuple(["abc", "d", "e", ""], actor("z")),
            tuple(["", "", "", ""], actor("z")),
            tuple(["", "", "", ""], OwedSubject::Wildcard),
            tuple(["a/b", "c", "d", "e"], actor("z")),
            tuple(["a", "b/c", "d", "e"], actor("z")),
            tuple(["/acp-owed2/h/", "q", "o", "n"], actor("z")),
            tuple([length_like, "", "c", "d"], actor("z")),
            tuple(["", length_like, "c", "d"], actor("z")),
            tuple([&long, "b", "c", "d"], actor("z")),
            tuple(["b", &long, "c", "d"], actor("z")),
            tuple(["a", "b", "c", "d"], actor("z")),
            tuple(["a", "b", "c", "d"], actor("zz")),
            tuple(["a", "b", "c", "d"], OwedSubject::Wildcard),
            tuple(["a", "b", "c", "d"], set("did", "key:z", "r")),
            tuple(["a", "b", "c", "d"], set("g", "o#m", "")),
            tuple(["a", "b", "c", "d"], set("g", "o", "m")),
            tuple(["a", "b", "c", "d"], set("go", "", "m")),
            tuple(["a", "b", "c", "d"], set("", "go", "m")),
            tuple(
                ["a", "b", "c", "d"],
                OwedSubject::TypedWildcard {
                    resource: "g".into(),
                },
            ),
            tuple(
                ["a", "b", "c", "d"],
                OwedSubject::TypedWildcard {
                    resource: String::new(),
                },
            ),
        ]
    }

    #[test]
    fn keys_of_distinct_tuples_differ() {
        let tuples = adversarial_tuples();
        for (i, left) in tuples.iter().enumerate() {
            for right in &tuples[i + 1..] {
                assert_ne!(head_key(left), head_key(right));
            }
        }
    }

    #[test]
    fn the_live_keys_of_a_tuple_sort_by_sequence_and_name_only_it() {
        let tuples = adversarial_tuples();
        for owner in &tuples {
            let prefix = live_prefix(owner);
            let seqs = [0, 1, 255, 256, u64::MAX];
            for pair in seqs.windows(2) {
                assert!(live_key(owner, pair[0]) < live_key(owner, pair[1]));
            }
            for seq in seqs {
                assert_eq!(live_seq(&live_key(owner, seq), &prefix).unwrap(), seq);
            }
            for other in &tuples {
                assert_eq!(live_key(other, 7).starts_with(&prefix), owner == other);
            }
        }
        let prefix = live_prefix(&tuples[0]);
        assert!(live_seq(&prefix, &prefix).is_err());
        assert!(live_seq(&[prefix.as_slice(), &[0; 9]].concat(), &prefix).is_err());
    }

    #[test]
    fn subject_keys_differ_by_object_and_actor() {
        let mut keys = Vec::new();
        for tuple in adversarial_tuples() {
            for name in ["z", "zz"] {
                let did = Did::new(format!("did:key:{name}")).unwrap();
                keys.push(subject_key(
                    &tuple.policy_id,
                    &tuple.resource,
                    &tuple.object,
                    &did,
                ));
            }
        }
        let count = keys.len();
        keys.sort();
        keys.dedup();
        // The same object appears more than once in the tuples above.
        let mut objects: Vec<_> = adversarial_tuples()
            .into_iter()
            .map(|tuple| (tuple.policy_id, tuple.resource, tuple.object))
            .collect();
        objects.sort();
        objects.dedup();
        assert_eq!(keys.len(), objects.len() * 2);
        assert!(count >= keys.len());
        assert!(keys.iter().all(|key| key.starts_with(SUBJECT_PREFIX)));
    }

    #[test]
    fn keys_of_distinct_objects_differ_and_stay_in_their_own_space() {
        let tuples = adversarial_tuples();
        let mut keys: Vec<Vec<u8>> = tuples
            .iter()
            .map(|tuple| object_key(&tuple.policy_id, &tuple.resource, &tuple.object))
            .collect();
        keys.sort();
        keys.dedup();
        let mut objects: Vec<_> = tuples
            .iter()
            .map(|tuple| (&tuple.policy_id, &tuple.resource, &tuple.object))
            .collect();
        objects.sort();
        objects.dedup();
        assert_eq!(keys.len(), objects.len());

        for key in &keys {
            assert!(key.starts_with(OBJECT_PREFIX));
            assert!(!key.starts_with(HEAD_PREFIX) && !key.starts_with(QUEUE_PREFIX));
        }
        for reserved in [UNSETTLED_KEY, SEQUENCE_KEY] {
            for space in [
                QUEUE_PREFIX,
                LIVE_PREFIX,
                HEAD_PREFIX,
                OBJECT_PREFIX,
                SUBJECT_PREFIX,
                ACTOR_PREFIX,
            ] {
                assert!(!reserved.starts_with(space));
            }
        }
        let actor = actor_key(&Did::new("did:key:owner").unwrap());
        assert!(actor.starts_with(ACTOR_PREFIX));
        for space in [QUEUE_PREFIX, HEAD_PREFIX, OBJECT_PREFIX] {
            assert!(!actor.starts_with(space));
        }
    }

    #[test]
    fn queue_keys_sort_by_sequence() {
        let seqs = [0, 1, 255, 256, 65_535, 65_536, u64::MAX - 1, u64::MAX];
        for pair in seqs.windows(2) {
            assert!(queue_key(pair[0]) < queue_key(pair[1]));
        }
        assert!(queue_key(u64::MAX).starts_with(&queue_prefix()));
    }

    /// Where the layout before the counts were split lived.
    const LEGACY_PREFIX: &[u8] = b"/acp-owed/";

    #[test]
    fn no_key_of_the_layout_starts_with_the_legacy_root() {
        let tuple = tuple(["a", "b", "c", "d"], actor("z"));
        let did = Did::new("did:key:z").unwrap();
        for key in [
            queue_key(1),
            live_key(&tuple, 1),
            head_key(&tuple),
            object_key("a", "b", "c"),
            subject_key("a", "b", "c", &did),
            actor_key(&did),
            discard_key(1),
            UNSETTLED_KEY.to_vec(),
            SEQUENCE_KEY.to_vec(),
        ] {
            assert!(!key.starts_with(LEGACY_PREFIX), "{key:?}");
        }
    }
}
