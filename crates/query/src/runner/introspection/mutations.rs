use async_graphql::dynamic::*;
use async_graphql::Value as GqlValue;
use schema::CollectionVersion;
use std::sync::Arc;

use super::descriptions as d;

/// Build the Mutation type.
pub(super) fn build_mutation_type(collections: &[Arc<CollectionVersion>]) -> Object {
    let mut mutation = Object::new("Mutation").description("Root mutation type");

    for collection in collections {
        // Embedded-only types don't have mutation fields
        if collection.is_embedded_only {
            continue;
        }

        let coll_name = collection.name.clone();
        let input_type = format!("{}MutationInputArg", coll_name);

        // add_<Collection>
        mutation = mutation.field(
            Field::new(
                format!("add_{}", coll_name),
                TypeRef::named_nn(&coll_name),
                |_| FieldFuture::new(async { Ok(Some(GqlValue::Null)) }),
            )
            .description(d::ADD_DOCUMENTS)
            .argument(
                InputValue::new("input", TypeRef::named_nn(&input_type))
                    .description(format!("Add {coll_name} documents")),
            ),
        );

        // update_<Collection>
        let update_coll_name = coll_name.clone();
        let update_input_type = input_type.clone();
        mutation = mutation.field(
            Field::new(
                format!("update_{}", coll_name),
                TypeRef::named_nn_list_nn(&update_coll_name),
                |_| FieldFuture::new(async { Ok(Some(GqlValue::Null)) }),
            )
            .description(d::UPDATE_DOCUMENTS)
            .argument(InputValue::new("docID", TypeRef::named("ID")).description(d::UPDATE_ID_ARG))
            .argument(
                InputValue::new("docIDs", TypeRef::named_list("ID")).description(d::UPDATE_IDS_ARG),
            )
            .argument(
                InputValue::new("input", TypeRef::named_nn(&update_input_type))
                    .description(d::UPDATE_INPUT_ARG),
            ),
        );

        // delete_<Collection>
        let del_coll_name = coll_name.clone();
        mutation = mutation.field(
            Field::new(
                format!("delete_{}", coll_name),
                TypeRef::named_nn_list_nn(&del_coll_name),
                |_| FieldFuture::new(async { Ok(Some(GqlValue::Null)) }),
            )
            .description(d::DELETE_DOCUMENTS)
            .argument(InputValue::new("docID", TypeRef::named("ID")).description(d::DELETE_ID_ARG))
            .argument(
                InputValue::new("docIDs", TypeRef::named_list("ID")).description(d::DELETE_IDS_ARG),
            )
            .argument(
                InputValue::new(
                    "filter",
                    TypeRef::named(format!("{}FilterArg", del_coll_name)),
                )
                .description(d::DELETE_FILTER_ARG),
            ),
        );

        // truncate_<Collection>
        let truncate_coll_name = coll_name.clone();
        mutation = mutation.field(
            Field::new(
                format!("truncate_{}", coll_name),
                TypeRef::named_nn(TypeRef::BOOLEAN),
                |_| FieldFuture::new(async { Ok(Some(GqlValue::Null)) }),
            )
            .description(d::TRUNCATE_DOCUMENTS)
            .argument(
                InputValue::new(
                    "filter",
                    TypeRef::named(format!("{}FilterArg", truncate_coll_name)),
                )
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
