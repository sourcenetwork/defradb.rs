mod connection_manager;
mod document;
mod feature_binaries;
mod filtered_replication;
mod idempotent_replay;
mod json_crdt;
mod long_history;
mod manage_relay;
mod management;
mod peer_details;
mod quarantine;
mod receiver_pull;
mod replication;
mod replication_advanced;
mod resilience;
mod sync;
mod text_crdt;
mod transports;
mod trust_boundary;
mod write_contention;

#[nuclei::test]
async fn rust_replicator_retry_intervals() {
    edge::compat(async {
        crate::replicator_retry_common::retry_intervals_test(
            integration_test::TestCluster::builder().with_p2p(),
        )
        .await;
    })
    .await;
}
