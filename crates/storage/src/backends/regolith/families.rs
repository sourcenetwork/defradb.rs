//! The families of the store's keys that regolith is told about, declared
//! once.
//!
//! A family is the bytes its keys start with (`h/p/` for the priority
//! index), the parts after them that name the one entity a key belongs to
//! (the document's short ID), each ended by a separator, and the class
//! regolith validates the family's keys by. The key classifier (`classifier`)
//! takes the class from here; the prefix extractor (`prefix`) builds its
//! prefix filters from the entity. Both read this one table, so they cannot
//! disagree on what one document or one collection is.
//!
//! Where an entity ends is decided by the bytes before its end, and the end
//! is a separator, so every key that starts with an entity's prefix has that
//! same entity. The prefix extractor relies on it: a prefix seek may pass
//! over a table whose filter lacks the entity its prefix names. A key cut
//! short, or malformed, has no entity.

use crate::keys::doc_id_index::doc_short_id_len;
use crate::keys::utils::{uvarint_len, SEPARATOR};

/// How regolith validates the keys of a family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class {
    /// As any key is validated.
    Ordinary,
    /// The rest of the key is the content identifier of its value.
    ContentAddressed,
    /// Written only by the commit-ordered append of the arrival journal.
    Log,
}

/// One part of a key that names its entity.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// A document short ID: an ordered uvarint, never zero.
    DocShortId,
    /// Any ordered uvarint: a collection's or an index's short ID.
    Uvarint,
    /// Any text but a separator, at least one byte.
    Text,
    /// These bytes exactly.
    Exactly(&'static [u8]),
}

impl Part {
    /// How many bytes of `rest` this part takes, or `None` when `rest` does
    /// not start with all of it.
    fn len(self, rest: &[u8]) -> Option<usize> {
        match self {
            Self::DocShortId => doc_short_id_len(rest),
            Self::Uvarint => uvarint_len(rest),
            Self::Text => text_len(rest),
            Self::Exactly(bytes) => rest.starts_with(bytes).then_some(bytes.len()),
        }
    }
}

/// The length of the text `rest` starts with, up to its separator: at least
/// one byte, and `None` when no separator ends it.
fn text_len(rest: &[u8]) -> Option<usize> {
    rest.iter()
        .position(|&byte| byte == SEPARATOR)
        .filter(|&len| len > 0)
}

/// A family of keys.
#[derive(Debug)]
pub(super) struct Family {
    /// The bytes every key of the family starts with, namespace included.
    start: &'static [u8],
    /// The parts after `start` that name a key's entity, each ended by a
    /// separator. Empty for a family nothing reads by entity.
    entity: &'static [Part],
    /// How regolith validates the family's keys.
    class: Class,
}

impl Family {
    /// How regolith validates this family's keys.
    pub(super) fn class(&self) -> Class {
        self.class
    }

    /// The bytes of `key` after this family's start.
    pub(super) fn rest<'a>(&self, key: &'a [u8]) -> &'a [u8] {
        key.get(self.start.len()..).unwrap_or_default()
    }

    /// The length of `key`'s entity prefix: the family's start and every
    /// part of the entity with the separator that ends it. `None` for a
    /// family with no entity, and for a key cut short or malformed.
    pub(super) fn entity_len(&self, key: &[u8]) -> Option<usize> {
        if self.entity.is_empty() {
            return None;
        }
        self.entity.iter().try_fold(self.start.len(), |end, part| {
            let end = end + part.len(key.get(end..)?)?;
            (key.get(end) == Some(&SEPARATOR)).then_some(end + 1)
        })
    }
}

const fn family(start: &'static [u8], entity: &'static [Part], class: Class) -> Family {
    Family {
        start,
        entity,
        class,
    }
}

