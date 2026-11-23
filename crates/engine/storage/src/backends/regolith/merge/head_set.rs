//! Head sets: the tips of a document's or a collection's DAG, one value per
//! set, written only by blind merges.
//!
//! A set holds names, each one head: a document set names (field, CID) as the
//! key bytes after the document's prefix, a collection set names a CID. The
//! caller owns the bytes of a name; the operator only orders them. The stored
//! value says, per name, one of two states: live with its priority, or ahead
//! (superseded before its own head arrived). A name in neither is not stored.
//!
//! A write is a [`Write`]: per name, one of three entries, each a function on
//! the name's state.
//!
//! | entry            | absent  | ahead            | live(q) |
//! |------------------|---------|------------------|---------|
//! | `add(p)`         | live(p) | absent (consumed)| live(q) |
//! | `supersede`      | absent  | ahead            | absent  |
//! | `supersede_ahead`| ahead   | ahead            | absent  |
//!
//! The operand stores, per name, the composition of its entries as the images
//! of the three states (absent, ahead, live): each image is the identity or a
//! constant state. Function composition is associative, so `partial_merge`
//! folds any run, in any grouping, to exactly what `full_merge` gives over the
//! unfolded run. Operands and values name their names in ascending byte order
//! and nothing else, so a fold is one walk over two sorted lists and a value
//! is canonical. Malformed bytes make every function here answer `None`.
//!
//! Each name's state is one part of the value ([`part`]): an operand touches
//! exactly the names it holds, so a reader that decided on some names reads
//! their parts and is refused at commit only by an operand that holds one of
//! them. A fold holds the union of the names of what it folds (no composition
//! of these entries is the identity), so the answer survives folding.
use std::collections::BTreeMap;

use super::{decode, encode, HEAD_SET, HEAD_SET_STATE};

/// What one name is in a set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Absent,
    Ahead,
    Live(u64),
}

/// What a function does to the states of one class: leave them, or send them
/// to one state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Image {
    Keep,
    To(State),
}

impl Image {
    fn resolve(self, input: State) -> State {
        match self {
            Self::Keep => input,
            Self::To(state) => state,
        }
    }
}

/// A name's function on states, as the images of absent, ahead and live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Func {
    absent: Image,
    ahead: Image,
    live: Image,
}

impl Func {
    const IDENTITY: Self = Self {
        absent: Image::Keep,
        ahead: Image::Keep,
        live: Image::Keep,
    };
    const SUPERSEDE: Self = Self {
        absent: Image::Keep,
        ahead: Image::Keep,
        live: Image::To(State::Absent),
    };
    const SUPERSEDE_AHEAD: Self = Self {
        absent: Image::To(State::Ahead),
        ahead: Image::Keep,
        live: Image::To(State::Absent),
    };

    fn add(priority: u64) -> Self {
        Self {
            absent: Image::To(State::Live(priority)),
            ahead: Image::To(State::Absent),
            live: Image::Keep,
        }
    }

    fn apply(self, state: State) -> State {
        match state {
            State::Absent => self.absent.resolve(state),
            State::Ahead => self.ahead.resolve(state),
            State::Live(_) => self.live.resolve(state),
        }
    }

    /// The function that applies `self`, then `next`.
    fn then(self, next: Self) -> Self {
        let chain = |first: Image, second: Image| match first {
            Image::Keep => second,
            Image::To(mid) => Image::To(next.apply(mid)),
        };
        Self {
            absent: chain(self.absent, next.absent),
            ahead: chain(self.ahead, next.ahead),
            live: chain(self.live, next.live),
        }
        .canonical()
    }

    /// Sending absent to absent, or ahead to ahead, is leaving them: one
    /// spelling per function.
    fn canonical(mut self) -> Self {
        if self.absent == Image::To(State::Absent) {
            self.absent = Image::Keep;
        }
        if self.ahead == Image::To(State::Ahead) {
            self.ahead = Image::Keep;
        }
        self
    }
}

fn put_uvarint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn take_uvarint(bytes: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        let low = u64::from(byte & 0x7f);
        // The tenth byte may carry only the top bit of a u64.
        if index == 9 && low > 1 {
            return None;
        }
        value |= low << (7 * index);
        if byte & 0x80 == 0 {
            *bytes = &bytes[index + 1..];
            return Some(value);
        }
    }
    None
}

fn take_byte(bytes: &mut &[u8]) -> Option<u8> {
    let (&first, rest) = bytes.split_first()?;
    *bytes = rest;
    Some(first)
}

fn take_name<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
    let len = usize::try_from(take_uvarint(bytes)?).ok()?;
    let (name, rest) = bytes.split_at_checked(len)?;
    *bytes = rest;
    Some(name)
}

fn put_name(out: &mut Vec<u8>, name: &[u8]) {
    put_uvarint(out, name.len() as u64);
    out.extend_from_slice(name);
}

