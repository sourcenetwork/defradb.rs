//! GraphQL introspection support.
//!
//! This module provides introspection query execution using async-graphql.
//! Introspection queries (__schema, __type) are executed against a dynamically
//! generated schema based on the current collections.

mod aggregates;
pub(crate) mod cache;
mod collection;
mod commits;
mod descriptions;
mod input_types;
mod mutations;
mod operators;

use async_graphql::{dynamic::*, Value as GqlValue};
use rapidhash::RapidHashMap;
use schema::CollectionVersion;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use crate::error::{QueryError, Result};

use descriptions as d;

use aggregates::{build_aggregate_types_for_collection, build_numeric_fields_enum};
use collection::{build_collection_type, build_commit_type};
use commits::{
    build_commit_count_field_arg, build_commit_fields_enum, build_commit_numeric_field_arg,
    build_commits_field_name_filter_arg, build_commits_filter_arg, build_commits_height_filter_arg,
    build_commits_order_arg, build_signature_type,
};
use input_types::{
    build_explicit_field_enum, build_field_enum, build_filter_input_type,
    build_mutation_input_type, build_order_input_type,
};
use mutations::{build_explain_enum, build_mutation_type, build_ordering_enum};
use operators::{
    build_bool_filter_arg, build_bool_list_operator_block, build_bool_operator_block,
    build_datetime_operator_block, build_float32_filter_arg, build_float32_list_operator_block,
    build_float32_operator_block, build_float64_filter_arg, build_float64_list_operator_block,
    build_float64_operator_block, build_float_operator_block, build_id_operator_block,
    build_int_filter_arg, build_int_list_operator_block, build_int_operator_block,
    build_not_null_bool_filter_arg, build_not_null_bool_list_operator_block,
    build_not_null_float32_filter_arg, build_not_null_float32_list_operator_block,
    build_not_null_float64_filter_arg, build_not_null_float64_list_operator_block,
    build_not_null_int_filter_arg, build_not_null_int_list_operator_block,
    build_not_null_string_filter_arg, build_not_null_string_list_operator_block,
    build_string_filter_arg, build_string_list_operator_block, build_string_operator_block,
};

