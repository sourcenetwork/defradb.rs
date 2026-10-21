//! The store's merge operator: every value a blind merge writes, and how
//! regolith folds it.
//!
//! A database has one operator, so [`DefraMergeOperator`] serves every
//! operand kind: standalone counters and full-text statistics (sums), document
//! values (part operands, `document::stored`), and HNSW nodes and metadata
//! (link additions and counts, [`vector`]), and head sets (per-name functions on
//! a set of heads, each name one part, [`head_set`]). Each operand names its
//! kind after a versioned magic, so the operator dispatches on the operand
//! itself. Base values keep their typed big-endian representation; operands
//! are never exposed as materialized values. The store installs it when it
//! opens, and its contract is checked beside it with regolith's
//! `check_merge_operator` and `check_touches`.
use regolith::MergeOperator;

const MAGIC: &[u8] = b"DM\x01";
const INT64: u8 = 1;
const FLOAT32: u8 = 2;
const FLOAT64: u8 = 3;
const STATS: u8 = 4;
const DOCUMENT: u8 = 5;
const VECTOR_LINKS: u8 = 6;
const VECTOR_COUNTS: u8 = 7;
const HEAD_SET: u8 = 8;
const HEAD_SET_STATE: u8 = 9;
pub mod head_set;
pub mod vector;

/// What every operand of `kind` starts with.
const fn header(kind: u8) -> [u8; 4] {
    [MAGIC[0], MAGIC[1], MAGIC[2], kind]
}

fn encode(kind: u8, value: &[u8]) -> Vec<u8> {
    [&header(kind), value].concat()
}

fn decode(bytes: &[u8]) -> Option<(u8, &[u8])> {
    let bytes = bytes.strip_prefix(MAGIC)?;
    Some((*bytes.first()?, &bytes[1..]))
}

/// Operands of the standalone counter values written before counters became
/// parts of the document value. Nothing writes them now; the operator still
/// folds them so a store keeps reading until the counter migration moves them
/// into their documents.
pub fn counter_i64(delta: i64) -> Vec<u8> {
    encode(INT64, &delta.to_be_bytes())
}
pub fn counter_f32(delta: f32) -> Vec<u8> {
    encode(FLOAT32, &delta.to_be_bytes())
}
pub fn counter_f64(delta: f64) -> Vec<u8> {
    encode(FLOAT64, &delta.to_be_bytes())
}
pub fn stats_delta(documents: i128, field_length: i128) -> Vec<u8> {
    encode(
        STATS,
        &[documents.to_be_bytes(), field_length.to_be_bytes()].concat(),
    )
}

/// Only the named fields are changed. The base must use the local value format.
pub fn document_parts(parts: &document::stored::StoredDocument) -> Option<Vec<u8>> {
    parts.encode_after(&header(DOCUMENT))
}

/// [`document_parts`] of a document holding only `part`, at `id`, written
/// without building that document.
pub fn document_part(id: u32, part: document::stored::Part<'_>) -> Option<Vec<u8>> {
    document::stored::encode_parts(&header(DOCUMENT), std::iter::once((id, part)))
}

pub struct DefraMergeOperator;

