use std::sync::Arc;

use acp::nac::{NodeACP, NodePermission};
use document::{DocID, NormalValue};
use identity::Did;
use storage::namespace::Namespace;

use crate::{Node, QueryRequest};

const AGENT: &str = "did:key:agent";

struct Projector;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for Projector {
    async fn reconcile(
        &self,
        _context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let view = context.view;
        if collection != "ProjectedTool" || doc_id.is_empty() {
            return Ok(());
        }
        let system = view.sibling(Namespace::Systemstore);
        let collection =
            db::collection::loader::load_collection_from_systemstore(&system, collection)
                .await
                .map_err(|e| storage::corekv::Error::Other(e.to_string()))?
                .ok_or_else(|| storage::corekv::Error::Other("missing collection".into()))?;
        let id =
            DocID::from_string(doc_id).map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        let doc = collection
            .get_by_doc_id(view, &system, &id)
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        let enabled = doc.as_ref().is_some_and(|doc| {
            matches!(
                doc.get("enabled"),
                Some(NormalValue::Bool(true) | NormalValue::NillableBool(Some(true)))
            )
        });
        let source = format!("tool:{doc_id}");
        let nac = NodeACP::new(Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view.sibling(Namespace::Acpstore)),
        ));
        nac.load()
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        let owner = context.node_did;
        if enabled {
            nac.change_source_permission(
                owner,
                NodePermission::CollectionPatch,
                Some(&source),
                "actor",
                AGENT,
                false,
            )
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        }
        nac.change_source_permission(
            owner,
            NodePermission::CollectionPatch,
            None,
            "source",
            &source,
            !enabled,
        )
        .await
        .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        Ok(())
    }
}

fn request(text: String) -> QueryRequest {
    QueryRequest::new(text)
}

async fn allowed(node: &Node) -> bool {
    node.nac_manager()
        .check_permission(&Did::new(AGENT).unwrap(), NodePermission::CollectionPatch)
        .await
        .unwrap()
}

#[test]
fn raw_document_mutations_stage_sources_in_the_same_transaction() {
    workers::block_on(async {
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_enabled()
            .with_document_projector(Arc::new(Projector))
            .build().await.unwrap();
        let owner = defra_core::RequestContext::default().with_identity(Some(node.node_identity_did().unwrap().to_owned()));
        node.add_collection_as(&owner, "type ProjectedTool { enabled: Boolean, name: String }").await.unwrap();
        let created = node.execute_request(request("mutation { create_ProjectedTool(input:{enabled:true, name:\"one\"}) { _docID } }".into())).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let id = created.data.unwrap()["add_ProjectedTool"][0]["_docID"].as_str().unwrap().to_owned();
        let second = node.execute_request(request("mutation { create_ProjectedTool(input:{enabled:true, name:\"two\"}) { _docID } }".into())).await;
        assert!(!second.has_errors(), "{:?}", second.errors);
        let second_id = second.data.unwrap()["add_ProjectedTool"][0]["_docID"].as_str().unwrap().to_owned();
        assert!(allowed(&node).await);

        let tx = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let disabled = node.execute_request_in_txn(request(format!("mutation {{ update_ProjectedTool(docID:\"{id}\", input:{{enabled:false}}) {{ _docID }} }}")), &tx).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(allowed(&node).await);
        node.runner().discard_txn_as(&owner, &tx).await.unwrap();
        assert!(allowed(&node).await);

        node.add_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        let disabled = node.execute_request(request(format!("mutation {{ update_ProjectedTool(docID:\"{id}\", input:{{enabled:false}}) {{ _docID }} }}"))).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(allowed(&node).await);
        node.delete_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        assert!(allowed(&node).await, "the other tool source remains");
        node.add_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        let disabled = node.execute_request(request(format!("mutation {{ update_ProjectedTool(docID:\"{second_id}\", input:{{enabled:false}}) {{ _docID }} }}"))).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(allowed(&node).await, "the direct grant remains");
        node.delete_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        assert!(!allowed(&node).await);
        node.shutdown().await.unwrap();
    }).unwrap();
}

