mod acp;
mod connection;
mod peer;
mod replication;
mod schema;
mod support;
mod sync;

#[nuclei::test]
async fn iroh_replicator_retry_intervals() {
    edge::compat(async move {
        crate::replicator_retry_common::retry_intervals_test(
            integration_test::TestCluster::builder().with_iroh_transport(),
        )
        .await;
    })
    .await;
}
