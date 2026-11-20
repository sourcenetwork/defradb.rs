//! Reads on a node whose Vera deployment cannot be reached: by default a
//! row whose access check cannot be answered is left out with no error; a
//! strict read fails with the retryable "access check unavailable" instead.

use std::sync::Arc;

use defra_core::RequestContext;
use defra_node::{EmbeddedVeraActor, Node};
use vera::vera_rs::{DelegationSigner, Ed25519Signer};

use super::{create_user, created, names, node_serving, read_users, users};

#[test]
fn a_strict_read_fails_while_the_deployment_cannot_be_reached() {
    use defra_node::{VeraRsActor, VeraRsConfig};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let chain_signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[44; 32]).expect("key"));
        let signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[45; 32]).expect("key"));
        let chain_owner = identity::Did::new(chain_signer.did()).unwrap();
        let reader = identity::Did::new(signer.did()).unwrap();
        let acting = RequestContext::default().with_identity(Some(reader.to_string()));

        let (chain, rpc) = node_serving(
            &directory.path().join("chain"),
            EmbeddedVeraActor::Signer(Arc::clone(&chain_signer)),
            vec![super::seed(chain_owner.as_str())],
        )
        .await;
        let trust = chain.embedded_vera_trust().expect("deployment").clone();
        let home = directory.path().join("consumer");
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(home.join("keys"), b"test-password").expect("keyring"),
        );
        let node = Node::builder()
            .data_path(home.join("data"))
            .with_vera_rs(VeraRsConfig {
                rpc_address: format!("http://{rpc}"),
                consensus_key: trust.consensus_key,
                genesis_id: None,
                deployment_id: trust.chain_id,
                actor: VeraRsActor::Signer(Arc::clone(&signer)),
                worker_dir: home.join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("consumer node");
        let policy = super::seeded_policy(&node, chain_owner.as_str()).await;
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        created(node.execute_request(create_user("kept", &reader)).await);

        chain.shutdown().await.expect("shutdown");
        drop(chain);

        let strict = node
            .execute_request(
                read_users(&reader).with_context(acting.clone().with_strict_access_reads()),
            )
            .await;
        assert!(strict.data.is_none(), "{:?}", strict.data);
        assert_eq!(strict.errors.len(), 1, "{:?}", strict.errors);
        assert_eq!(
            strict.errors[0].code(),
            Some(query::executor::ACCESS_CHECK_UNAVAILABLE_ERROR_CODE),
            "{:?}",
            strict.errors
        );
        let default = node.execute_request(read_users(&reader)).await;
        assert!(
            names(default).is_empty(),
            "by default the unchecked row is left out without an error"
        );
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
