#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod searchable_encryption {
    mod add;
    mod delete;
    mod list;
    mod patch;
    mod peer_acp;
    mod peer_add;
    mod peer_delete;
    mod peer_query;
    mod query;
    mod replicator;
    mod truncate;
}
