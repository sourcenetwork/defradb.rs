#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod collection {
    mod add_relation_with_collection;
    mod refresh {
        mod list_actions;
    }
    mod truncate {
        mod add;
        mod branchable_add;
        mod dac;
        mod filter;
        mod index_filter;
        mod list_actions;
        mod parallel;
        mod simple;
        mod view;
    }
}
