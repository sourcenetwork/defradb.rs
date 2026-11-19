//! Reads and writes of owed changes, through whatever transaction runs them.
//!
//! The counts of what is owed (per object, per actor a delete names on an
//! object, and for the node) are owed values
//! ([`storage::merge::owed`]): a staged part that only [`stage`] adds to and
//! a settled part that only [`settle`] adds to, both by blind merges. What is
//! owed is the difference. A count is never put or deleted, so a reader of
//! its staged part is refused at commit only by staging, never by a settle
//! (`proofs/tla/OwedParts.tla`, `proofs/lean/OwedParts.lean`). A fully
//! settled count stays as one small folded operand and reads as nothing
//! owed, exactly as an absent key does.
//!
//! A tuple keeps no count: its live sequences are its record of what is owed
//! on it (`proofs/lean/OwedLive.lean`), and stagers of one tuple are ordered
//! by its head key, which only staging reads and writes.

use storage::corekv::{IterOptions, Reader, Store, Writer};
use storage::merge::owed::{self, part, Counters};

use super::keys::{self, OwedTupleState};
use super::record::{
    NewOwedChange, OwedAuthority, OwedChange, OwedOp, OwedState, OwedSubject, OwedTuple,
};
use crate::error::{Error, Result};
use crate::relation::OWNER_RELATION;

fn storage_error(error: storage::corekv::Error) -> Error {
    Error::Storage(error.to_string())
}

fn inconsistent(what: &str, seq: u64) -> Error {
    Error::Storage(format!("owed ACP change {seq} has no {what}"))
}

/// The first counter of a count: changes on the node; deletes wider than one
/// actor on an object; deletes naming an actor on an object.
pub(super) const FIRST: usize = 0;
/// The second counter: deletes of the owner relation naming an actor on an
/// object. The node's and an object's leave it at zero.
pub(super) const SECOND: usize = 1;
/// How far the settled part sits from the staged one.
const SETTLED: usize = 2;

/// What a count owes, of its first and of its second counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Owed {
    pub(super) first: u64,
    pub(super) second: u64,
}

impl Owed {
    fn of(counters: Counters) -> Self {
        Self {
            first: counters[FIRST].saturating_sub(counters[FIRST + SETTLED]),
            second: counters[SECOND].saturating_sub(counters[SECOND + SETTLED]),
        }
    }
}

/// The parts a reader names to decide on the staged `counters`.
pub(super) fn staged_parts(counters: &[usize]) -> Vec<u32> {
    counters.iter().map(|&counter| part(counter)).collect()
}

/// What the count at `key` owes, read through `parts`: validated at commit
/// against staging that touches one of them, never against a settle. No
/// parts reads it unvalidated. An absent count owes nothing.
pub(super) async fn owed_at<R>(reader: &R, key: &[u8], parts: &[u32]) -> Result<Owed>
where
    R: Reader + ?Sized,
{
    let Some(bytes) = reader.get_parts(key, parts).await.map_err(storage_error)? else {
        return Ok(Owed::default());
    };
    owed::read(bytes.as_ref())
        .map(Owed::of)
        .ok_or_else(|| Error::Serialization("owed ACP count is malformed".into()))
}

/// What a live key holds: whether its change is a delete.
const LIVE_ADD: &[u8] = &[0];
const LIVE_DELETE: &[u8] = &[1];

/// What is unsettled on one tuple, from its live sequences: the first and
/// last are its oldest and latest, and how many there are, and how many of
/// them are deletes, are its counts. Only settling passes and reports ask,
/// in transactions of their own.
pub(super) async fn tuple_state<R>(reader: &R, tuple: &OwedTuple) -> Result<Option<OwedTupleState>>
where
    R: Reader + ?Sized,
{
    let prefix = keys::live_prefix(tuple);
    let mut iter = reader
        .iterator(IterOptions::new().with_prefix(prefix.clone()))
        .await
        .map_err(storage_error)?;
    let mut state: Option<OwedTupleState> = None;
    while let Some(entry) = iter.next().await.map_err(storage_error)? {
        let seq = keys::live_seq(&entry.key, &prefix)?;
        let delete = match entry.value.as_ref() {
            LIVE_ADD => 0,
            LIVE_DELETE => 1,
            _ => {
                return Err(Error::Serialization(
                    "owed ACP live key is malformed".into(),
                ))
            }
        };
        state = Some(match state {
            Some(state) => OwedTupleState {
                latest: seq,
                unsettled: state.unsettled.saturating_add(1),
                deletes: state.deletes.saturating_add(delete),
                ..state
            },
            None => OwedTupleState {
                oldest: seq,
                latest: seq,
                unsettled: 1,
                deletes: delete,
            },
        });
    }
    Ok(state)
}

