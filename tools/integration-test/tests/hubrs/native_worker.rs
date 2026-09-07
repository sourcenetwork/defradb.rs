use std::time::Duration;

use alloy_sol_types::SolCall;
use hub_client::{HubClient, ACP_ADDRESS};
use hub_domain::{ConsensusPublicKey, NativeTx, ReceiptResponse};
use hub_modules::acp::abi::IAcp;
use keyring::{FileKeyring, Keyring};
use sourcehub::hub_rs::NativeWorker;

async fn confirmed(
    client: &HubClient,
    wire: &[u8],
    trusted: &ConsensusPublicKey,
) -> ReceiptResponse {
    let hash = NativeTx::decode_wire(wire).unwrap().tx_id().0;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(response) = client.read_receipt(hash, trusted).await.unwrap() {
                return response;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
#[serial_test::serial]
async fn native_worker_recovers_pending_and_rejected_submissions() {
    let mut hub = super::helpers::start_hub_cluster().await;
    let trusted = *hub_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .unwrap()
        .epoch_info()
        .output
        .public()
        .public();
    let client = HubClient::new(hub.node(0).rpc_url());
    let root = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(root.path().join("keys"), b"test-password").unwrap();
    let directory = root.path().join("worker");
    let mut worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    let did = worker.did().to_owned();
    let payload = IAcp::createPolicyCall {
        policy: b"name: worker\nresources:\n  - name: document\n"
            .to_vec()
            .into(),
        marshalType: 1,
    }
    .abi_encode();
    let wire = worker
        .prepare(ACP_ADDRESS, payload.clone().into())
        .unwrap()
        .to_vec();
    drop(worker);
    let worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    assert_eq!(worker.pending().unwrap(), wire);
    client.send_native_tx(&wire).await.unwrap();
    let proof = confirmed(&client, &wire, &trusted).await;
    drop(worker);

    hub.restart_node(0).unwrap();
    hub.wait_ready(Duration::from_secs(30)).await.unwrap();
    let mut worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    assert_eq!(worker.did(), did);
    assert_eq!(worker.next_sequence(), 0);
    assert_eq!(
        worker.prepare(ACP_ADDRESS, payload.clone().into()).unwrap(),
        wire
    );
    let mut altered = proof.clone();
    altered.receipts[0].receipt.status = false.into();
    assert!(worker.acknowledge(&altered, &trusted).is_err());
    assert_eq!(worker.pending().unwrap(), wire);
    let recovered = confirmed(&client, &wire, &trusted).await;
    assert!(worker.acknowledge(&recovered, &trusted).unwrap().success());
    assert_eq!(worker.next_sequence(), 1);
    assert!(worker.pending().is_none());

    let rejected = worker
        .prepare(ACP_ADDRESS, vec![1, 2, 3, 4].into())
        .unwrap()
        .to_vec();
    let mut other = NativeWorker::open(&root.path().join("other"), &keyring, 9001).unwrap();
    assert_ne!(worker.did(), other.did());
    let independent = other.prepare(ACP_ADDRESS, payload.into()).unwrap().to_vec();
    let (first, second) = tokio::join!(
        client.send_native_tx(&rejected),
        client.send_native_tx(&independent)
    );
    first.unwrap();
    second.unwrap();
    let (failed, succeeded) = tokio::join!(
        confirmed(&client, &rejected, &trusted),
        confirmed(&client, &independent, &trusted)
    );
    assert!(worker.acknowledge(&proof, &trusted).is_err());
    assert_eq!(worker.pending().unwrap(), rejected);

    let state = directory.join("state.json");
    std::fs::rename(&state, directory.join("saved.json")).unwrap();
    std::fs::create_dir(&state).unwrap();
    assert!(worker.acknowledge(&failed, &trusted).is_err());
    assert_eq!(worker.next_sequence(), 1);
    assert_eq!(worker.pending().unwrap(), rejected);
    std::fs::remove_dir(&state).unwrap();
    std::fs::rename(directory.join("saved.json"), &state).unwrap();
    assert!(!worker.acknowledge(&failed, &trusted).unwrap().success());
    assert!(other.acknowledge(&succeeded, &trusted).unwrap().success());
    drop(worker);
    let worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    assert_eq!(worker.did(), did);
    assert_eq!(worker.next_sequence(), 2);
    assert!(worker.pending().is_none());
    assert_eq!(other.next_sequence(), 1);
    assert_eq!(keyring.list().unwrap().len(), 2);
    for worker in [&worker, &other] {
        let proof = client
            .read_current_record(
                hub_domain::ModuleId::NativeNonce,
                &hub_modules::native_account::keys::native_nonce_key(worker.did()),
                failed.revision.height.max(succeeded.revision.height),
                &trusted,
                hub_client::RECORD_PROOF_BYTES,
            )
            .await
            .unwrap();
        let bytes = proof.record.value.unwrap();
        assert_eq!(
            u64::from_le_bytes(bytes.as_ref().try_into().unwrap()),
            worker.next_sequence()
        );
    }
}
