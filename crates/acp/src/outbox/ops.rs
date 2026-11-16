//! Reads and writes of owed changes, through whatever transaction runs them.

use storage::corekv::{IterOptions, Reader, Store, Writer};

use super::keys::{self, OwedTupleState};
use super::record::{NewOwedChange, OwedAuthority, OwedChange, OwedOp, OwedState, OwedTuple};
use crate::error::{Error, Result};

fn storage_error(error: storage::corekv::Error) -> Error {
    Error::Storage(error.to_string())
}

fn inconsistent(what: &str, seq: u64) -> Error {
    Error::Storage(format!("owed ACP change {seq} has no {what}"))
}

pub(super) async fn tuple_state<R>(reader: &R, tuple: &OwedTuple) -> Result<Option<OwedTupleState>>
where
    R: Reader + ?Sized,
{
    reader
        .get(&keys::tuple_key(tuple))
        .await
        .map_err(storage_error)?
        .map(|bytes| OwedTupleState::decode(&bytes))
        .transpose()
}

pub(super) async fn unsettled_on<R>(
    reader: &R,
    policy_id: &str,
    resource: &str,
    object: &str,
) -> Result<u64>
where
    R: Reader + ?Sized,
{
    reader
        .get(&keys::object_key(policy_id, resource, object))
        .await
        .map_err(storage_error)?
        .map_or(Ok(0), |bytes| keys::decode_count(&bytes))
}

/// The node's count, read without validation: every staging and settling
/// transaction merges into it, and none may conflict on it.
pub(super) async fn unsettled<R>(reader: &R) -> Result<u64>
where
    R: Reader + ?Sized,
{
    merged_count(reader, keys::UNSETTLED_KEY).await
}

/// How many changes one acting DID has unsettled, read without validation as
/// the node's count is.
pub(super) async fn unsettled_by<R>(reader: &R, actor: &identity::Did) -> Result<u64>
where
    R: Reader + ?Sized,
{
    merged_count(reader, &keys::actor_key(actor)).await
}

