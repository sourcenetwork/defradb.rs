//! The JSON CRDT: every path of a JSON field is its own register.
//!
//! A JSON block carries [`JsonOp`]s, each one edit at one path: set a plain
//! value (a scalar or an array), make the path a map, or delete it. A field
//! stores a [`Frontier`]: for each path, the edits no later edit of that path
//! or of a path above it has seen. Edits racing on one path all stay, and
//! reading resolves them by priority, then block ID; the losers are the
//! conflicts query's answer.
//!
//! Edits at different depths meet two ways. An edit removes the edits it had
//! seen at its own path and below. An edit below a path its author had not
//! seen survives, and a set below a path makes that path a map, so an update
//! beats a concurrent delete. A delete below a path makes nothing.
//!
//! A [`Frontier`] is also an operand: the entries it removes and the entries
//! it adds. Applying one after another is [`Frontier::compose`], which is
//! exact and associative, and an operand never removes an entry its writer had
//! not seen, so concurrent operands commute and a writer of one path never
//! conflicts with a writer of another.
//!
//! The contract is `proofs/tla/JsonCrdt.tla`; the path key encoding and the
//! register laws are proven in `proofs/lean/JsonCrdt`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// A path into a JSON value: its object keys, outermost first. The empty path
/// is the field itself.
pub type Path = Vec<String>;

/// One edit at one path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonEdit {
    /// The path holds this plain value: a scalar or an array, never an object.
    #[serde(rename = "set")]
    Set(Value),
    /// The path holds a map.
    #[serde(rename = "map")]
    Map,
    /// The path is gone.
    #[serde(rename = "del")]
    Delete,
}

/// One edit a JSON block carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonOp {
    pub path: Path,
    pub edit: JsonEdit,
}

/// An edit on a frontier: the op, the block that made it, and the block IDs
/// of the edits above its path that its writer had seen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: Path,
    pub block: Vec<u8>,
    pub priority: u64,
    pub edit: JsonEdit,
    pub seen: Vec<Vec<u8>>,
}

/// A JSON field's stored value, or an operand on it: the entries it removes,
/// by path and block, and the entries it adds. A stored value removes nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frontier {
    pub removes: Vec<(Path, Vec<u8>)>,
    pub entries: Vec<Entry>,
}

/// Removed entries by (path key, block), each with its path and block.
type Removes = BTreeMap<(Vec<u8>, Vec<u8>), (Path, Vec<u8>)>;

/// One edit that lost a race at a path, as the conflicts query lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub path: Path,
    pub block: Vec<u8>,
    pub priority: u64,
    pub edit: JsonEdit,
}

/// The key of a path: each segment with its 0 bytes written `0 255`, then
/// `0 1`. A path's key is a prefix of another's exactly when the path is that
/// one or above it, so a key range holds a path and everything below it.
pub fn encode_path(path: &[String]) -> Vec<u8> {
    let mut key = Vec::with_capacity(path.iter().map(|s| s.len() + 2).sum());
    for segment in path {
        for &byte in segment.as_bytes() {
            key.push(byte);
            if byte == 0 {
                key.push(255);
            }
        }
        key.extend_from_slice(&[0, 1]);
    }
    key
}

/// The path whose key is `key`, if it is one.
pub fn decode_path(mut key: &[u8]) -> Option<Path> {
    let mut path = Vec::new();
    let mut segment = Vec::new();
    while let Some((&byte, rest)) = key.split_first() {
        if byte != 0 {
            segment.push(byte);
            key = rest;
            continue;
        }
        let (&mark, rest) = rest.split_first()?;
        match mark {
            255 => segment.push(0),
            1 => path.push(String::from_utf8(std::mem::take(&mut segment)).ok()?),
            _ => return None,
        }
        key = rest;
    }
    segment.is_empty().then_some(path)
}