const KEEP: u8 = 0;
const TO_ABSENT: u8 = 1;
const TO_AHEAD: u8 = 2;
const TO_LIVE: u8 = 3;

fn put_image(out: &mut Vec<u8>, image: Image) {
    match image {
        Image::Keep => out.push(KEEP),
        Image::To(State::Absent) => out.push(TO_ABSENT),
        Image::To(State::Ahead) => out.push(TO_AHEAD),
        Image::To(State::Live(priority)) => {
            out.push(TO_LIVE);
            put_uvarint(out, priority);
        }
    }
}

fn take_image(bytes: &mut &[u8]) -> Option<Image> {
    Some(match take_byte(bytes)? {
        KEEP => Image::Keep,
        TO_ABSENT => Image::To(State::Absent),
        TO_AHEAD => Image::To(State::Ahead),
        TO_LIVE => Image::To(State::Live(take_uvarint(bytes)?)),
        _ => return None,
    })
}

/// Every entry of `body`, in the strictly ascending order of its names.
fn list<'a, T>(
    mut body: &'a [u8],
    mut entry: impl FnMut(&mut &'a [u8]) -> Option<T>,
) -> Option<Vec<(&'a [u8], T)>> {
    let count = take_uvarint(&mut body)?;
    let mut entries: Vec<(&[u8], T)> = Vec::new();
    for _ in 0..count {
        let name = take_name(&mut body)?;
        if entries.last().is_some_and(|(last, _)| *last >= name) {
            return None;
        }
        entries.push((name, entry(&mut body)?));
    }
    body.is_empty().then_some(entries)
}

fn operand_entries(body: &[u8]) -> Option<Vec<(&[u8], Func)>> {
    list(body, |bytes| {
        Some(
            Func {
                absent: take_image(bytes)?,
                ahead: take_image(bytes)?,
                live: take_image(bytes)?,
            }
            .canonical(),
        )
    })
}

fn state_entries(body: &[u8]) -> Option<Vec<(&[u8], State)>> {
    list(body, |bytes| match take_byte(bytes)? {
        0 => Some(State::Ahead),
        1 => Some(State::Live(take_uvarint(bytes)?)),
        _ => None,
    })
}

fn encode_operand<'a>(entries: impl ExactSizeIterator<Item = (&'a [u8], Func)>) -> Vec<u8> {
    let mut body = Vec::new();
    put_uvarint(&mut body, entries.len() as u64);
    for (name, func) in entries {
        put_name(&mut body, name);
        put_image(&mut body, func.absent);
        put_image(&mut body, func.ahead);
        put_image(&mut body, func.live);
    }
    encode(HEAD_SET, &body)
}

fn encode_state(set: &BTreeMap<&[u8], State>) -> Vec<u8> {
    let mut body = Vec::new();
    put_uvarint(&mut body, set.len() as u64);
    for (name, state) in set {
        put_name(&mut body, name);
        match state {
            State::Live(priority) => {
                body.push(1);
                put_uvarint(&mut body, *priority);
            }
            State::Ahead | State::Absent => body.push(0),
        }
    }
    encode(HEAD_SET_STATE, &body)
}

fn operand_body(bytes: &[u8]) -> Option<&[u8]> {
    let (HEAD_SET, body) = decode(bytes)? else {
        return None;
    };
    Some(body)
}

/// The stored value after `operands`, in order, onto `base`.
pub(super) fn full<'a>(base: Option<&'a [u8]>, operands: &[&'a [u8]]) -> Option<Vec<u8>> {
    let mut set: BTreeMap<&[u8], State> = match base {
        Some(bytes) => {
            let (HEAD_SET_STATE, body) = decode(bytes)? else {
                return None;
            };
            state_entries(body)?.into_iter().collect()
        }
        None => BTreeMap::new(),
    };
    for &operand in operands {
        for (name, func) in operand_entries(operand_body(operand)?)? {
            match func.apply(set.get(name).copied().unwrap_or(State::Absent)) {
                State::Absent => set.remove(name),
                state => set.insert(name, state),
            };
        }
    }
    Some(encode_state(&set))
}

/// One operand that does what `left`, then `right`, does: per name, the
/// composition of the two functions, and a name only one of them mentions
/// keeps its function.
pub(super) fn fold(left: &[u8], right: &[u8]) -> Option<Vec<u8>> {
    let left = operand_entries(operand_body(left)?)?;
    let right = operand_entries(operand_body(right)?)?;
    let (mut left, mut right) = (left.into_iter().peekable(), right.into_iter().peekable());
    let mut folded: Vec<(&[u8], Func)> = Vec::new();
    loop {
        let entry = match (left.peek(), right.peek()) {
            (Some((a, _)), Some((b, _))) if a == b => {
                let ((name, first), (_, second)) = (left.next()?, right.next()?);
                (name, first.then(second))
            }
            (Some((a, _)), Some((b, _))) if a < b => left.next()?,
            (Some(_), None) => left.next()?,
            (_, Some(_)) => right.next()?,
            (None, None) => break,
        };
        if entry.1 != Func::IDENTITY {
            folded.push(entry);
        }
    }
    Some(encode_operand(folded.into_iter()))
}

