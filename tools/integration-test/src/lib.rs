use std::path::PathBuf;

pub mod actions;
mod cli_command;
mod client_type;
pub mod clients;
pub mod commit_gate;
mod cross_runtime;
pub mod export;
mod feature_binaries;
mod go_node;
pub mod index_gate;
pub mod introspection_sdl;
pub mod multiplier;
pub mod nodes;
mod purge;
mod reserved_port;
pub mod scenario;
mod stub_embedder;
pub mod test_case;
pub mod transport_choice;
pub mod wasm_lens;

pub use cli_command::cli_command;
pub use client_type::ClientType;
pub use scenario::m;

/// Locate the Go compatibility binary provisioned by CI, or an explicit override.
pub fn go_compat_binary() -> PathBuf {
    if let Some(path) =
        std::env::var_os("DEFRA_GO_BINARY").or_else(|| std::env::var_os("DEFRA_GO_BIN"))
    {
        return PathBuf::from(path);
    }
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    cached_go_compat_binary(
        &home,
        defra_version::GO_COMPAT_TAG,
        defra_version::GO_COMPAT_COMMIT,
    )
}

fn cached_go_compat_binary(home: &std::path::Path, tag: &str, commit: &str) -> PathBuf {
    let pin = if tag.is_empty() { commit } else { tag };
    home.join(".cache/defra-harness").join(pin).join("defradb")
}

#[cfg(test)]
mod go_compat_tests {
    use super::cached_go_compat_binary;
    use std::path::Path;

    #[test]
    fn cached_binary_uses_tag_when_available() {
        let home = Path::new("/test-home");
        assert_eq!(
            cached_go_compat_binary(home, "v1.2.0", "abc"),
            home.join(".cache/defra-harness/v1.2.0/defradb")
        );
        assert_eq!(
            cached_go_compat_binary(home, "", "abc"),
            home.join(".cache/defra-harness/abc/defradb")
        );
    }
}

pub use cross_runtime::{assert_query_equivalent, query_both};
pub use feature_binaries::{build_cli_variant, listing_tests, vera_cli_binary, TestBuild};
pub use purge::purge_node;

// Re-export modules from defra-harness
pub use defra_harness::client;
pub use defra_harness::cluster;
pub use defra_harness::divergences;
pub use defra_harness::fixtures;
pub use defra_harness::identity;
pub use defra_harness::node;
pub use defra_harness::observe;
pub use defra_harness::poll;
pub use defra_harness::ports;

// Convenience re-exports
pub use defra_harness::fixtures::*;
pub use defra_harness::identity::*;
pub use defra_harness::poll::poll_until;
pub use defra_harness::BinarySource;
pub use defra_harness::DefraClient;
pub use defra_harness::NodeKind;
pub use defra_harness::TestCluster;
pub use defra_harness::TestClusterBuilder;

// Re-export modules moved to backbone
pub use defra_harness::p2p_helpers;
pub use defra_harness::sse;

pub use defra_harness::p2p_helpers::{
    extract_doc_id, extract_p2p_addr, extract_p2p_addr_with_identity, extract_peer_id,
    setup_three_node_chain, setup_two_node_iroh, setup_two_node_replicated, wait_for_doc_count,
    wait_for_field_values, P2P_POLL_INTERVAL, P2P_TIMEOUT,
};
pub use defra_harness::sse::{
    open_events_sse, open_merge_events_sse, open_peer_events_sse, wait_for_merge_events,
    wait_for_peer_events,
};

#[ctor::ctor(unsafe)]
fn init_workspace_root() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("failed to canonicalize workspace root");
    std::env::set_var("DEFRA_WORKSPACE_ROOT", &root);
}

/// Return the absolute path to the defradb.rs workspace root.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("failed to canonicalize workspace root")
}

/// Return the configured Rust CLI binary, falling back to the CLI built on
/// the tests' own profile.
pub fn rust_binary() -> PathBuf {
    std::env::var_os("DEFRA_RUST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            TestBuild::current(&workspace_root())
                .out_dir
                .join("defradb")
        })
}