/// The ID a JSON block's edits take: a digest of its priority, its parents
/// and its plain ops. Every peer computes it from the block, and the writer
/// from the same three before the block exists.
pub fn block_id(priority: u64, parents: &[Vec<u8>], data: &[u8]) -> Vec<u8> {
    let mut parents: Vec<&Vec<u8>> = parents.iter().collect();
    parents.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(b"defra-json-block\0");
    hasher.update(priority.to_be_bytes());
    hasher.update((parents.len() as u64).to_be_bytes());
    for parent in parents {
        hasher.update((parent.len() as u64).to_be_bytes());
        hasher.update(parent);
    }
    hasher.update((data.len() as u64).to_be_bytes());
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// The `data` of a JSON block carrying `ops`.
pub fn encode_ops(ops: &[JsonOp]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::into_writer(ops, &mut bytes)
        .map_err(|error| Error::Serialization(format!("json ops: {error}")))?;
    Ok(bytes)
}

/// The `data` of a JSON block a local update writes: `ops`, and a nonce
/// drawn for the write. Two writers that make the same edits on the same
/// heads at the same priority then write different data, so their blocks take
/// different IDs ([`block_id`]) and stay two operations, as their CIDs are two
/// blocks.
pub fn encode_write(nonce: i64, ops: &[JsonOp]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::into_writer(&Write { nonce, ops }, &mut bytes)
        .map_err(|error| Error::Serialization(format!("json ops: {error}")))?;
    Ok(bytes)
}

/// A local update's block data: a CBOR map, where a create's is the bare list.
#[derive(Serialize)]
struct Write<'ops> {
    nonce: i64,
    ops: &'ops [JsonOp],
}

#[derive(Deserialize)]
struct WriteOps {
    ops: Vec<JsonOp>,
}

/// The ops a JSON block's `data` holds, from [`encode_ops`] or
/// [`encode_write`].
pub fn decode_ops(bytes: &[u8]) -> Result<Vec<JsonOp>> {
    const CBOR_MAP: u8 = 0xa0;
    let decoded = if bytes.first().is_some_and(|byte| byte & 0xe0 == CBOR_MAP) {
        ciborium::from_reader(bytes).map(|write: WriteOps| write.ops)
    } else {
        ciborium::from_reader(bytes)
    };
    decoded.map_err(|error| Error::Serialization(format!("json ops: {error}")))
}

/// The fewest ops that turn `old` into `new`, both `None` when absent. Maps
/// are compared key by key; anything else that changed is set whole, and an
/// object replacing a non-object becomes a map and a set of each of its keys.
pub fn diff(old: Option<&Value>, new: Option<&Value>) -> Vec<JsonOp> {
    let mut ops = Vec::new();
    diff_at(&mut Vec::new(), old, new, &mut ops);
    ops
}

fn diff_at(path: &mut Path, old: Option<&Value>, new: Option<&Value>, ops: &mut Vec<JsonOp>) {
    let edit = |path: &Path, edit| JsonOp {
        path: path.clone(),
        edit,
    };
    match (old, new) {
        (None, None) => {}
        (Some(_), None) => ops.push(edit(&*path, JsonEdit::Delete)),
        (Some(Value::Object(old)), Some(Value::Object(new))) => {
            for key in old.keys().filter(|key| !new.contains_key(*key)) {
                path.push(key.clone());
                ops.push(edit(&*path, JsonEdit::Delete));
                path.pop();
            }
            for (key, value) in new {
                path.push(key.clone());
                diff_at(path, old.get(key), Some(value), ops);
                path.pop();
            }
        }
        (_, Some(Value::Object(new))) => {
            ops.push(edit(&*path, JsonEdit::Map));
            for (key, value) in new {
                path.push(key.clone());
                diff_at(path, None, Some(value), ops);
                path.pop();
            }
        }
        (old, Some(new)) => {
            if old != Some(new) {
                ops.push(edit(&*path, JsonEdit::Set(new.clone())));
            }
        }
    }
}

/// What a path reads as on its own.
enum Node<'a> {
    Absent,
    Map,
    Value(&'a Value),
}

/// A candidate in a path's race, with what it says about that path.
#[derive(Clone, Copy)]
struct Racer<'a> {
    entry: &'a Entry,
    /// It edits this very path, rather than setting a path below it.
    own: bool,
}

impl Racer<'_> {
    fn is_map(&self) -> bool {
        !self.own || self.entry.edit == JsonEdit::Map
    }
}

/// `a` beats `b`: higher priority, or the same priority and a higher block ID.
fn beats(a: &Entry, b: &Entry) -> bool {
    (a.priority, &a.block) > (b.priority, &b.block)
}

/// The entries of a frontier by path key, for range reads of a path and
/// everything below it.
struct Index<'a> {
    by_path: BTreeMap<Vec<u8>, Vec<&'a Entry>>,
}