/// Build an async-graphql schema from collections for introspection.
pub fn build_introspection_schema(
    collections: &[Arc<CollectionVersion>],
) -> std::result::Result<Schema, SchemaError> {
    // Build a mapping from collection ID to collection name for relation resolution
    let mut id_to_name: RapidHashMap<String, String> = collections
        .iter()
        .map(|c| (c.collection_id.clone(), c.name.clone()))
        .collect();

    // Add version_id → name entries so that CID-based relation references
    // (common in views with embedded schemas) can resolve to collection names.
    for c in collections {
        if !c.version_id.is_empty() {
            id_to_name
                .entry(c.version_id.clone())
                .or_insert_with(|| c.name.clone());
        }
    }

    // Add relative_id → name entries for collections in collection sets.
    // This allows SelfRef fields (which use relative_id as their identifier)
    // to resolve to the correct collection name during introspection.
    for c in collections {
        if let Some(ref set) = c.collection_set {
            id_to_name.insert(set.relative_id.to_string(), c.name.clone());
        }
    }

    // Start with basic scalar types
    // Register Mutation root when collections exist so MutationInputArg types are reachable
    let mutation_name = if collections.is_empty() {
        None
    } else {
        Some("Mutation")
    };
    let mut schema_builder = Schema::build("Query", mutation_name, Some("Subscription"));

    // Build a Query type with fields for each collection
    let mut query_type = Object::new("Query").description("Root query type");
    // Subscriptions are served over SSE; this type only describes them.
    let mut subscription_type = Subscription::new("Subscription");

    // Create object types for each collection and add query fields
    for collection in collections {
        // Create object type for this collection (always register for type system)
        let obj_type = build_collection_type(collection, &id_to_name);
        schema_builder = schema_builder.register(obj_type);

        // Create filter input type
        let filter_type = build_filter_input_type(collection, &id_to_name);
        schema_builder = schema_builder.register(filter_type);

        // Create order input type
        let order_type = build_order_input_type(collection, &id_to_name);
        schema_builder = schema_builder.register(order_type);

        // Create Field enum for this collection (e.g., UserField)
        let field_enum = build_field_enum(collection);
        schema_builder = schema_builder.register(field_enum);
        if !collection.is_embedded_only {
            schema_builder = schema_builder.register(build_explicit_field_enum(collection));
        }

        // Add mutation input types (needed even for embedded types since
        // non-embedded types may reference them in their mutation inputs)
        let mutation_input = build_mutation_input_type(collection);
        schema_builder = schema_builder.register(mutation_input);

        // Add aggregate selector types
        let agg_types = build_aggregate_types_for_collection(collection);
        for agg_type in agg_types {
            schema_builder = schema_builder.register(agg_type);
        }

        // Add numeric fields enum
        let numeric_enum = build_numeric_fields_enum(collection);
        schema_builder = schema_builder.register(numeric_enum);

        // Embedded-only types (interface types from view SDL) are registered in the type
        // system but not as root query fields - they can only be accessed via relations.
        if collection.is_embedded_only {
            continue;
        }

        // Add query field for this collection (e.g., User)
        // Args sorted alphabetically to match Go introspection output
        query_type = query_type.field(select_args(&collection.name).into_iter().fold(
            Field::new(
                &collection.name,
                TypeRef::named_nn_list_nn(&collection.name),
                |_ctx| FieldFuture::new(async { Ok(Some(GqlValue::List(vec![]))) }),
            ),
            Field::argument,
        ));
        subscription_type = subscription_type.field(subscription_field(
            &collection.name,
            TypeRef::named_list(&collection.name),
            select_args(&collection.name),
        ));
    }

    let arrival = Object::new("DocumentArrival")
        .field(Field::new("cursor", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("docID", TypeRef::named_nn("ID"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }));
    let arrival_page = Object::new("DocumentArrivalPage")
        .field(Field::new("head", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("next", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new(
            "entries",
            TypeRef::named_nn_list_nn("DocumentArrival"),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::List(vec![]))) }),
        ));
    schema_builder = schema_builder.register(arrival).register(arrival_page);
    query_type = query_type.field(
        Field::new(
            "_documentArrivals",
            TypeRef::named_nn("DocumentArrivalPage"),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::Null)) }),
        )
        .description("Reads committed document arrivals in local commit order. Creates made by the current transaction appear only after that transaction commits.")
        .argument(InputValue::new("collection", TypeRef::named_nn("String")))
        .argument(InputValue::new("after", TypeRef::named("String")))
        .argument(InputValue::new("limit", TypeRef::named("Int")))
        .argument(InputValue::new("docID", TypeRef::named_nn_list("ID"))),
    );

    let update = Object::new("DocumentUpdate")
        .field(Field::new("cursor", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("docID", TypeRef::named_nn("ID"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("cid", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }));
    let update_page = Object::new("DocumentUpdatePage")
        .field(Field::new("head", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("next", TypeRef::named_nn("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new(
            "entries",
            TypeRef::named_nn_list_nn("DocumentUpdate"),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::List(vec![]))) }),
        ));
    schema_builder = schema_builder.register(update).register(update_page);
    query_type = query_type.field(
        Field::new(
            "_documentUpdates",
            TypeRef::named_nn("DocumentUpdatePage"),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::Null)) }),
        )
        .argument(InputValue::new("collection", TypeRef::named_nn("String")))
        .argument(InputValue::new("after", TypeRef::named("String")))
        .argument(InputValue::new("limit", TypeRef::named("Int"))),
    );

    let json_conflict = Object::new("JsonConflict")
        .field(Field::new(
            "path",
            TypeRef::named_nn_list_nn("String"),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::List(vec![]))) }),
        ))
        .field(Field::new("cid", TypeRef::named("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("height", TypeRef::named_nn("Int"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("value", TypeRef::named("JSON"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }))
        .field(Field::new("deleted", TypeRef::named_nn("Boolean"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }));
    schema_builder = schema_builder.register(json_conflict);
    query_type = query_type.field(
        Field::new(
            "_jsonConflicts",
            TypeRef::named_nn_list_nn("JsonConflict"),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::List(vec![]))) }),
        )
        .description("Lists, for each path of a document's Json field, the concurrent edits its winner beat.")
        .argument(InputValue::new("collection", TypeRef::named_nn("String")))
        .argument(InputValue::new("docID", TypeRef::named_nn("ID")))
        .argument(InputValue::new("field", TypeRef::named_nn("String"))),
    );

    // Register Commit type and supporting types
    schema_builder = schema_builder
        .register(build_commit_type())
        .register(build_signature_type())
        .register(build_commits_filter_arg())
        .register(build_commits_field_name_filter_arg())
        .register(build_commits_height_filter_arg())
        .register(build_commits_order_arg())
        .register(build_commit_fields_enum())
        .register(build_commit_count_field_arg())
        .register(build_commit_numeric_field_arg());

    // Add _commits query field (unconditionally, like Go)
    query_type = query_type.field(
        commits_args().into_iter().fold(
            Field::new("_commits", TypeRef::named_list("Commit"), |_| {
                FieldFuture::new(async { Ok(Some(GqlValue::List(vec![]))) })
            })
            .description(d::COMMITS_QUERY),
            Field::argument,
        ),
    );
    subscription_type = subscription_type.field(
        subscription_field("_commits", TypeRef::named_list("Commit"), commits_args())
            .description(d::COMMITS_QUERY),
    );
    schema_builder = schema_builder.register(subscription_type);

    // Register standard scalars and filter types
    schema_builder = schema_builder
        .register(Scalar::new("DateTime").description(d::DATETIME_SCALAR))
        .register(Scalar::new("Blob").description(d::BLOB_SCALAR))
        .register(Scalar::new("JSON").description(d::JSON_SCALAR))
        .register(Scalar::new("Float32").description(d::FLOAT32_SCALAR))
        .register(Scalar::new("Float64").description(d::FLOAT64_SCALAR))
        .register(build_explain_enum())
        .register(build_ordering_enum())
        .register(build_id_operator_block())
        .register(build_string_operator_block())
        .register(build_int_operator_block())
        .register(build_float_operator_block())
        .register(build_float32_operator_block())
        .register(build_float64_operator_block())
        .register(build_bool_operator_block())
        .register(build_datetime_operator_block())
        // List operator blocks for inline array filters
        .register(build_not_null_int_filter_arg())
        .register(build_not_null_float64_filter_arg())
        .register(build_not_null_float32_filter_arg())
        .register(build_not_null_bool_filter_arg())
        .register(build_not_null_string_filter_arg())
        .register(build_int_filter_arg())
        .register(build_float64_filter_arg())
        .register(build_float32_filter_arg())
        .register(build_bool_filter_arg())
        .register(build_string_filter_arg())
        // List operator blocks
        .register(build_int_list_operator_block())
        .register(build_not_null_int_list_operator_block())
        .register(build_float64_list_operator_block())
        .register(build_not_null_float64_list_operator_block())
        .register(build_float32_list_operator_block())
        .register(build_not_null_float32_list_operator_block())
        .register(build_bool_list_operator_block())
        .register(build_not_null_bool_list_operator_block())
        .register(build_string_list_operator_block())
        .register(build_not_null_string_list_operator_block());

    // Add top-level aggregate fields to Query
    if !collections.is_empty() {
        // _count: takes one arg per non-embedded collection
        let mut count_field = Field::new("COUNT", TypeRef::named_nn("Int"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        })
        .description(d::COUNT);
        for collection in collections {
            if collection.is_embedded_only {
                continue;
            }
            count_field = count_field.argument(InputValue::new(
                &collection.name,
                TypeRef::named(format!("{}__CountSelector", collection.name)),
            ));
        }
        query_type = query_type.field(count_field);

        // _sum, _avg: takes one arg per non-embedded collection
        for (agg_name, description) in &[
            ("SUM", d::SUM),
            ("AVG", d::AVG),
            ("MIN", d::MIN),
            ("MAX", d::MAX),
        ] {
            let output = match *agg_name {
                "SUM" | "AVG" => TypeRef::named_nn("Float"),
                _ => TypeRef::named("Float"),
            };
            let mut agg_field = Field::new(*agg_name, output, |_| {
                FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
            })
            .description(*description);
            for collection in collections {
                if collection.is_embedded_only {
                    continue;
                }
                agg_field = agg_field.argument(InputValue::new(
                    &collection.name,
                    TypeRef::named(format!("{}__NumericSelector", collection.name)),
                ));
            }
            query_type = query_type.field(agg_field);
        }
    }

    // If no collections, add a placeholder field to Query (required by GraphQL spec)
    if collections.is_empty() {
        query_type = query_type.field(Field::new("_placeholder", TypeRef::named("String"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        }));
    }

    // Add a hidden field referencing ExplainType so it appears in introspection.
    // async-graphql only includes registered types that are reachable from the type graph.
    // ExplainType is used as a directive argument in Go but we need it in __schema.types.
    query_type = query_type.field(
        Field::new("_explainType", TypeRef::named("ExplainType"), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::Null)) })
        })
        .argument(InputValue::new("type", TypeRef::named("ExplainType"))),
    );

    schema_builder = schema_builder.register(query_type);

    // Add mutation type if we have collections
    if !collections.is_empty() {
        let mutation_type = build_mutation_type(collections);
        schema_builder = schema_builder.register(mutation_type);
    }

    schema_builder.finish()
}

