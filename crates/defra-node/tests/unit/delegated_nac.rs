use acp::nac::{PermissionProvenance, PermissionVia};
use acp::NodePermission;
use defra_core::RequestContext;
use identity::Did;
use query::txn::SourcePermission;

use super::Node;

const OWNER: &str = "did:key:owner";
const ADMIN: &str = "did:key:admin";
const DELEGATE: &str = "did:key:delegate";
const HOLDER: &str = "did:key:holder";
const OTHER: &str = "did:key:other";
const AUDITOR: &str = "did:key:auditor";
const HELD: NodePermission = NodePermission::CollectionPatch;
const UNHELD: NodePermission = NodePermission::CollectionTruncate;

fn context(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

async fn holds(node: &Node, did: &str, permission: NodePermission) -> bool {
    node.nac_manager()
        .check_permission(&Did::new(did).unwrap(), permission)
        .await
        .unwrap()
}

async fn grant(node: &Node, by: &str, relation: &str, target: &str) -> crate::Result<bool> {
    node.add_nac_actor_relationship_as(&context(by), relation, target)
        .await
}

async fn revoke(node: &Node, by: &str, relation: &str, target: &str) -> crate::Result<bool> {
    node.delete_nac_actor_relationship_as(&context(by), relation, target)
        .await
}

fn refused(result: crate::Result<bool>) -> bool {
    match result {
        Ok(_) => false,
        Err(error) => {
            let text = error.to_string();
            assert!(
                text.contains("not authorized") || text.contains("relation not in resource"),
                "a refusal, not a failure: {text}"
            );
            true
        }
    }
}

/// The owner, one admin, a delegate holding the operations and HELD (and
/// DacBypass, which it still may not hand out), and a holder of HELD without
/// the operations.
async fn delegating_node() -> Node {
    let node = Node::builder()
        .with_node_acp_owner(OWNER)
        .build()
        .await
        .unwrap();
    grant(&node, OWNER, "admin", ADMIN).await.unwrap();
    grant(&node, OWNER, HELD.as_str(), ADMIN).await.unwrap();
    for permission in [
        NodePermission::NacRelationAdd,
        NodePermission::NacRelationDelete,
        HELD,
        NodePermission::DacBypass,
    ] {
        grant(&node, OWNER, permission.as_str(), DELEGATE)
            .await
            .unwrap();
    }
    grant(&node, OWNER, HELD.as_str(), HOLDER).await.unwrap();
    node
}

#[test]
fn delegate_grants_and_revokes_only_held_non_admin_permissions() {
    workers::block_on(async {
        let node = delegating_node().await;

        assert!(!grant(&node, DELEGATE, HELD.as_str(), OTHER).await.unwrap());
        assert!(holds(&node, OTHER, HELD).await);

        assert!(refused(
            grant(&node, DELEGATE, UNHELD.as_str(), OTHER).await
        ));
        for admin_class in [
            NodePermission::DacBypass,
            NodePermission::NacRelationAdd,
            NodePermission::NacDisable,
        ] {
            assert!(refused(
                grant(&node, DELEGATE, admin_class.as_str(), OTHER).await
            ));
            assert!(!holds(&node, OTHER, admin_class).await);
        }
        assert!(refused(grant(&node, DELEGATE, HELD.as_str(), "*").await));
        assert!(refused(grant(&node, DELEGATE, "admin", OTHER).await));
        assert!(refused(revoke(&node, DELEGATE, "admin", ADMIN).await));
        assert!(refused(grant(&node, DELEGATE, "owner", OTHER).await));
        assert!(refused(grant(&node, HOLDER, HELD.as_str(), OTHER).await));
        assert!(!holds(&node, "did:key:anyone", HELD).await);
        assert!(node
            .nac_manager()
            .is_admin(&Did::new(ADMIN).unwrap())
            .await
            .unwrap());

        assert!(refused(revoke(&node, DELEGATE, HELD.as_str(), ADMIN).await));
        assert!(refused(revoke(&node, DELEGATE, HELD.as_str(), OWNER).await));
        grant(&node, OWNER, UNHELD.as_str(), OTHER).await.unwrap();
        assert!(refused(
            revoke(&node, DELEGATE, UNHELD.as_str(), OTHER).await
        ));
        assert!(holds(&node, OTHER, UNHELD).await);

        assert!(revoke(&node, DELEGATE, HELD.as_str(), OTHER).await.unwrap());
        assert!(!holds(&node, OTHER, HELD).await);
        assert!(revoke(&node, DELEGATE, HELD.as_str(), HOLDER)
            .await
            .unwrap());
        assert!(!holds(&node, HOLDER, HELD).await);

        assert!(revoke(&node, OWNER, HELD.as_str(), DELEGATE).await.unwrap());
        assert!(refused(grant(&node, DELEGATE, HELD.as_str(), OTHER).await));
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn admin_relationship_authority_is_unchanged() {
    workers::block_on(async {
        let node = delegating_node().await;
        assert!(
            !grant(&node, ADMIN, NodePermission::DacBypass.as_str(), OTHER)
                .await
                .unwrap()
        );
        assert!(holds(&node, OTHER, NodePermission::DacBypass).await);
        assert!(!grant(&node, ADMIN, UNHELD.as_str(), "*").await.unwrap());
        assert!(holds(&node, "did:key:anyone", UNHELD).await);
        assert!(revoke(&node, ADMIN, UNHELD.as_str(), "*").await.unwrap());
        assert!(!grant(&node, ADMIN, "admin", OTHER).await.unwrap());
        assert!(revoke(&node, ADMIN, "admin", OTHER).await.unwrap());
        assert!(revoke(&node, ADMIN, HELD.as_str(), DELEGATE).await.unwrap());
        assert!(refused(grant(&node, OTHER, UNHELD.as_str(), HOLDER).await));
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

fn edge(
    permission: NodePermission,
    parent: Option<&str>,
    kind: &str,
    subject: &str,
    delete: bool,
) -> SourcePermission {
    SourcePermission {
        permission: permission.as_str().into(),
        parent_source: parent.map(str::to_owned),
        subject_kind: kind.into(),
        subject: subject.into(),
        delete,
    }
}

async fn stage(node: &Node, by: &str, edges: &[SourcePermission]) -> bool {
    let who = context(by);
    let handle = node.runner().begin_txn_as(&who, false).await.unwrap();
    let tx = who.clone().with_txn(Some(handle.as_str().parse().unwrap()));
    for item in edges {
        if node
            .change_nac_source_permission_as(&tx, item)
            .await
            .is_err()
        {
            node.runner().discard_txn_as(&who, &handle).await.unwrap();
            return false;
        }
    }
    node.runner().commit_txn_as(&who, &handle).await.unwrap();
    true
}

#[test]
fn delegate_source_edges_follow_the_same_rule() {
    workers::block_on(async {
        let node = delegating_node().await;
        let chain = [
            edge(HELD, None, "source", "tools:one", false),
            edge(HELD, Some("tools:one"), "actor", OTHER, false),
        ];
        assert!(stage(&node, DELEGATE, &chain).await);
        assert!(holds(&node, OTHER, HELD).await);

        assert!(
            !stage(
                &node,
                DELEGATE,
                &[edge(UNHELD, None, "source", "tools:two", false)]
            )
            .await
        );
        assert!(
            !stage(
                &node,
                DELEGATE,
                &[edge(
                    NodePermission::DacBypass,
                    None,
                    "source",
                    "tools:two",
                    false
                )]
            )
            .await
        );
        assert!(
            !stage(
                &node,
                HOLDER,
                &[edge(HELD, None, "source", "tools:two", false)]
            )
            .await
        );
        assert!(
            stage(
                &node,
                DELEGATE,
                &[edge(HELD, Some("tools:one"), "actor", ADMIN, false)]
            )
            .await
        );
        assert!(
            !stage(
                &node,
                DELEGATE,
                &[edge(HELD, Some("tools:one"), "actor", ADMIN, true)]
            )
            .await
        );
        assert!(
            stage(
                &node,
                OWNER,
                &[
                    edge(UNHELD, None, "source", "tools:one", false),
                    edge(UNHELD, Some("tools:one"), "actor", OTHER, false)
                ]
            )
            .await
        );
        assert!(
            !stage(
                &node,
                DELEGATE,
                &[edge(UNHELD, Some("tools:one"), "actor", OTHER, true)]
            )
            .await
        );
        assert!(holds(&node, OTHER, UNHELD).await);

        assert!(
            stage(
                &node,
                DELEGATE,
                &[edge(HELD, Some("tools:one"), "actor", OTHER, true)]
            )
            .await
        );
        assert!(!holds(&node, OTHER, HELD).await);
        assert!(
            stage(
                &node,
                DELEGATE,
                &[edge(HELD, None, "source", "tools:one", true)]
            )
            .await
        );
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

fn via(provenance: &[PermissionProvenance], permission: NodePermission) -> Vec<PermissionVia> {
    provenance
        .iter()
        .filter(|item| item.permission == permission)
        .map(|item| item.via.clone())
        .collect()
}

#[test]
fn explain_reports_provenance_behind_its_gate() {
    workers::block_on(async {
        let node = delegating_node().await;
        grant(&node, OWNER, NodePermission::NacStatus.as_str(), AUDITOR)
            .await
            .unwrap();
        assert!(
            stage(
                &node,
                OWNER,
                &[
                    edge(
                        NodePermission::CollectionGet,
                        None,
                        "source",
                        "tools:one",
                        false
                    ),
                    edge(
                        NodePermission::CollectionGet,
                        Some("tools:one"),
                        "source",
                        "context:one",
                        false
                    ),
                    edge(
                        NodePermission::CollectionGet,
                        Some("context:one"),
                        "actor",
                        HOLDER,
                        false
                    ),
                ],
            )
            .await
        );

        let owner = node
            .explain_nac_subject_as(&context(OWNER), OWNER)
            .await
            .unwrap();
        assert_eq!(owner.len(), NodePermission::all().len());
        assert!(owner.iter().all(|item| item.via == PermissionVia::Owner));

        let admin = node
            .explain_nac_subject_as(&context(ADMIN), ADMIN)
            .await
            .unwrap();
        assert_eq!(
            via(&admin, HELD),
            [PermissionVia::Admin, PermissionVia::Direct]
        );
        assert_eq!(
            via(&admin, NodePermission::DacBypass),
            [PermissionVia::Admin]
        );

        for requestor in [HOLDER, ADMIN, OWNER, AUDITOR] {
            let holder = node
                .explain_nac_subject_as(&context(requestor), HOLDER)
                .await
                .unwrap();
            assert_eq!(via(&holder, HELD), [PermissionVia::Direct]);
            assert_eq!(
                via(&holder, NodePermission::CollectionGet),
                [PermissionVia::Source {
                    path: vec!["tools:one".into(), "context:one".into()]
                }]
            );
            assert_eq!(holder.len(), 2, "{holder:?}");
        }

        for requestor in [DELEGATE, OTHER] {
            let denied = node
                .explain_nac_subject_as(&context(requestor), HOLDER)
                .await
                .unwrap_err();
            assert!(denied.to_string().contains("not authorized"), "{denied}");
        }
        assert!(node
            .explain_nac_subject_as(&RequestContext::default(), HOLDER)
            .await
            .is_err());

        node.disable_nac_as(&context(OWNER)).await.unwrap();
        for requestor in [DELEGATE, OTHER] {
            assert!(
                node.explain_nac_subject_as(&context(requestor), HOLDER)
                    .await
                    .is_err(),
                "disabling NAC must not disclose another subject's grants"
            );
        }
        assert!(node
            .explain_nac_subject_as(&context(AUDITOR), HOLDER)
            .await
            .is_ok());
        assert!(node
            .explain_nac_subject_as(&context(OWNER), "*")
            .await
            .is_err());
        let other = node
            .explain_nac_subject_as(&context(OTHER), OTHER)
            .await
            .unwrap();
        assert!(other.is_empty(), "{other:?}");
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

const LISTING_POLICY: &str = r#"
name: Relationship listing
resources:
  - name: users
    relations:
      - name: reader
      - name: admin
        manages: [reader]
    permissions:
      - name: read
        expr: reader + admin
      - name: update
      - name: delete
"#;
const DOC_OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const MANAGER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const READER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
const STRANGER: &str = "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG";

#[test]
fn document_relationship_listing_is_owner_or_manager_only() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let policy = node
            .add_dac_policy_as(&context(DOC_OWNER), LISTING_POLICY)
            .await
            .unwrap();
        node.add_collection(&format!(
            "type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}"
        ))
        .await
        .unwrap();
        let created = node
            .execute_request(
                query::QueryRequest::new("mutation { add_Users(input: {name: \"a\"}) { _docID } }")
                    .with_identity(Some(Did::new(DOC_OWNER).unwrap())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        node.add_dac_actor_relationship_as(&context(DOC_OWNER), "Users", &doc, "admin", MANAGER)
            .await
            .unwrap();
        node.add_dac_actor_relationship_as(&context(DOC_OWNER), "Users", &doc, "reader", READER)
            .await
            .unwrap();

        for requestor in [DOC_OWNER, MANAGER] {
            let mut listed: Vec<_> = node
                .list_dac_relationships_as(&context(requestor), "Users", &doc)
                .await
                .unwrap()
                .into_iter()
                .map(|item| (item.relation, item.subject))
                .collect();
            listed.sort();
            assert_eq!(
                listed,
                [
                    ("admin".to_owned(), MANAGER.to_owned()),
                    ("owner".to_owned(), DOC_OWNER.to_owned()),
                    ("reader".to_owned(), READER.to_owned()),
                ]
            );
        }
        for requestor in [READER, STRANGER] {
            assert!(node
                .list_dac_relationships_as(&context(requestor), "Users", &doc)
                .await
                .is_err());
        }
        assert!(node
            .list_dac_relationships_as(&RequestContext::default(), "Users", &doc)
            .await
            .is_err());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// The facts the native gate observes for `requestor` changing `relation` on
/// `target`, read through the public NAC API, in Lean's `Facts` shape.
async fn observe(
    node: &Node,
    requestor: &str,
    relation: &str,
    target: &str,
    delete: bool,
) -> serde_json::Value {
    let nac = node.nac_manager();
    let did = Did::new(requestor).unwrap();
    let operation = if delete {
        NodePermission::NacRelationDelete
    } else {
        NodePermission::NacRelationAdd
    };
    let holds_permission = match NodePermission::parse(relation) {
        Some(permission) => nac.check_permission(&did, permission).await.unwrap(),
        None => false,
    };
    let privileged = target == "*" || nac.is_admin(&Did::new(target).unwrap()).await.unwrap();
    serde_json::json!({
        "requestorAdmin": nac.is_admin(&did).await.unwrap(),
        "relation": relation,
        "holdsOperation": nac.check_permission(&did, operation).await.unwrap(),
        "holdsPermission": holds_permission,
        "wildcardSubject": target == "*",
        "privilegedSubject": privileged,
        "delete": delete,
    })
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn delegation_matches_executed_lean_model() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let observations = workers::block_on(async {
        let node = delegating_node().await;
        grant(&node, OWNER, UNHELD.as_str(), OTHER).await.unwrap();
        let mut observations = Vec::new();
        for (requestor, relation, target, delete) in [
            (DELEGATE, HELD.as_str(), OTHER, false),
            (DELEGATE, UNHELD.as_str(), HOLDER, false),
            (DELEGATE, NodePermission::DacBypass.as_str(), OTHER, false),
            (
                DELEGATE,
                NodePermission::NacRelationAdd.as_str(),
                OTHER,
                false,
            ),
            (DELEGATE, HELD.as_str(), "*", false),
            (DELEGATE, "admin", OTHER, false),
            (DELEGATE, "admin", ADMIN, true),
            (DELEGATE, HELD.as_str(), HOLDER, true),
            (DELEGATE, HELD.as_str(), ADMIN, true),
            (DELEGATE, UNHELD.as_str(), OTHER, true),
            (HOLDER, HELD.as_str(), OTHER, false),
            (OTHER, UNHELD.as_str(), HOLDER, false),
            (ADMIN, NodePermission::DacBypass.as_str(), OTHER, false),
            (ADMIN, HELD.as_str(), "*", false),
            (ADMIN, "admin", OTHER, false),
            (OWNER, UNHELD.as_str(), OTHER, true),
        ] {
            let facts = observe(&node, requestor, relation, target, delete).await;
            let result = if delete {
                revoke(&node, requestor, relation, target).await
            } else {
                grant(&node, requestor, relation, target).await
            };
            let accepted = result.is_ok();
            if accepted && delete {
                grant(&node, OWNER, relation, target).await.unwrap();
            } else if accepted {
                revoke(&node, OWNER, relation, target).await.unwrap();
            }
            observations.push((facts, accepted));
        }
        node.shutdown().await.unwrap();
        observations
    })
    .unwrap();
    let inputs: Vec<_> = observations
        .iter()
        .map(|(input, _)| input.clone())
        .collect();
    let mut lean = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "NodeAcpDelegation.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(lean.stdin.take().unwrap(), "{}", serde_json::json!(inputs)).unwrap();
    let output = lean.wait_with_output().unwrap();
    assert!(output.status.success());
    let model: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(model.as_array().unwrap().len(), observations.len());
    let mut admitted = 0;
    for (modeled, (_, actual)) in model.as_array().unwrap().iter().zip(observations) {
        assert_eq!(modeled["admits"].as_bool().unwrap(), actual, "{modeled}");
        admitted += usize::from(actual);
    }
    assert!(admitted > 0 && admitted < model.as_array().unwrap().len());
}

#[cfg(feature = "http")]
fn http_identity() -> identity::RawIdentity {
    identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap()
}

#[cfg(feature = "http")]
fn bearer(identity: &identity::RawIdentity, audience: &str) -> String {
    let token = identity::new_token(
        identity,
        std::time::Duration::from_secs(60),
        Some(audience.into()),
        None,
    )
    .unwrap();
    format!("Bearer {}", String::from_utf8(token).unwrap())
}

#[cfg(feature = "http")]
#[test]
fn http_inspection_routes_apply_the_embedded_gates() {
    use identity::Identity as _;

    edge::testing::edge()
        .block_on(async {
            let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
            let owner = http_identity();
            let owner_did = owner.did().unwrap().to_string();
            let public = owner.public_key_bytes();
            defra_core::signing::store_identity(
                &owner_did,
                defra_core::signing::SigningConfig {
                    key_type: defra_core::signing::SigningKeyType::Ed25519,
                    private_key_bytes: owner.private_key_bytes(),
                    public_key_bytes: public.clone(),
                    public_key_hex: hex::encode(public),
                    remote_signer: None,
                    signing_authorization: None,
                },
            )
            .expect("the store takes the identity");
            let holder = http_identity();
            let holder_did = holder.did().unwrap().to_string();
            let stranger = http_identity();
            let stranger_did = stranger.did().unwrap().to_string();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let node = Node::builder()
                .with_node_identity_did(&owner_did)
                .with_node_acp_enabled()
                .with_http(crate::HttpConfig::with_listener(listener).unwrap())
                .build()
                .await
                .unwrap();
            // Go reads the collection before it writes a document to it.
            for permission in [
                HELD,
                NodePermission::CollectionGet,
                NodePermission::DocumentUpdate,
                NodePermission::DacRelationAdd,
            ] {
                grant(&node, &owner_did, permission.as_str(), &holder_did)
                    .await
                    .unwrap();
            }
            let policy = node
                .add_dac_policy_as(&context(&owner_did), LISTING_POLICY)
                .await
                .unwrap();
            node.add_collection(&format!(
                "type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}"
            ))
            .await
            .unwrap();
            let created = node
                .execute_request(
                    query::QueryRequest::new(
                        "mutation { add_Users(input: {name: \"a\"}) { _docID } }",
                    )
                    .with_identity(Some(Did::new(&holder_did).unwrap())),
                )
                .await;
            assert!(!created.has_errors(), "{:?}", created.errors);
            let doc = created.data.unwrap()["add_Users"][0]["_docID"]
                .as_str()
                .unwrap()
                .to_owned();
            let client = net::http::Client::builder(edge::testing::edge())
                .no_proxy()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap();
            let audience = address.to_string();
            let get = |who: &identity::RawIdentity, path: String| {
                client
                    .get(format!("http://{address}{path}"))
                    .header("Authorization", bearer(who, &audience))
                    .send()
            };

            let explain = format!("/api/v0/acp/node/explain?subject={holder_did}");
            for (who, did, allowed) in [
                (&holder, &holder_did, true),
                (&owner, &owner_did, true),
                (&stranger, &stranger_did, false),
            ] {
                let response = get(who, explain.clone()).await.unwrap();
                let embedded = node
                    .explain_nac_subject_as(&context(did), &holder_did)
                    .await;
                assert_eq!(response.status().is_success(), allowed, "{did}");
                assert_eq!(embedded.is_ok(), allowed, "{did}");
                if allowed {
                    let served: Vec<PermissionProvenance> = response.json().await.unwrap();
                    assert_eq!(served, embedded.unwrap());
                    assert_eq!(via(&served, HELD), [PermissionVia::Direct]);
                }
            }

            let listing =
                format!("/api/v0/acp/document/relationships?collection=Users&docID={doc}");
            for (who, did, allowed) in [
                (&holder, &holder_did, true),
                (&stranger, &stranger_did, false),
            ] {
                let response = get(who, listing.clone()).await.unwrap();
                let embedded = node
                    .list_dac_relationships_as(&context(did), "Users", &doc)
                    .await;
                assert_eq!(response.status().is_success(), allowed, "{did}");
                assert_eq!(embedded.is_ok(), allowed, "{did}");
                if allowed {
                    let served: Vec<acp::DocumentRelationship> = response.json().await.unwrap();
                    assert_eq!(served, embedded.unwrap());
                    assert_eq!(
                        served,
                        [acp::DocumentRelationship {
                            relation: "owner".into(),
                            subject: holder_did.clone()
                        }]
                    );
                }
            }
            node.shutdown().await.unwrap();
        })
        .unwrap();
}