impl<'a> Index<'a> {
    fn new(frontier: &'a Frontier) -> Self {
        let mut by_path: BTreeMap<Vec<u8>, Vec<&'a Entry>> = BTreeMap::new();
        for entry in &frontier.entries {
            by_path
                .entry(encode_path(&entry.path))
                .or_default()
                .push(entry);
        }
        Self { by_path }
    }

    /// The entries at `key`'s path and every path below it, with whether
    /// each is at the path itself.
    fn under<'k>(&'k self, key: &'k [u8]) -> impl Iterator<Item = (bool, &'a Entry)> + 'k {
        self.by_path
            .range(key.to_vec()..)
            .take_while(move |(k, _)| k.starts_with(key))
            .flat_map(move |(k, entries)| {
                let own = k.len() == key.len();
                entries.iter().map(move |entry| (own, *entry))
            })
    }

    /// The race at `key`'s path: its own edits and the sets below it. With
    /// any set below, the path's own deletes sit out, since an update beats a
    /// concurrent delete.
    fn racers(&self, key: &[u8]) -> Vec<Racer<'a>> {
        let all: Vec<Racer<'a>> = self
            .under(key)
            .filter(|(own, entry)| *own || entry.edit != JsonEdit::Delete)
            .map(|(own, entry)| Racer { entry, own })
            .collect();
        let implied = all.iter().any(|racer| !racer.own);
        all.into_iter()
            .filter(|racer| !(implied && racer.own && racer.entry.edit == JsonEdit::Delete))
            .collect()
    }

    fn winner(&self, key: &[u8]) -> Option<Racer<'a>> {
        self.racers(key).into_iter().reduce(|best, racer| {
            if beats(racer.entry, best.entry) {
                racer
            } else {
                best
            }
        })
    }

    fn node(&self, key: &[u8]) -> Node<'a> {
        match self.winner(key) {
            None => Node::Absent,
            Some(racer) if racer.is_map() => Node::Map,
            Some(racer) => match &racer.entry.edit {
                JsonEdit::Set(value) => Node::Value(value),
                JsonEdit::Map => Node::Map,
                JsonEdit::Delete => Node::Absent,
            },
        }
    }

    /// The value at `path`, whose key is `key`, given that every path above
    /// it is a map.
    fn build(&self, path: &mut Path, key: &[u8]) -> Option<Value> {
        match self.node(key) {
            Node::Absent => None,
            Node::Value(value) => Some(value.clone()),
            Node::Map => {
                let depth = path.len();
                let children: BTreeSet<&str> = self
                    .under(key)
                    .filter(|(own, _)| !own)
                    .map(|(_, entry)| entry.path[depth].as_str())
                    .collect();
                let mut map = Map::new();
                for child in children {
                    path.push(child.to_owned());
                    let child_key = encode_path(path);
                    if let Some(value) = self.build(path, &child_key) {
                        map.insert(child.to_owned(), value);
                    }
                    path.pop();
                }
                Some(Value::Object(map))
            }
        }
    }
}