/// Every family, in the order a key is matched against them: the first
/// family whose start the key starts with is the key's family.
///
/// Where one start begins another, the longer comes first: `bm` before `b`,
/// and `d/d/`, `d/del/` and `d/v/` before `d/`. The order keeps an entity
/// stable as a key grows: a key still shorter than one of the longer starts
/// falls to the shorter family, which names no entity in so few bytes (`b`
/// names none, and `d/` needs six).
static FAMILIES: [Family; 16] = [
    // Block merge markers: decision keys, not blocks.
    family(b"bm", &[], Class::Ordinary),
    // Blocks, keyed by the CID of their bytes (`keys::BlockstoreKey`).
    family(b"b", &[], Class::ContentAddressed),
    // The arrival journal: one collection's head, rows and once keys.
    family(b"s/arrival/", &[], Class::Log),
    // A document's and a collection's head sets, one key each, written only
    // by blind merges (`keys::HeadstoreDocSetKey`, `keys::HeadstoreColSetKey`,
    // `db::block::heads`), and the guards that record a collection block's
    // publication (`keys::HeadstoreColPublishedKey`). Ordinary: a merge is
    // blind at every class, and a set is read without parts, or by the parts
    // of the names a publication decides on.
    family(b"h/hd/", &[], Class::Ordinary),
    family(b"h/hc/", &[], Class::Ordinary),
    family(b"h/rc/", &[], Class::Ordinary),
    // A document's commit priority index (`keys::HeadstorePriorityKey`), whose
    // entry also records a head's publication, and the guards an earlier
    // layout wrote for that, which the head-set migration drops at open.
    family(b"h/p/", &[Part::DocShortId], Class::Ordinary),
    family(b"h/r/", &[Part::DocShortId], Class::Ordinary),
    // A collection's document values, deletion markers and schema versions
    // (`keys::doc_key`, `keys::deleted_doc_key`, a collection's version key).
    family(b"d/d/", &[Part::Text], Class::Ordinary),
    family(b"d/del/", &[Part::Text], Class::Ordinary),
    family(b"d/v/", &[Part::Text], Class::Ordinary),
    // A vector index build's pending log: its head and rows
    // (`keys::vector_pending_log`).
    family(b"d/vplog/", &[], Class::Log),
    // The same log on a store with no namespace, where vector index tests and
    // tools write vector keys unprefixed.
    family(b"/vplog/", &[], Class::Log),
    // One index's entries (`keys::IndexDataStoreKey`), and one collection's
    // field values of one instance (`keys::DataStoreKey`).
    family(b"d/", &[Part::Uvarint, Part::Uvarint], Class::Ordinary),
    // One ACP object's relationship tuples, under their policy
    // (`zanzibar::Relationship::storage_key`), and in the earlier ACP store
    // (`acp::RelationTuple::storage_key`).
    family(
        b"a/zanzibar/",
        &[Part::Text, Part::Exactly(b"rel"), Part::Text, Part::Text],
        Class::Ordinary,
    ),
    family(b"a/acp/", &[Part::Text, Part::Text], Class::Ordinary),
];

/// The family `key` belongs to, or `None` for a key of no declared family.
pub(super) fn family_of(key: &[u8]) -> Option<&'static Family> {
    // Every write is classified, so a key is compared only with the
    // families up to the last one its first byte can start, and with the
    // whole start only of one whose first byte matches.
    let first = key.first()?;
    FAMILIES[..usize::from(FAMILIES_BY_FIRST_BYTE[usize::from(*first)])]
        .iter()
        .find(|family| family.start.first() == Some(first) && key.starts_with(family.start))
}

/// For each first byte of a key, one past the last family whose start begins
/// with it, `0` for none.
static FAMILIES_BY_FIRST_BYTE: [u8; 256] = families_by_first_byte();

const fn families_by_first_byte() -> [u8; 256] {
    assert!(FAMILIES.len() < u8::MAX as usize);
    let mut table = [0u8; 256];
    let mut index = 0;
    while index < FAMILIES.len() {
        table[FAMILIES[index].start[0] as usize] = index as u8 + 1;
        index += 1;
    }
    table
}

/// The length of `key`'s entity prefix, or `None` when its family names no
/// entity, or when it is cut short or malformed.
pub(super) fn entity_len(key: &[u8]) -> Option<usize> {
    family_of(key)?.entity_len(key)
}

/// Keys of every family and of none, cut short or malformed as often as
/// whole: what property tests of the table's readers walk.
#[cfg(test)]
pub(super) fn keys() -> impl proptest::strategy::Strategy<Value = Vec<u8>> {
    use proptest::prelude::*;

    let starts: Vec<Vec<u8>> = FAMILIES
        .iter()
        .flat_map(|family| (0..=family.start.len()).map(|len| family.start[..len].to_vec()))
        .collect();
    let pieces: Vec<Vec<u8>> = [
        b"/".as_slice(),
        b"rel/",
        b"7/",
        b"12/",
        b"\x01/",
        b"\xf0\x2f/",
        b"\xf9\x00\x01/",
        b"\x00/",
        b"\xf9\x00\x00/",
        b"users/",
        b"bae-1/",
        b"x",
    ]
    .iter()
    .map(|piece| piece.to_vec())
    .collect();
    let piece = prop_oneof![
        proptest::sample::select(pieces),
        any::<u8>().prop_map(|byte| vec![byte]),
    ];
    (
        proptest::sample::select(starts),
        proptest::collection::vec(piece, 0..8),
    )
        .prop_map(|(mut key, pieces)| {
            for piece in pieces {
                key.extend(piece);
            }
            key
        })
}

#[cfg(test)]
mod tests;