/// Generate `rust_<name>` and `go_<name>` test wrappers for a single-node test.
///
/// Usage: `for_each_runtime!(name, inner_fn);` (bare builder)
///        `for_each_runtime!(name, inner_fn, .with_acp_local());` (with modifiers)
#[macro_export]
macro_rules! for_each_runtime {
    ($name:ident, $inner:ident) => {
        ::paste::paste! {
            #[nuclei::test]
            async fn [<rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(1).build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            async fn [<go_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().go_nodes(1).build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }
        }
    };
    ($name:ident, $inner:ident, $($modifier:tt)+) => {
        ::paste::paste! {
            #[nuclei::test]
            async fn [<rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(1) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            async fn [<go_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().go_nodes(1) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }
        }
    };
}

/// Generate `rust_rust_<name>`, `go_go_<name>`, and `go_rust_<name>` test wrappers
/// for a multi-node P2P test.
///
/// Usage: `for_each_p2p_topology!(name, inner_fn, .with_p2p());`
#[macro_export]
macro_rules! for_each_p2p_topology {
    ($name:ident, $inner:ident, $($modifier:tt)+) => {
        ::paste::paste! {
            #[nuclei::test]
            async fn [<rust_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(2) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            async fn [<go_go_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().go_nodes(2) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            async fn [<go_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(1).go_nodes(1) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }
        }
    };
}

/// Generate `rust_rust_<name>`, `go_go_<name>`, and `go_rust_<name>` test wrappers
/// for a 3-node P2P test (node0=target, node1=flooder, node2=legitimate).
///
/// Usage: `for_each_p2p_topology_3!(name, inner_fn, .with_p2p());`
#[macro_export]
macro_rules! for_each_p2p_topology_3 {
    ($name:ident, $inner:ident, $($modifier:tt)+) => {
        ::paste::paste! {
            #[nuclei::test]
            async fn [<rust_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(3) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            async fn [<go_go_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().go_nodes(3) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            async fn [<go_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(2).go_nodes(1) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }
        }
    };
}

/// Like `for_each_p2p_topology!` but marks all generated tests as `#[ignore]`.
/// Use for stress tests that are too resource-intensive for normal CI runs.
///
/// Run with: `cargo test -p integration-test --test <binary> -- --ignored`
#[macro_export]
macro_rules! for_each_p2p_topology_ignored {
    ($name:ident, $inner:ident, $($modifier:tt)+) => {
        ::paste::paste! {
            #[nuclei::test]
            #[ignore]
            async fn [<rust_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(2) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            #[ignore]
            async fn [<go_go_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().go_nodes(2) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            #[ignore]
            async fn [<go_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(1).go_nodes(1) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }
        }
    };
}

/// Like `for_each_p2p_topology_3!` but marks all generated tests as `#[ignore]`.
/// Use for stress tests that are too resource-intensive for normal CI runs.
///
/// Run with: `cargo test -p integration-test --test <binary> -- --ignored`
#[macro_export]
macro_rules! for_each_p2p_topology_3_ignored {
    ($name:ident, $inner:ident, $($modifier:tt)+) => {
        ::paste::paste! {
            #[nuclei::test]
            #[ignore]
            async fn [<rust_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(3) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            #[ignore]
            async fn [<go_go_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().go_nodes(3) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }

            #[nuclei::test]
            #[ignore]
            async fn [<go_rust_ $name>]() {
                ::edge::compat(async {
                    let _root = $crate::workspace_root();
                    let cluster = $crate::TestCluster::builder().rust_nodes(2).go_nodes(1) $($modifier)+ .build().await.unwrap();
                    $inner(cluster).await;
                })
                .await;
            }
        }
    };
}

/// The path of the enclosing test function, for naming an exported scenario.
#[macro_export]
macro_rules! scenario_id {
    () => {{
        fn here() {}
        let name = ::std::any::type_name_of_val(&here);
        &name[..name.len() - "::here".len()]
    }};
}
