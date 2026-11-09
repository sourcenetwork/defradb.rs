//! Commit-related introspection type builders.
//!
//! Supporting types for the Commit object and _commits query field,
//! matching Go's commits.go type definitions.

use async_graphql::dynamic::*;
use async_graphql::Value as GqlValue;

use super::descriptions as d;

macro_rules! null_field {
    ($name:expr, $ty:expr) => {
        Field::new($name, $ty, |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        })
    };
}

/// Build the Signature object type.
pub(super) fn build_signature_type() -> Object {
    Object::new("Signature")
        .description(d::COMMIT_SIGNATURE)
        .field(null_field!("identity", TypeRef::named("String")).description(d::SIGNATURE_IDENTITY))
        .field(null_field!("type", TypeRef::named("String")).description(d::SIGNATURE_TYPE))
        .field(null_field!("value", TypeRef::named("String")).description(d::SIGNATURE_VALUE))
}

/// Build the CommitsFilterArg input object.
pub(super) fn build_commits_filter_arg() -> InputObject {
    InputObject::new("CommitsFilterArg")
        .description(d::COMMITS_FILTER)
        .field(
            InputValue::new("_and", TypeRef::named_list("CommitsFilterArg"))
                .description(d::AND_OPERATOR),
        )
        .field(
            InputValue::new("_or", TypeRef::named_list("CommitsFilterArg"))
                .description(d::OR_OPERATOR),
        )
        .field(
            InputValue::new("fieldName", TypeRef::named("CommitsFieldNameFilterArg"))
                .description(d::COMMITS_FILTER_FIELD_NAME),
        )
        .field(InputValue::new(
            "height",
            TypeRef::named("CommitsHeightFilterArg"),
        ))
}

/// Build the CommitsFieldNameFilterArg input object.
pub(super) fn build_commits_field_name_filter_arg() -> InputObject {
    InputObject::new("CommitsFieldNameFilterArg")
        .description(d::COMMITS_FIELD_NAME_FILTER)
        .field(InputValue::new("_eq", TypeRef::named("String")).description(d::EQ_OPERATOR))
        .field(InputValue::new("_in", TypeRef::named_list("String")).description(d::IN_OPERATOR))
        .field(InputValue::new("_ne", TypeRef::named("String")))
        .field(InputValue::new("_nin", TypeRef::named_list("String")).description(d::NIN_OPERATOR))
}

/// Build the CommitsHeightFilterArg input object.
pub(super) fn build_commits_height_filter_arg() -> InputObject {
    InputObject::new("CommitsHeightFilterArg")
        .field(InputValue::new("_eq", TypeRef::named("Int")))
        .field(InputValue::new("_geq", TypeRef::named("Int")))
        .field(InputValue::new("_gt", TypeRef::named("Int")))
        .field(InputValue::new("_in", TypeRef::named_list("Int")))
        .field(InputValue::new("_leq", TypeRef::named("Int")))
        .field(InputValue::new("_lt", TypeRef::named("Int")))
        .field(InputValue::new("_neq", TypeRef::named("Int")))
        .field(InputValue::new("_nin", TypeRef::named_list("Int")))
}

/// Build the commitsOrderArg input object.
pub(super) fn build_commits_order_arg() -> InputObject {
    InputObject::new("commitsOrderArg")
        .description(d::ORDER_ARG)
        .field(InputValue::new("cid", TypeRef::named("Ordering")).description(d::COMMIT_CID))
        .field(InputValue::new("docID", TypeRef::named("Ordering")).description(d::COMMIT_DOC_ID))
        .field(InputValue::new("height", TypeRef::named("Ordering")).description(d::COMMIT_HEIGHT))
}

/// Build the commitFields enum.
pub(super) fn build_commit_fields_enum() -> Enum {
    Enum::new("commitFields")
        .description(d::COMMIT_FIELDS)
        .item(EnumItem::new("cid").description(d::COMMIT_CID))
        .item(EnumItem::new("docID").description(d::COMMIT_DOC_ID))
        .item(EnumItem::new("fieldName").description(d::COMMIT_FIELD_NAME))
        .item(EnumItem::new("height").description(d::COMMIT_HEIGHT))
}

/// Build the commitCountFieldArg enum.
pub(super) fn build_commit_count_field_arg() -> Enum {
    Enum::new("commitCountFieldArg")
        .description(d::COUNT)
        .item(EnumItem::new("heads").description(d::COMMIT_LINKS))
        .item(EnumItem::new("links").description(d::COMMIT_LINKS))
}

/// Build the commitNumericFieldArg enum.
pub(super) fn build_commit_numeric_field_arg() -> Enum {
    Enum::new("commitNumericFieldArg")
        .item(EnumItem::new("delta"))
        .item(EnumItem::new("height"))
}
