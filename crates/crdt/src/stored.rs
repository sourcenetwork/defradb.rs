//! Local document values: each field a part with a stable ID, the kind of CRDT
//! state it holds, its priority and the state's bytes.
//!
//! This format is separate from the canonical CBOR used for blocks and DocIDs.
//! What a part's bytes mean, and how two parts merge, is up to the
//! [`FieldCrdt`](crate::FieldCrdt) that stores its kind ([`part_crdt`]).
use std::collections::BTreeMap;
use std::sync::Arc;

use document::{Document, Error, NormalValue, Result};
use schema::CollectionVersion;

use crate::field::part_crdt;

const MAGIC: &[u8; 4] = b"DFV\x01";

/// The kind of state a part holds, as its stored tag byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueKind {
    Lww = 0,
    Int64 = 1,
    Float32 = 2,
    Float64 = 3,
    Text = 4,
    Json = 5,
}

impl ValueKind {
    fn from_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => ValueKind::Lww,
            1 => ValueKind::Int64,
            2 => ValueKind::Float32,
            3 => ValueKind::Float64,
            4 => ValueKind::Text,
            5 => ValueKind::Json,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredField {
    pub name: String,
    pub kind: ValueKind,
    pub priority: u64,
    pub value: Vec<u8>,
}

impl StoredField {
    pub fn materialize(&self) -> Option<NormalValue> {
        self.part().materialize()
    }

    pub fn part(&self) -> Part<'_> {
        Part {
            name: &self.name,
            kind: self.kind,
            priority: self.priority,
            value: &self.value,
        }
    }
}

/// One field as a document value stores it, borrowed: what
/// [`StoredDocument::encode`] writes for each of its fields.
#[derive(Debug, Clone, Copy)]
pub struct Part<'a> {
    pub name: &'a str,
    pub kind: ValueKind,
    pub priority: u64,
    pub value: &'a [u8],
}

impl Part<'_> {
    /// The value the part reads as, or `None` when its bytes are not a state
    /// of its kind.
    pub fn materialize(self) -> Option<NormalValue> {
        part_crdt(self.kind).materialize(self)
    }
}

/// `prefix`, then the document value of `parts`: what
/// [`StoredDocument::encode_after`] writes for a document of those fields,
/// without building one. `None` where a document of them cannot be stored:
/// an ID out of ascending order or repeated, an empty or repeated name, or a
/// value its kind cannot hold.
pub fn encode_parts<'a>(
    prefix: &[u8],
    parts: impl Iterator<Item = (u32, Part<'a>)> + Clone,
) -> Option<Vec<u8>> {
    let (count, size) = parts
        .clone()
        .fold((0usize, 0usize), |(count, size), (_, part)| {
            (count + 1, size + 21 + part.name.len() + part.value.len())
        });
    let mut bytes = Vec::with_capacity(prefix.len() + MAGIC.len() + 4 + size);
    bytes.extend_from_slice(prefix);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&u32::try_from(count).ok()?.to_be_bytes());
    // One field cannot repeat a name, so only a document of several sorts them.
    let mut names = Vec::with_capacity(if count > 1 { count } else { 0 });
    let mut previous = None;
    for (id, part) in parts {
        if previous.is_some_and(|previous| previous >= id)
            || part.name.is_empty()
            || part.materialize().is_none()
        {
            return None;
        }
        previous = Some(id);
        if count > 1 {
            names.push(part.name);
        }
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.push(part.kind as u8);
        bytes.extend_from_slice(&part.priority.to_be_bytes());
        for value in [part.name.as_bytes(), part.value] {
            bytes.extend_from_slice(&u32::try_from(value.len()).ok()?.to_be_bytes());
            bytes.extend_from_slice(value);
        }
    }
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(bytes)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoredDocument {
    pub fields: BTreeMap<u32, StoredField>,
}

impl StoredDocument {
    pub fn encode(&self) -> Option<Vec<u8>> {
        self.encode_after(&[])
    }

    /// `prefix`, then what [`encode`](Self::encode) writes, in one allocation.
    pub fn encode_after(&self, prefix: &[u8]) -> Option<Vec<u8>> {
        encode_parts(
            prefix,
            self.fields.iter().map(|(id, field)| (*id, field.part())),
        )
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        Self::decode_inner(bytes, true)
    }

