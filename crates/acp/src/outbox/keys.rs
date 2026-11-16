//! Where owed relationship changes live in the ACP namespace.
//!
//! - `/acp-owed/q/<seq>`: the queue. `seq` is eight big-endian bytes, so the
//!   queue reads in sequence order.
//! - `/acp-owed/t/<policy><resource><object><relation><subject>`: what is
//!   unsettled on one tuple ([`OwedTupleState`]).
//! - `/acp-owed/o/<policy><resource><object>`: how many changes are unsettled
//!   on one object.
//! - `/acp-owed/a/<actor>`: how many changes are unsettled that one acting
//!   DID staged.
//! - `/acp-owed/n`: how many changes are unsettled on the node.
//! - `/acp-owed/seq`: the counter sequences are allocated from.
//!
//! Each named part is preceded by its length as eight big-endian bytes, as an
//! owed registration's key parts are, so no two tuples and no two objects
//! share a key, and the keys of one object's tuples are exactly those that
//! start with its three parts. A subject is one part: a kind byte, then the
//! actor's DID, or the length-prefixed parts of a set, or a resource.

use identity::Did;

use super::record::{OwedSubject, OwedTuple};
use crate::error::{Error, Result};

const QUEUE_PREFIX: &[u8] = b"/acp-owed/q/";
const TUPLE_PREFIX: &[u8] = b"/acp-owed/t/";
const OBJECT_PREFIX: &[u8] = b"/acp-owed/o/";
const ACTOR_PREFIX: &[u8] = b"/acp-owed/a/";
pub(super) const UNSETTLED_KEY: &[u8] = b"/acp-owed/n";
pub(super) const SEQUENCE_KEY: &[u8] = b"/acp-owed/seq";

const INDEX_VERSION: u8 = 1;
const LENGTH_BYTES: usize = 8;

const SUBJECT_WILDCARD: u8 = 0;
const SUBJECT_ACTOR: u8 = 1;
const SUBJECT_ENTITY_SET: u8 = 2;
const SUBJECT_TYPED_WILDCARD: u8 = 3;

fn push_part(key: &mut Vec<u8>, part: &[u8]) {
    key.extend_from_slice(&(part.len() as u64).to_be_bytes());
    key.extend_from_slice(part);
}

/// The part `bytes` starts with, and what follows it.
fn take_part(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (length, rest) = bytes.split_first_chunk::<LENGTH_BYTES>()?;
    let length = usize::try_from(u64::from_be_bytes(*length)).ok()?;
    rest.split_at_checked(length)
}

