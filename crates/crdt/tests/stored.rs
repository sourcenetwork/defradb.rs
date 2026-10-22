use crdt::field::{CounterField, LwwField};
use crdt::stored::{document_from_storage, encode_parts, Part, StoredDocument, ValueKind};
use document::NormalValue;
use schema::CollectionVersion;

const MAGIC: &[u8; 4] = b"DFV\x01";

/// Whether two values of one register store the same state. An absent field
/// stores nothing, as nil does.
fn stores_same(
    left: Option<&NormalValue>,
    right: Option<&NormalValue>,
) -> Result<bool, crdt::FieldCrdtError> {
    let stored = |value: Option<&NormalValue>| {
        LwwField::stored("f".into(), 0, value.unwrap_or(&NormalValue::Null))
            .map(|state| state.value)
    };
    Ok(stored(left)? == stored(right)?)
}

use schema::{CType, CollectionBuilder, FieldDescription, FieldKind, ScalarKind};

fn test_collection() -> CollectionVersion {
    ["name", "same", "count"]
        .iter()
        .enumerate()
        .fold(
            CollectionBuilder::new("Test", "bafytest"),
            |builder, (i, name)| {
                builder.field(FieldDescription::new(
                    i.to_string(),
                    *name,
                    FieldKind::Scalar(ScalarKind::None),
                    CType::LwwRegister,
                ))
            },
        )
        .build()
}

#[test]
fn document_from_storage_materializes_a_stored_document() {
    let mut stored = StoredDocument::default();
    stored.fields.insert(
        1,
        LwwField::stored("name".to_owned(), 1, &NormalValue::String("Ada".to_owned())).unwrap(),
    );
    let bytes = stored.encode().unwrap();

    let document = document_from_storage(&bytes, test_collection()).unwrap();
    assert_eq!(
        document.get("name").and_then(NormalValue::as_str),
        Some("Ada")
    );
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
                stores_same(Some(left), Some(right)).unwrap(),
                "{left:?} and {right:?}"
            );
        }
    }
    assert!(stores_same(
        Some(&NormalValue::StringArray(vec!["a".to_owned()])),
        Some(&NormalValue::NillableStringArray(Some(
            vec!["a".to_owned()]
        ))),
    )
    .unwrap());
    assert!(stores_same(None, Some(&NormalValue::Null)).unwrap());
    assert!(stores_same(None, Some(&NormalValue::NillableStringArray(None))).unwrap());
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
            !stores_same(Some(&empty), Some(&other)).unwrap(),
            "{other:?}"
        );
    }
    assert!(!stores_same(None, Some(&empty)).unwrap());
    assert!(!stores_same(
        Some(&NormalValue::Int(5)),
        Some(&NormalValue::String("5".to_owned())),
    )
    .unwrap());
}

#[test]
fn document_from_storage_rejects_invalid_field_values() {
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
    assert!(document_from_storage(&bytes, test_collection()).is_err());
}

#[test]
fn stored_document_rejects_duplicate_field_names() {
    let mut stored = StoredDocument::default();
    for id in [1, 2] {
        stored.fields.insert(
            id,
            LwwField::stored("same".to_owned(), 1, &NormalValue::Null).unwrap(),
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
        CounterField::stored(
            "count".to_owned(),
            ValueKind::Int64,
            7,
            &NormalValue::Int(-3),
        )
        .unwrap(),
    );
    stored.fields.insert(
        5,
        LwwField::stored("name".to_owned(), 9, &NormalValue::String("Ada".to_owned())).unwrap(),
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