/// How many changes are unsettled on one object: its tuples' live
/// sequences. Only reports and tests ask; no decision rests on it.
pub(super) async fn unsettled_on<R>(
    reader: &R,
    policy_id: &str,
    resource: &str,
    object: &str,
) -> Result<u64>
where
    R: Reader + ?Sized,
{
    let prefix = keys::object_live_prefix(policy_id, resource, object);
    let mut iter = reader
        .iterator(IterOptions::new().with_prefix(prefix))
        .await
        .map_err(storage_error)?;
    let mut count = 0;
    while iter.next().await.map_err(storage_error)?.is_some() {
        count += 1;
    }
    Ok(count)
}

/// The node's count, read without validation: every staging and settling
/// transaction merges into it, and none may conflict on it.
pub(super) async fn unsettled<R>(reader: &R) -> Result<u64>
where
    R: Reader + ?Sized,
{
    Ok(owed_at(reader, keys::UNSETTLED_KEY, &[]).await?.first)
}

/// The node's count, read validated on what was staged only: a transaction
/// that stages and commits after this read aborts the reader, whose re-run
/// reads again; a settle never does, and only lowers what is owed.
pub(super) async fn unsettled_validated<R>(reader: &R) -> Result<u64>
where
    R: Reader + ?Sized,
{
    let parts = staged_parts(&[FIRST]);
    Ok(owed_at(reader, keys::UNSETTLED_KEY, &parts).await?.first)
}

/// How many changes one acting DID has unsettled, read without validation as
/// the node's count is.
pub(super) async fn unsettled_by<R>(reader: &R, actor: &identity::Did) -> Result<u64>
where
    R: Reader + ?Sized,
{
    let Some(bytes) = reader
        .get_parts(&keys::actor_key(actor), &[])
        .await
        .map_err(storage_error)?
    else {
        return Ok(0);
    };
    let count: [u8; 8] = bytes
        .as_ref()
        .try_into()
        .map_err(|_| Error::Serialization("owed ACP count is malformed".into()))?;
    Ok(u64::try_from(i64::from_be_bytes(count)).unwrap_or(0))
}

pub(super) async fn change<R>(reader: &R, seq: u64) -> Result<Option<OwedChange>>
where
    R: Reader + ?Sized,
{
    reader
        .get(&keys::queue_key(seq))
        .await
        .map_err(storage_error)?
        .map(|bytes| OwedChange::decode(&bytes))
        .transpose()
}

/// The changes after `after` in sequence order, until their records pass
/// `max_bytes`: at least one when any is left.
pub(super) async fn page<R>(
    reader: &R,
    after: Option<u64>,
    max_bytes: usize,
) -> Result<Vec<OwedChange>>
where
    R: Reader + ?Sized,
{
    let mut options = IterOptions::new().with_prefix(keys::queue_prefix());
    match after {
        Some(u64::MAX) => return Ok(Vec::new()),
        Some(seq) => options = options.with_start(keys::queue_key(seq + 1)),
        None => {}
    }
    let mut iter = reader.iterator(options).await.map_err(storage_error)?;
    let mut page = Vec::new();
    let mut bytes = 0;
    while bytes < max_bytes {
        let Some(entry) = iter.next().await.map_err(storage_error)? else {
            break;
        };
        bytes += entry.value.len();
        page.push(OwedChange::decode(&entry.value)?);
    }
    Ok(page)
}

/// How many changes the store still owes in the layout before the counts
/// were split ([`keys::LEGACY_QUEUE_PREFIX`]): one record each, deleted when it
/// settled. The rest of that layout (counts settled to zero, the sequence
/// counter, notes of discarded changes) owes nothing and is not counted.
pub(crate) async fn legacy_changes<R>(reader: &R) -> Result<u64>
where
    R: Reader + ?Sized,
{
    let options = IterOptions::new().with_prefix(keys::LEGACY_QUEUE_PREFIX.to_vec());
    let mut iter = reader.iterator(options).await.map_err(storage_error)?;
    let mut count = 0;
    while iter.next().await.map_err(storage_error)?.is_some() {
        count += 1;
    }
    Ok(count)
}