/// Execute an introspection query, building the schema for it first.
#[cfg(test)]
pub(crate) async fn execute_introspection(
    collections: &[Arc<CollectionVersion>],
    query: &str,
) -> Result<JsonValue> {
    let schema = build_introspection_schema(collections)
        .map_err(|e| QueryError::introspection(format!("failed to build schema: {}", e)))?;
    execute_against(&schema, query).await
}

/// Execute an introspection query against an already-built schema.
pub(crate) async fn execute_against(schema: &Schema, query: &str) -> Result<JsonValue> {
    let request = async_graphql::Request::new(query);
    let response = schema.execute(request).await;

    // Check for errors
    if !response.errors.is_empty() {
        let error_messages: Vec<String> =
            response.errors.iter().map(|e| e.message.clone()).collect();
        return Err(QueryError::introspection(error_messages.join(", ")));
    }

    // Convert response to JSON
    let json = serde_json::to_value(&response.data)
        .map_err(|e| QueryError::introspection(format!("failed to serialize response: {}", e)))?;

    Ok(json)
}

/// The arguments of a collection select, on the query and subscription roots.
fn select_args(collection: &str) -> Vec<InputValue> {
    vec![
        InputValue::new("cid", TypeRef::named_nn_list("ID")).description(d::CID_ARG),
        InputValue::new("docID", TypeRef::named_nn_list("ID")).description(d::DOC_IDS_ARG),
        InputValue::new("filter", TypeRef::named(format!("{collection}FilterArg")))
            .description(d::SELECT_FILTER_ARG),
        InputValue::new(
            "groupBy",
            TypeRef::named_nn_list(format!("{collection}Field")),
        )
        .description(d::GROUP_BY_ARG),
        InputValue::new("limit", TypeRef::named("Int")).description(d::LIMIT_ARG),
        InputValue::new("offset", TypeRef::named("Int")).description(d::OFFSET_ARG),
        InputValue::new(
            "order",
            TypeRef::named_list(format!("{collection}OrderArg")),
        )
        .description(d::ORDER_ARG),
        InputValue::new("showDeleted", TypeRef::named("Boolean")).description(d::SHOW_DELETED_ARG),
    ]
}

