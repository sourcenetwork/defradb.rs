//! Starts DefraDB nodes as processes and drives them the way a test does.
//!
//! - [`TestCluster`] starts Rust and Go nodes on ports of their own, waits
//!   for each to be ready, and stops, restarts, pauses and throttles them.
//! - [`DefraClient`] runs the `client` commands of a node's own binary.
//! - [`sse`] collects a node's server-sent events, [`observe`] follows its
//!   log, and [`fixtures`], [`identity`] and [`p2p_helpers`] hold what the
//!   tests share.
//!
//! Every `async` function runs on the caller's tokio runtime.

use std::path::PathBuf;

pub mod client;
pub mod cluster;
pub mod divergences;
pub mod error;
pub mod fixtures;
pub mod identity;
mod infra;
pub mod node;
pub mod observe;
pub mod p2p_helpers;
pub mod poll;
pub mod ports;
pub mod sse;

pub use client::DefraClient;
pub use cluster::{StoppedNode, TestCluster, TestClusterBuilder, Throttle};
pub use divergences::NodeKind;
pub use error::{Error, Result};
pub use fixtures::{
    documents_schema_with_policy, interaction_schema_with_policy, multi_resource_policy,
    peak_schema_with_policy, secret_schema_with_policy, tweet_schema_with_policy, typed_schema,
    users_schema_with_policy, workout_schema_with_policy, HIKING_ACP_POLICY, MULTI_ROLE_ACP_POLICY,
    PRODUCT_SCHEMA, SECRET_ACP_POLICY, STANDARD_FIELDS, USER_ACP_POLICY, XARCHIVE_ACP_POLICY,
};
pub use identity::{
    generate_ed25519_identity, generate_identity, generate_secp256r1_identity, TestIdentity,
};
pub use node::{
    start_node, BinarySource, KeyringBackend, NodeConfig, OrbisSignerConfig, RunningNode,
    VeraNodeConfig, VERA_ACP_DOCUMENT_TYPE,
};
pub use p2p_helpers::{
    extract_doc_id, extract_p2p_addr, extract_p2p_addr_with_identity, extract_peer_id,
    setup_three_node_chain, setup_two_node_iroh, setup_two_node_replicated, wait_for_doc_count,
    wait_for_field_values, P2P_POLL_INTERVAL, P2P_TIMEOUT,
};
pub use poll::poll_until;
pub use sse::{
    open_events_sse, open_merge_events_sse, open_peer_events_sse, wait_for_merge_events,
    wait_for_peer_events, SseEvents, SseWaitError,
};

/// The workspace whose `defradb` the harness builds and runs, and under whose
/// `target/e2e` it keeps run directories.
///
/// `DEFRA_WORKSPACE_ROOT` when it is set, else the workspace this crate is
/// checked out in. A consumer outside that workspace sets the variable.
pub fn workspace_root() -> PathBuf {
    if let Ok(root) = std::env::var("DEFRA_WORKSPACE_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("failed to canonicalize workspace root")
}
