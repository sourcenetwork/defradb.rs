use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use crypto::keys::secp256k1::Secp256k1PrivateKey;
use integration_test::{TestCluster, TestIdentity};
use keyring::FileKeyring;
use vera::vera_rs::NativeWorker;
use vera_client::{parse_policy_id, VeraClient, RECORD_PROOF_BYTES};
use vera_domain::{ConsensusPublicKey, ModuleId};
use vera_modules::acp::{
    abi::IAcp,
    object_state,
    types::{Object, PolicyCmd},
};

use super::{assert_visibility, replication, submit};

fn owner_token(owner: &TestIdentity, worker: &NativeWorker) -> String {
    let key =
        Secp256k1PrivateKey::from_bytes(&hex::decode(&owner.private_key_hex).unwrap()).unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256K","typ":"vera-delegation-v1+jwt"}"#);
    // Keep the document's exact owner DID, including Defra's uncompressed key encoding.
    let claims = serde_json::json!({
        "iss": owner.did,
        "sub": worker.did(),
        "aud": format!("vera:{}", worker.deployment_id()),
        "scope": "acp:policy",
        "iat": now,
        "nbf": now.saturating_sub(30),
        "exp": now + 300,
    });
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    let message = format!("{header}.{payload}");
    let (signature, _) = key
        .underlying()
        .sign_prehash_recoverable(&crypto::sha256(message.as_bytes()))
        .unwrap();
    format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

pub(super) async fn exercise(
    cluster: &TestCluster,
    client: &VeraClient,
    trusted: &ConsensusPublicKey,
    policy: &str,
    document: &str,
    identities: [&TestIdentity; 2],
    commits: &[String],
) {
    let [owner, reader] = identities;
    let root = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(root.path().join("keys"), b"test-password").unwrap();
    let mut worker = NativeWorker::open(&root.path().join("archive"), &keyring, 9001).unwrap();
    let token = owner_token(owner, &worker);
    let object = Object {
        resource: "users".into(),
        id: document.into(),
    };
    for (phase, command, owner_allowed) in [
        (
            "object archived",
            PolicyCmd::ArchiveObject(object.clone()),
            false,
        ),
        (
            "object unarchived",
            PolicyCmd::UnarchiveObject(object),
            true,
        ),
    ] {
        let height = submit(
            client,
            &mut worker,
            trusted,
            IAcp::bearerPolicyCmdCall {
                bearerToken: token.clone(),
                policyId: parse_policy_id(policy).unwrap(),
                cmd: serde_json::to_vec(&command).unwrap().into(),
            },
        )
        .await;
        replication::wait_for_height(cluster, height).await;
        let state = client
            .read_current_record(
                ModuleId::Acp,
                &object_state::key(policy, "users", document),
                height,
                trusted,
                RECORD_PROOF_BYTES,
            )
            .await
            .expect("certified object incarnation");
        assert_eq!(
            object_state::decode(state.record.value.as_ref().expect("advanced incarnation"))
                .unwrap(),
            1,
            "{phase}: unarchive must preserve the advanced incarnation"
        );
        for index in 0..cluster.len() {
            let node = cluster.client(index);
            assert_visibility(
                phase,
                &node,
                document,
                Some(&owner.private_key_hex),
                if owner_allowed { commits } else { &[] },
            );
            assert_visibility(phase, &node, document, Some(&reader.private_key_hex), &[]);
            assert_visibility(phase, &node, document, None, &[]);
        }
    }
    cluster
        .client(0)
        .acp_relationship_add(
            "User",
            document,
            "reader",
            &reader.did,
            &owner.private_key_hex,
        )
        .expect("explicit grant after unarchive");
    replication::wait_for_submission(cluster, 0).await;
    let minimum = client
        .block_number()
        .await
        .expect("published regrant height");
    let page = client
        .read_relationship_page(parse_policy_id(policy).unwrap(), None, 16, minimum, trusted)
        .await
        .expect("certified current-incarnation grant");
    assert!(page.continuation.is_none());
    let grants: Vec<_> = page
        .records
        .iter()
        .filter(|record| {
            record.relationship.object_id == document && record.relationship.relation == "reader"
        })
        .collect();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].incarnation, 1);
    for index in 0..cluster.len() {
        let node = cluster.client(index);
        assert_visibility(
            "explicit incarnation grant",
            &node,
            document,
            Some(&owner.private_key_hex),
            commits,
        );
        assert_visibility(
            "explicit incarnation grant",
            &node,
            document,
            Some(&reader.private_key_hex),
            commits,
        );
        assert_visibility("anonymous after unarchive", &node, document, None, &[]);
    }
}
