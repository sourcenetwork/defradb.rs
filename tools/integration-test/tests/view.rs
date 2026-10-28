#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod view {
    mod one_to_many {
        mod simple;
        mod with_alias;
        mod with_count;
        mod with_introspection;
        mod with_transform;
    }
    mod one_to_one {
        mod identical_schema;
        mod simple;
        mod with_restart;
        mod with_transform;
    }
    mod simple {
        mod materialized;
        mod simple;
        mod transform_cid_presence;
        mod transform_restart;
        mod with_alias;
        mod with_contraints;
        mod with_default_value;
        mod with_embeddings;
        mod with_filter;
        mod with_introspection;
        mod with_transform;
        mod with_transform_aggregate;
        mod with_transform_cid;
    }
}
