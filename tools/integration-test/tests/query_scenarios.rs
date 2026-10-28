#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

#[path = "query_scenarios"]
mod query {
    mod commits {
        mod branchables {
            mod add;
            mod cid;
            mod cid_doc_id;
            mod delete;
            mod field_id;
            mod if_;
            mod peer;
            mod peer_index;
            mod peer_update;
            mod simple;
            mod update;
        }
        mod simple;
        mod with_cid;
        mod with_cid_shared_field;
        mod with_delete;
        mod with_depth;
        mod with_doc_id;
        mod with_doc_id_cid;
        mod with_doc_id_count;
        mod with_doc_id_field;
        mod with_doc_id_group_order;
        mod with_doc_id_limit;
        mod with_doc_id_limit_offset;
        mod with_doc_id_order;
        mod with_doc_id_order_limit_offset;
        mod with_doc_id_prop;
        mod with_doc_id_shared_field;
        mod with_doc_id_typename;
        mod with_filter_compound;
        mod with_filter_in;
        mod with_nested_links;
        mod with_null_input;
    }
    mod inline_array {
        mod simple;
        mod with_average;
        mod with_average_filter;
        mod with_average_order;
        mod with_average_sum;
        mod with_count;
        mod with_count_filter;
        mod with_count_limit;
        mod with_count_limit_offset;
        mod with_count_order;
        mod with_datetime;
        mod with_filter_all;
        mod with_filter_any;
        mod with_filter_eq;
        mod with_filter_none;
        mod with_group;
        mod with_max;
        mod with_max_doc_id;
        mod with_max_filter;
        mod with_max_limit_offset;
        mod with_max_limit_offset_order;
        mod with_max_order;
        mod with_min;
        mod with_min_doc_id;
        mod with_min_filter;
        mod with_min_limit_offset;
        mod with_min_limit_offset_order;
        mod with_min_order;
        mod with_nonnillable_scalars;
        mod with_sum;
        mod with_sum_filter;
        mod with_sum_limit;
        mod with_sum_limit_offset;
        mod with_sum_limit_offset_order;
        mod with_sum_order;
    }
    mod json {
        mod with_aggregate;
        mod with_all;
        mod with_any;
        mod with_eq;
        mod with_geq;
        mod with_gt;
        mod with_in;
        mod with_leq;
        mod with_like;
        mod with_lt;
        mod with_neq;
        mod with_nin;
        mod with_nlike;
        mod with_none;
    }
    mod many_to_many {
        mod simple;
        mod with_nested_query;
    }
    mod one_to_many {
        mod one_sided;
        mod simple;
        mod with_average;
        mod with_cid_doc_id;
        mod with_count;
        mod with_count_filter;
        mod with_count_groupby;
        mod with_count_limit;
        mod with_count_limit_offset;
        mod with_count_order;
        mod with_doc_id;
        mod with_doc_ids;
        mod with_filter;
        mod with_filter_related_id;
        mod with_group;
        mod with_group_filter;
        mod with_group_related_id;
        mod with_group_related_id_alias;
        mod with_id_field;
        mod with_limit;
        mod with_max;
        mod with_min;
        mod with_order_filter;
        mod with_order_filter_limit;
        mod with_related_id;
        mod with_same_field_name;
        mod with_sum;
        mod with_sum_filter_order;
        mod with_sum_limit;
        mod with_sum_limit_offset;
        mod with_sum_limit_offset_order;
        mod with_sum_order;
        mod with_typename;
    }
    mod one_to_many_multiple {
        mod with_average;
        mod with_average_filter;
        mod with_count;
        mod with_count_filter;
        mod with_multiple_filter;
        mod with_sum;
        mod with_sum_filter;
    }
    mod one_to_many_to_many {
        mod joins;
    }
    mod one_to_many_to_one {
        mod joins;
        mod simple;
        mod with_filter;
        mod with_order;
        mod with_order_limit;
        mod with_sum;
        mod with_sum_order_limit;
    }
    mod one_to_one {
        mod simple;
        mod with_clashing_id_field;
        mod with_count_filter;
        mod with_filter;
        mod with_filter_order;
        mod with_fragments;
        mod with_group_related_id;
        mod with_group_related_id_alias;
        mod with_order;
        mod with_version;
    }
    mod one_to_one_multiple {
        mod simple;
    }
    mod one_to_one_to_many {
        mod simple;
    }
    mod one_to_one_to_one {
        mod simple;
        mod with_order;
    }
    mod one_to_two_many {
        mod simple;
        mod with_order;
    }
    mod simple {
        mod simple;
        mod with_aggregate_nonnillable;
        mod with_average;
        mod with_average_filter;
        mod with_average_order;
        mod with_cid;
        mod with_cid_branchable;
        mod with_cid_doc_id;
        mod with_cid_doc_id_branchable;
        mod with_cid_shared_field;
        mod with_count;
        mod with_count_filter;
        mod with_count_groupby;
        mod with_deleted_field;
        mod with_doc_id;
        mod with_doc_id_filter;
        mod with_doc_ids;
        mod with_filter {
            mod with_alias;
            mod with_and;
            mod with_eq_blob;
            mod with_eq_datetime;
            mod with_eq_float;
            mod with_eq_int;
            mod with_eq_string;
            mod with_geq_datetime;
            mod with_geq_float;
            mod with_geq_int;
            mod with_gt_datetime;
            mod with_gt_float;
            mod with_gt_int;
            mod with_in;
            mod with_in_blob;
            mod with_leq_datetime;
            mod with_leq_float;
            mod with_leq_int;
            mod with_like_blob;
            mod with_like_string;
            mod with_lt_datetime;
            mod with_lt_float;
            mod with_lt_int;
            mod with_neq_bool;
            mod with_neq_datetime;
            mod with_neq_float;
            mod with_neq_int;
            mod with_neq_string;
            mod with_nin;
            mod with_nlike_string;
            mod with_not;
            mod with_or;
        }
        mod with_fragments;
        mod with_group;
        mod with_group_aggregate_alias_filter;
        mod with_group_average;
        mod with_group_average_count;
        mod with_group_average_filter;
        mod with_group_average_limit;
        mod with_group_average_limit_offset;
        mod with_group_average_max;
        mod with_group_average_min;
        mod with_group_average_sum;
        mod with_group_count;
        mod with_group_count_filter;
        mod with_group_count_limit;
        mod with_group_count_limit_offset;
        mod with_group_count_max;
        mod with_group_count_min;
        mod with_group_count_sum;
        mod with_group_doc_id;
        mod with_group_doc_ids;
        mod with_group_filter;
        mod with_group_limit;
        mod with_group_limit_offset;
        mod with_group_max;
        mod with_group_max_filter;
        mod with_group_max_limit;
        mod with_group_max_limit_offset;
        mod with_group_min;
        mod with_group_min_filter;
        mod with_group_min_limit;
        mod with_group_min_limit_offset;
        mod with_group_order;
        mod with_group_sum;
        mod with_group_sum_filter;
        mod with_group_sum_limit;
        mod with_group_sum_limit_offset;
        mod with_group_typename;
        mod with_limit_offset;
        mod with_max;
        mod with_max_filter;
        mod with_max_order;
        mod with_min;
        mod with_min_filter;
        mod with_min_order;
        mod with_multiple_types;
        mod with_null_input;
        mod with_operation_alias;
        mod with_operation_name;
        mod with_order;
        mod with_order_filter;
        mod with_restart;
        mod with_similarity;
        mod with_similarity_vector_index;
        mod with_sum;
        mod with_sum_filter;
        mod with_sum_order;
        mod with_typename;
        mod with_variables;
        mod with_version;
        mod with_version_cid;
        mod with_version_cid_doc_ids;
        mod with_version_order;
    }
}