/// The arguments of `_commits`, on the query and subscription roots.
fn commits_args() -> Vec<InputValue> {
    vec![
        InputValue::new("cid", TypeRef::named_nn_list("ID")).description(d::COMMIT_CID_ARG),
        InputValue::new("depth", TypeRef::named("Int")).description(d::COMMITS_DEPTH_ARG),
        InputValue::new("docID", TypeRef::named_nn_list("ID")).description(d::COMMIT_DOC_ID_ARG),
        InputValue::new("filter", TypeRef::named("CommitsFilterArg"))
            .description(d::COMMIT_FILTER_ARG),
        InputValue::new("groupBy", TypeRef::named_nn_list("commitFields"))
            .description(d::GROUP_BY_ARG),
        InputValue::new("limit", TypeRef::named("Int")).description(d::LIMIT_ARG),
        InputValue::new("offset", TypeRef::named("Int")).description(d::OFFSET_ARG),
        InputValue::new("order", TypeRef::named_list("commitsOrderArg")).description(d::ORDER_ARG),
    ]
}

/// A subscription root field that streams nothing: introspection only reads it.
fn subscription_field(name: &str, ty: TypeRef, args: Vec<InputValue>) -> SubscriptionField {
    args.into_iter().fold(
        SubscriptionField::new(name, ty, |_| {
            SubscriptionFieldFuture::new(async {
                Ok(futures::stream::empty::<async_graphql::Result<GqlValue>>())
            })
        }),
        SubscriptionField::argument,
    )
}

