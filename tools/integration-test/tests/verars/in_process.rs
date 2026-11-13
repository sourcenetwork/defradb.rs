//! The provider-level scenarios against a Vera node in the test process.
//!
//! The scenarios are the ones the daemon tests run (`verified_records`,
//! `native_worker`); here the provider reaches the node through its
//! in-process method table, and nothing of what it reads crosses a socket.

use std::sync::Arc;

use acp::{DocumentACP as _, DocumentPermission};

use super::embedded::{self, Embedded};
use super::{native_worker, verified_records};

#[nuclei::test]
#[serial_test::serial]
async fn in_process_archived_owner_record_does_not_authorize_access() {
    edge::compat(async {
        let node = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider = node
            .in_process(embedded::worker(root.path()))
            .await
            .expect("provider");
        verified_records::archived_owner_record(Arc::new(provider)).await;
        node.stop().await;
    })
    .await;
}

#[nuclei::test]
#[serial_test::serial]
async fn in_process_permissions_are_proven_and_end_with_the_node() {
    edge::compat(async {
        let node = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider = node
            .in_process(embedded::worker(root.path()))
            .await
            .expect("provider");
        let after = verified_records::permissions(Arc::new(provider)).await;
        assert!(after
            .document_acp
            .check_doc_access(
                &after.identity,
                DocumentPermission::Read,
                &after.spec_policy,
                "file",
                "report"
            )
            .await
            .expect("decision while the node runs"));

        // Over the network a node that goes away leaves the provider with
        // stale headers. In process there is nothing left to ask: the
        // decision that was just given is not given again.
        node.stop().await;
        let stopped = std::time::Instant::now();
        loop {
            let repeated = after
                .document_acp
                .check_doc_access(
                    &after.identity,
                    DocumentPermission::Read,
                    &after.spec_policy,
                    "file",
                    "report",
                )
                .await;
            let fresh = after
                .document_acp
                .check_doc_access(
                    &after.identity,
                    DocumentPermission::Read,
                    &after.policy,
                    "file",
                    &format!("never-asked-{}", stopped.elapsed().as_millis()),
                )
                .await;
            eprintln!("PROBE {:>5} ms repeated={repeated:?} fresh={fresh:?}", stopped.elapsed().as_millis());
            if repeated.is_err() || stopped.elapsed() > std::time::Duration::from_secs(50) {
                break;
            }
            nuclei::time::sleep(std::time::Duration::from_secs(3)).await;
        }
        panic!("probe");
    })
    .await;
}

#[nuclei::test]
#[serial_test::serial]
async fn in_process_provider_recovers_pending_before_concurrent_requests() {
    edge::compat(async {
        let node = Embedded::start().await;
        // The rejected request is sent over the node's HTTP listener, behind
        // the provider's back; the provider itself stays in process.
        native_worker::provider_recovers_pending(&node.rpc_url(), |worker| node.in_process(worker))
            .await;
        node.stop().await;
    })
    .await;
}