async fn put<T>(txn: &T, key: &[u8], value: &[u8]) -> Result<()>
where
    T: Writer + ?Sized,
{
    txn.set_shared(key, value).await.map_err(storage_error)
}

async fn remove<T>(txn: &T, key: &[u8]) -> Result<()>
where
    T: Writer + ?Sized,
{
    txn.delete_shared(key).await.map_err(storage_error)
}

async fn merge<T>(txn: &T, key: &[u8], operand: &[u8]) -> Result<()>
where
    T: Writer + ?Sized,
{
    txn.merge_shared(key, operand).await.map_err(storage_error)
}

/// The amounts one change adds to a count: to the staged part when it is
/// staged, to the settled part when it is settled.
fn amounts(first: bool, second: bool, settled: bool) -> Counters {
    let at = if settled { SETTLED } else { 0 };
    let mut counters = [0; owed::COUNTERS];
    counters[FIRST + at] = u64::from(first);
    counters[SECOND + at] = u64::from(second);
    counters
}

/// Merge one change into every count it is part of, staged or settled.
/// Only merges: no count is read here, and none is put or deleted.
async fn account<T>(txn: &T, change: &OwedChange, settled: bool) -> Result<()>
where
    T: Writer + ?Sized,
{
    let tuple = &change.tuple;
    let delete = change.op == OwedOp::Delete;
    let delta = |first, second| owed::delta(amounts(first, second, settled));
    if delete && !matches!(tuple.subject, OwedSubject::Actor { .. }) {
        let object = keys::object_key(&tuple.policy_id, &tuple.resource, &tuple.object);
        merge(txn, &object, &delta(true, false)).await?;
    }
    if let (true, OwedSubject::Actor { did }) = (delete, &tuple.subject) {
        let subject = keys::subject_key(&tuple.policy_id, &tuple.resource, &tuple.object, did);
        merge(
            txn,
            &subject,
            &delta(true, tuple.relation == OWNER_RELATION),
        )
        .await?;
    }
    merge(txn, keys::UNSETTLED_KEY, &delta(true, false)).await?;
    let actor = storage::merge::counter_i64(if settled { -1 } else { 1 });
    merge(txn, &keys::actor_key(&change.actor), &actor).await
}

/// Record `new` in `txn`. The tuple's head key is read before the sequence
/// is allocated and written after: a transaction that stages the same tuple
/// and commits first aborts this one, and the attempt that then commits read
/// that commit before it took its own, later, sequence. Settling never
/// touches the head key, so a settle of the tuple aborts nothing.
pub(super) async fn stage<T>(
    txn: &T,
    sequences: &dyn Store,
    new: NewOwedChange,
) -> Result<OwedChange>
where
    T: Reader + Writer + ?Sized,
{
    let head_key = keys::head_key(&new.tuple);
    txn.get(&head_key).await.map_err(storage_error)?;
    let seq = sequences
        .allocate(keys::SEQUENCE_KEY, 1)
        .await
        .map_err(storage_error)?
        .start;
    let change = OwedChange::staged(seq, new);
    let record = change.encode_admitted()?;

    put(txn, &keys::queue_key(seq), &record).await?;
    put(txn, &head_key, &seq.to_be_bytes()).await?;
    let live = if change.op == OwedOp::Delete {
        LIVE_DELETE
    } else {
        LIVE_ADD
    };
    put(txn, &keys::live_key(&change.tuple, seq), live).await?;
    account(txn, &change, false).await?;
    Ok(change)
}

/// Delete the record of change `seq`, which the provider now holds or its
/// owner gave up, and merge it into the settled part of every count it is in.
/// `false` when no such change is owed. The record, read validated, is what
/// makes a settle count only a change still owed: two settles of one change
/// conflict on it, and the second finds nothing.
pub(super) async fn settle<T>(txn: &T, seq: u64) -> Result<bool>
where
    T: Reader + Writer + ?Sized,
{
    let Some(settled) = change(txn, seq).await? else {
        return Ok(false);
    };
    let live = keys::live_key(&settled.tuple, seq);
    if txn.get(&live).await.map_err(storage_error)?.is_none() {
        return Err(inconsistent("live sequence", seq));
    }
    remove(txn, &keys::queue_key(seq)).await?;
    remove(txn, &live).await?;
    account(txn, &settled, true).await?;
    Ok(true)
}

