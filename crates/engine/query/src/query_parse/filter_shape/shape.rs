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

/// One generated filter input type.
#[derive(Debug, Clone, Copy)]
pub(super) enum Shape<'a> {
    /// `<Collection>FilterArg`.
    Collection(&'a CollectionVersion),
    /// `<Collection>EncryptedFilterArg`: its encrypted index fields, by `_eq`.
    Encrypted(&'a CollectionVersion),
    /// A scalar `<Kind>OperatorBlock`.
    Ops(Ops),
    /// A `<Kind>ListOperatorBlock` over its element's operator block.
    List(Ops),
    /// A scalar `<Kind>FilterArg`, which a scalar list's aggregate filters by.
    ScalarFilter(Ops),
    /// `CommitsFilterArg`.
    Commits,
    /// A value no shape constrains (`JSON`, an operator's operand).
    Any,
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

    pub(super) fn related(
        &self,
        from: &'a CollectionVersion,
        kind: &FieldKind,
    ) -> Option<&'a CollectionVersion> {
        let id = kind.relation_collection_id()?;
        if id.is_empty() || matches!(kind, FieldKind::SelfRef { .. }) {
            return Some(from);
        }
        let found = self
            .all
            .iter()
            .map(Arc::as_ref)
            .find(|c| c.collection_id == id || c.version_id == id || c.name == id);
        if found.is_none() {
            self.missing.set(true);
        }
        found
    }

    /// The block `key` of `shape` is checked against.
    pub(super) fn field(&self, shape: Shape<'a>, key: &str) -> Field<'a> {
        match shape {
            Shape::Collection(collection) => self.collection_field(collection, key),
            Shape::Encrypted(collection) => {
                if collection
                    .encrypted_indexes
                    .iter()
                    .any(|index| index.field_name == key)
                {
                    Field::One(Shape::Ops(Ops::EqOnly))
                } else {
                    Field::Unknown
                }
            }
            Shape::Ops(ops) if ops.defines(key) => Field::One(Shape::Any),
            Shape::Ops(_) => Field::Unknown,
            Shape::List(ops) => match key {
                "_all" | "_any" | "_none" => Field::One(Shape::Ops(ops)),
                "_eq" | "_neq" => Field::One(Shape::Any),
                // A Rust extension to Go's list blocks.
                "_count" => Field::One(Shape::Ops(Ops::Comparison)),
                _ => Field::Unknown,
            },
            Shape::ScalarFilter(ops) => match key {
                "_and" | "_or" => Field::ListOf(shape),
                key if ops.defines(key) => Field::One(Shape::Any),
                _ => Field::Unknown,
            },
            Shape::Commits => match key {
                "_and" | "_or" => Field::ListOf(shape),
                "fieldName" => Field::One(Shape::Ops(Ops::Equality)),
                // A Rust extension to Go's commits filter.
                "height" => Field::One(Shape::Ops(Ops::Comparison)),
                _ => Field::Unknown,
            },
            Shape::Any => Field::One(Shape::Any),
        }
    }

    fn collection_field(&self, collection: &'a CollectionVersion, key: &str) -> Field<'a> {
        match key {
            "_and" | "_or" => return Field::ListOf(Shape::Collection(collection)),
            "_not" => return Field::One(Shape::Collection(collection)),
            "_docID" => return Field::One(Shape::Ops(Ops::Equality)),
            // `_alias` is JSON in Go. The other three are Rust-only filter
            // keys Go's block lacks, kept as a recorded divergence.
            "_alias" | "_deleted" | "_version" | "GROUP" | "__typename" => {
                return Field::One(Shape::Any)
            }
            _ => {}
        }
        if let Some(field) = collection.fields.iter().find(|f| f.name == key) {
            if field.name == "_docID" {
                return Field::One(Shape::Ops(Ops::Equality));
            }
            if is_relation_id(collection, key) {
                return Field::One(Shape::Ops(Ops::Equality));
            }
            return Field::One(self.kind_shape(collection, &field.kind));
        }
        if is_relation_id(collection, key) {
            return Field::One(Shape::Ops(Ops::Equality));
        }
        Field::Unknown
    }

    /// The block a field of `kind` is filtered by.
    pub(super) fn kind_shape(
        &self,
        collection: &'a CollectionVersion,
        kind: &FieldKind,
    ) -> Shape<'a> {
        match kind {
            FieldKind::Scalar(scalar) => scalar_ops(*scalar).map_or(Shape::Any, Shape::Ops),
            FieldKind::ScalarArray(array) => {
                scalar_ops(array.element_kind()).map_or(Shape::Any, Shape::List)
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
                scalar_ops(array.element_kind()).map_or(Shape::Any, Shape::ScalarFilter)
            }
            _ => Shape::Any,
        }
    }
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
