//! The generated filter input types, as graphql-go sees them: which keys
//! each block defines, and the block each key's value is checked against.

use std::cell::Cell;
use std::sync::Arc;

use schema::{CollectionVersion, FieldKind, ScalarKind};

/// The operators a scalar operator block defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ops {
    /// `Int`, `Float32`, `Float64`, `DateTime`: equality, membership and range.
    Comparison,
    /// `ID`, `Boolean`: equality and membership.
    Equality,
    /// `String`, `Blob`: equality, membership, the pattern operators and, as
    /// a Rust extension Go's blocks lack, range (text compares
    /// lexicographically, RFC 3339 text as time).
    Text,
    /// An encrypted index's field, which `encrypted_<Collection>` filters by `_eq` only.
    EqOnly,
}

impl Ops {
    fn defines(self, key: &str) -> bool {
        let key = canonical(key);
        match self {
            Self::Comparison => matches!(
                key,
                "_eq" | "_geq" | "_gt" | "_in" | "_leq" | "_lt" | "_neq" | "_nin"
            ),
            Self::Equality => matches!(key, "_eq" | "_in" | "_neq" | "_nin"),
            Self::Text => matches!(
                key,
                "_eq"
                    | "_geq"
                    | "_gt"
                    | "_ilike"
                    | "_in"
                    | "_leq"
                    | "_like"
                    | "_lt"
                    | "_neq"
                    | "_nilike"
                    | "_nin"
                    | "_nlike"
            ),
            Self::EqOnly => key == "_eq",
        }
    }
}

/// The spelling Go's blocks use for one of the operator aliases the Rust
/// filter evaluator also accepts (`FilterOp::parse`), a recorded divergence.
fn canonical(key: &str) -> &str {
    match key {
        "_ne" => "_neq",
        "_gte" | "_ge" => "_geq",
        "_lte" | "_le" => "_leq",
        other => other,
    }
}

/// A scalar block: the operators it defines, and the name Go gives it,
/// which a non-object value given for it is reported by.
#[derive(Debug, Clone, Copy)]
pub(super) struct Block {
    ops: Ops,
    name: BlockName,
}

#[derive(Debug, Clone, Copy)]
enum BlockName {
    /// Go's name for the scalar (`Int`, `String`), and whether the block is
    /// the `NotNull` one.
    Scalar(&'static str, bool),
    /// `_commits`'s `fieldName` block, `CommitsFieldNameFilterArg`.
    CommitsFieldName,
    /// A Rust extension Go generates no block for.
    Extension,
}

impl Block {
    fn scalar(kind: ScalarKind, not_null: bool) -> Option<Self> {
        Some(Self {
            ops: scalar_ops(kind)?,
            name: BlockName::Scalar(go_scalar_name(kind)?, not_null),
        })
    }

    fn id() -> Self {
        Self {
            ops: Ops::Equality,
            name: BlockName::Scalar("ID", false),
        }
    }

    fn extension(ops: Ops) -> Self {
        Self {
            ops,
            name: BlockName::Extension,
        }
    }

    fn name(&self, suffix: &str) -> Option<String> {
        match self.name {
            BlockName::Scalar(kind, not_null) => {
                let prefix = if not_null { "NotNull" } else { "" };
                Some(format!("{prefix}{kind}{suffix}"))
            }
            BlockName::CommitsFieldName => Some("CommitsFieldNameFilterArg".to_string()),
            BlockName::Extension => None,
        }
    }
}

/// One generated filter input type.
#[derive(Debug, Clone, Copy)]
pub(super) enum Shape<'a> {
    /// `<Collection>FilterArg`.
    Collection(&'a CollectionVersion),
    /// `<Collection>EncryptedFilterArg`: its encrypted index fields, by `_eq`.
    Encrypted(&'a CollectionVersion),
    /// A scalar `<Kind>OperatorBlock`, or an encrypted field's
    /// `<Kind>EncryptedFilterArg`.
    Ops(Block),
    /// A `<Kind>ListOperatorBlock` over its element's operator block.
    List(Block),
    /// A scalar `<Kind>FilterArg`, which a scalar list's aggregate filters by.
    ScalarFilter(Block),
    /// `CommitsFilterArg`.
    Commits,
    /// A value no shape constrains (`JSON`, an operator's operand).
    Any,
}

impl Shape<'_> {
    /// The input type's name as Go generates it: `None` for a value no
    /// input type constrains, and for a Rust extension Go has no type for.
    pub(super) fn name(&self) -> Option<String> {
        match self {
            Self::Collection(collection) => Some(format!("{}FilterArg", collection.name)),
            Self::Encrypted(collection) => Some(format!("{}EncryptedFilterArg", collection.name)),
            Self::Commits => Some("CommitsFilterArg".to_string()),
            Self::Ops(block) if block.ops == Ops::EqOnly => block.name("EncryptedFilterArg"),
            Self::Ops(block) => block.name("OperatorBlock"),
            Self::List(block) => block.name("ListOperatorBlock"),
            Self::ScalarFilter(block) => block.name("FilterArg"),
            Self::Any => None,
        }
    }
}

/// Where a key leads: the block its value is checked against, or nothing
/// when the block does not define the key.
#[derive(Debug, Clone, Copy)]
pub(super) enum Field<'a> {
    /// The key is not defined.
    Unknown,
    /// A value of this shape.
    One(Shape<'a>),
    /// A list of values of this shape (a single value counts as a list of one).
    ListOf(Shape<'a>),
}

/// Finds the collection a relation field points at, among those loaded.
pub(super) struct Collections<'a> {
    all: &'a [Arc<CollectionVersion>],
    missing: Cell<bool>,
}

impl<'a> Collections<'a> {
    pub(super) fn new(all: &'a [Arc<CollectionVersion>]) -> Self {
        Self {
            all,
            missing: Cell::new(false),
        }
    }

