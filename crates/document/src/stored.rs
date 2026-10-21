//! Local document values with stable field IDs and CRDT ordering metadata.
//!
//! This format is separate from the canonical CBOR used for blocks and DocIDs.
//! Empty LWW bytes are a tombstone; their priority must survive materialization.
use std::collections::BTreeMap;

use crate::encoding::{cbor_to_normal_value, normal_value_to_cbor};
use crate::{Document, Error, NormalValue, Result};

const MAGIC: &[u8; 4] = b"DFV\x01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueKind {
    Lww = 0,
    Int64 = 1,
    Float32 = 2,
    Float64 = 3,
    /// A Text field's materialized string, as CBOR like an LWW value. Every
    /// writer of a Text part read it validated first, so an operand replaces
    /// the stored text in commit order whatever its priority.
    Text = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredField {
    pub name: String,
    pub kind: ValueKind,
    pub priority: u64,
    /// CBOR for LWW values; big-endian numbers for counters.
    pub value: Vec<u8>,
}

/// What an LWW field stores for `value`: nothing for nil, its CBOR otherwise.
fn lww_cbor(value: &NormalValue) -> Result<Option<ciborium::Value>> {
    if value.is_nil() {
        return Ok(None);
    }
    normal_value_to_cbor(value).map(Some)
}

/// Whether two values of one LWW field store the same value. A value read
/// back from storage carries no kind (an empty array of any kind reads back as
/// an empty JSON array), so values compare by what they store, never by
/// variant. An absent field stores nothing, as nil does.
pub fn stores_same_lww(left: Option<&NormalValue>, right: Option<&NormalValue>) -> Result<bool> {
    let stored = |value: Option<&NormalValue>| value.map_or(Ok(None), lww_cbor);
    Ok(stored(left)? == stored(right)?)
}

impl StoredField {
    /// A Text part: the field's whole string, or nothing for nil.
    pub fn text(name: String, priority: u64, value: &NormalValue) -> Result<Self> {
        Ok(Self {
            kind: ValueKind::Text,
            ..Self::lww(name, priority, value)?
        })
    }

    pub fn lww(name: String, priority: u64, value: &NormalValue) -> Result<Self> {
        let bytes = match lww_cbor(value)? {
            Some(cbor) => {
                let mut bytes = Vec::with_capacity(value.cbor_size_hint());
                ciborium::into_writer(&cbor, &mut bytes)
                    .map_err(|error| Error::CborEncode(error.to_string()))?;
                bytes
            }
            None => Vec::new(),
        };
        Ok(Self {
            name,
            kind: ValueKind::Lww,
            priority,
            value: bytes,
        })
    }

    /// A counter part: its value for a document put, or its increment for an
    /// operand. Integers, `Float32` and `Float64` convert as the schema kind
    /// says; an LWW kind is refused.
    pub fn counter(
        name: String,
        kind: ValueKind,
        priority: u64,
        value: &NormalValue,
    ) -> Result<Self> {
        let bytes = match kind {
            ValueKind::Int64 => value.as_int().map(|v| v.to_be_bytes().to_vec()),
            ValueKind::Float32 => value
                .as_float32()
                .or_else(|| value.as_float64().map(|v| v as f32))
                .or_else(|| value.as_int().map(|v| v as f32))
                .map(|v| v.to_be_bytes().to_vec()),
            ValueKind::Float64 => value
                .as_float64()
                .or_else(|| value.as_float32().map(f64::from))
                .or_else(|| value.as_int().map(|v| v as f64))
                .map(|v| v.to_be_bytes().to_vec()),
            ValueKind::Lww | ValueKind::Text => None,
        };
        let Some(value) = bytes else {
            return Err(Error::InvalidFieldValue {
                field: name,
                message: format!("not a counter value of kind {kind:?}"),
            });
        };
        Ok(Self {
            name,
            kind,
            priority,
            value,
        })
    }

    pub fn materialize(&self) -> Option<NormalValue> {
        materialize(self.kind, &self.value)
    }

    fn part(&self) -> Part<'_> {
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
    /// CBOR for LWW values; big-endian numbers for counters.
    pub value: &'a [u8],
}