impl MergeOperator for DefraMergeOperator {
    fn name(&self) -> &'static str {
        "defra-values-v1"
    }

    fn full_merge(&self, _key: &[u8], base: Option<&[u8]>, operands: &[&[u8]]) -> Option<Vec<u8>> {
        let Some(first) = operands.first() else {
            return base.map(<[u8]>::to_vec);
        };
        let (kind, _) = decode(first)?;
        if matches!(kind, VECTOR_LINKS | VECTOR_COUNTS) {
            return vector::full(kind, base, operands);
        }
        if kind == HEAD_SET {
            return head_set::full(base, operands);
        }
        if kind == DOCUMENT {
            let mut document = match base {
                Some(bytes) => document::stored::StoredDocument::decode(bytes)?,
                None => document::stored::StoredDocument::default(),
            };
            for operand in operands {
                let (DOCUMENT, bytes) = decode(operand)? else {
                    return None;
                };
                document.apply(document::stored::StoredDocument::decode(bytes)?)?;
            }
            return document.encode();
        }

        let mut value = base.map(<[u8]>::to_vec).unwrap_or_else(|| {
            vec![
                0;
                match kind {
                    INT64 | FLOAT64 => 8,
                    FLOAT32 => 4,
                    STATS => 32,
                    _ => 0,
                }
            ]
        });
        for operand in operands {
            let (next_kind, delta) = decode(operand)?;
            if next_kind != kind {
                return None;
            }
            value = match kind {
                INT64 => i64::from_be_bytes(value.as_slice().try_into().ok()?)
                    .wrapping_add(i64::from_be_bytes(delta.try_into().ok()?))
                    .to_be_bytes()
                    .to_vec(),
                FLOAT32 => (f32::from_be_bytes(value.as_slice().try_into().ok()?)
                    + f32::from_be_bytes(delta.try_into().ok()?))
                .to_be_bytes()
                .to_vec(),
                FLOAT64 => (f64::from_be_bytes(value.as_slice().try_into().ok()?)
                    + f64::from_be_bytes(delta.try_into().ok()?))
                .to_be_bytes()
                .to_vec(),
                STATS => {
                    if value.len() != 32 || delta.len() != 32 {
                        return None;
                    }
                    let documents = i128::from_be_bytes(value[..16].try_into().ok()?)
                        .checked_add(i128::from_be_bytes(delta[..16].try_into().ok()?))?;
                    let length = i128::from_be_bytes(value[16..].try_into().ok()?)
                        .checked_add(i128::from_be_bytes(delta[16..].try_into().ok()?))?;
                    [documents.to_be_bytes(), length.to_be_bytes()].concat()
                }
                _ => return None,
            };
        }
        Some(value)
    }

    fn touches(&self, _key: &[u8], operand: &[u8], parts: &[u32]) -> bool {
        if let Some((kind @ (VECTOR_LINKS | VECTOR_COUNTS), bytes)) = decode(operand) {
            return vector::touches(kind, bytes, parts);
        }
        if let Some((HEAD_SET, bytes)) = decode(operand) {
            return head_set::touches(bytes, parts);
        }
        let Some((DOCUMENT, bytes)) = decode(operand) else {
            return true;
        };
        let Some(document) = document::stored::StoredDocument::decode(bytes) else {
            return true;
        };
        document.fields.keys().any(|id| parts.contains(id))
    }

    fn partial_merge(&self, _key: &[u8], left: &[u8], right: &[u8]) -> Option<Vec<u8>> {
        if matches!(decode(left), Some((HEAD_SET, _))) {
            return head_set::fold(left, right);
        }
        if matches!(decode(left), Some((VECTOR_LINKS, _))) {
            return vector::fold(left, right);
        }
        if let (Some((DOCUMENT, left)), Some((DOCUMENT, right))) = (decode(left), decode(right)) {
            let mut left = document::stored::StoredDocument::decode(left)?;
            let right = document::stored::StoredDocument::decode(right)?;
            // A join of LWW fields is associative, including deletion markers,
            // a Text replacement keeps the later one in any grouping, and
            // wrapping integer counter sums are exact in any grouping.
            // Floating-point counter sums are not, so they keep their order.
            // Json operands compose exactly, keeping what each removes.
            if left
                .fields
                .values()
                .chain(right.fields.values())
                .any(|field| {
                    !matches!(
                        field.kind,
                        document::stored::ValueKind::Lww
                            | document::stored::ValueKind::Int64
                            | document::stored::ValueKind::Text
                            | document::stored::ValueKind::Json
                    )
                })
            {
                return None;
            }
            left.fold(right)?;
            return document_parts(&left);
        }
        let (INT64, left) = decode(left)? else {
            return None;
        };
        let (INT64, right) = decode(right)? else {
            return None;
        };
        Some(counter_i64(
            i64::from_be_bytes(left.try_into().ok()?)
                .wrapping_add(i64::from_be_bytes(right.try_into().ok()?)),
        ))
    }
    // Floats and checked i128 stats retain their operand order. Folding a pair
    // could change rounding or hide an overflow against a later-supplied base.
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
