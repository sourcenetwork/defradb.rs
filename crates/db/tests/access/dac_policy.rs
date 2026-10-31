//! DAC policies through the core: `DB::add_dac_policy_as` and its readers,
//! the one path every edge calls, with Go's checks, order and texts.

use std::sync::Arc;

use acp::{LightClientStatus, MemoryZanzibarStore, Policy, RemotePolicies, ZanzibarStore};
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
/// asked.
#[derive(Default)]
struct Remote {
    creators: Atom<Vec<String>>,
}

#[async_trait::async_trait]
impl RemotePolicies for Remote {
    async fn add_policy(&self, creator: &str, _policy: &str) -> Result<String, String> {
        let mut creators = self.creators.load_clone();
        creators.push(creator.to_owned());
        self.creators.store(creators);
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
fn a_policy_needs_a_creator_and_data() {
    workers::block_on(async {
        let (db, _) = local_db();
        let err = db
            .add_dac_policy_as(&RequestContext::default(), POLICY)
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "policy creator can not be empty");
        let err = db
            .add_dac_policy_as(&acting(OWNER_DID), "")
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "policy data can not be empty");
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
        let kept = mirror.get_policy(&id).await.unwrap().expect("a local copy");
        assert_eq!(kept.name, "Sensitive Rows");
        db.validate_resource_interface(&id, "users").await.unwrap();

        let err = db
            .add_dac_policy_with_counter_as(&context, POLICY, 1)
            .await
            .unwrap_err();
        assert!(
            matches!(err, DacPolicyError::ExplicitCounterRemote),
            "{err}"
        );
        let status = db.acp_light_client_status_as(&context).await.unwrap();
        assert_eq!(status.height, 7);
    })
    .unwrap()
}