#[cfg(test)]
mod tests {
    use regolith::sync::Barrier;

    use super::*;
    use schema::{FieldDescription, FieldKind, ScalarArrayKind};

    fn assert_non_null_aggregate(object: &JsonValue, field_name: &str, scalar_name: &str) {
        let field = object["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["name"] == field_name)
            .unwrap();
        assert_eq!(field["type"]["kind"], "NON_NULL");
        assert_eq!(field["type"]["ofType"]["kind"], "SCALAR");
        assert_eq!(field["type"]["ofType"]["name"], scalar_name);
    }

    /// A bare `{ __typename }` (e.g. a GraphQL health probe) must resolve to
    /// the root query type name via the introspection engine, not fail as an
    /// unknown collection. Regression test for #1124.
    #[test]
    fn root_typename_returns_query() {
        workers::block_on(async {
            let result = execute_introspection(&[], "{ __typename }")
                .await
                .expect("introspection execution should succeed");
            assert_eq!(result, serde_json::json!({ "__typename": "Query" }));
        })
        .unwrap()
    }

    #[test]
    fn aggregate_nullability_matches_go() {
        workers::block_on(async {
            let collection = CollectionVersion::new("Users", "v1", "users", vec![]);
            let result = execute_introspection(
                &[Arc::new(collection)],
                r#"query {
                collection: __type(name: "Users") {
                    fields { name type { kind name ofType { kind name } } }
                }
                query: __type(name: "Query") {
                    fields { name type { kind name ofType { kind name } } }
                }
                commit: __type(name: "Commit") {
                    fields { name type { kind name ofType { kind name } } }
                }
            }"#,
            )
            .await
            .expect("introspection execution should succeed");

            for type_name in ["collection", "query", "commit"] {
                assert_non_null_aggregate(&result[type_name], "AVG", "Float");
                assert_non_null_aggregate(&result[type_name], "COUNT", "Int");
                assert_non_null_aggregate(&result[type_name], "SUM", "Float");
            }
        })
        .unwrap()
    }

    #[test]
    fn collection_count_and_array_types_match_go() {
        workers::block_on(async {
            let collection = CollectionVersion::new(
                "Users",
                "v1",
                "users",
                vec![FieldDescription::new(
                    "1",
                    "tags",
                    FieldKind::ScalarArray(ScalarArrayKind::StringArray),
                    schema::CType::LwwRegister,
                )],
            );
            let result = execute_introspection(
                &[Arc::new(collection)],
                r#"query {
                selector: __type(name: "Users__CountSelector") {
                    inputFields {
                        name
                        type { kind name ofType { kind name ofType { kind name } } }
                    }
                }
                collection: __type(name: "Users") {
                    fields {
                        name
                        type { kind name ofType { kind name ofType { kind name } } }
                    }
                }
            }"#,
            )
            .await
            .expect("introspection execution should succeed");

            let group_by = result["selector"]["inputFields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|field| field["name"] == "groupBy")
                .unwrap();
            assert_eq!(group_by["type"]["kind"], "LIST");
            assert_eq!(group_by["type"]["ofType"]["kind"], "NON_NULL");
            assert_eq!(group_by["type"]["ofType"]["ofType"]["name"], "UsersField");

            let tags = result["collection"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|field| field["name"] == "tags")
                .unwrap();
            assert_eq!(tags["type"]["kind"], "LIST");
            assert_eq!(tags["type"]["ofType"]["kind"], "NON_NULL");
            assert_eq!(tags["type"]["ofType"]["ofType"]["name"], "String");
        })
        .unwrap()
    }

