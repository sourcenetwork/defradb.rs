//! A request's context reaches the writes and reads it makes, and only its
//! own: the creator, the encryption and the signing authorization of each
//! create, and the DAC bypass of each read.

use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use defra_core::RequestContext;
use document::{DocID, Document};
use identity::Did;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::{CollectionVersion, FieldDescription, FieldKind, PolicyDescription};

use crate::error::Result;
use crate::executor::{QueryExecutor, QueryRequest, QueryResponse};
use crate::mutator::{CreateResult, DeleteResult, DocMutator, UpdateResult};
use crate::runner::QueryRunner;
use crate::test_utils::MockFetcher;

const CALLER: &str = "did:key:z6MktwupdmLXVVqTzCw4i46r4uGyosGXRnR3XjN4Zq7oMMsw";
const ADMIN: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

/// What each create saw of its request's context.
#[derive(Default)]
struct ContextMutator {
    creators: SegQueue<Option<String>>,
    encryptions: SegQueue<Option<defra_core::encryption::EncryptionConfig>>,
    authorizations: SegQueue<Option<defra_core::signing::SigningAuthorization>>,
}

fn drain<T: 'static>(queue: &SegQueue<T>) -> Vec<T> {
    std::iter::from_fn(|| queue.pop()).collect()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocMutator for ContextMutator {
    async fn create(&self, collection_name: &str, doc: Document) -> Result<CreateResult> {
        self.create_as(&RequestContext::ambient(), collection_name, doc)
            .await
    }

    async fn create_as(
        &self,
        context: &RequestContext,
        _collection_name: &str,
        mut doc: Document,
    ) -> Result<CreateResult> {
        self.creators.push(context.creator().map(str::to_owned));
        self.encryptions.push(context.encryption().cloned());
        self.authorizations.push(
            context
                .signing()
                .and_then(|signing| signing.signing_authorization.clone()),
        );
        let seq = self.creators.len();
        doc.set_id(DocID::new_v0_from_seed(&format!("context-doc-{seq}")));
        let id = doc
            .id()
            .cloned()
            .ok_or_else(|| crate::error::QueryError::execution("the seeded document has no id"))?;
        Ok(CreateResult::new(id, doc))
    }

    async fn update(
        &self,
        _collection_name: &str,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        Ok(UpdateResult::new(doc, modified_fields.len()))
    }

    async fn delete(&self, _collection_name: &str, doc_id: &DocID) -> Result<DeleteResult> {
        Ok(DeleteResult::new(doc_id.clone(), true))
    }

    async fn exists(&self, _collection_name: &str, _doc_id: &DocID) -> Result<bool> {
        Ok(false)
    }

    async fn get_for_update(
        &self,
        _collection_name: &str,
        _doc_id: &DocID,
    ) -> Result<Option<Document>> {
        Ok(None)
    }
}

fn users() -> CollectionVersion {
    CollectionVersion::new(
        "User",
        "v1",
        "coll-user",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

fn assert_ok(response: &QueryResponse) {
    assert!(!response.has_errors(), "{:?}", response.errors);
}

#[test]
fn a_mutation_creates_as_its_caller_and_no_one_else() {
    workers::block_on(async {
        let mutator = Arc::new(ContextMutator::default());
        let runner =
            QueryRunner::new(MockFetcher::new(), vec![users()]).with_mutator(mutator.clone());
        let caller = Did::new(CALLER).unwrap();

        assert_ok(
            &runner
                .execute(
                    QueryRequest::new(
                        r#"mutation { add_User(input: [{ name: "Alice" }]) { _docID } }"#,
                    )
                    .with_identity(Some(caller.clone())),
                )
                .await,
        );
        assert_ok(
            &runner
                .execute(QueryRequest::new(
                    r#"mutation { add_User(input: [{ name: "Bob" }]) { _docID } }"#,
                ))
                .await,
        );

        assert_eq!(
            drain(&mutator.creators),
            vec![Some(caller.to_string()), None],
            "the caller of one mutation created the next one's documents"
        );
    })
    .unwrap()
}

#[test]
fn each_create_carries_only_its_own_mutations_encryption() {
    workers::block_on(async {
        let mutator = Arc::new(ContextMutator::default());
        let runner =
            QueryRunner::new(MockFetcher::new(), vec![users()]).with_mutator(mutator.clone());

        runner
            .execute_mutation(
                r#"mutation {
                sealed: add_User(input: { name: "Alice" }, encryptFields: ["name"]) { _docID }
                plain: add_User(input: { name: "Bob" }) { _docID }
            }"#,
            )
            .await
            .unwrap();

        let encryptions = drain(&mutator.encryptions);
        assert_eq!(encryptions.len(), 2);
        assert!(encryptions[0]
            .as_ref()
            .is_some_and(|config| config.should_encrypt_field("name") && !config.encrypt_doc));
        assert!(
            encryptions[1].is_none(),
            "an encrypted mutation's config reached the next mutation of its request"
        );
    })
    .unwrap()
}

/// Grants every create an access decision a remote signer signs under, and
/// hides the documents it names from everyone but their owner.
#[derive(Default)]
struct PolicyAcp {
    private_docs: RapidHashMap<String, Did>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for PolicyAcp {
    async fn register_doc_object(
        &self,
        _identity: &Did,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(self.private_docs.contains_key(doc_id))
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        _permission: DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(match self.private_docs.get(doc_id) {
            None => true,
            Some(owner) => identity.did() == Some(owner),
        })
    }

    async fn create_access_decision(
        &self,
        _identity: &Identity,
        _policy_id: &str,
        _resource_name: &str,
        _object_id: &str,
        _permission: &str,
    ) -> acp::Result<Option<String>> {
        Ok(Some("decision-1".to_string()))
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
        Ok(())
    }
}

struct UnusedSigner;

impl defra_core::signing::RemoteSigner for UnusedSigner {
    fn sign_sync(
        &self,
        _data: &[u8],
        _authorization: Option<&defra_core::signing::SigningAuthorization>,
    ) -> std::result::Result<Vec<u8>, String> {
        Err("the context mutator signs nothing".to_string())
    }
}

#[test]
fn a_remote_signed_create_carries_its_access_decision_in_its_context() {
    workers::block_on(async {
        let mut notes = CollectionVersion::new(
            "Note",
            "v1",
            "coll-note",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "body", FieldKind::string(), schema::CType::LwwRegister),
            ],
        );
        notes.policy = Some(PolicyDescription::new("policy", "notes"));
        let mutator = Arc::new(ContextMutator::default());
        let runner = QueryRunner::new(MockFetcher::new(), vec![notes])
            .with_mutator(mutator.clone())
            .with_acp(Arc::new(PolicyAcp::default()));
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: Vec::new(),
            public_key_bytes: Vec::new(),
            public_key_hex: "remote".to_string(),
            remote_signer: Some(Arc::new(UnusedSigner)),
            signing_authorization: None,
        };

        assert_ok(
            &runner
                .execute(
                    QueryRequest::new(r#"mutation { add_Note(input: { body: "a" }) { _docID } }"#)
                        .with_identity(Some(Did::new(CALLER).unwrap()))
                        .with_context(RequestContext::default().with_signing(Some(signing))),
                )
                .await,
        );

        assert_eq!(
            drain(&mutator.authorizations),
            vec![Some(defra_core::signing::SigningAuthorization::Decision {
                decision_id: "decision-1".to_string(),
            })]
        );
    })
    .unwrap()
}

