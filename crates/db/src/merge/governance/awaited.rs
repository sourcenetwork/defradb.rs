use cid::Cid;
use document::NormalValue;
use schema::{CType, CollectionVersion};

/// An input a deferred composite waits for.
#[derive(Debug, Clone, PartialEq)]
pub enum Awaited {
    /// A composite, by CID; a document is named by its genesis composite.
    Composite(Cid),
    /// Any composite merging into `collection` whose `@immutable` scalar LWW
    /// `field` equals `value`. For an entry that does not exist yet, so has
    /// no CID to name.
    ImmutableField {
        collection: String,
        field: String,
        value: NormalValue,
    },
}

impl Awaited {
    pub fn immutable_field(
        collection: impl Into<String>,
        field: impl Into<String>,
        value: NormalValue,
    ) -> Self {
        Self::ImmutableField {
            collection: collection.into(),
            field: field.into(),
            value,
        }
    }
}

impl From<Cid> for Awaited {
    fn from(cid: Cid) -> Self {
        Self::Composite(cid)
    }
}

/// The index key of an awaited input: values compared by canonical CBOR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum WaitKey {
    Composite(Cid),
    ImmutableField {
        collection: String,
        field: String,
        value: Vec<u8>,
    },
}

impl WaitKey {
    pub(crate) fn immutable_field(
        collection: &str,
        field: &str,
        value: &NormalValue,
    ) -> Result<Self, String> {
        Ok(Self::ImmutableField {
            collection: collection.to_string(),
            field: field.to_string(),
            value: crate::block::builder::encode_value_as_cbor(value)?,
        })
    }
}

/// Whether `field` is set once and so stable across replicas: an
/// `@immutable` scalar LWW field.
pub(crate) fn is_immutable_scalar_field(collection: &CollectionVersion, field: &str) -> bool {
    collection
        .fields
        .iter()
        .find(|candidate| candidate.name == field)
        .is_some_and(|field| {
            field.immutable && field.crdt_type == CType::LwwRegister && field.kind.is_scalar()
        })
}
