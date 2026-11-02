#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod explain {
    mod debug {
        mod add;
        mod basic;
        mod dagscan;
        mod delete;
        mod delete_with_error;
        mod group;
        mod group_with_average;
        mod group_with_doc_id;
        mod group_with_doc_id_child;
        mod group_with_filter;
        mod group_with_filter_child;
        mod group_with_limit;
        mod group_with_limit_child;
        mod group_with_order;
        mod group_with_order_child;
        mod top_with_average;
        mod top_with_count;
        mod top_with_max;
        mod top_with_min;
        mod top_with_sum;
        mod type_join;
        mod type_join_many;
        mod type_join_one;
        mod type_join_with_filter;
        mod type_join_with_filter_doc_id;
        mod update;
        mod upsert;
        mod with_average;
        mod with_average_join;
        mod with_count;
        mod with_count_join;
        mod with_filter;
        mod with_filter_doc_id;
        mod with_index;
        mod with_limit;
        mod with_limit_count;
        mod with_limit_join;
        mod with_max;
        mod with_max_join;
        mod with_min;
        mod with_min_join;
        mod with_order;
        mod with_order_join;
        mod with_orphan;
        mod with_similarity;
        mod with_sum;
        mod with_sum_join;
        mod with_view;
        mod with_view_transform;
    }
    mod default {
        mod add;
        mod basic;
        mod dagscan;
        mod delete;
        mod delete_with_error;
        mod group;
        mod group_with_average;
        mod group_with_doc_id;
        mod group_with_doc_id_child;
        mod group_with_filter;
        mod group_with_filter_child;
        mod group_with_limit;
        mod group_with_limit_child;
        mod group_with_order;
        mod group_with_order_child;
        mod invalid_type_arg;
        mod top_with_average;
        mod top_with_count;
        mod top_with_max;
        mod top_with_min;
        mod top_with_sum;
        mod type_join;
        mod type_join_many;
        mod type_join_one;
        mod type_join_with_filter;
        mod type_join_with_filter_doc_id;
        mod update;
        mod upsert;
        mod with_average;
        mod with_average_join;
        mod with_count;
        mod with_count_join;
        mod with_filter;
        mod with_filter_doc_id;
        mod with_index;
        mod with_limit;
        mod with_limit_count;
        mod with_limit_join;
        mod with_max;
        mod with_max_join;
        mod with_min;
        mod with_min_join;
        mod with_order;
        mod with_order_join;
        mod with_sum;
        mod with_sum_join;
    }
    mod execute {
        mod add;
        mod dagscan;
        mod delete;
        mod group;
        mod query_deleted_docs;
        mod scan;
        mod top_level;
        mod type_join;
        mod update;
        mod upsert;
        mod with_average;
        mod with_count;
        mod with_index;
        mod with_limit;
        mod with_max;
        mod with_min;
        mod with_order;
        mod with_orphan;
        mod with_similarity;
        mod with_sum;
    }
    mod simple {
        mod basic;
        mod with_index;
    }
}
