//! Each write collects into the batch signing session its request's context
//! names, and a request outside that session collects nothing into it.

use crate::common::{fixture::make_test_db_with_bus, schema::test_collection};
use db::AutoCommitMutator;
use defra_core::{batch_signing, RequestContext};
use document::{Document, NormalValue};
use query::mutator::DocMutator;

#[test]
fn creates_updates_and_deletes_collect_into_their_requests_session() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
        let session = "every-write-collects";
        let context = RequestContext::default().with_batch_session(Some(session.to_string()));

        batch_signing::batch_start(session);
        let created = mutator
            .create_as(
                &context,
                "TestDoc",
                Document::from_json_str(r#"{"x":1}"#, test_collection()).unwrap(),
            )
            .await
            .unwrap();
        assert!(batch_signing::batch_take_cids(session)
            .unwrap()
            .contains(&created.commit_cid.unwrap()));

        batch_signing::batch_start(session);
        let mut doc = created.document;
        doc.set("x", NormalValue::Int(2)).unwrap();
        let updated = mutator
            .update_as(
                &context,
                "TestDoc",
                doc,
                ["x".to_string()].into_iter().collect(),
            )
            .await
            .unwrap();
        assert!(batch_signing::batch_take_cids(session)
            .unwrap()
            .contains(&updated.commit_cid.unwrap()));

        batch_signing::batch_start(session);
        let deleted = mutator
            .delete_as(&context, "TestDoc", &created.doc_id)
            .await
            .unwrap();
        assert!(batch_signing::batch_take_cids(session)
            .unwrap()
            .contains(&deleted.commit_cid.unwrap()));

        batch_signing::batch_start(session);
        mutator
            .create_as(
                &RequestContext::default(),
                "TestDoc",
                Document::from_json_str(r#"{"x":3}"#, test_collection()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            batch_signing::batch_take_cids(session).unwrap(),
            Vec::new(),
            "a request outside the session collected into it"
        );
    })
    .unwrap()
}

const BEARER_DID: &str = "did:key:request-context-bearer-hook";

/// An external ACP provider that records the bearer each call presents: the
/// one of the request it is bound to.
#[derive(Default)]
struct BoundObserver {
    seen: std::sync::Arc<kovan_queue::seg_queue::SegQueue<Option<String>>>,
    bearer: Option<String>,
}

impl BoundObserver {
    fn record(&self) {
        self.seen.push(self.bearer.clone());
    }
}

#[async_trait::async_trait]
impl acp::DocumentACP for BoundObserver {
    fn bind_request(
        &self,
        context: &RequestContext,
    ) -> Option<std::sync::Arc<dyn acp::DocumentACP>> {
        Some(std::sync::Arc::new(Self {
            seen: std::sync::Arc::clone(&self.seen),
            bearer: context.bearer_token_for(BEARER_DID),
        }))
    }

    async fn register_doc_object(
        &self,
        _identity: &identity::Did,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
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
        _identity: &acp::Identity,
        _permission: acp::DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(true)
    }

    async fn add_actor_relationship(
        &self,
        _requestor: &identity::Did,
        _target: &identity::Did,
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
        _requestor: &identity::Did,
        _target: &identity::Did,
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
fn a_create_registers_with_the_external_acp_using_its_requests_bearer() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        let mut schema = test_collection();
        schema.policy = Some(schema::PolicyDescription::new("bearer-policy", "TestDoc"));
        db.create_collection(schema.clone()).await.unwrap();
        let observer = std::sync::Arc::new(BoundObserver::default());
        db.set_document_acp(std::sync::Arc::new(acp::ExternalDocumentAcp::new(
            observer.clone(),
            db.store().clone(),
        )));
        let context = RequestContext::default()
            .with_identity(Some(BEARER_DID.to_string()))
            .with_creator(Some(BEARER_DID.to_string()))
            .with_bearer(Some(BEARER_DID.to_string()), Some("captured".to_string()));

        AutoCommitMutator::new(db)
            .create_as(
                &context,
                "TestDoc",
                Document::from_json_str(r#"{"x":1}"#, schema).unwrap(),
            )
            .await
            .unwrap();

        let seen: Vec<_> = std::iter::from_fn(|| observer.seen.pop()).collect();
        assert!(
            !seen.is_empty(),
            "the create never reached the external ACP"
        );
        assert!(
            seen.iter()
                .all(|bearer| bearer.as_deref() == Some("captured")),
            "the external ACP acted without the request's bearer: {seen:?}"
        );
    })
    .unwrap()
}