fn materialize(kind: ValueKind, value: &[u8]) -> Option<NormalValue> {
    match kind {
        ValueKind::Lww | ValueKind::Text if value.is_empty() => Some(NormalValue::Null),
        ValueKind::Lww | ValueKind::Text => {
            let mut reader = value;
            let value = ciborium::from_reader(&mut reader).ok()?;
            if !reader.is_empty() {
                return None;
            }
            cbor_to_normal_value(value).ok()
        }
        ValueKind::Int64 => Some(NormalValue::Int(i64::from_be_bytes(value.try_into().ok()?))),
        ValueKind::Float32 => Some(NormalValue::Float64(f64::from(f32::from_be_bytes(
            value.try_into().ok()?,
        )))),
        ValueKind::Float64 => Some(NormalValue::Float64(f64::from_be_bytes(
            value.try_into().ok()?,
        ))),
    }
}

/// Whether a field of `kind` can store `value`: exactly when [`materialize`]
/// reads it back, decided for a counter by its width alone.
fn stores(kind: ValueKind, value: &[u8]) -> bool {
    match kind {
        ValueKind::Lww | ValueKind::Text => materialize(kind, value).is_some(),
        ValueKind::Int64 | ValueKind::Float64 => value.len() == 8,
        ValueKind::Float32 => value.len() == 4,
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
            || !stores(part.kind, part.value)
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
            let kind = match take(&mut input, 1)?[0] {
                0 => ValueKind::Lww,
                1 => ValueKind::Int64,
                2 => ValueKind::Float32,
                3 => ValueKind::Float64,
                4 => ValueKind::Text,
                _ => return None,
            };
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
                || (validate_values && !stores(field.kind, &field.value))
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

    pub fn materialize(&self) -> Option<Document> {
        let mut doc = Document::new();
        for field in self.fields.values() {
            let value = field.materialize()?;
            if !value.is_nil() {
                doc.set(&field.name, value);
            }
        }
        doc.clean();
        Some(doc)
    }

    /// Apply one operand in commit order. Callers put a fresh value instead
    /// when the schema version changes; IDs and names cannot be repurposed.
    pub fn apply(&mut self, operand: Self) -> Option<()> {
        for (id, mut incoming) in operand.fields {
            let Some(current) = self.fields.get_mut(&id) else {
                if self
                    .fields
                    .values()
                    .any(|field| field.name == incoming.name)
                {
                    return None;
                }
                // A counter part the value lacks starts from zero, so the
                // increment lands exactly as it would on a stored zero.
                if !matches!(incoming.kind, ValueKind::Lww | ValueKind::Text) {
                    let zero: &[u8] = if incoming.kind == ValueKind::Float32 {
                        &[0; 4]
                    } else {
                        &[0; 8]
                    };
                    incoming.value = counter_sum(incoming.kind, zero, &incoming.value)?;
                }
                self.fields.insert(id, incoming);
                continue;
            };
            if current.kind != incoming.kind || current.name != incoming.name {
                return None;
            }
            current.value = match incoming.kind {
                ValueKind::Lww => {
                    if (incoming.priority, &incoming.value) <= (current.priority, &current.value) {
                        continue;
                    }
                    incoming.value
                }
                ValueKind::Text => incoming.value,
                kind => counter_sum(kind, &current.value, &incoming.value)?,
            };
            current.priority = current.priority.max(incoming.priority);
        }
        Some(())
    }
}

/// `current + increment` for a counter of `kind`, both big-endian: wrapping
/// for integers, IEEE-754 addition at the kind's own precision for floats.
fn counter_sum(kind: ValueKind, current: &[u8], increment: &[u8]) -> Option<Vec<u8>> {
    Some(match kind {
        ValueKind::Int64 => i64::from_be_bytes(current.try_into().ok()?)
            .wrapping_add(i64::from_be_bytes(increment.try_into().ok()?))
            .to_be_bytes()
            .to_vec(),
        ValueKind::Float32 => (f32::from_be_bytes(current.try_into().ok()?)
            + f32::from_be_bytes(increment.try_into().ok()?))
        .to_be_bytes()
        .to_vec(),
        ValueKind::Float64 => (f64::from_be_bytes(current.try_into().ok()?)
            + f64::from_be_bytes(increment.try_into().ok()?))
        .to_be_bytes()
        .to_vec(),
        ValueKind::Lww | ValueKind::Text => return None,
    })
}

/// `stored` plus `delta` for a counter of `kind`, added exactly as the merge
/// operator adds an increment to a stored value. An absent value counts as 0.
pub fn add_counter(
    name: &str,
    kind: ValueKind,
    stored: Option<&NormalValue>,
    delta: &NormalValue,
) -> Result<NormalValue> {
    const ZERO: NormalValue = NormalValue::Int(0);
    let part = |value| StoredField::counter(name.to_owned(), kind, 0, value);
    let mut sum = StoredDocument {
        fields: [(0, part(stored.unwrap_or(&ZERO))?)].into(),
    };
    let increment = StoredDocument {
        fields: [(0, part(delta)?)].into(),
    };
    sum.apply(increment)
        .and_then(|()| sum.fields.get(&0)?.materialize())
        .ok_or_else(|| Error::InvalidFieldValue {
            field: name.to_owned(),
            message: "the counter sum cannot be stored".into(),
        })
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

impl Document {
    /// Decode a local stored value without changing the wire CBOR decoder.
    pub fn from_storage_cbor(bytes: &[u8]) -> Result<Self> {
        if bytes.starts_with(MAGIC) {
            StoredDocument::decode_inner(bytes, false)
                .and_then(|value| value.materialize())
                .ok_or_else(|| Error::CborDecode("invalid local document value".into()))
        } else {
            Self::from_cbor(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_storage_cbor_materializes_a_stored_document() {
        let mut stored = StoredDocument::default();
        stored.fields.insert(
            1,
            StoredField::lww("name".to_owned(), 1, &NormalValue::String("Ada".to_owned())).unwrap(),
        );
        let bytes = stored.encode().unwrap();

        let document = Document::from_storage_cbor(&bytes).unwrap();
        assert_eq!(
            document.get("name").and_then(NormalValue::as_str),
            Some("Ada")
        );
    }

    #[test]
    fn a_text_operand_replaces_the_stored_text_whatever_its_priority() {
        let text = |priority, value: &str| {
            let mut doc = StoredDocument::default();
            doc.fields.insert(
                1,
                StoredField::text(
                    "body".to_owned(),
                    priority,
                    &NormalValue::String(value.to_owned()),
                )
                .unwrap(),
            );
            doc
        };
        let mut stored = text(5, "zzz");
        stored.apply(text(3, "abc")).unwrap();
        let body = &stored.fields[&1];
        assert_eq!(
            body.materialize().as_ref().and_then(NormalValue::as_str),
            Some("abc")
        );
        assert_eq!(body.priority, 5);
        let decoded = StoredDocument::decode(&stored.encode().unwrap()).unwrap();
        assert_eq!(decoded.fields[&1].kind, ValueKind::Text);
        assert!(text(1, "a").apply(lww_body()).is_none());
    }

    fn lww_body() -> StoredDocument {
        let mut doc = StoredDocument::default();
        doc.fields.insert(
            1,
            StoredField::lww("body".to_owned(), 9, &NormalValue::String("x".to_owned())).unwrap(),
        );
        doc
    }

    #[test]
    fn values_that_store_alike_compare_the_same_whatever_their_variant() {
        let empty = [
            NormalValue::JsonArray(Vec::new()),
            NormalValue::StringArray(Vec::new()),
            NormalValue::NillableStringArray(Some(Vec::new())),
            NormalValue::NillableStringElementArray(Vec::new()),
            NormalValue::IntArray(Vec::new()),
        ];
        for left in &empty {
            for right in &empty {
                assert!(
                    stores_same_lww(Some(left), Some(right)).unwrap(),
                    "{left:?} and {right:?}"
                );
            }
        }
        assert!(stores_same_lww(
            Some(&NormalValue::StringArray(vec!["a".to_owned()])),
            Some(&NormalValue::NillableStringArray(Some(
                vec!["a".to_owned()]
            ))),
        )
        .unwrap());
        assert!(stores_same_lww(None, Some(&NormalValue::Null)).unwrap());
        assert!(stores_same_lww(None, Some(&NormalValue::NillableStringArray(None))).unwrap());
    }

    #[test]
    fn values_that_store_differently_compare_as_changed() {
        let empty = NormalValue::JsonArray(Vec::new());
        for other in [
            NormalValue::StringArray(vec!["a".to_owned()]),
            NormalValue::NillableStringArray(None),
            NormalValue::String(String::new()),
        ] {
            assert!(
                !stores_same_lww(Some(&empty), Some(&other)).unwrap(),
                "{other:?}"
            );
        }
        assert!(!stores_same_lww(None, Some(&empty)).unwrap());
        assert!(!stores_same_lww(
            Some(&NormalValue::Int(5)),
            Some(&NormalValue::String("5".to_owned())),
        )
        .unwrap());
    }

    #[test]
    fn from_storage_cbor_rejects_invalid_field_values() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&1_u32.to_be_bytes()); // field count
        bytes.extend_from_slice(&1_u32.to_be_bytes()); // field ID
        bytes.push(ValueKind::Lww as u8);
        bytes.extend_from_slice(&1_u64.to_be_bytes()); // priority
        bytes.extend_from_slice(&4_u32.to_be_bytes());
        bytes.extend_from_slice(b"name");
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.push(0xff); // invalid CBOR

        assert!(StoredDocument::decode(&bytes).is_none());
        assert!(Document::from_storage_cbor(&bytes).is_err());
    }

    #[test]
    fn stored_document_rejects_duplicate_field_names() {
        let mut stored = StoredDocument::default();
        for id in [1, 2] {
            stored.fields.insert(
                id,
                StoredField::lww("same".to_owned(), 1, &NormalValue::Null).unwrap(),
            );
        }
        assert!(stored.encode().is_none());

        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&2_u32.to_be_bytes()); // field count
        for id in [1_u32, 2] {
            bytes.extend_from_slice(&id.to_be_bytes());
            bytes.push(ValueKind::Lww as u8);
            bytes.extend_from_slice(&1_u64.to_be_bytes()); // priority
            bytes.extend_from_slice(&4_u32.to_be_bytes());
            bytes.extend_from_slice(b"same");
            bytes.extend_from_slice(&0_u32.to_be_bytes()); // null value
        }
        assert!(StoredDocument::decode(&bytes).is_none());
    }

    /// Borrowed parts encode, after any prefix, exactly as a stored document
    /// of the same fields does, and a document they cannot form is refused.
    #[test]
    fn encoded_parts_are_the_stored_document_of_those_fields() {
        let mut stored = StoredDocument::default();
        stored.fields.insert(
            2,
            StoredField::counter(
                "count".to_owned(),
                ValueKind::Int64,
                7,
                &NormalValue::Int(-3),
            )
            .unwrap(),
        );
        stored.fields.insert(
            5,
            StoredField::lww("name".to_owned(), 9, &NormalValue::String("Ada".to_owned())).unwrap(),
        );
        let parts = || stored.fields.iter().map(|(id, field)| (*id, field.part()));
        assert_eq!(encode_parts(&[], parts()), stored.encode());
        assert_eq!(
            encode_parts(b"pre", parts()).unwrap(),
            [b"pre".as_slice(), &stored.encode().unwrap()].concat()
        );
        let decoded = StoredDocument::decode(&encode_parts(&[], parts()).unwrap()).unwrap();
        assert_eq!(decoded, stored);

        let field = |id: u32, name: &'static str| {
            (
                id,
                Part {
                    name,
                    kind: ValueKind::Int64,
                    priority: 1,
                    value: &[0; 8],
                },
            )
        };
        assert!(encode_parts(&[], [field(2, "a"), field(1, "b")].into_iter()).is_none());
        assert!(encode_parts(&[], [field(1, "a"), field(1, "b")].into_iter()).is_none());
        assert!(encode_parts(&[], [field(1, "a"), field(2, "a")].into_iter()).is_none());
        assert!(encode_parts(&[], [field(1, "")].into_iter()).is_none());
        let short = Part {
            name: "a",
            kind: ValueKind::Int64,
            priority: 1,
            value: &[0; 4],
        };
        assert!(encode_parts(&[], std::iter::once((1, short))).is_none());
    }
}