    /// Whether `bytes` are in this format rather than a whole CBOR value
    /// written before it existed. Checks the leading tag only.
    pub fn is_field_format(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    fn decode_inner(bytes: &[u8], validate_values: bool) -> Option<Self> {
        let mut input = bytes.strip_prefix(MAGIC)?;
        let count = u32::from_be_bytes(take(&mut input, 4)?.try_into().ok()?) as usize;
        // Every field has at least 21 bytes of header. Never allocate from an
        // unchecked on-disk count, and never accept duplicate IDs or names.
        if count > input.len() / 21 {
            return None;
        }
        let mut fields = BTreeMap::new();
        for _ in 0..count {
            let id = u32::from_be_bytes(take(&mut input, 4)?.try_into().ok()?);
            let kind = ValueKind::from_tag(take(&mut input, 1)?[0])?;
            let priority = u64::from_be_bytes(take(&mut input, 8)?.try_into().ok()?);
            let name = std::str::from_utf8(take_sized(&mut input)?)
                .ok()?
                .to_owned();
            let value = take_sized(&mut input)?.to_vec();
            let field = StoredField {
                name,
                kind,
                priority,
                value,
            };
            if field.name.is_empty()
                || (validate_values && field.materialize().is_none())
                || fields.insert(id, field).is_some()
            {
                return None;
            }
        }
        let mut names: Vec<_> = fields.values().map(|field| field.name.as_str()).collect();
        names.sort_unstable();
        if names.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        input.is_empty().then_some(Self { fields })
    }

    pub fn materialize(&self, collection: impl Into<Arc<CollectionVersion>>) -> Option<Document> {
        let mut doc = Document::new(collection);
        for field in self.fields.values() {
            let value = field.materialize()?;
            if !value.is_nil() {
                doc.load(field.name.clone(), value);
            }
        }
        doc.clean();
        Some(doc)
    }

    /// Apply one operand in commit order: each part merges into the part of
    /// the same ID, or into its kind's starting state when the value lacks
    /// it. The result is a stored value, so a tagged part settles
    /// ([`TaggedCrdt::settle`](crate::field::TaggedCrdt::settle)). Callers
    /// put a fresh value instead when the schema version changes; IDs and
    /// names cannot be repurposed.
    pub fn apply(&mut self, operand: Self) -> Option<()> {
        let ids: Vec<u32> = operand.fields.keys().copied().collect();
        self.fold(operand)?;
        for id in ids {
            let part = self.fields.get_mut(&id)?;
            if let Some(tagged) = part_crdt(part.kind).tagged() {
                tagged.settle(part)?;
            }
        }
        Some(())
    }

    /// Fold a later operand into this one, leaving an operand for a value
    /// not known yet: as [`apply`](Self::apply), but a tagged part keeps what
    /// it removes from that value.
    pub fn fold(&mut self, operand: Self) -> Option<()> {
        for (id, incoming) in operand.fields {
            if !self.fields.contains_key(&id)
                && self
                    .fields
                    .values()
                    .any(|field| field.name == incoming.name)
            {
                return None;
            }
            let current = self.fields.entry(id).or_insert_with(|| StoredField {
                name: incoming.name.clone(),
                kind: incoming.kind,
                priority: 0,
                value: Vec::new(),
            });
            if current.kind != incoming.kind || current.name != incoming.name {
                return None;
            }
            part_crdt(incoming.kind).merge(current, incoming)?;
        }
        Some(())
    }
}

fn take<'a>(input: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
    let (head, tail) = input.split_at_checked(length)?;
    *input = tail;
    Some(head)
}
fn take_sized<'a>(input: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u32::from_be_bytes(take(input, 4)?.try_into().ok()?) as usize;
    take(input, length)
}

/// A document from its local stored value, in this format or as the whole
/// CBOR value written before it existed. The wire CBOR decoder is unchanged.
pub fn document_from_storage(
    bytes: &[u8],
    collection: impl Into<Arc<CollectionVersion>>,
) -> Result<Document> {
    if StoredDocument::is_field_format(bytes) {
        StoredDocument::decode_inner(bytes, false)
            .and_then(|value| value.materialize(collection))
            .ok_or_else(|| Error::CborDecode("invalid local document value".into()))
    } else {
        Document::from_cbor(bytes, collection)
    }
}