/// The flag every name's part carries. An id without it is no name's part.
const NAME_PART: u32 = 1 << 31;

/// The part of a set that is `name`'s state, to read by parts: a hash of the
/// name with `NAME_PART` set. Two names may share a part, which only makes
/// a read of one refused by the other's operands too.
pub fn part(name: &[u8]) -> u32 {
    NAME_PART | rapidhash::v3::rapidhash_v3(name) as u32
}

/// Whether the operand `body` touches one of `parts`: whether it holds a name
/// whose part is one of them. No parts, an id that is no name's part, or
/// malformed bytes: true.
pub(super) fn touches(body: &[u8], parts: &[u32]) -> bool {
    if parts.is_empty() || parts.iter().any(|id| id & NAME_PART == 0) {
        return true;
    }
    operand_entries(body)
        .is_none_or(|entries| entries.iter().any(|(name, _)| parts.contains(&part(name))))
}

/// What one writer does to one set: build it, take its operand, and merge
/// that into the set's key. One write per set per transaction.
#[derive(Clone, Debug, Default)]
pub struct Write {
    entries: BTreeMap<Vec<u8>, Func>,
}

impl Write {
    fn then(&mut self, name: &[u8], func: Func) {
        let held = self.entries.entry(name.to_vec()).or_insert(Func::IDENTITY);
        *held = held.then(func);
    }

    /// `name` becomes a head at `priority`; an ahead `name` is consumed.
    pub fn add(&mut self, name: &[u8], priority: u64) {
        self.then(name, Func::add(priority));
    }

    /// `name` stops being a head, and its own head has already arrived.
    pub fn supersede(&mut self, name: &[u8]) {
        self.then(name, Func::SUPERSEDE);
    }

    /// `name` stops being a head and its own head has not arrived: it is held
    /// ahead until its `add`. Only for a name whose head is truly unpublished.
    pub fn supersede_ahead(&mut self, name: &[u8]) {
        self.then(name, Func::SUPERSEDE_AHEAD);
    }

    /// True when the write changes nothing, so there is no operand to merge.
    pub fn is_empty(&self) -> bool {
        self.entries.values().all(|func| *func == Func::IDENTITY)
    }

    /// The operand that carries this write.
    pub fn operand(&self) -> Vec<u8> {
        let live: Vec<(&[u8], Func)> = self
            .entries
            .iter()
            .filter(|(_, func)| **func != Func::IDENTITY)
            .map(|(name, func)| (name.as_slice(), *func))
            .collect();
        encode_operand(live.into_iter())
    }
}

/// A stored head set, decoded: its live and ahead names in ascending order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Set {
    names: Vec<(Vec<u8>, State)>,
}

impl Set {
    /// Apply an operand to an already decoded snapshot, using the same
    /// transitions as the engine fold. This updates a read cache, not storage.
    pub fn apply(&mut self, write: &Write) {
        for (name, func) in &write.entries {
            let position = self.names.binary_search_by(|(held, _)| held.cmp(name));
            let previous = position.map(|at| self.names[at].1).unwrap_or(State::Absent);
            match (position, func.apply(previous)) {
                (Ok(at), State::Absent) => {
                    self.names.remove(at);
                }
                (Err(_), State::Absent) => {}
                (Ok(at), state) => self.names[at].1 = state,
                (Err(at), state) => self.names.insert(at, (name.clone(), state)),
            }
        }
    }

    /// The set a stored value holds, or `None` when the bytes are not one.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let (HEAD_SET_STATE, body) = decode(bytes)? else {
            return None;
        };
        Some(Self {
            names: state_entries(body)?
                .into_iter()
                .map(|(name, state)| (name.to_vec(), state))
                .collect(),
        })
    }

    /// The live names with their priorities, in ascending name order.
    pub fn live(&self) -> impl Iterator<Item = (&[u8], u64)> + '_ {
        self.names.iter().filter_map(|(name, state)| match state {
            State::Live(priority) => Some((name.as_slice(), *priority)),
            State::Ahead | State::Absent => None,
        })
    }

    /// The ahead names, in ascending order.
    pub fn ahead(&self) -> impl Iterator<Item = &[u8]> + '_ {
        self.names.iter().filter_map(|(name, state)| match state {
            State::Ahead => Some(name.as_slice()),
            State::Live(_) | State::Absent => None,
        })
    }

    /// Whether `name` is live.
    pub fn is_live(&self, name: &[u8]) -> bool {
        self.state(name)
            .is_some_and(|state| matches!(state, State::Live(_)))
    }

    fn state(&self, name: &[u8]) -> Option<State> {
        self.names
            .binary_search_by(|(held, _)| held.as_slice().cmp(name))
            .ok()
            .map(|at| self.names[at].1)
    }
}
