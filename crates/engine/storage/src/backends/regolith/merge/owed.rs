//! Owed state: what a node owes a remote ACP provider, as four counters per
//! value, written only by blind merges.
//!
//! Counters 0 and 1 are the staged part, which only the transactions that
//! stage owed changes add to; counters 2 and 3 are the settled part, which
//! only settling adds to. What is owed is staged minus settled. Each counter
//! is one part of the value ([`part`]): an operand touches exactly the
//! counters it adds to, so a reader that decided on the staged part is never
//! refused at commit by a settle. Folding adds counter by counter, which is
//! exact, associative and commutative, and a fold touches what either of its
//! operands touches (`proofs/lean/OwedParts.lean`).
//!
//! A value is the four counters as eight big-endian bytes each; an absent
//! key reads as four zeros. An operand is the four amounts to add, after the
//! operator's header. An overflowing counter fails the merge.

use super::{decode, encode, OWED};

/// How many counters an owed value holds.
pub const COUNTERS: usize = 4;

const COUNTER_BYTES: usize = 8;

/// Four counters: staged at 0 and 1, settled at 2 and 3.
pub type Counters = [u64; COUNTERS];

/// The part a reader names to decide on counter `index`.
pub const fn part(index: usize) -> u32 {
    index as u32
}

/// The operand that adds `amounts` to an owed value.
pub fn delta(amounts: Counters) -> Vec<u8> {
    encode(OWED, &to_bytes(&amounts))
}

/// The counters an owed value holds, or `None` when the bytes are not one.
pub fn read(value: &[u8]) -> Option<Counters> {
    from_bytes(value)
}

fn to_bytes(counters: &Counters) -> Vec<u8> {
    counters
        .iter()
        .flat_map(|counter| counter.to_be_bytes())
        .collect()
}

fn from_bytes(bytes: &[u8]) -> Option<Counters> {
    let (chunks, []) = bytes.as_chunks::<COUNTER_BYTES>() else {
        return None;
    };
    if chunks.len() != COUNTERS {
        return None;
    }
    let mut counters = [0; COUNTERS];
    for (counter, chunk) in counters.iter_mut().zip(chunks) {
        *counter = u64::from_be_bytes(*chunk);
    }
    Some(counters)
}

fn operand(bytes: &[u8]) -> Option<Counters> {
    match decode(bytes)? {
        (OWED, amounts) => from_bytes(amounts),
        _ => None,
    }
}

fn add(left: Counters, right: Counters) -> Option<Counters> {
    let mut sum = [0; COUNTERS];
    for (index, counter) in sum.iter_mut().enumerate() {
        *counter = left[index].checked_add(right[index])?;
    }
    Some(sum)
}

/// `base`, or zeros, with every operand added in turn.
pub(super) fn full(base: Option<&[u8]>, operands: &[&[u8]]) -> Option<Vec<u8>> {
    let mut value = match base {
        Some(bytes) => from_bytes(bytes)?,
        None => [0; COUNTERS],
    };
    for bytes in operands {
        value = add(value, operand(bytes)?)?;
    }
    Some(to_bytes(&value))
}

/// Two operands as one: their amounts added counter by counter.
pub(super) fn fold(left: &[u8], right: &[u8]) -> Option<Vec<u8>> {
    Some(delta(add(operand(left)?, operand(right)?)?))
}

/// Whether the operand adds to a counter `parts` names. A malformed operand
/// touches everything.
pub(super) fn touches(bytes: &[u8], parts: &[u32]) -> bool {
    match operand(bytes) {
        Some(amounts) => amounts
            .iter()
            .enumerate()
            .any(|(index, amount)| *amount != 0 && parts.contains(&part(index))),
        None => true,
    }
}