fn take_text(bytes: &[u8]) -> Option<(String, &[u8])> {
    let (part, rest) = take_part(bytes)?;
    Some((std::str::from_utf8(part).ok()?.to_string(), rest))
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

fn subject_of(part: &[u8]) -> Option<OwedSubject> {
    let (kind, body) = part.split_first()?;
    let text = || std::str::from_utf8(body).ok().map(str::to_string);
    match *kind {
        SUBJECT_WILDCARD if body.is_empty() => Some(OwedSubject::Wildcard),
        SUBJECT_ACTOR => Some(OwedSubject::Actor {
            did: Did::new(text()?).ok()?,
        }),
        SUBJECT_ENTITY_SET => {
            let (resource, rest) = take_text(body)?;
            let (object, rest) = take_text(rest)?;
            let (relation, rest) = take_text(rest)?;
            rest.is_empty().then_some(OwedSubject::EntitySet {
                resource,
                object,
                relation,
            })
        }
        SUBJECT_TYPED_WILDCARD => Some(OwedSubject::TypedWildcard { resource: text()? }),
        _ => None,
    }
}

pub(super) fn queue_prefix() -> Vec<u8> {
    QUEUE_PREFIX.to_vec()
}

pub(super) fn queue_key(seq: u64) -> Vec<u8> {
    [QUEUE_PREFIX, &seq.to_be_bytes()].concat()
}

/// Every key of the tuples of one object starts with this.
pub(super) fn object_tuples_prefix(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    let mut key = TUPLE_PREFIX.to_vec();
    for part in [policy_id, resource, object] {
        push_part(&mut key, part.as_bytes());
    }
    key
}

pub(super) fn tuple_key(tuple: &OwedTuple) -> Vec<u8> {
    let mut key = object_tuples_prefix(&tuple.policy_id, &tuple.resource, &tuple.object);
    push_part(&mut key, tuple.relation.as_bytes());
    push_part(&mut key, &subject_part(&tuple.subject));
    key
}

/// The relation and subject a tuple key names after its object's `prefix`.
pub(super) fn relation_and_subject(key: &[u8], prefix: &[u8]) -> Result<(String, OwedSubject)> {
    key.strip_prefix(prefix)
        .and_then(|rest| {
            let (relation, rest) = take_text(rest)?;
            let (subject, rest) = take_part(rest)?;
            rest.is_empty().then_some(())?;
            Some((relation, subject_of(subject)?))
        })
        .ok_or_else(|| Error::Serialization("owed ACP tuple key is malformed".into()))
}

pub(super) fn object_key(policy_id: &str, resource: &str, object: &str) -> Vec<u8> {
    let mut key = OBJECT_PREFIX.to_vec();
    for part in [policy_id, resource, object] {
        push_part(&mut key, part.as_bytes());
    }
    key
}

pub(super) fn actor_key(actor: &Did) -> Vec<u8> {
    let mut key = ACTOR_PREFIX.to_vec();
    push_part(&mut key, actor.as_str().as_bytes());
    key
}

/// What is unsettled on one tuple. A change is unsettled while it has a
/// record, whatever its state: one the provider refused still counts.
///
/// Stored as one version byte, then `oldest` and `latest` as eight big-endian
/// bytes each, then `unsettled` and `deletes` as four each. A tuple nothing is
/// owed on has no state.
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

impl OwedTupleState {
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![INDEX_VERSION];
        bytes.extend_from_slice(&self.oldest.to_be_bytes());
        bytes.extend_from_slice(&self.latest.to_be_bytes());
        bytes.extend_from_slice(&self.unsettled.to_be_bytes());
        bytes.extend_from_slice(&self.deletes.to_be_bytes());
        bytes
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self> {
        let fields = match bytes {
            [INDEX_VERSION, fields @ ..] => <&[u8; 24]>::try_from(fields).ok(),
            _ => None,
        }
        .ok_or_else(|| Error::Serialization("owed ACP tuple state is malformed".into()))?;
        let u64_at =
            |at: usize| u64::from_be_bytes(fields[at..at + 8].try_into().unwrap_or([0; 8]));
        let u32_at =
            |at: usize| u32::from_be_bytes(fields[at..at + 4].try_into().unwrap_or([0; 4]));
        Ok(Self {
            oldest: u64_at(0),
            latest: u64_at(8),
            unsettled: u32_at(16),
            deletes: u32_at(20),
        })
    }
}

/// An object's count of unsettled changes: one version byte and the count as
/// eight big-endian bytes. An object nothing is owed on has no count.
pub(super) fn encode_count(count: u64) -> Vec<u8> {
    [&[INDEX_VERSION][..], &count.to_be_bytes()].concat()
}

pub(super) fn decode_count(bytes: &[u8]) -> Result<u64> {
    match bytes {
        [INDEX_VERSION, count @ ..] => count.try_into().ok().map(u64::from_be_bytes),
        _ => None,
    }
    .ok_or_else(|| Error::Serialization("owed ACP object count is malformed".into()))
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
            tuple(["/acp-owed/t/", "q", "o", "n"], actor("z")),
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
                assert_ne!(tuple_key(left), tuple_key(right));
            }
        }
    }

    #[test]
    fn a_tuple_key_names_its_object_relation_and_subject() {
        for tuple in adversarial_tuples() {
            let key = tuple_key(&tuple);
            let prefix = object_tuples_prefix(&tuple.policy_id, &tuple.resource, &tuple.object);
            assert_eq!(
                relation_and_subject(&key, &prefix).unwrap(),
                (tuple.relation.clone(), tuple.subject.clone())
            );
        }
    }

    #[test]
    fn only_the_tuples_of_an_object_start_with_its_prefix() {
        let tuples = adversarial_tuples();
        for owner in &tuples {
            let prefix = object_tuples_prefix(&owner.policy_id, &owner.resource, &owner.object);
            for other in &tuples {
                let same_object = (&owner.policy_id, &owner.resource, &owner.object)
                    == (&other.policy_id, &other.resource, &other.object);
                assert_eq!(tuple_key(other).starts_with(&prefix), same_object);
            }
        }
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
            assert!(!key.starts_with(TUPLE_PREFIX) && !key.starts_with(QUEUE_PREFIX));
        }
        for reserved in [UNSETTLED_KEY, SEQUENCE_KEY] {
            for space in [QUEUE_PREFIX, TUPLE_PREFIX, OBJECT_PREFIX, ACTOR_PREFIX] {
                assert!(!reserved.starts_with(space));
            }
        }
        let actor = actor_key(&Did::new("did:key:owner").unwrap());
        assert!(actor.starts_with(ACTOR_PREFIX));
        for space in [QUEUE_PREFIX, TUPLE_PREFIX, OBJECT_PREFIX] {
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

    #[test]
    fn a_malformed_tuple_key_is_refused() {
        let tuple = tuple(["a", "b", "c", "d"], actor("z"));
        let prefix = object_tuples_prefix("a", "b", "c");
        let mut key = tuple_key(&tuple);
        key.push(0);
        assert!(relation_and_subject(&key, &prefix).is_err());
        key.truncate(key.len() - 3);
        assert!(relation_and_subject(&key, &prefix).is_err());
        assert!(relation_and_subject(&prefix, &prefix).is_err());
    }

    #[test]
    fn the_tuple_state_and_the_count_round_trip() {
        for (oldest, latest, unsettled, deletes) in [
            (0, 0, 1, 0),
            (9, 12, 2, 1),
            (u64::MAX, u64::MAX, u32::MAX, u32::MAX),
        ] {
            let state = OwedTupleState {
                oldest,
                latest,
                unsettled,
                deletes,
            };
            assert_eq!(OwedTupleState::decode(&state.encode()).unwrap(), state);
        }
        let state = OwedTupleState {
            oldest: 258,
            latest: 259,
            unsettled: 3,
            deletes: 2,
        };
        let bytes = [
            1, 0, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0, 1, 3, 0, 0, 0, 3, 0, 0, 0, 2,
        ];
        assert_eq!(state.encode(), bytes);
        let mut unknown = bytes;
        unknown[0] = 2;
        assert!(OwedTupleState::decode(&unknown).is_err());
        assert!(OwedTupleState::decode(&bytes[..24]).is_err());
        assert!(OwedTupleState::decode(&[bytes.as_slice(), &[0]].concat()).is_err());

        for count in [1, 256, u64::MAX] {
            assert_eq!(decode_count(&encode_count(count)).unwrap(), count);
        }
        assert_eq!(encode_count(258), [1, 0, 0, 0, 0, 0, 0, 1, 2]);
        assert!(decode_count(&[1, 0]).is_err());
        assert!(decode_count(&[2, 0, 0, 0, 0, 0, 0, 0, 1]).is_err());
    }
}
