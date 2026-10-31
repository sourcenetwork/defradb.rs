#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod index {
    mod array;
    mod array_composite;
    mod array_unique;
    mod array_unique_composite;
    mod async_backfill;
    mod async_backfill_migration;
    mod backfill_large;
    mod backfill_txn;
    mod backfill_unique;
    mod crdt;
    mod delete;
    mod get;
    mod json;
    mod json_array;
    mod json_composite;
    mod json_unique;
    mod json_unique_array;
    mod list_actions;
    mod list_collection_name;
    mod new;
    mod new_composite;
    mod new_get;
    mod new_unique;
    mod new_unique_composite;
    mod new_unique_composite_one_to_one;
    mod order_query;
    mod patch;
    mod patch_relation;
    mod query_performance;
    mod query_with_composite_index_field_order;
    mod query_with_composite_index_only_filter;
    mod query_with_composite_inxed_on_relation;
    mod query_with_compound_filter_relation;
    mod query_with_group;
    mod query_with_index_combined_filter;
    mod query_with_index_filter_compound;
    mod query_with_index_on_datetime;
    mod query_with_index_only_field_order;
    mod query_with_index_only_filter;
    mod query_with_relation_filter;
    mod query_with_relation_sub_filter;
    mod query_with_relation_sub_order;
    mod query_with_relation_sub_order_complex;
    mod query_with_unique_composite_index_filter;
    mod query_with_unique_index_on_relation_filter;
    mod query_with_unique_index_only_filter;
    mod range_filter;
    mod status;
    mod update_unique;
    mod update_unique_composite;
    mod vector_fallback_warning;
}
