//! The sender scope of a document head, and which head of a scope is current.
//!
//! A peer announces heads of a document (or a collection) one after another.
//! Within one sender scope (the announcing peer, the collection and the
//! document) the receiver remembers a current head, the newest by (priority,
//! CID). A head retires the current head only when it links it as a parent,
//! so fetching the new head fetches the old one. A version order alone proves
//! nothing: heads written concurrently on different peers reach a hub as
//! siblings, and the hub forwards both under its own ID. Neither links the
//! other, so each keeps its own obligation. A head with no sender or no
//! priority belongs to no scope and is independent of every other. The
//! in-memory pending registry and the durable registrations decide with the
//! one rule here.

use cid::Cid;

/// The announcing peer, collection and document of a head.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Scope {
    source_peer: String,
    collection_id: String,
    doc_id: String,
}

/// A head's place in its scope: by priority, then by CID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HeadVersion {
    pub(crate) priority: u64,
    pub(crate) cid: Cid,
}

/// What a head is to its scope's current head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeDecision {
    /// It belongs to no scope.
    Independent,
    /// It is the current head, the scope has none, or it does not link the
    /// current head: it keeps its own obligation and retires nothing.
    Current,
    /// It links the current head as a parent, and retires it.
    Supersedes(Cid),
}

impl Scope {
    /// The scope and version of the head `root`, or `None` when it has no
    /// sender or no priority.
    pub(crate) fn of(
        root: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> Option<(Self, HeadVersion)> {
        Some((
            Self {
                source_peer: source_peer?.to_owned(),
                collection_id: collection_id.to_owned(),
                doc_id: doc_id.to_owned(),
            },
            HeadVersion {
                priority: head_priority?,
                cid: root,
            },
        ))
    }

    /// The scope's key under `prefix`, each part length-prefixed so no part
    /// can run into the next.
    pub(crate) fn key(&self, prefix: &[u8]) -> Vec<u8> {
        let parts = [&self.source_peer, &self.collection_id, &self.doc_id];
        let mut key = Vec::with_capacity(
            prefix.len() + parts.iter().map(|part| part.len() + 9).sum::<usize>(),
        );
        key.extend_from_slice(prefix);
        for part in parts {
            key.extend_from_slice(format!("{}:", part.len()).as_bytes());
            key.extend_from_slice(part.as_bytes());
        }
        key
    }
}

impl HeadVersion {
    /// The priority as 8 big-endian bytes, then the CID's bytes.
    pub(crate) fn to_bytes(self) -> Vec<u8> {
        let mut bytes = self.priority.to_be_bytes().to_vec();
        bytes.extend_from_slice(&self.cid.to_bytes());
        bytes
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (priority, cid) = bytes.split_first_chunk::<8>()?;
        Some(Self {
            priority: u64::from_be_bytes(*priority),
            cid: Cid::try_from(cid).ok()?,
        })
    }
}

impl ScopeDecision {
    /// The decision for a head at `version`, linking `parents`, when the
    /// scope's current head is `current`.
    pub(crate) fn of(version: HeadVersion, parents: &[Cid], current: Option<HeadVersion>) -> Self {
        match current {
            Some(current) if current.cid != version.cid && parents.contains(&current.cid) => {
                Self::Supersedes(current.cid)
            }
            _ => Self::Current,
        }
    }

    /// Whether the scope has a current head and `version` is it or links it.
    pub(crate) fn refreshes_or_supersedes(
        version: HeadVersion,
        parents: &[Cid],
        current: Option<HeadVersion>,
    ) -> bool {
        current.is_some_and(|current| current.cid == version.cid || parents.contains(&current.cid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multihash_codetable::{Code, MultihashDigest};

    fn cid(seed: &[u8]) -> Cid {
        Cid::new_v1(0x55, Code::Sha2_256.digest(seed))
    }

    fn version(priority: u64, seed: &[u8]) -> HeadVersion {
        HeadVersion {
            priority,
            cid: cid(seed),
        }
    }

    #[test]
    fn a_head_with_no_sender_or_no_priority_has_no_scope() {
        assert!(Scope::of(cid(b"a"), None, "c", "d", Some(1)).is_none());
        assert!(Scope::of(cid(b"a"), Some("p"), "c", "d", None).is_none());
        assert!(Scope::of(cid(b"a"), Some("p"), "c", "d", Some(1)).is_some());
    }

    #[test]
    fn a_head_retires_only_the_current_head_it_links() {
        let old = version(1, b"old");
        let new = version(2, b"new");
        assert_eq!(ScopeDecision::of(new, &[], None), ScopeDecision::Current);
        assert_eq!(
            ScopeDecision::of(new, &[old.cid], Some(new)),
            ScopeDecision::Current
        );
        assert_eq!(
            ScopeDecision::of(new, &[old.cid], Some(old)),
            ScopeDecision::Supersedes(old.cid)
        );
        assert_eq!(
            ScopeDecision::of(old, &[], Some(new)),
            ScopeDecision::Current
        );
        assert!(ScopeDecision::refreshes_or_supersedes(new, &[], Some(new)));
        assert!(ScopeDecision::refreshes_or_supersedes(
            new,
            &[old.cid],
            Some(old)
        ));
        assert!(!ScopeDecision::refreshes_or_supersedes(old, &[], Some(new)));
        assert!(!ScopeDecision::refreshes_or_supersedes(
            new,
            &[old.cid],
            None
        ));
    }

    #[test]
    fn a_sibling_neither_retires_nor_is_covered_whatever_its_version() {
        let base = version(1, b"base");
        let left = version(2, b"left");
        let right = version(2, b"right");
        let higher = version(3, b"higher");
        for (head, current) in [(left, right), (right, left), (higher, left), (left, higher)] {
            assert_eq!(
                ScopeDecision::of(head, &[base.cid], Some(current)),
                ScopeDecision::Current
            );
            assert!(!ScopeDecision::refreshes_or_supersedes(
                head,
                &[base.cid],
                Some(current)
            ));
        }
    }

    #[test]
    fn scope_keys_do_not_run_parts_together() {
        let (left, _) = Scope::of(cid(b"a"), Some("ab"), "c", "d", Some(1)).unwrap();
        let (right, _) = Scope::of(cid(b"a"), Some("a"), "bc", "d", Some(1)).unwrap();
        assert_ne!(left.key(b"/scope/"), right.key(b"/scope/"));
        assert!(left.key(b"/scope/").starts_with(b"/scope/"));
    }

    #[test]
    fn a_head_version_round_trips_through_its_bytes() {
        let head = version(u64::MAX - 1, b"head");
        assert_eq!(HeadVersion::from_bytes(&head.to_bytes()), Some(head));
        assert_eq!(HeadVersion::from_bytes(b"short"), None);
    }
}
