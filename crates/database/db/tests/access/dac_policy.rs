//! DAC policies through the core: `DB::add_dac_policy_as` and its readers,
//! the one path every edge calls, with Go's checks, order and texts.

use std::sync::Arc;

use acp::{
    LightClientStatus, MemoryZanzibarStore, Policy, PolicyNonce, RemotePolicies, ZanzibarStore,
};
use db::{DacPolicies, DacPolicyError, NacConfig, NacManager, DB};
use defra_core::RequestContext;
use identity::Did;
use regolith::sync::Atom;
use storage::RegolithStore;

const POLICY: &str = r#"
name: Sensitive Rows
resources:
  - name: users
    relations:
      - name: reader
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
"#;
const OWNER_DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER_DID: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

fn acting(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

fn local_db() -> (DB<RegolithStore>, Arc<MemoryZanzibarStore>) {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let store = Arc::new(MemoryZanzibarStore::new());
    db.set_dac_policies(Arc::new(DacPolicies::local(store.clone())));
    (db, store)
}

/// A remote ACP that creates every policy under one ID and remembers who
/// asked, with which nonce.
#[derive(Default)]
struct Remote {
    creators: Atom<Vec<String>>,
    nonces: Atom<Vec<Option<PolicyNonce>>>,
}

#[async_trait::async_trait]
impl RemotePolicies for Remote {
    async fn add_policy(
        &self,
        creator: &str,
        _policy: &str,
        nonce: Option<PolicyNonce>,
    ) -> Result<String, String> {
        let mut creators = self.creators.load_clone();
        creators.push(creator.to_owned());
        self.creators.store(creators);
        let mut nonces = self.nonces.load_clone();
        nonces.push(nonce);
        self.nonces.store(nonces);
        Ok("remote-policy".to_owned())
    }

    async fn get_policy(&self, _policy_id: &str) -> Result<Option<Policy>, String> {
        Ok(None)
    }

    fn light_client_status(&self) -> Result<LightClientStatus, String> {
        Ok(LightClientStatus {
            height: 7,
            ..Default::default()
        })
    }
}

fn remote_db() -> (DB<RegolithStore>, Arc<Remote>, Arc<MemoryZanzibarStore>) {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let remote = Arc::new(Remote::default());
    let mirror = Arc::new(MemoryZanzibarStore::new());
    db.set_dac_policies(Arc::new(DacPolicies::remote(
        remote.clone(),
        mirror.clone(),
    )));
    (db, remote, mirror)
}

/// vera.rs derives a policy's ID from its definition, creator and nonce, so
/// a counter names none of its policies: the request is refused before the
/// remote ACP is asked, and nothing is kept.
#[test]
fn a_counter_names_no_policy_on_a_remote_acp() {
    workers::block_on(async {
        let (db, remote, mirror) = remote_db();
        let err = db
            .add_dac_policy_with_counter_as(&acting(OWNER_DID), POLICY, 1)
            .await
            .unwrap_err();
        assert!(matches!(err, DacPolicyError::CounterNeedsLocalAcp), "{err}");
        assert!(remote.creators.load_clone().is_empty());
        assert!(mirror.list_policies().await.unwrap().is_empty());
    })
    .unwrap()
}

/// The caller's nonce reaches the remote ACP, which derives the ID from it,
/// and the policy it names is kept locally.
#[test]
fn a_callers_nonce_reaches_the_remote_acp_and_its_policy_is_kept() {
    workers::block_on(async {
        let (db, remote, mirror) = remote_db();
        let nonce = PolicyNonce([7; 32]);
        let id = db
            .add_dac_policy_with_nonce_as(&acting(OWNER_DID), POLICY, nonce)
            .await
            .unwrap();
        assert_eq!(id, "remote-policy");
        assert_eq!(remote.nonces.load_clone(), vec![Some(nonce)]);
        assert_eq!(remote.creators.load_clone(), vec![OWNER_DID.to_owned()]);
        assert!(mirror.get_policy(&id).await.unwrap().is_some());
    })
    .unwrap()
}

/// A local ACP numbers its policies, as Go's does: a nonce cannot make its
/// ID, so the request is refused rather than given an ID made another way.
#[test]
fn a_nonce_is_refused_by_a_local_acp() {
    workers::block_on(async {
        let (db, store) = local_db();
        let err = db
            .add_dac_policy_with_nonce_as(&acting(OWNER_DID), POLICY, PolicyNonce([7; 32]))
            .await
            .unwrap_err();
        assert!(matches!(err, DacPolicyError::NonceNeedsRemoteAcp), "{err}");
        assert!(store.list_policies().await.unwrap().is_empty());
    })
    .unwrap()
}

/// Go's order still holds for a request with a nonce: the policy is checked
/// before the nonce is.
#[test]
fn an_invalid_policy_with_a_nonce_is_refused_for_the_policy() {
    workers::block_on(async {
        let (db, _) = local_db();
        let err = db
            .add_dac_policy_with_nonce_as(
                &acting(OWNER_DID),
                "resources:\n  - name: users\n",
                PolicyNonce::default(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("name required"), "{err}");
    })
    .unwrap()
}

#[test]
fn a_node_without_acp_refuses_with_gos_text() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let err = db
            .add_dac_policy_as(&acting(OWNER_DID), POLICY)
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "operation requires ACP, but ACP not available"
        );
    })
    .unwrap()
}

