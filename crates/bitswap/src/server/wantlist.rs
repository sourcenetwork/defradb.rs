//! A peer's wanted blocks with their priorities.

use cid::Cid;
use rapidhash::RapidHashMap;

use crate::message::{Priority, WantType};

/// One wanted block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The wanted cid.
    pub cid: Cid,
    /// Request priority.
    pub priority: Priority,
    /// Block or HAVE.
    pub want_type: WantType,
}

/// A raw list of wanted blocks and their priorities.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Wantlist {
    set: RapidHashMap<Cid, Entry>,
}

impl Wantlist {
    /// Number of entries.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Removes every entry.
    pub fn clear(&mut self) {
        self.set.clear();
    }

    /// Adds an entry; a want-have never overrides a want-block. True when the list changed.
    pub fn add(&mut self, cid: Cid, priority: Priority, want_type: WantType) -> bool {
        let entry = Entry {
            cid,
            priority,
            want_type,
        };
        match self.set.entry(cid) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(entry);
                true
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if slot.get().want_type == WantType::Block || want_type == WantType::Have {
                    return false;
                }
                slot.insert(entry);
                true
            }
        }
    }

    /// Removes the cid.
    pub fn remove(&mut self, cid: &Cid) -> Option<Entry> {
        self.set.remove(cid)
    }

    /// The entry for the cid.
    pub fn get(&self, cid: &Cid) -> Option<&Entry> {
        self.set.get(cid)
    }

    /// Removes the cid, except that a have-removal leaves a want-block in place.
    pub fn remove_type(&mut self, cid: &Cid, want_type: WantType) -> Option<Entry> {
        match self.set.entry(*cid) {
            std::collections::hash_map::Entry::Vacant(_) => None,
            std::collections::hash_map::Entry::Occupied(slot) => {
                if slot.get().want_type == WantType::Block && want_type == WantType::Have {
                    return None;
                }
                Some(slot.remove())
            }
        }
    }

    /// The entries, highest priority first.
    pub fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self.set.values().copied().collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
        entries
    }
}
