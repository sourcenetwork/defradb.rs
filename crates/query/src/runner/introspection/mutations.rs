use async_graphql::dynamic::*;
use async_graphql::Value as GqlValue;
use schema::CollectionVersion;
use std::sync::Arc;

use super::descriptions as d;

/// Build the Mutation type, with Go's fields, arguments and types. `docIDs` on
/// update and delete is defradb.rs's own spelling of `docID`, kept alongside.
pub(super) fn build_mutation_type(collections: &[Arc<CollectionVersion>]) -> Object {
    let mut mutation = Object::new("Mutation").description("Root mutation type");

    for collection in collections {
        // Embedded-only types have no mutations, and `@chunk` collections are
        // written only through their parent.
        if collection.is_embedded_only || collection.is_generated() {
            continue;
        }

        let name = collection.name.as_str();
        let input = format!("{name}MutationInputArg");
        let filter = format!("{name}FilterArg");
        let field = |prefix: &str, ty: TypeRef| {
            Field::new(format!("{prefix}_{name}"), ty, |_| {
                FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
            })
        };

        mutation = mutation
            .field(
                field("add", TypeRef::named_list(name))
                    .description(d::ADD_DOCUMENTS)
                    .argument(
                        InputValue::new("encrypt", TypeRef::named(TypeRef::BOOLEAN))
                            .description(d::ENCRYPT_ARG),
                    )
                    .argument(
                        InputValue::new(
                            "encryptFields",
                            TypeRef::named_nn_list(format!("{name}ExplicitField")),
                        )
                        .description(d::ENCRYPT_FIELDS_ARG),
                    )
                    .argument(
                        InputValue::new("input", TypeRef::named_nn_list(&input))
                            .description(format!("Add {name} documents")),
                    ),
            )
            .field(
                field("update", TypeRef::named_list(name))
                    .description(d::UPDATE_DOCUMENTS)
                    .argument(
                        InputValue::new("docID", TypeRef::named_nn_list(TypeRef::ID))
                            .description(d::UPDATE_IDS_ARG),
                    )
                    .argument(
                        InputValue::new("docIDs", TypeRef::named_list(TypeRef::ID))
                            .description(d::UPDATE_IDS_ARG),
                    )
                    .argument(
                        InputValue::new("filter", TypeRef::named(&filter))
                            .description(d::UPDATE_FILTER_ARG),
                    )
                    .argument(
                        InputValue::new("input", TypeRef::named(&input))
                            .description(d::UPDATE_INPUT_ARG),
                    ),
            )
            .field(
                field("upsert", TypeRef::named_list(name))
                    .description(d::UPSERT_DOCUMENTS)
                    .argument(
                        InputValue::new("add", TypeRef::named_nn(&input))
                            .description(d::UPSERT_ADD_ARG),
                    )
                    .argument(
                        InputValue::new("filter", TypeRef::named_nn(&filter))
                            .description(d::UPSERT_FILTER_ARG),
                    )
                    .argument(
                        InputValue::new("update", TypeRef::named_nn(&input))
                            .description(d::UPDATE_INPUT_ARG),
                    ),
            )
            .field(
                field("delete", TypeRef::named_list(name))
                    .description(d::DELETE_DOCUMENTS)
                    .argument(
                        InputValue::new("docID", TypeRef::named_nn_list(TypeRef::ID))
                            .description(d::DELETE_IDS_ARG),
                    )
                    .argument(
                        InputValue::new("docIDs", TypeRef::named_list(TypeRef::ID))
                            .description(d::DELETE_IDS_ARG),
                    )
                    .argument(
                        InputValue::new("filter", TypeRef::named(&filter))
                            .description(d::DELETE_FILTER_ARG),
                    ),
            )
            .field(
                field("truncate", TypeRef::named_nn(TypeRef::BOOLEAN))
                    .description(d::TRUNCATE_DOCUMENTS)
                    .argument(
                        InputValue::new("filter", TypeRef::named(&filter))
                            .description(d::TRUNCATE_FILTER_ARG),
                    ),
            );
    }

    mutation
}

/// Build the ExplainType enum.
pub(super) fn build_explain_enum() -> Enum {
    Enum::new("ExplainType")
        .description(
            "ExplainType is an enum selecting the type of explanation done by the @explain directive.",
        )
        .item(
            EnumItem::new("simple")
                .description("Simple explanation - dump of the plan graph."),
        )
        .item(EnumItem::new("execute").description(
            "Deeper explanation - insights gathered by executing the plan graph.",
        ))
        .item(
            EnumItem::new("debug")
                .description("Like simple explain, but more verbose nodes (no attributes)."),
        )
}

/// Build the Ordering enum.
pub(super) fn build_ordering_enum() -> Enum {
    Enum::new("Ordering")
        .item(EnumItem::new("ASC").description(d::ORDER_ASC))
        .item(EnumItem::new("DESC").description(d::ORDER_DESC))
}
