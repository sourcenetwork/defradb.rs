#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

#[path = "encryption_scenarios"]
mod encryption {
    mod commit;
    mod commit_relation;
    mod field;
    mod field_commit;
    mod field_query;
    mod peer;
    mod peer_acp;
    mod peer_acp_branchable;
    mod peer_branchable_diverged_versions;
    mod peer_multipeer;
    mod peer_nac;
    mod peer_sec_index;
    mod peer_share;
    mod peer_subscription;
    mod query;
    mod query_relation;
}