/// Record what a submission of change `seq` came to: its state, how many
/// submissions in a row the provider answered this way, and its words. Only
/// the change's own record is written; its tuple's state and the counts stay
/// as they are, since the change is as unsettled as before. `false` when no
/// such change is owed.
///
/// The provider's words are cut until the record fits.
pub(super) async fn mark<T>(
    txn: &T,
    seq: u64,
    state: OwedState,
    attempts: u32,
    error: Option<String>,
) -> Result<bool>
where
    T: Reader + Writer + ?Sized,
{
    let Some(mut marked) = change(txn, seq).await? else {
        return Ok(false);
    };
    marked.attempts = attempts;
    marked.state = state;
    marked.last_error = error;
    let record = loop {
        match marked.encode() {
            Ok(record) => break record,
            Err(Error::OwedChangeTooLarge { .. }) if marked.shorten_reason() => {}
            Err(error) => return Err(error),
        }
    };
    put(txn, &keys::queue_key(seq), &record).await?;
    Ok(true)
}

/// Record a fresh authority with change `seq` and make it owed again: staged,
/// with no submission counted against it and no error kept. Only the
/// change's own record is written. `false` when no such change is owed.
pub(super) async fn reauthorize<T>(txn: &T, seq: u64, authority: OwedAuthority) -> Result<bool>
where
    T: Reader + Writer + ?Sized,
{
    let Some(mut owed) = change(txn, seq).await? else {
        return Ok(false);
    };
    owed.authority = authority;
    owed.state = OwedState::Staged;
    owed.attempts = 0;
    owed.last_error = None;
    put(txn, &keys::queue_key(seq), &owed.encode_admitted()?).await?;
    Ok(true)
}

/// The most notes of discarded changes a node keeps. A note is deleted when
/// its change's actor is told; past this bound the oldest goes first.
pub const MAX_DISCARD_NOTES: usize = 1_000;

/// Delete the record of change `seq` without the provider holding it, as
/// [`settle`] does, and keep a note of it and of `reason` for its actor. The
/// note holds the change and no credential. `false` when no such change is
/// owed.
pub(super) async fn discard<T>(txn: &T, seq: u64, reason: &str) -> Result<bool>
where
    T: Reader + Writer + ?Sized,
{
    let Some(owed) = change(txn, seq).await? else {
        return Ok(false);
    };
    if !settle(txn, seq).await? {
        return Ok(false);
    }
    let mut note = owed.discarded(reason.to_owned());
    let record = loop {
        match note.encode() {
            Ok(record) => break record,
            Err(Error::OwedChangeTooLarge { .. }) if note.shorten_reason() => {}
            Err(error) => return Err(error),
        }
    };
    let options = IterOptions::new().with_prefix(keys::discard_prefix());
    let mut iter = txn.iterator(options).await.map_err(storage_error)?;
    let mut oldest = None;
    let mut kept = 0;
    while let Some(entry) = iter.next().await.map_err(storage_error)? {
        oldest.get_or_insert(entry.key);
        kept += 1;
    }
    drop(iter);
    if let (Some(oldest), true) = (oldest, kept >= MAX_DISCARD_NOTES) {
        remove(txn, &oldest).await?;
    }
    put(txn, &keys::discard_key(seq), &record).await?;
    Ok(true)
}

/// The note of the discarded change `seq`, if one is kept.
pub(super) async fn discarded<R>(reader: &R, seq: u64) -> Result<Option<OwedChange>>
where
    R: Reader + ?Sized,
{
    reader
        .get(&keys::discard_key(seq))
        .await
        .map_err(storage_error)?
        .map(|bytes| OwedChange::decode(&bytes))
        .transpose()
}

/// Every note of a discarded change, in sequence order. There are at most
/// [`MAX_DISCARD_NOTES`].
pub(super) async fn discards<R>(reader: &R) -> Result<Vec<OwedChange>>
where
    R: Reader + ?Sized,
{
    let options = IterOptions::new().with_prefix(keys::discard_prefix());
    let mut iter = reader.iterator(options).await.map_err(storage_error)?;
    let mut notes = Vec::new();
    while let Some(entry) = iter.next().await.map_err(storage_error)? {
        notes.push(OwedChange::decode(&entry.value)?);
    }
    Ok(notes)
}

/// Delete the note of the discarded change `seq`: its actor was told.
pub(super) async fn acknowledge<T>(txn: &T, seq: u64) -> Result<()>
where
    T: Writer + ?Sized,
{
    remove(txn, &keys::discard_key(seq)).await
}