async fn merged_count<R>(reader: &R, key: &[u8]) -> Result<u64>
where
    R: Reader + ?Sized,
{
    let Some(bytes) = reader.get_parts(key, &[]).await.map_err(storage_error)? else {
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

/// The relation, subject and state of every tuple owed on one object.
pub(super) async fn tuples_on<R>(
    reader: &R,
    policy_id: &str,
    resource: &str,
    object: &str,
) -> Result<Vec<(String, super::OwedSubject, OwedTupleState)>>
where
    R: Reader + ?Sized,
{
    let prefix = keys::object_tuples_prefix(policy_id, resource, object);
    let mut iter = reader
        .iterator(IterOptions::new().with_prefix(prefix.clone()))
        .await
        .map_err(storage_error)?;
    let mut tuples = Vec::new();
    while let Some(entry) = iter.next().await.map_err(storage_error)? {
        let (relation, subject) = keys::relation_and_subject(&entry.key, &prefix)?;
        tuples.push((relation, subject, OwedTupleState::decode(&entry.value)?));
    }
    Ok(tuples)
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

/// Move the node's count and the acting DID's by `delta`. Both are blind
/// merges, so no two transactions conflict on them.
async fn count<T>(txn: &T, actor: &identity::Did, delta: i64) -> Result<()>
where
    T: Writer + ?Sized,
{
    let delta = storage::merge::counter_i64(delta);
    txn.merge_shared(keys::UNSETTLED_KEY, &delta)
        .await
        .map_err(storage_error)?;
    txn.merge_shared(&keys::actor_key(actor), &delta)
        .await
        .map_err(storage_error)
}

/// Record `new` in `txn`. The tuple's state is read before the sequence is
/// allocated and written after: a transaction that stages the same tuple and
/// commits first aborts this one, and the attempt that then commits read that
/// commit before it took its own, later, sequence.
pub(super) async fn stage<T>(
    txn: &T,
    sequences: &dyn Store,
    new: NewOwedChange,
) -> Result<OwedChange>
where
    T: Reader + Writer + ?Sized,
{
    let tuple_key = keys::tuple_key(&new.tuple);
    let earlier = tuple_state(txn, &new.tuple).await?;
    let on_object = unsettled_on(
        txn,
        &new.tuple.policy_id,
        &new.tuple.resource,
        &new.tuple.object,
    )
    .await?;
    let seq = sequences
        .allocate(keys::SEQUENCE_KEY, 1)
        .await
        .map_err(storage_error)?
        .start;
    let change = OwedChange::staged(seq, new);
    let record = change.encode_admitted()?;

    put(txn, &keys::queue_key(seq), &record).await?;
    let delete = u32::from(change.op == OwedOp::Delete);
    let state = match earlier {
        Some(earlier) => OwedTupleState {
            oldest: earlier.oldest,
            latest: seq,
            unsettled: earlier.unsettled + 1,
            deletes: earlier.deletes + delete,
        },
        None => OwedTupleState {
            oldest: seq,
            latest: seq,
            unsettled: 1,
            deletes: delete,
        },
    };
    put(txn, &tuple_key, &state.encode()).await?;
    let object_key = keys::object_key(
        &change.tuple.policy_id,
        &change.tuple.resource,
        &change.tuple.object,
    );
    put(txn, &object_key, &keys::encode_count(on_object + 1)).await?;
    count(txn, &change.actor, 1).await?;
    Ok(change)
}

/// The lowest and highest sequences of `tuple` in the queue, `settled` aside.
async fn remaining<R>(reader: &R, tuple: &OwedTuple, settled: u64) -> Result<Option<(u64, u64)>>
where
    R: Reader + ?Sized,
{
    let options = IterOptions::new().with_prefix(keys::queue_prefix());
    let mut iter = reader.iterator(options).await.map_err(storage_error)?;
    let mut bounds = None;
    while let Some(entry) = iter.next().await.map_err(storage_error)? {
        let change = OwedChange::decode(&entry.value)?;
        if change.seq != settled && change.tuple == *tuple {
            bounds = Some((bounds.map_or(change.seq, |(oldest, _)| oldest), change.seq));
        }
    }
    Ok(bounds)
}

/// Delete the record of change `seq`, which the provider now holds or its
/// owner gave up, and keep its tuple's state and both counts true. `false`
/// when no such change is owed.
///
/// When other changes of the tuple stay owed and `seq` was its oldest or its
/// latest, the queue is read once to find which of them now is.
pub(super) async fn settle<T>(txn: &T, seq: u64) -> Result<bool>
where
    T: Reader + Writer + ?Sized,
{
    let Some(settled) = change(txn, seq).await? else {
        return Ok(false);
    };
    let tuple = &settled.tuple;
    let state = tuple_state(txn, tuple)
        .await?
        .ok_or_else(|| inconsistent("tuple state", seq))?;
    let delete = u32::from(settled.op == OwedOp::Delete);
    if state.deletes < delete {
        return Err(inconsistent("delete its tuple counts", seq));
    }
    let on_object = unsettled_on(txn, &tuple.policy_id, &tuple.resource, &tuple.object).await?;
    if on_object == 0 {
        return Err(inconsistent("object count", seq));
    }

    remove(txn, &keys::queue_key(seq)).await?;
    let tuple_key = keys::tuple_key(tuple);
    if state.unsettled <= 1 {
        remove(txn, &tuple_key).await?;
    } else {
        let (oldest, latest) = if seq == state.oldest || seq == state.latest {
            remaining(txn, tuple, seq)
                .await?
                .ok_or_else(|| inconsistent("other change its tuple counts", seq))?
        } else {
            (state.oldest, state.latest)
        };
        let state = OwedTupleState {
            oldest,
            latest,
            unsettled: state.unsettled - 1,
            deletes: state.deletes - delete,
        };
        put(txn, &tuple_key, &state.encode()).await?;
    }
    let object_key = keys::object_key(&tuple.policy_id, &tuple.resource, &tuple.object);
    if on_object == 1 {
        remove(txn, &object_key).await?;
    } else {
        put(txn, &object_key, &keys::encode_count(on_object - 1)).await?;
    }
    count(txn, &settled.actor, -1).await?;
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
