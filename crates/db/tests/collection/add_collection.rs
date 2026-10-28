//! `Client::add_collection`: the one place collections are added from SDL.

use acp::{MemoryZanzibarStore, Policy, PolicyLookup};
use client::{Client, Error};
use db::{NacConfig, NacManager, DB};
use defra_core::RequestContext;
use identity::Did;
use std::sync::Arc;
use storage::RegolithStore;

const OWNER_DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER_DID: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

fn new_db() -> DB<RegolithStore> {
    DB::new(RegolithStore::in_memory().unwrap()).unwrap()
}

fn acting(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_string()))
}

struct NoPolicies;

#[async_trait::async_trait]
impl PolicyLookup for NoPolicies {
    async fn get_policy(&self, _policy_id: &str) -> Result<Option<Policy>, String> {
        Ok(None)
    }
}

#[test]
fn relation_to_an_existing_collection_resolves() {
    workers::block_on(async {
        let db = new_db();
        let ctx = RequestContext::default();
        db.add_collection(&ctx, "type Author { name: String }")
            .await
            .unwrap();
        let created = db
            .add_collection(&ctx, "type Book { title: String author: Author }")
            .await
            .unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].name, "Book");
    })
    .unwrap()
}

#[test]
fn one_invalid_type_creates_nothing() {
    workers::block_on(async {
        let db = new_db();
        let err = db
            .add_collection(
                &RequestContext::default(),
                "type Good { n: Int } type Bad { x: NotAType }",
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::Parse(_) | Error::Validation(_)),
            "{err:?}"
        );
        assert!(db.list_collections().unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn caller_without_collection_patch_is_denied() {
    workers::block_on(async {
        let db = new_db();
        let manager = Arc::new(NacManager::new(
            Arc::new(MemoryZanzibarStore::new()),
            NacConfig::new().with_enabled().with_dev_mode(),
        ));
        manager
            .initialize(Some(&Did::new(OWNER_DID).unwrap()))
            .await
            .unwrap();
        db.set_nac_manager(manager);

        let err = db
            .add_collection(&acting(STRANGER_DID), "type Widget { n: Int }")
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotAuthorized(_)), "{err:?}");
        assert!(db.list_collections().unwrap().is_empty());

        db.add_collection(&acting(OWNER_DID), "type Widget { n: Int }")
            .await
            .expect("the owner may add collections");
    })
    .unwrap()
}

#[test]
fn missing_policy_is_rejected_when_a_lookup_is_installed() {
    workers::block_on(async {
        let db = new_db();
        let sdl = "type Doc @policy(id: \"deadbeef\", resource: \"doc\") { n: Int }";
        let ctx = RequestContext::default();

        db.add_collection(&ctx, sdl)
            .await
            .expect("without a lookup the policy is not checked");
        let db = new_db();
        db.set_policy_lookup(Arc::new(NoPolicies));
        let err = db.add_collection(&ctx, sdl).await.unwrap_err();
        assert!(matches!(err, Error::Policy(_)), "{err:?}");
        assert_eq!(
            err.to_string(),
            "policyID specified does not exist with acp. PolicyID: deadbeef"
        );
        assert!(db.list_collections().unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn errors_carry_go_text_without_a_prefix() {
    workers::block_on(async {
        let db = new_db();
        let ctx = RequestContext::default();
        db.add_collection(&ctx, "type Users { name: String }")
            .await
            .unwrap();

        let duplicate = db
            .add_collection(&ctx, "type Users { name: String }")
            .await
            .unwrap_err();
        assert_eq!(
            duplicate.to_string(),
            "collection already exists. Name: Users"
        );

        let unknown = db
            .add_collection(&ctx, "type Pets { name: NotAType }")
            .await
            .unwrap_err();
        assert_eq!(
            unknown.to_string(),
            "no type found for given name. Field: name, Kind: NotAType"
        );

        let non_null = db
            .add_collection(
                &ctx,
                "type Dogs { name: String } type Owner { dogs: [Dogs!] }",
            )
            .await
            .unwrap_err();
        assert_eq!(
            non_null.to_string(),
            "NonNull variants for type are not supported. Type: Dogs"
        );
    })
    .unwrap()
}