    /// Whether a relation pointed at a collection that was not loaded, so
    /// its filter was not checked.
    pub(super) fn missed(&self) -> bool {
        self.missing.get()
    }

    /// The collection a relation of `kind` on `from` points at. A self
    /// reference with a relative ID names a member of `from`'s collection
    /// set: in a cycle `A.b -> B.c -> C.a`, `A.b` names B's place.
    pub(super) fn related(
        &self,
        from: &'a CollectionVersion,
        kind: &FieldKind,
    ) -> Option<&'a CollectionVersion> {
        let id = kind.relation_collection_id()?;
        if id.is_empty() {
            return Some(from);
        }
        let found = if matches!(kind, FieldKind::SelfRef { .. }) {
            let set = from.collection_set.as_ref()?;
            let member = |c: &CollectionVersion| {
                c.collection_set.as_ref().is_some_and(|other| {
                    other.collection_set_id == set.collection_set_id
                        && other.relative_id.to_string() == id
                })
            };
            if member(from) {
                return Some(from);
            }
            self.all.iter().map(Arc::as_ref).find(|c| member(c))
        } else {
            self.all
                .iter()
                .map(Arc::as_ref)
                .find(|c| c.collection_id == id || c.version_id == id || c.name == id)
        };
        if found.is_none() {
            self.missing.set(true);
        }
        found
    }

