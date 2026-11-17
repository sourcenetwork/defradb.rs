//! Verified updates and deletes against a real chain: admitted with no
//! credential on the signer's permission as the chain proves it, refused for
//! a signer without it or whose grant the owner revoked, and refused when the
//! chain cannot be reached. The node never holds the signer's key.

use std::sync::Arc;

use acp::{DocumentACP as _, RemotePolicies as _};
use db::block::builder::{
    ClientDeleteSignatures, ClientUpdateSignatures, ClientUpdateSnapshot, VerifiedClientMutation,
};
use defra_core::RequestContext;
use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraPolicy, Node, QueryRequest, QueryResponse, SignerCredential,
    VeraRsActor, VeraRsConfig,
};
use serde_json::json;
use vera::vera_rs::{caller_delegation, DelegationSigner, Ed25519Signer};
use vera::PolicyAction;

use super::verified::{Attempt, Deployed, User, LIFETIME};
use super::{names, node_serving, read_users, users, POLICY};

/// What a verified write does to the document.
#[derive(Clone, Copy)]
enum Edit {
    Update(&'static str),
    Delete,
}

/// The document's history as `reader` reads it now.
async fn snapshot(node: &Node, reader: &User, doc: &str) -> ClientUpdateSnapshot {
    let history = node
        .execute_request(
            QueryRequest::new("query($docID:[String!]!) { _commits(docID:$docID) { cid docID fieldName height delta collectionVersionId links { cid fieldName } heads { cid } signature { type identity value } } }")
                .with_variables(json!({ "docID": [doc] }))
                .with_identity(Some(reader.did.clone())),
        )
        .await;
    assert!(!history.has_errors(), "{:?}", history.errors);
    ClientUpdateSnapshot::from_commits(doc, history.data.unwrap()["_commits"].as_array().unwrap())
        .unwrap()
}

/// `signer`'s verified `edit` of `doc` on the history `snapshot`, handed to
/// the node with no credential.
async fn edit(
    node: &Node,
    signer: &User,
    doc: &str,
    snapshot: &ClientUpdateSnapshot,
    edit: Edit,
) -> (defra_node::TransactionHandle, QueryResponse) {
    let schema = node.get_collection("Users").unwrap().unwrap();
    let crdts = storage::Crdts::default();
    let (graph, request) = match edit {
        Edit::Update(name) => {
            let patch = json!({ "name": name });
            let signed = ClientUpdateSignatures::from_input(
                &crdts,
                &schema,
                doc,
                &patch,
                snapshot,
                &signer.blocks,
            )
            .unwrap();
            let graph = VerifiedClientMutation::verify_update(
                &crdts,
                &schema,
                doc,
                &patch,
                snapshot,
                signed.evidence(),
            )
            .unwrap();
            let request = QueryRequest::new("mutation($id:ID!, $input:UsersMutationInputArg!) { update_Users(docID:$id,input:$input) {_docID} }")
                .with_variables(json!({ "id": doc, "input": patch }));
            (graph, request)
        }
        Edit::Delete => {
            let signed =
                ClientDeleteSignatures::from_snapshot(&schema, doc, snapshot, &signer.blocks)
                    .unwrap();
            let graph =
                VerifiedClientMutation::verify_delete(&schema, doc, snapshot, signed.evidence())
                    .unwrap();
            let request =
                QueryRequest::new("mutation($id:ID!) { delete_Users(docIDs:[$id]) {_docID} }")
                    .with_variables(json!({ "id": doc }));
            (graph, request)
        }
    };
    let txn = node.begin_transaction(false).await.unwrap();
    let response = node
        .execute_verified_mutation_in_txn(
            request.with_identity(Some(signer.did.clone())),
            &txn,
            &graph,
            SignerCredential::None,
        )
        .await;
    (txn, response)
}

/// The staged edit was admitted, and commits.
async fn admitted(node: &Node, (txn, response): (defra_node::TransactionHandle, QueryResponse)) {
    assert!(!response.has_errors(), "{:?}", response.errors);
    node.commit_transaction(&txn).await.expect("commit");
}

/// The edit was refused before it wrote anything: there is nothing to
/// commit.
async fn refused(node: &Node, (txn, response): (defra_node::TransactionHandle, QueryResponse)) {
    assert!(response.has_errors(), "an edit was admitted");
    assert!(node.commit_transaction(&txn).await.is_err());
}

/// `user`'s document called `name`, created under a delegation bound to its
/// registration and registered on the chain with `user` as owner.
async fn registered(on: &Deployed, user: &User, name: &str, nonce: u8) -> String {
    let Attempt { doc, txn, response } = on
        .create(user, name, |doc| async move {
            SignerCredential::RequestBound(
                on.bound(user, &doc, PolicyAction::RegisterObject, nonce)
                    .await,
            )
        })
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    on.node.commit_transaction(&txn).await.expect("commit");
    assert_eq!(
        on.owner_on_chain(&doc).await.as_deref(),
        Some(user.did.as_str())
    );
    doc
}

/// A user creates a document under a request-bound delegation, then updates
/// and deletes it with verified writes and no credential, its key never held
/// by the node. Another user without the permission is refused before
/// anything is written.
#[test]
fn a_verified_update_and_delete_are_admitted_on_the_chains_permission() {
    workers::block_on(async {
        let on = Deployed::start(9415).await;
        let (user, stranger) = (User::new(), User::new());
        let doc = registered(&on, &user, "before", 1).await;

        let history = snapshot(&on.node, &user, &doc).await;
        refused(
            &on.node,
            edit(&on.node, &stranger, &doc, &history, Edit::Update("taken")).await,
        )
        .await;
        refused(
            &on.node,
            edit(&on.node, &stranger, &doc, &history, Edit::Delete).await,
        )
        .await;
        assert_eq!(
            names(on.node.execute_request(read_users(&user.did)).await),
            ["before"]
        );

        admitted(
            &on.node,
            edit(&on.node, &user, &doc, &history, Edit::Update("after")).await,
        )
        .await;
        assert_eq!(
            names(on.node.execute_request(read_users(&user.did)).await),
            ["after"]
        );
        let history = snapshot(&on.node, &user, &doc).await;
        admitted(
            &on.node,
            edit(&on.node, &user, &doc, &history, Edit::Delete).await,
        )
        .await;
        assert!(names(on.node.execute_request(read_users(&user.did)).await).is_empty());
        assert!(on
            .node
            .owed_acp_as(
                &RequestContext::default().with_identity(Some(user.did.to_string())),
                user.did.as_str()
            )
            .await
            .expect("report")
            .is_empty());
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// The owner grants a writer on the chain, the writer's verified update is
/// admitted; once the owner revokes that grant on the chain, the writer's
/// next update is refused.
#[test]
fn a_writer_whose_grant_was_revoked_on_chain_is_refused() {
    workers::block_on(async {
        let on = Deployed::start(9416).await;
        let (user, writer) = (User::new(), User::new());
        let doc = registered(&on, &user, "shared", 1).await;
        let delegation = caller_delegation(&user.delegations, &on.audience, LIFETIME)
            .await
            .expect("delegation");
        let owner = RequestContext::default()
            .with_identity(Some(user.did.to_string()))
            .with_bearer(Some(user.did.to_string()), Some(delegation));
        // The writer also reads, so after the revocation it still sees the
        // document: what refuses it is the permission to update.
        for relation in ["writer", "reader"] {
            on.node
                .add_dac_actor_relationship_as(&owner, "Users", &doc, relation, writer.did.as_str())
                .await
                .expect("grant");
        }

        let history = snapshot(&on.node, &user, &doc).await;
        admitted(
            &on.node,
            edit(&on.node, &writer, &doc, &history, Edit::Update("written")).await,
        )
        .await;
        assert_eq!(
            names(on.node.execute_request(read_users(&user.did)).await),
            ["written"]
        );

        on.node
            .delete_dac_actor_relationship_as(&owner, "Users", &doc, "writer", writer.did.as_str())
            .await
            .expect("revocation");
        let vera = on.node.vera_acp().expect("vera.rs document ACP").clone();
        assert!(!vera
            .check_named_permission(&writer.did, "writer", &on.policy, "users", &doc)
            .await
            .expect("relation check"));
        drop(vera);
        let history = snapshot(&on.node, &user, &doc).await;
        refused(
            &on.node,
            edit(&on.node, &writer, &doc, &history, Edit::Update("revoked")).await,
        )
        .await;
        assert_eq!(
            names(on.node.execute_request(read_users(&user.did)).await),
            ["written"]
        );
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A node that verifies against a chain it does not host: once the chain
/// stops, a verified update is refused, as no fresh decision can be read.
#[test]
fn a_verified_update_is_refused_when_the_chain_cannot_be_reached() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let chain_signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key"));
        let actor: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[43; 32]).expect("key"));
        let rpc = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .unwrap();
        let chain = node_serving(
            &directory.path().join("chain"),
            EmbeddedVeraActor::Signer(Arc::clone(&chain_signer)),
            vec![EmbeddedVeraPolicy {
                definition: POLICY.to_string(),
                counter: 0,
                owner: chain_signer.did().to_string(),
            }],
            Some(rpc),
        )
        .await;
        let trust = chain.embedded_vera_trust().expect("deployment").clone();

        let home = tempfile::tempdir().unwrap();
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(home.path().join("keys"), b"test-password")
                .expect("keyring"),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let http = listener.local_addr().unwrap();
        let node = Node::builder()
            .enable_signing()
            .with_http(defra_node::HttpConfig::with_listener(listener).unwrap())
            .with_vera_rs(VeraRsConfig {
                rpc_address: format!("http://{rpc}"),
                consensus_key: trust.consensus_key,
                deployment_id: trust.chain_id,
                actor: VeraRsActor::Signer(Arc::clone(&actor)),
                worker_dir: home.path().join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("node");
        let acting = RequestContext::default().with_identity(Some(actor.did().to_string()));
        let policy = node
            .add_dac_policy_with_counter_as(&acting, POLICY, 0)
            .await
            .expect("seeded policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let audience = node
            .vera_acp()
            .expect("vera.rs document ACP")
            .light_client_status()
            .expect("status")
            .delegation
            .expect("a vera.rs node names its delegation audience");
        let on = Deployed {
            node,
            http,
            rpc,
            policy,
            audience,
            _directory: home,
        };

        let user = User::new();
        let delegation = caller_delegation(&user.delegations, &on.audience, LIFETIME)
            .await
            .expect("delegation");
        let Attempt { doc, txn, response } = on
            .create(&user, "kept", |_| async {
                SignerCredential::Delegation(delegation)
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.node.commit_transaction(&txn).await.expect("commit");
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );
        let history = snapshot(&on.node, &user, &doc).await;

        chain.shutdown().await.expect("shutdown");
        drop(chain);
        refused(
            &on.node,
            edit(&on.node, &user, &doc, &history, Edit::Update("blind")).await,
        )
        .await;
        refused(
            &on.node,
            edit(&on.node, &user, &doc, &history, Edit::Delete).await,
        )
        .await;
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