impl Frontier {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        ciborium::into_writer(self, &mut bytes)
            .map_err(|error| Error::Serialization(format!("json frontier: {error}")))?;
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ciborium::from_reader(bytes)
            .map_err(|error| Error::Serialization(format!("json frontier: {error}")))
    }

    /// Apply `later` after this one: drop what it removes, add what it adds.
    /// The result removes what either removed, so it is itself an operand,
    /// and composing is associative.
    pub fn compose(&mut self, later: Frontier) {
        let key = |path: &[String], block: &[u8]| (encode_path(path), block.to_vec());
        let removed: BTreeSet<(Vec<u8>, Vec<u8>)> = later
            .removes
            .iter()
            .map(|(path, block)| key(path, block))
            .collect();
        let mut entries: BTreeMap<(Vec<u8>, Vec<u8>), Entry> = std::mem::take(&mut self.entries)
            .into_iter()
            .map(|entry| (key(&entry.path, &entry.block), entry))
            .filter(|(k, _)| !removed.contains(k))
            .collect();
        for entry in later.entries {
            entries.insert(key(&entry.path, &entry.block), entry);
        }
        self.entries = entries.into_values().collect();
        let removes: Removes = std::mem::take(&mut self.removes)
            .into_iter()
            .chain(later.removes)
            .map(|(path, block)| (key(&path, &block), (path, block)))
            .collect();
        self.removes = removes.into_values().collect();
    }

    /// The same frontier as a stored value: what it removes no longer matters.
    pub fn settle(&mut self) {
        self.removes.clear();
    }

    /// The operand that writes `ops`, from the block `block` at `priority`,
    /// onto this frontier. `in_past` says whether an entry's block is in the
    /// new block's causal past: the edits it had seen at an op's path and
    /// below leave, and the edits above an op's path it had seen are recorded.
    pub fn delta<F>(&self, ops: &[JsonOp], block: &[u8], priority: u64, in_past: F) -> Frontier
    where
        F: Fn(&[u8]) -> bool,
    {
        let mut removes = Removes::new();
        let mut entries = Vec::with_capacity(ops.len());
        for op in ops {
            let mut seen = BTreeSet::new();
            for entry in &self.entries {
                if !in_past(&entry.block) {
                    continue;
                }
                if entry.path.starts_with(&op.path) {
                    removes.insert(
                        (encode_path(&entry.path), entry.block.clone()),
                        (entry.path.clone(), entry.block.clone()),
                    );
                } else if op.path.starts_with(&entry.path) {
                    seen.insert(entry.block.clone());
                }
            }
            entries.push(Entry {
                path: op.path.clone(),
                block: block.to_vec(),
                priority,
                edit: op.edit.clone(),
                seen: seen.into_iter().collect(),
            });
        }
        Frontier {
            removes: removes.into_values().collect(),
            entries,
        }
    }

    /// The operand of a local write: its block's past is everything stored.
    pub fn local_delta(&self, ops: &[JsonOp], block: &[u8], priority: u64) -> Frontier {
        self.delta(ops, block, priority, |_| true)
    }

    /// The JSON this frontier reads as, `None` when the field is absent.
    pub fn materialize(&self) -> Option<Value> {
        Index::new(self).build(&mut Vec::new(), &[])
    }

    /// The JSON this frontier reads as, `Value::Null` when absent.
    pub fn value(&self) -> Value {
        self.materialize().unwrap_or(Value::Null)
    }

    /// Every edit that raced a path's winner and lost without the winner
    /// having seen it, except an implied map losing to a map: they agree. A
    /// block is listed once per path, however many paths below it sets.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let index = Index::new(self);
        let mut paths: BTreeSet<Vec<u8>> = BTreeSet::new();
        for entry in &self.entries {
            for depth in 0..=entry.path.len() {
                paths.insert(encode_path(&entry.path[..depth]));
            }
        }
        let mut conflicts = Vec::new();
        for key in paths {
            let Some(winner) = index.winner(&key) else {
                continue;
            };
            let racers = index
                .under(&key)
                .filter(|(own, entry)| *own || entry.edit != JsonEdit::Delete)
                .map(|(own, entry)| Racer { entry, own });
            let mut listed: BTreeSet<&[u8]> = BTreeSet::new();
            for racer in racers {
                let entry = racer.entry;
                if entry.block == winner.entry.block
                    || (racer.is_map() && winner.is_map())
                    || listed.contains(entry.block.as_slice())
                {
                    continue;
                }
                let concurrent = match (racer.own, winner.own) {
                    (true, true) => true,
                    (false, true) => !entry.seen.contains(&winner.entry.block),
                    (true, false) => !winner.entry.seen.contains(&entry.block),
                    (false, false) => true,
                };
                if concurrent {
                    let Some(path) = decode_path(&key) else {
                        continue;
                    };
                    listed.insert(&entry.block);
                    conflicts.push(Conflict {
                        path,
                        block: entry.block.clone(),
                        priority: entry.priority,
                        edit: if racer.own {
                            entry.edit.clone()
                        } else {
                            JsonEdit::Map
                        },
                    });
                }
            }
        }
        conflicts
    }
}

/// One block of a Json field's DAG, for [`replay`]: its CID, its parents'
/// CIDs, its priority and its plain ops `data`.
pub struct ReplayBlock {
    pub cid: Vec<u8>,
    pub parents: Vec<Vec<u8>>,
    pub priority: u64,
    pub data: Vec<u8>,
}