#[test]
fn the_dac_bypass_in_a_request_reads_documents_dac_hides() {
    workers::block_on(async {
        let owner = Did::new(CALLER).unwrap();
        let admin = Did::new(ADMIN).unwrap();
        let mut company = CollectionVersion::new(
            "Company",
            "v1",
            "coll-company",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            ],
        );
        company.policy = Some(PolicyDescription::new("policy", "companies"));
        let private = "bae-47bd7c29-69cc-5b8a-856f-caaa93d9ace0";
        let fetcher = MockFetcher::new();
        fetcher.add_doc(
            "Company",
            crate::test_utils::doc_from_json(&format!(
                r#"{{"_docID":"{private}","name":"Private"}}"#
            )),
        );
        let acp = Arc::new(PolicyAcp {
            private_docs: RapidHashMap::from_iter([(private.to_string(), owner)]),
        });
        let runner = QueryRunner::new(fetcher, vec![company]).with_acp(acp);
        let visible = |response: QueryResponse| {
            assert_ok(&response);
            response
                .data
                .and_then(|data| data["Company"].as_array().map(Vec::len))
                .unwrap_or(0)
        };
        let read = |identity: Option<Did>, bypass: bool| {
            runner.execute(
                QueryRequest::new("{ Company { name } }")
                    .with_identity(identity)
                    .with_context(RequestContext::default().with_dac_bypass(bypass)),
            )
        };

        assert_eq!(visible(read(Some(admin.clone()), true).await), 1);
        assert_eq!(visible(read(Some(admin), false).await), 0);
        assert_eq!(
            visible(read(None, true).await),
            0,
            "the bypass never lets an anonymous request read a private document"
        );
    })
    .unwrap()
}

/// Records the bearer each access check presents: the one of the request it
/// is bound to.
#[derive(Default)]
struct BearerAcp {
    seen: Arc<SegQueue<Option<String>>>,
    bearer: Option<String>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for BearerAcp {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn DocumentACP>> {
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
            bearer: context.bearer_token_for(CALLER),
        }))
    }

    async fn register_doc_object(
        &self,
        _identity: &Did,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(true)
    }

    async fn check_doc_access(
        &self,
        _identity: &Identity,
        _permission: DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        self.seen.push(self.bearer.clone());
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
        Ok(())
    }
}

#[test]
fn each_read_checks_access_with_its_own_requests_bearer() {
    workers::block_on(async {
        let mut company = CollectionVersion::new(
            "Company",
            "v1",
            "coll-company",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            ],
        );
        company.policy = Some(PolicyDescription::new("policy", "companies"));
        let fetcher = MockFetcher::new();
        fetcher.add_doc(
            "Company",
            crate::test_utils::doc_from_json(
                r#"{"_docID":"bae-47bd7c29-69cc-5b8a-856f-caaa93d9ace0","name":"Acme"}"#,
            ),
        );
        let acp = Arc::new(BearerAcp::default());
        let runner = QueryRunner::new(fetcher, vec![company]).with_acp(acp.clone());
        let read = |token: &str| {
            runner.execute(
                QueryRequest::new("{ Company { name } }")
                    .with_identity(Some(Did::new(CALLER).unwrap()))
                    .with_context(
                        RequestContext::default()
                            .with_bearer(Some(CALLER.to_string()), Some(token.to_string())),
                    ),
            )
        };

        let (first, second) = futures::join!(read("first"), read("second"));
        assert_ok(&first);
        assert_ok(&second);

        let seen: std::collections::BTreeSet<_> = drain(&acp.seen).into_iter().collect();
        assert_eq!(
            seen,
            [Some("first".to_string()), Some("second".to_string())].into(),
            "a read checked access with another request's bearer, or none"
        );
    })
    .unwrap()
}
