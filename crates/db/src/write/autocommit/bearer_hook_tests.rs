use super::*;
use acp::{DocumentPermission, Identity};
use identity::Did;
use regolith::sync::Atom;
use schema::{CollectionVersion, FieldDescription, FieldKind, PolicyDescription};

const DID: &str = "did:key:autocommit-hook-test";
const TOKEN: &str = "bearer-captured-when-staged";

#[derive(Default)]
struct TokenObserver(Atom<Vec<Option<String>>>);

impl TokenObserver {
    fn record(&self) {
        let token = defra_core::signing::get_request_bearer_token(DID);
        self.0.rcu(|seen| {
            let mut next = seen.clone();
            next.push(token.clone());
            next
        });
    }

    fn seen(&self) -> Vec<Option<String>> {
        self.0.load_clone()
    }
}

#[async_trait]
impl acp::DocumentACP for TokenObserver {
    async fn register_doc_object(
        &self,
        identity: &Did,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        assert_eq!(identity.as_str(), DID);
        self.record();
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        self.record();
        Ok(false)
    }

    async fn check_doc_access(
        &self,
        _identity: &Identity,
        _permission: DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(true)
    }

    async fn add_actor_relationship(
        &self,
        _requestor: &Did,
        _target: &Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn delete_actor_relationship(
        &self,
        _requestor: &Did,
        _target: &Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn unregister_doc_object(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        self.record();
        Ok(())
    }
}

#[test]
fn deferred_registration_hook_uses_bearer_captured_before_commit() {
    workers::block_on(async {
        let db = Arc::new(DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let mut schema = CollectionVersion::new(
            "BearerHookDoc",
            "v1",
            "bearer-hook-doc",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "name", FieldKind::string()),
            ],
        );
        schema.policy = Some(PolicyDescription::new(
            "bearer-hook-policy",
            "BearerHookDoc",
        ));
        db.create_collection(schema).await.unwrap();
        let collection = db.get_collection("BearerHookDoc").unwrap().unwrap();
        let observer = Arc::new(TokenObserver::default());
        db.set_document_acp(Arc::new(acp::ExternalDocumentAcp::new(
            observer.clone(),
            db.store().clone(),
        )));

        let txn = db.new_txn(false).await.unwrap();
        acp::PendingRegistrations::new(&txn.acpstore().unwrap())
            .record(&acp::PendingRegistration::new(
                Did::new(DID).unwrap(),
                "bearer-hook-policy",
                "BearerHookDoc",
                "doc-1",
            ))
            .await
            .unwrap();
        defra_core::signing::scope_broadcast_creator_did(Some(DID.to_string()), async {
            defra_core::signing::scope_request_bearer_token(
                Some(DID.to_string()),
                Some(TOKEN.to_string()),
                async {
                    AutoCommitMutator::new(db.clone())
                        .register_created_doc_with_acp(&txn, &collection, "doc-1".into())
                        .unwrap();
                },
            )
            .await;
        })
        .await;

        assert_eq!(defra_core::signing::get_request_bearer_token(DID), None);
        txn.commit().await.unwrap();
        assert_eq!(
            observer.seen(),
            vec![Some(TOKEN.to_string()), Some(TOKEN.to_string())]
        );
        assert_eq!(defra_core::signing::get_request_bearer_token(DID), None);
    })
    .unwrap()
}

#[test]
fn create_retry_callback_uses_bearer_captured_before_commit() {
    workers::block_on(async {
        let observer = Arc::new(TokenObserver::default());
        let creator = Did::new(DID).unwrap();
        let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
        let owed = datastore::BasicTxn::new(store.as_ref(), 0, false)
            .await
            .unwrap();
        acp::PendingRegistrations::new(&owed.acpstore())
            .record(&acp::PendingRegistration::new(
                creator.clone(),
                "policy",
                "resource",
                "doc-2",
            ))
            .await
            .unwrap();
        owed.commit().await.unwrap();
        let callback = super::create::acp_registration_retry_callback(
            Arc::new(acp::ExternalDocumentAcp::new(observer.clone(), store)),
            creator,
            "policy".into(),
            "resource".into(),
            "doc-2".into(),
            Some(TOKEN.to_string()),
        );

        assert_eq!(defra_core::signing::get_request_bearer_token(DID), None);
        callback().await.unwrap();
        assert_eq!(defra_core::signing::get_request_bearer_token(DID), None);
        let observed = observer.seen();
        assert_eq!(
            observed,
            vec![Some(TOKEN.to_string()), Some(TOKEN.to_string())]
        );
    })
    .unwrap()
}

#[test]
fn deferred_unregistration_hook_uses_bearer_captured_before_commit() {
    workers::block_on(async {
        let db = Arc::new(DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let mut schema = CollectionVersion::new(
            "BearerHookDeleteDoc",
            "v1",
            "bearer-hook-delete-doc",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "name", FieldKind::string()),
            ],
        );
        schema.policy = Some(PolicyDescription::new(
            "bearer-hook-delete-policy",
            "BearerHookDeleteDoc",
        ));
        db.create_collection(schema).await.unwrap();
        let collection = db.get_collection("BearerHookDeleteDoc").unwrap().unwrap();
        let observer = Arc::new(TokenObserver::default());
        db.set_document_acp(observer.clone());

        let txn = db.new_txn(false).await.unwrap();
        defra_core::signing::scope_broadcast_creator_did(Some(DID.to_string()), async {
            defra_core::signing::scope_request_bearer_token(
                Some(DID.to_string()),
                Some(TOKEN.to_string()),
                async {
                    AutoCommitMutator::new(db.clone())
                        .register_deleted_doc_with_acp(&txn, &collection, "doc-1".into())
                        .unwrap();
                },
            )
            .await;
        })
        .await;

        assert_eq!(defra_core::signing::get_request_bearer_token(DID), None);
        txn.commit().await.unwrap();
        assert_eq!(observer.seen(), vec![Some(TOKEN.to_string())]);
        assert_eq!(defra_core::signing::get_request_bearer_token(DID), None);
    })
    .unwrap()
}