    /// The block `key` of `shape` is checked against.
    pub(super) fn field(&self, shape: Shape<'a>, key: &str) -> Field<'a> {
        match shape {
            Shape::Collection(collection) => self.collection_field(collection, key),
            Shape::Encrypted(collection) => encrypted_field(collection, key),
            Shape::Ops(block) if block.ops.defines(key) => Field::One(Shape::Any),
            Shape::Ops(_) => Field::Unknown,
            Shape::List(block) => match canonical(key) {
                "_all" | "_any" | "_none" => Field::One(Shape::Ops(block)),
                "_eq" | "_neq" => Field::One(Shape::Any),
                // A Rust extension to Go's list blocks.
                "_count" => Field::One(Shape::Ops(Block::extension(Ops::Comparison))),
                _ => Field::Unknown,
            },
            Shape::ScalarFilter(block) => match key {
                "_and" | "_or" => Field::ListOf(shape),
                key if block.ops.defines(key) => Field::One(Shape::Any),
                _ => Field::Unknown,
            },
            Shape::Commits => match key {
                "_and" | "_or" => Field::ListOf(shape),
                "fieldName" => Field::One(Shape::Ops(Block {
                    ops: Ops::Equality,
                    name: BlockName::CommitsFieldName,
                })),
                // A Rust extension to Go's commits filter.
                "height" => Field::One(Shape::Ops(Block::extension(Ops::Comparison))),
                _ => Field::Unknown,
            },
            Shape::Any => Field::One(Shape::Any),
        }
    }

    fn collection_field(&self, collection: &'a CollectionVersion, key: &str) -> Field<'a> {
        match key {
            "_and" | "_or" => return Field::ListOf(Shape::Collection(collection)),
            "_not" => return Field::One(Shape::Collection(collection)),
            "_docID" => return Field::One(Shape::Ops(Block::id())),
            // `_alias` is JSON in Go. The other three are Rust-only filter
            // keys Go's block lacks, kept as a recorded divergence.
            "_alias" | "_deleted" | "_version" | "GROUP" | "__typename" => {
                return Field::One(Shape::Any)
            }
            _ => {}
        }
        if let Some(field) = collection.fields.iter().find(|f| f.name == key) {
            if field.name == "_docID" || is_relation_id(collection, key) {
                return Field::One(Shape::Ops(Block::id()));
            }
            return Field::One(self.kind_shape(collection, &field.kind));
        }
        if is_relation_id(collection, key) {
            return Field::One(Shape::Ops(Block::id()));
        }
        Field::Unknown
    }

    /// The block a field of `kind` is filtered by. A scalar field's block is
    /// named for its scalar whether or not the field is nillable, as Go
    /// names it; a list's is `NotNull` when its elements are not nillable.
    pub(super) fn kind_shape(
        &self,
        collection: &'a CollectionVersion,
        kind: &FieldKind,
    ) -> Shape<'a> {
        match kind {
            FieldKind::Scalar(scalar) => {
                Block::scalar(*scalar, false).map_or(Shape::Any, Shape::Ops)
            }
            FieldKind::ScalarArray(array) => {
                Block::scalar(array.element_kind(), !array.has_nillable_elements())
                    .map_or(Shape::Any, Shape::List)
            }
            _ => self
                .related(collection, kind)
                .map_or(Shape::Any, Shape::Collection),
        }
    }

    /// The block a scalar list field's aggregate `filter` is checked against.
    pub(super) fn scalar_filter(kind: &FieldKind) -> Shape<'a> {
        match kind {
            FieldKind::ScalarArray(array) => {
                Block::scalar(array.element_kind(), !array.has_nillable_elements())
                    .map_or(Shape::Any, Shape::ScalarFilter)
            }
            _ => Shape::Any,
        }
    }
}

/// An `encrypted_<Collection>` filter's key: an encrypted index's field,
/// filtered by `_eq` in a block Go names `[NotNull]<Kind>EncryptedFilterArg`.
fn encrypted_field<'a>(collection: &CollectionVersion, key: &str) -> Field<'a> {
    if !collection
        .encrypted_indexes
        .iter()
        .any(|index| index.field_name == key)
    {
        return Field::Unknown;
    }
    let name = match collection.field_by_name(key).map(|field| &field.kind) {
        Some(FieldKind::Scalar(scalar)) => go_scalar_name(*scalar)
            .map_or(BlockName::Extension, |name| {
                BlockName::Scalar(name, !scalar.is_nillable())
            }),
        _ => BlockName::Extension,
    };
    Field::One(Shape::Ops(Block {
        ops: Ops::EqOnly,
        name,
    }))
}

/// `_<relation>ID`, the backing field of a relation.
fn is_relation_id(collection: &CollectionVersion, key: &str) -> bool {
    key.strip_prefix('_')
        .and_then(|rest| rest.strip_suffix("ID"))
        .is_some_and(|name| {
            collection
                .fields
                .iter()
                .any(|f| f.name == name && f.kind.is_relation())
        })
}

/// The name Go's schema gives a scalar, which its blocks are named after.
fn go_scalar_name(kind: ScalarKind) -> Option<&'static str> {
    match kind.base_kind() {
        ScalarKind::DocID => Some("ID"),
        ScalarKind::NillableBool => Some("Boolean"),
        ScalarKind::NillableInt => Some("Int"),
        ScalarKind::NillableFloat64 => Some("Float64"),
        ScalarKind::NillableFloat32 => Some("Float32"),
        ScalarKind::NillableDateTime => Some("DateTime"),
        ScalarKind::NillableString => Some("String"),
        ScalarKind::NillableBlob => Some("Blob"),
        _ => None,
    }
}

fn scalar_ops(kind: ScalarKind) -> Option<Ops> {
    match kind.base_kind() {
        ScalarKind::DocID | ScalarKind::NillableBool => Some(Ops::Equality),
        ScalarKind::NillableInt
        | ScalarKind::NillableFloat64
        | ScalarKind::NillableFloat32
        | ScalarKind::NillableDateTime => Some(Ops::Comparison),
        ScalarKind::NillableString | ScalarKind::NillableBlob => Some(Ops::Text),
        _ => None,
    }
}