#[test]
fn an_invalid_policy_carries_gos_wrapper_and_the_reason() {
    workers::block_on(async {
        let (db, _) = local_db();
        let err = db
            .add_dac_policy_as(&acting(OWNER_DID), "resources:\n  - name: users\n")
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("failed to add policy with acp: ")
                && message.contains("name required")
                && message.ends_with(&format!("Type: Local DAC, CreatorID: {OWNER_DID}")),
            "{message}"
        );
    })
    .unwrap()
}

#[test]
fn node_access_is_checked_before_the_creator() {
    workers::block_on(async {
        let (db, _) = local_db();
        let manager = Arc::new(NacManager::new(
            Arc::new(MemoryZanzibarStore::new()),
            NacConfig::new().with_enabled().with_dev_mode(),
        ));
        manager
            .initialize(Some(&Did::new(OWNER_DID).unwrap()))
            .await
            .unwrap();
        db.set_nac_manager(manager);

        for context in [RequestContext::default(), acting(STRANGER_DID)] {
            let err = db.add_dac_policy_as(&context, POLICY).await.unwrap_err();
            assert!(
                matches!(err, DacPolicyError::Db(db::Error::NotAuthorized { .. })),
                "{err}"
            );
        }
        db.add_dac_policy_as(&acting(OWNER_DID), POLICY)
            .await
            .expect("the owner may add policies");
    })
    .unwrap()
}

#[test]
fn an_added_policy_is_read_back_and_guards_a_collection() {
    workers::block_on(async {
        let (db, _) = local_db();
        let context = acting(OWNER_DID);
        let id = db.add_dac_policy_as(&context, POLICY).await.unwrap();

        let stored = db.get_dac_policy_as(&context, &id).await.unwrap().unwrap();
        assert_eq!(stored.name, "Sensitive Rows");
        let listed = db.list_dac_policies_as(&context).await.unwrap();
        assert!(listed.iter().any(|policy| policy.id == id));
        assert!(db
            .get_dac_policy_as(&context, "missing")
            .await
            .unwrap()
            .is_none());

        db.validate_resource_interface(&id, "users").await.unwrap();
        let err = db
            .validate_resource_interface(&id, "orders")
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .starts_with("resource does not exist on the specified policy"),
            "{err}"
        );
    })
    .unwrap()
}

#[test]
fn a_local_node_has_no_light_client() {
    workers::block_on(async {
        let (db, _) = local_db();
        let err = db
            .acp_light_client_status_as(&acting(OWNER_DID))
            .await
            .unwrap_err();
        assert!(matches!(err, DacPolicyError::NoLightClient), "{err}");
    })
    .unwrap()
}

#[test]
fn assignment_reads_require_the_subject_and_do_not_treat_unsupported_as_empty() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.set_dac_policies(Arc::new(DacPolicies::remote(
            Arc::new(Remote::default()),
            Arc::new(MemoryZanzibarStore::new()),
        )));
        let query = acp::AssignmentQuery {
            policy: "policy".into(),
            resource: "document".into(),
            relation: "reader".into(),
            subject: "did:key:other".into(),
            issuer: OWNER_DID.into(),
            not_before: None,
        };
        let error = db
            .certified_assignments_as(&acting(OWNER_DID), &query)
            .await
            .unwrap_err();
        assert!(matches!(error, DacPolicyError::AssignmentSubject));
        let query = acp::AssignmentQuery {
            subject: OWNER_DID.into(),
            ..query
        };
        let error = db
            .certified_assignments_as(&acting(OWNER_DID), &query)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DacPolicyError::Assignments(acp::AssignmentReadError::Unsupported)
        ));
    })
    .unwrap();
}

#[test]
fn a_remote_policy_gets_the_caller_as_creator_and_a_local_copy() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let remote = Arc::new(Remote::default());
        let mirror = Arc::new(MemoryZanzibarStore::new());
        db.set_dac_policies(Arc::new(DacPolicies::remote(
            remote.clone(),
            mirror.clone(),
        )));
        let context = acting(OWNER_DID);

        let id = db.add_dac_policy_as(&context, POLICY).await.unwrap();
        assert_eq!(id, "remote-policy");
        assert_eq!(remote.creators.load_clone(), vec![OWNER_DID.to_owned()]);
        assert_eq!(
            remote.nonces.load_clone(),
            vec![None],
            "without a nonce the remote ACP draws its own"
        );
        let kept = mirror.get_policy(&id).await.unwrap().expect("a local copy");
        assert_eq!(kept.name, "Sensitive Rows");
        db.validate_resource_interface(&id, "users").await.unwrap();

        let status = db.acp_light_client_status_as(&context).await.unwrap();
        assert_eq!(status.height, 7);
    })
    .unwrap()
}