/// The frontier of a whole Json DAG: every block merged after its parents,
/// which a higher priority than each of them guarantees. A block whose
/// parents are the tips merged so far has every entry in its past; any other
/// walks down its ancestors only as far as the entries it touches reach.
/// Each op reads only its own path, the paths below it and the ones above.
pub fn replay(mut blocks: Vec<ReplayBlock>) -> Result<Frontier> {
    blocks.sort_by(|a, b| (a.priority, &a.cid).cmp(&(b.priority, &b.cid)));
    let mut merged: BTreeMap<Vec<u8>, Merged> = BTreeMap::new();
    let mut tips: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut entries: BTreeMap<EntryKey, Entry> = BTreeMap::new();
    for block in blocks {
        let id = block_id(block.priority, &block.parents, &block.data);
        let ops = decode_ops(&block.data)?;
        let parents: BTreeSet<Vec<u8>> = block.parents.iter().cloned().collect();
        let all_past = parents == tips;
        let mut touched: Vec<Touched<'_>> = Vec::new();
        for (op_index, op) in ops.iter().enumerate() {
            let key = encode_path(&op.path);
            let below = entries
                .range((key.clone(), Vec::new())..)
                .take_while(|((path, _), _)| path.starts_with(&key));
            touched.extend(below.map(|(key, entry)| Touched {
                below: true,
                op_index,
                key,
                entry,
            }));
            for depth in 0..op.path.len() {
                let above = encode_path(&op.path[..depth]);
                let at = entries
                    .range((above.clone(), Vec::new())..)
                    .take_while(|((path, _), _)| *path == above);
                touched.extend(at.map(|(key, entry)| Touched {
                    below: false,
                    op_index,
                    key,
                    entry,
                }));
            }
        }
        let past = if all_past {
            BTreeSet::new()
        } else {
            let lowest = touched.iter().map(|t| t.entry.priority).min();
            past_ids(&merged, &block.parents, lowest.unwrap_or(u64::MAX))
        };
        let in_past = |entry: &Entry| all_past || past.contains(&entry.block);
        let mut removed = Vec::new();
        let mut seen: Vec<BTreeSet<Vec<u8>>> = vec![BTreeSet::new(); ops.len()];
        for touched in touched {
            if !in_past(touched.entry) {
                continue;
            }
            if touched.below {
                removed.push(touched.key.clone());
            } else {
                seen[touched.op_index].insert(touched.entry.block.clone());
            }
        }
        for key in removed {
            entries.remove(&key);
        }
        for (op, seen) in ops.into_iter().zip(seen) {
            entries.insert(
                (encode_path(&op.path), id.clone()),
                Entry {
                    path: op.path,
                    block: id.clone(),
                    priority: block.priority,
                    edit: op.edit,
                    seen: seen.into_iter().collect(),
                },
            );
        }
        for parent in &parents {
            tips.remove(parent);
        }
        tips.insert(block.cid.clone());
        merged.insert(
            block.cid,
            Merged {
                id,
                priority: block.priority,
                parents: block.parents,
            },
        );
    }
    Ok(Frontier {
        removes: Vec::new(),
        entries: entries.into_values().collect(),
    })
}

/// An entry's place in a replay: its path key and its block ID.
type EntryKey = (Vec<u8>, Vec<u8>);

/// A block a replay has merged: its ID, priority and parents' CIDs.
struct Merged {
    id: Vec<u8>,
    priority: u64,
    parents: Vec<Vec<u8>>,
}

/// An entry an op of the block being replayed reaches: at or below its
/// path, or above it.
struct Touched<'a> {
    below: bool,
    op_index: usize,
    key: &'a EntryKey,
    entry: &'a Entry,
}

/// The IDs of the merged blocks below `parents`, down to priority `lowest`:
/// no block further down can be an entry of that priority or above.
fn past_ids(
    merged: &BTreeMap<Vec<u8>, Merged>,
    parents: &[Vec<u8>],
    lowest: u64,
) -> BTreeSet<Vec<u8>> {
    let mut ids = BTreeSet::new();
    let mut visited: BTreeSet<&[u8]> = BTreeSet::new();
    let mut waiting: BTreeSet<(u64, &[u8])> = BTreeSet::new();
    let mut next: Vec<&[u8]> = parents.iter().map(Vec::as_slice).collect();
    loop {
        for cid in next.drain(..) {
            if let Some((cid, block)) = merged.get_key_value(cid) {
                if visited.insert(cid.as_slice()) {
                    waiting.insert((block.priority, cid.as_slice()));
                }
            }
        }
        let Some((priority, cid)) = waiting.pop_last() else {
            break;
        };
        if priority < lowest {
            break;
        }
        if let Some(block) = merged.get(cid) {
            ids.insert(block.id.clone());
            next.extend(block.parents.iter().map(Vec::as_slice));
        }
    }
    ids
}

#[cfg(test)]
#[path = "json_crdt_tests.rs"]
mod tests;
