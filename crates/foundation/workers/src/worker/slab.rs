//! A slab of values named by generation-tagged keys, so a key that outlives
//! its value (a waker woken after its task finished, a timer cancelled after
//! it fired) finds nothing instead of a newer value in the same place.

/// A slot's key: its index in the low half, its generation in the high half.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key(u64);

/// Generations use 31 bits, so a key's top bit is free for a flag.
const GENERATIONS: u32 = 0x7fff_ffff;

impl Key {
    fn new(index: usize, generation: u32) -> Key {
        Key(u64::from(generation & GENERATIONS) << 32 | index as u64)
    }

    pub(crate) fn from_bits(bits: u64) -> Key {
        Key(bits)
    }

    fn index(self) -> usize {
        (self.0 & u64::from(u32::MAX)) as usize
    }

    fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// The key as one word, for an atomic.
    pub(crate) fn to_bits(self) -> u64 {
        self.0
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Values by [`Key`], reusing freed slots.
pub(crate) struct Slab<T> {
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    len: usize,
}

impl<T> Slab<T> {
    pub(crate) fn new() -> Slab<T> {
        Slab {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    /// The key the next [`insert`](Self::insert) will return.
    pub(crate) fn next_key(&self) -> Key {
        match self.free.last() {
            Some(&index) => Key::new(index, self.slots[index].generation),
            None => Key::new(self.slots.len(), 0),
        }
    }

    pub(crate) fn insert(&mut self, value: T) -> Key {
        self.len += 1;
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index];
                slot.value = Some(value);
                Key::new(index, slot.generation)
            }
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    value: Some(value),
                });
                Key::new(self.slots.len() - 1, 0)
            }
        }
    }

    pub(crate) fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        self.slots
            .get_mut(key.index())
            .filter(|slot| slot.generation == key.generation())
            .and_then(|slot| slot.value.as_mut())
    }

    pub(crate) fn contains(&self, key: Key) -> bool {
        self.slots
            .get(key.index())
            .is_some_and(|slot| slot.generation == key.generation() && slot.value.is_some())
    }

    pub(crate) fn remove(&mut self, key: Key) -> Option<T> {
        let slot = self
            .slots
            .get_mut(key.index())
            .filter(|slot| slot.generation == key.generation())?;
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1) & GENERATIONS;
        self.free.push(key.index());
        self.len -= 1;
        Some(value)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Takes every value out, leaving the slab empty.
    pub(crate) fn drain(&mut self) -> Vec<T> {
        let taken: Vec<T> = self
            .slots
            .iter_mut()
            .filter_map(|slot| {
                let value = slot.value.take()?;
                slot.generation = slot.generation.wrapping_add(1) & GENERATIONS;
                Some(value)
            })
            .collect();
        self.free = (0..self.slots.len()).rev().collect();
        self.len = 0;
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_key_finds_nothing_after_its_slot_is_reused() {
        let mut slab = Slab::new();
        let first = slab.insert("a");
        assert_eq!(slab.remove(first), Some("a"));
        assert_eq!(slab.next_key().index(), first.index());
        let second = slab.insert("b");
        assert_eq!(second.index(), first.index());
        assert!(slab.get_mut(first).is_none());
        assert!(!slab.contains(first));
        assert_eq!(slab.remove(first), None);
        assert_eq!(slab.get_mut(second), Some(&mut "b"));
    }

    #[test]
    fn next_key_predicts_insert_and_drain_empties() {
        let mut slab = Slab::new();
        let predicted = slab.next_key();
        assert_eq!(slab.insert(1), predicted);
        slab.insert(2);
        let mut values = slab.drain();
        values.sort();
        assert_eq!(values, vec![1, 2]);
        assert!(slab.is_empty());
        assert!(!slab.contains(predicted));
        let reused = slab.insert(3);
        assert_eq!(slab.remove(reused), Some(3));
    }
}