struct RefusingProjector;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for RefusingProjector {
    async fn reconcile(
        &self,
        _context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let view = context.view;
        if collection == "ProjectedTool" && !doc_id.is_empty() {
            view.sibling(Namespace::Acpstore)
                .set(b"test-projection", b"staged")
                .await?;
            return Err(storage::corekv::Error::Other("projection rejected".into()));
        }
        Ok(())
    }
}

#[test]
fn refused_projection_rolls_back_document_and_staged_native_changes() {
    workers::block_on(async {
        let node = Node::builder()
            .enable_signing()
            .with_document_projector(Arc::new(RefusingProjector))
            .build()
            .await
            .unwrap();
        node.add_collection("type ProjectedTool { enabled: Boolean }")
            .await
            .unwrap();
        let result = node
            .execute("mutation { create_ProjectedTool(input:{enabled:true}) { _docID } }")
            .await;
        assert!(result.has_errors());
        let docs = node.execute("{ ProjectedTool { _docID } }").await;
        assert!(!docs.has_errors(), "{:?}", docs.errors);
        assert!(docs.data.unwrap()["ProjectedTool"]
            .as_array()
            .unwrap()
            .is_empty());
        let txn = node.database().new_txn(true).await.unwrap();
        assert!(txn
            .acpstore()
            .unwrap()
            .get(b"test-projection")
            .await
            .unwrap()
            .is_none());
        txn.discard().unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

struct ForeignOwnerProjector;

struct ScopedOwnerProjector {
    reconcile_docs: Arc<regolith::sync::Atom<Vec<String>>>,
    rules: Vec<acp::projection::Rule>,
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for ScopedOwnerProjector {
    fn projection_rules(&self) -> &[acp::projection::Rule] {
        &self.rules
    }
    async fn reconcile(
        &self,
        context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        let docs = Vec::clone(&self.reconcile_docs.load());
        for doc in docs {
            context
                .change_projected_relationship(
                    "ScopedProjected",
                    &doc,
                    "reader",
                    acp::Subject::entity(Did::new("did:key:reconciled").unwrap()),
                    false,
                )
                .await?;
            let outside_scope = context
                .change_projected_relationship(
                    "ScopedProjected",
                    &doc,
                    "reader_manager",
                    acp::Subject::entity(Did::new("did:key:reader").unwrap()),
                    false,
                )
                .await;
            assert!(matches!(
                outside_scope,
                Err(storage::corekv::Error::ProjectionPermissionDenied)
            ));
        }
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        if collection == "ScopedProjected" && !doc_id.is_empty() {
            context
                .change_projected_relationship(
                    collection,
                    doc_id,
                    "reader",
                    acp::Subject::entity(Did::new("did:key:reader").unwrap()),
                    false,
                )
                .await?;
        }
        Ok(())
    }
}

#[test]
fn scoped_projection_uses_owner_authority_without_read_bypass() {
    use crypto::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    workers::block_on(async {
        let owner_key = crypto::generate_ed25519().unwrap();
        let owner = identity::RawIdentity::from_private_key(owner_key.clone()).unwrap();
        let node_key = crypto::generate_ed25519().unwrap();
        let public = node_key.public_key();
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: node_key.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let reconcile_docs = Arc::new(regolith::sync::Atom::new(Vec::new()));
        let node = Node::builder()
            .with_signing_key(crate::SigningKey::Ed25519(signing.private_key_bytes.clone()))
            .with_node_acp_owner(owner.did().unwrap().to_string())
            .with_document_projector(Arc::new(ScopedOwnerProjector {
                reconcile_docs: Arc::clone(&reconcile_docs),
                rules: vec![acp::projection::Rule {
                    collection: "ScopedProjected".into(),
                    resource: "records".into(),
                    relation: "reader".into(),
                    target_kind: acp::projection::TargetKind::Entity,
                }],
            }))
            .build()
            .await
            .unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let admin = defra_core::RequestContext::default()
            .with_identity(Some(owner.did().unwrap().to_string()));
        for permission in [NodePermission::DocumentUpdate, NodePermission::DocumentRead,
            NodePermission::CollectionGet, NodePermission::DacRelationAdd] {
            node.add_nac_actor_relationship_as(&admin, permission.as_str(), &node_did).await.unwrap();
        }
        assert!(!node.nac_manager().check_permission(&Did::new(&node_did).unwrap(), NodePermission::DacBypass).await.unwrap());
        let policy = node.add_dac_policy_as(&admin, "name: Scoped projection\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n      - name: reader_manager\n        types: [actor]\n        manages: [reader]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection_as(&admin, &format!("type ScopedProjected @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let result = node.execute_request(crate::EmbeddedSigner::from_config(signing).unwrap().attach(
            QueryRequest::new("mutation { create_ScopedProjected(input:{name:\"one\"}) { _docID } }")
                .with_identity(Some(Did::new(&node_did).unwrap())))).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let doc_id = result.data.unwrap()["add_ScopedProjected"][0]["_docID"].as_str().unwrap().to_owned();
        assert!(node.document_acp().check_named_permission(&Did::new("did:key:reader").unwrap(), "read", &policy, "records", &doc_id).await.unwrap());
        reconcile_docs.store(vec![doc_id.clone()]);
        node.database().reconcile_document_projector().await.unwrap();
        assert!(node.document_acp().check_named_permission(&Did::new("did:key:reconciled").unwrap(), "read", &policy, "records", &doc_id).await.unwrap());
        let owner_public = owner_key.public_key();
        let owner_signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: owner_key.raw_owned(),
            public_key_bytes: owner_public.raw_owned(),
            public_key_hex: hex::encode(owner_public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let foreign = node.execute_request(crate::EmbeddedSigner::from_config(owner_signing).unwrap().attach(
            QueryRequest::new("mutation { create_ScopedProjected(input:{name:\"foreign\"}) { _docID } }")
                .with_identity(Some(owner.did().unwrap())))).await;
        assert!(!foreign.has_errors(), "{:?}", foreign.errors);
        let foreign_doc = foreign.data.unwrap()["add_ScopedProjected"][0]["_docID"].as_str().unwrap().to_owned();
        reconcile_docs.store(vec![foreign_doc.clone()]);
        assert!(node.database().reconcile_document_projector().await.is_err());
        assert!(!node.document_acp().check_named_permission(&Did::new("did:key:reconciled").unwrap(), "read", &policy, "records", &foreign_doc).await.unwrap());
        node.add_dac_actor_relationship_as(&admin, "ScopedProjected", &foreign_doc, "reader_manager", &node_did).await.unwrap();
        node.database().reconcile_document_projector().await.unwrap();
        assert!(node.document_acp().check_named_permission(&Did::new("did:key:reconciled").unwrap(), "read", &policy, "records", &foreign_doc).await.unwrap());
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for ForeignOwnerProjector {
    async fn reconcile(
        &self,
        _context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        if collection != "ForeignProjected" || doc_id.is_empty() {
            return Ok(());
        }
        let system = context.view.sibling(Namespace::Systemstore);
        let collection =
            db::collection::loader::load_collection_from_systemstore(&system, collection)
                .await
                .map_err(|e| storage::corekv::Error::Other(e.to_string()))?
                .ok_or_else(|| storage::corekv::Error::Other("missing projected schema".into()))?;
        let policy = collection
            .schema()
            .policy
            .as_ref()
            .ok_or_else(|| storage::corekv::Error::Other("missing projected policy".into()))?;
        let acp_view = context.view.sibling(Namespace::Acpstore);
        let scoped = context
            .document_acp
            .bind_transaction(acp_view.clone())
            .ok_or_else(|| storage::corekv::Error::Other("configured ACP cannot bind".into()))?;
        let owner = scoped
            .get_doc_owner(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?
            .ok_or_else(|| storage::corekv::Error::Other("creator not registered".into()))?;
        if &owner == context.node_did {
            return Err(storage::corekv::Error::Other(
                "expected foreign document owner".into(),
            ));
        }
        context
            .document_acp
            .change_node_admin_relationship(
                acp_view,
                context.node_did,
                acp::Subject::wildcard(),
                &policy.id,
                &policy.resource_name,
                doc_id,
                "reader",
                false,
            )
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        Ok(())
    }
}

#[test]
fn local_signed_foreign_creator_is_registered_before_projector_admin_grant() {
    use crypto::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    workers::block_on(async {
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let user = identity::RawIdentity::from_private_key(private).unwrap();
        let user_did = user.did().unwrap();
        let node = Node::builder().enable_signing().with_node_acp_enabled()
            .with_document_projector(Arc::new(ForeignOwnerProjector)).build().await.unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let owner = defra_core::RequestContext::default().with_identity(Some(node_did));
        for permission in [NodePermission::DocumentUpdate, NodePermission::DocumentRead, NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&owner, permission.as_str(), user_did.as_str()).await.unwrap();
        }
        let policy = node.add_dac_policy_as(&owner, "name: Projection fixture\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection_as(&owner, &format!("type ForeignProjected @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let response = node.execute_request(crate::EmbeddedSigner::from_config(signing).unwrap().attach(QueryRequest::new("mutation { create_ForeignProjected(input:{name:\"one\"}) { _docID } }")
                .with_identity(Some(user_did.clone())))).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let doc_id = response.data.unwrap()["add_ForeignProjected"][0]["_docID"].as_str().unwrap().to_owned();
        assert_eq!(node.document_acp().get_doc_owner(&policy, "records", &doc_id).await.unwrap(), Some(user_did));
        assert!(node.document_acp().check_named_permission(&Did::new("did:key:reader").unwrap(), "read", &policy, "records", &doc_id).await.unwrap());
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn foreign_owner_projection_without_node_bypass_is_refused() {
    use crypto::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    workers::block_on(async {
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let user = identity::RawIdentity::from_private_key(private).unwrap();
        let user_did = user.did().unwrap();
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_owner(user_did.to_string())
            .with_document_projector(Arc::new(ForeignOwnerProjector))
            .build().await.unwrap();
        let admin = defra_core::RequestContext::default().with_identity(Some(user_did.to_string()));
        let node_did = Did::new(node.node_identity_did().unwrap()).unwrap();
        assert!(!node.nac_manager().check_permission(&node_did, NodePermission::DacBypass).await.unwrap());
        let policy = node.add_dac_policy_as(&admin, "name: Projection fixture\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection_as(&admin, &format!("type ForeignProjected @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let response = node.execute_request(crate::EmbeddedSigner::from_config(signing).unwrap().attach(
            QueryRequest::new("mutation { create_ForeignProjected(input:{name:\"one\"}) { _docID } }")
                .with_identity(Some(user_did)))).await;
        assert!(
            format!("{:?}", response.errors).contains("native node-administrator relationship authority is required"),
            "unexpected error: {:?}", response.errors
        );
        node.shutdown().await.unwrap();
    }).unwrap();
}

/// This projector computes only the source changes its document needs.
struct ActorAuthorizedProjector;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for ActorAuthorizedProjector {
    async fn reconcile(&self, _: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        name: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        use acp::{Subject, ZanzibarStore};
        use storage::corekv::Error;
        if name != "ActorProjectedTool" || doc_id.is_empty() {
            return Ok(());
        }
        let view = context.view;
        let system = view.sibling(Namespace::Systemstore);
        let collection = db::collection::loader::load_collection_from_systemstore(&system, name)
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .ok_or_else(|| Error::Other("missing fixture collection".into()))?;
        let id = DocID::from_string(doc_id).map_err(|e| Error::Other(e.to_string()))?;
        let doc = collection
            .get_by_doc_id(view, &system, &id)
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        let enabled = doc.as_ref().is_some_and(|doc| {
            matches!(
                doc.get("enabled"),
                Some(NormalValue::Bool(true) | NormalValue::NillableBool(Some(true)))
            )
        });
        let source = format!("actor-projection:{doc_id}");
        let source_object = format!("source.{}", hex::encode(source.as_bytes()));
        let store =
            acp::zanzibar::store::TransactionZanzibarStore::new(view.sibling(Namespace::Acpstore));
        let relation = NodePermission::CollectionPatch.as_str();
        let roots = store
            .get_relation_subjects(
                acp::NODE_POLICY_ID,
                acp::NODE_RESOURCE_NAME,
                acp::NODE_OBJECT_ID,
                relation,
            )
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        let root = Subject::entity_set(acp::NODE_RESOURCE_NAME, source_object.clone(), relation);
        if enabled && !roots.contains(&root) {
            let members = store
                .get_relation_subjects(
                    acp::NODE_POLICY_ID,
                    acp::NODE_RESOURCE_NAME,
                    &source_object,
                    relation,
                )
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
            if !members.contains(&Subject::entity(Did::new(AGENT).unwrap())) {
                context
                    .change_source_permission(
                        NodePermission::CollectionPatch,
                        Some(&source),
                        "actor",
                        AGENT,
                        false,
                    )
                    .await?;
            }
            context
                .change_source_permission(
                    NodePermission::CollectionPatch,
                    None,
                    "source",
                    &source,
                    false,
                )
                .await?;
        } else if !enabled && roots.contains(&root) {
            context
                .change_source_permission(
                    NodePermission::CollectionPatch,
                    None,
                    "source",
                    &source,
                    true,
                )
                .await?;
        }
        Ok(())
    }
}

#[test]
fn source_projection_authorizes_the_actual_mutation_actor_atomically() {
    use crypto::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    workers::block_on(async {
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let user = identity::RawIdentity::from_private_key(private).unwrap().did().unwrap();
        let signer = crate::EmbeddedSigner::from_config(signing).unwrap();
        let node = Node::builder().enable_signing().with_node_acp_enabled()
            .with_document_projector(Arc::new(ActorAuthorizedProjector)).build().await.unwrap();
        let owner = defra_core::RequestContext::default()
            .with_identity(Some(node.node_identity_did().unwrap().to_owned()));
        for permission in [NodePermission::DocumentUpdate, NodePermission::DocumentRead, NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&owner, permission.as_str(), user.as_str()).await.unwrap();
        }
        node.add_collection_as(&owner, "type ActorProjectedTool { enabled: Boolean, name: String }").await.unwrap();
        let created = node.execute_request(request("mutation { create_ActorProjectedTool(input:{enabled:false,name:\"before\"}) {_docID} }".into())).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let id = created.data.unwrap()["add_ActorProjectedTool"][0]["_docID"].as_str().unwrap().to_owned();
        let user_request = |body: String| QueryRequest::new(body).with_identity(Some(user.clone()));
        let update = |enabled: bool| format!("mutation {{ update_ActorProjectedTool(docID:\"{id}\",input:{{enabled:{enabled}}}) {{_docID}} }}");
        let rejected = node.execute_request(signer.attach(user_request(update(true)))).await;
        assert!(rejected.has_errors(), "non-admin source grant must be rejected");
        assert_eq!(rejected.errors[0].code(), Some(query::executor::PERMISSION_DENIED_ERROR_CODE));
        assert!(!allowed(&node).await);
        let before = node.execute_request(request(format!("{{ ActorProjectedTool(docID:\"{id}\") {{ enabled }} }}"))).await;
        assert_eq!(before.data.unwrap()["ActorProjectedTool"][0]["enabled"], false);
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let enabled = node.execute_request_in_txn(request(update(true)), &handle).await;
        assert!(!enabled.has_errors(), "{:?}", enabled.errors);
        assert!(!allowed(&node).await);
        let denied = node.execute_request_in_txn(signer.attach(user_request(update(false))), &handle).await;
        assert!(denied.has_errors(), "non-admin source removal must be rejected");
        assert_eq!(denied.errors[0].code(), Some(query::executor::PERMISSION_DENIED_ERROR_CODE));
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node).await);
        let harmless = node.execute_request(signer.attach(user_request(format!("mutation {{ update_ActorProjectedTool(docID:\"{id}\",input:{{name:\"after\"}}) {{_docID}} }}")))).await;
        assert!(!harmless.has_errors(), "{:?}", harmless.errors);
        let after = node.execute_request(request(format!("{{ ActorProjectedTool(docID:\"{id}\") {{ enabled name }} }}"))).await;
        assert_eq!(after.data.unwrap()["ActorProjectedTool"][0]["enabled"], true);
        let disabled = node.execute_request(request(update(false))).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(!allowed(&node).await);
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn revoked_source_manager_conflicts_with_the_staged_document_and_grant() {
    use crypto::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    workers::block_on(async {
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let actor = identity::RawIdentity::from_private_key(private).unwrap().did().unwrap();
        let signer = crate::EmbeddedSigner::from_config(signing).unwrap();
        let node = Node::builder().enable_signing().with_node_acp_enabled()
            .with_document_projector(Arc::new(ActorAuthorizedProjector)).build().await.unwrap();
        let owner = defra_core::RequestContext::default()
            .with_identity(Some(node.node_identity_did().unwrap().to_owned()));
        for permission in [NodePermission::DocumentUpdate, NodePermission::DocumentRead, NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&owner, permission.as_str(), actor.as_str()).await.unwrap();
        }
        node.add_nac_actor_relationship_as(&owner, "admin", actor.as_str()).await.unwrap();
        node.add_collection_as(&owner, "type ActorProjectedTool { enabled: Boolean, name: String }").await.unwrap();
        let created = node.execute_request(request("mutation { create_ActorProjectedTool(input:{enabled:false,name:\"before\"}) {_docID} }".into())).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let id = created.data.unwrap()["add_ActorProjectedTool"][0]["_docID"].as_str().unwrap().to_owned();
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let mutation = QueryRequest::new(format!("mutation {{ update_ActorProjectedTool(docID:\"{id}\",input:{{enabled:true}}) {{_docID}} }}"))
            .with_identity(Some(actor.clone()));
        let staged = node.execute_request_in_txn(signer.attach(mutation), &handle).await;
        assert!(!staged.has_errors(), "{:?}", staged.errors);
        assert!(!allowed(&node).await);
        assert!(node.delete_nac_actor_relationship_as(&owner, "admin", actor.as_str()).await.unwrap());
        assert!(node.runner().commit_txn_as(&owner, &handle).await.is_err(), "revoked source authority must conflict at commit");
        assert!(!allowed(&node).await);
        let current = node.execute_request(request(format!("{{ ActorProjectedTool(docID:\"{id}\") {{ enabled }} }}"))).await;
        assert!(!current.has_errors(), "{:?}", current.errors);
        assert_eq!(current.data.unwrap()["ActorProjectedTool"][0]["enabled"], false);
        node.shutdown().await.unwrap();
    }).unwrap();
}

/// An identity-only node acts as no one by default, but its trusted projector
/// still acts as the node's DID, so the node builds.
#[test]
fn an_identity_only_node_gives_its_projector_the_node_did() {
    workers::block_on(async {
        let node = match Node::builder()
            .enable_signing()
            .with_identity_only()
            .with_document_projector(Arc::new(Projector))
            .build()
            .await
        {
            Ok(node) => node,
            Err(error) => panic!("an identity-only node with a projector must build: {error}"),
        };
        assert!(node.node_identity_did().is_some());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