    #[test]
    fn mutation_input_fields_are_nullable() {
        workers::block_on(async {
            let collection = CollectionVersion::new(
                "Users",
                "v1",
                "users",
                vec![
                    FieldDescription::new(
                        "1",
                        "name",
                        FieldKind::Scalar(schema::ScalarKind::String),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "2",
                        "tags",
                        FieldKind::ScalarArray(ScalarArrayKind::StringArray),
                        schema::CType::LwwRegister,
                    ),
                ],
            );
            let result = execute_introspection(
                &[Arc::new(collection)],
                r#"{
                __type(name: "UsersMutationInputArg") {
                    inputFields {
                        name
                        type { kind name ofType { kind name } }
                    }
                }
            }"#,
            )
            .await
            .expect("introspection execution should succeed");

            let fields = result["__type"]["inputFields"].as_array().unwrap();
            let name = fields.iter().find(|field| field["name"] == "name").unwrap();
            assert_eq!(name["type"]["kind"], "SCALAR");
            assert_eq!(name["type"]["name"], "String");

            let tags = fields.iter().find(|field| field["name"] == "tags").unwrap();
            assert_eq!(tags["type"]["kind"], "LIST");
            assert_eq!(tags["type"]["ofType"]["kind"], "SCALAR");
            assert_eq!(tags["type"]["ofType"]["name"], "String");
        })
        .unwrap()
    }

    #[test]
    fn mutation_input_is_safe_for_concurrent_first_use() {
        workers::block_on(async {
            const CALLERS: usize = 16;
            const QUERY: &str = r#"{
            __type(name: "UsersMutationInputArg") {
                inputFields {
                    name
                    type { kind name ofType { kind name } }
                }
            }
        }"#;

            let collection = CollectionVersion::new(
                "Users",
                "v1",
                "users",
                vec![
                    FieldDescription::new(
                        "1",
                        "name",
                        FieldKind::Scalar(schema::ScalarKind::NillableString),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "2",
                        "tags",
                        FieldKind::ScalarArray(ScalarArrayKind::StringArray),
                        schema::CType::LwwRegister,
                    ),
                ],
            );
            let schema = build_introspection_schema(&[Arc::new(collection)]).unwrap();
            let barrier = Arc::new(Barrier::new(CALLERS));

            let handles: Vec<_> = (0..CALLERS)
                .map(|_| {
                    let (schema, barrier) = (schema.clone(), barrier.clone());
                    workers::spawn(None, "test", move || async move {
                        barrier.wait().await;
                        let response = schema.execute(async_graphql::Request::new(QUERY)).await;
                        assert!(response.errors.is_empty(), "{:?}", response.errors);
                        serde_json::to_value(response.data).unwrap()
                    })
                    .unwrap()
                })
                .collect();
            let mut results = Vec::<JsonValue>::with_capacity(CALLERS);
            for handle in handles {
                results.push(handle.await.unwrap());
            }

            for result in &results[1..] {
                assert_eq!(result, &results[0]);
            }
            let fields = results[0]["__type"]["inputFields"].as_array().unwrap();
            assert_eq!(fields.len(), 2);
            assert!(fields.iter().any(|field| field["name"] == "name"));
            assert!(fields.iter().any(|field| field["name"] == "tags"));
        })
        .unwrap()
    }
}
