//! The arrival journal's keys, as regolith's commit-ordered append sees them.
//!
//! An [`AppendLog`](crate::corekv::AppendLog) names a log's head key and its
//! entry prefix, which its clones share as one [`LogKeys`]; the key of the
//! entry at a position is the prefix followed by the position as 20
//! zero-padded decimal digits, which sort in position order and hold every
//! `u64`. regolith asks for these keys inside the ordered step of a commit,
//! which holds up every other writer, so a key is built in place, with no
//! allocation of its own. Keys already written never change form: a store
//! keeps reading its journal across releases.

use regolith::LogLayout;

use crate::corekv::types::LogKeys;

/// The decimal digits of the largest position, `u64::MAX`.
const POSITION_DIGITS: usize = 20;

impl LogLayout for LogKeys {
    fn head_key(&self) -> &[u8] {
        &self.head_key
    }

    fn entry_key(&self, position: u64, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.entry_prefix);
        out.extend_from_slice(&position_digits(position));
    }

    fn max_entry_key_len(&self) -> usize {
        self.entry_prefix.len() + POSITION_DIGITS
    }
}

/// `position` in zero-padded decimal, as `format!("{position:020}")` writes
/// it.
fn position_digits(mut position: u64) -> [u8; POSITION_DIGITS] {
    let mut digits = [b'0'; POSITION_DIGITS];
    for digit in digits.iter_mut().rev() {
        *digit = b'0' + (position % 10) as u8;
        position /= 10;
    }
    digits
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn journal() -> LogKeys {
        LogKeys {
            head_key: b"s/arrival/7/head".to_vec(),
            entry_prefix: b"s/arrival/7/row/".to_vec(),
        }
    }

    fn entry(log: &LogKeys, position: u64) -> Vec<u8> {
        let mut key = Vec::new();
        log.entry_key(position, &mut key);
        key
    }

    /// The form every journal written so far uses, kept byte for byte.
    #[test]
    fn entry_keys_keep_the_zero_padded_decimal_form() {
        let log = journal();
        for position in [
            0,
            1,
            9,
            10,
            12_345,
            u64::from(u32::MAX),
            u64::MAX - 1,
            u64::MAX,
        ] {
            assert_eq!(
                entry(&log, position),
                format!("s/arrival/7/row/{position:020}").into_bytes()
            );
        }
    }

    /// regolith's contract: one key per position, none longer than the
    /// declared bound, and the head key is no entry's key.
    #[test]
    fn every_position_has_its_own_bounded_key_apart_from_the_head() {
        let log = journal();
        assert_eq!(entry(&log, u64::MAX).len(), log.max_entry_key_len());
        assert!(!log.head_key().starts_with(&log.entry_prefix));
        assert_ne!(entry(&log, 0), entry(&log, 1));
    }

    /// `AppendLog::entry_key`, which readers use, names the key the engine
    /// writes.
    #[test]
    fn readers_build_the_entry_key_the_engine_writes() {
        let log = crate::corekv::AppendLog::new(
            b"s/arrival/7/head".to_vec(),
            b"s/arrival/7/row/".to_vec(),
        );
        for position in [0, 1, 42, u64::MAX] {
            assert_eq!(log.entry_key(position), entry(log.keys(), position));
        }
    }

    /// The key is appended to `out`, which may arrive with spare capacity.
    #[test]
    fn an_entry_key_only_extends_its_buffer() {
        let log = journal();
        let mut out = Vec::with_capacity(log.max_entry_key_len());
        log.entry_key(42, &mut out);
        assert_eq!(out, b"s/arrival/7/row/00000000000000000042");
    }

    proptest! {
        /// Entry keys sort as their positions do, so a page of the journal
        /// is a key range, and keep the form journals were written in.
        #[test]
        fn entry_keys_sort_in_position_order(a in any::<u64>(), b in any::<u64>()) {
            let log = journal();
            prop_assert_eq!(entry(&log, a).cmp(&entry(&log, b)), a.cmp(&b));
            prop_assert_eq!(
                entry(&log, a),
                format!("s/arrival/7/row/{a:020}").into_bytes()
            );
        }
    }
}
