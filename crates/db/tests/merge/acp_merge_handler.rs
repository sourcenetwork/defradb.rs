use acp::DocumentACP;
use acp::Identity;
use acp::LocalDocumentACP;
use acp::MemoryAcpStore;
use db::merge::acp_merge_handler::*;
use db::merge::merge_handler::hook::CompositeFrame;
use db::merge::merge_handler::hook::CompositeMergeHook;
use defra_core::merge::BlockMetadata;
use defra_core::merge::MergeOutcome;
use identity::Did;
use schema::CollectionVersion;
use schema::PolicyDescription;
use std::sync::Arc;

const OWNER: &str = "did:key:z6Mkowner";
const ATTACKER: &str = "did:key:z6Mkattacker";
const NODE: &str = "did:key:z6Mknode";

fn protected_collection() -> CollectionVersion {
    CollectionVersion::new("Users", "v1", "col1", vec![])
        .with_policy(PolicyDescription::new("policy-1", "users"))
}

fn hook(strict: bool) -> AcpCompositeMergeHook {
    hook_with_acp(
        strict,
        Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new()))),
    )
}

fn hook_with_acp(strict: bool, acp: Arc<LocalDocumentACP>) -> AcpCompositeMergeHook {
    let hook = AcpCompositeMergeHook::new(Some(Identity::Authenticated(Did::new(NODE).unwrap())));
    hook.set_document_acp(acp);
    hook.set_strict_replicated_doc_access(strict);
    hook
}

async fn registered_acp() -> Arc<LocalDocumentACP> {
    let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
    acp.register_doc_object(&Did::new(OWNER).unwrap(), "policy-1", "users", "doc1")
        .await
        .unwrap();
    acp
}

async fn grant(acp: &LocalDocumentACP, target: &str, relation: &str) {
    acp.add_actor_relationship(
        &Did::new(OWNER).unwrap(),
        &Did::new(target).unwrap(),
        "policy-1",
        "users",
        "doc1",
        relation,
        &[],
    )
    .await
    .unwrap();
}

fn update(signer: Option<&str>) -> CompositeFrame<'_> {
    CompositeFrame {
        is_genesis: false,
        status: 1,
        signer,
    }
}

fn delete(signer: Option<&str>) -> CompositeFrame<'_> {
    CompositeFrame {
        status: 2,
        ..update(signer)
    }
}

async fn judge(hook: &AcpCompositeMergeHook, frame: CompositeFrame<'_>) -> Option<MergeOutcome> {
    hook.on_protected_update("doc1", &protected_collection(), frame)
        .await
        .unwrap()
}

#[tokio::test]
async fn local_acp_allows_unregistered_encrypted_document() {
    let result = hook(false)
        .on_encrypted_link(
            "doc1",
            &protected_collection(),
            &BlockMetadata::normal("doc1", "col1", "creator", Some("peer"), false),
        )
        .await
        .unwrap();

    assert_eq!(result, None);
}

#[tokio::test]
async fn strict_acp_retries_unregistered_encrypted_document() {
    let result = hook(true)
        .on_encrypted_link(
            "doc1",
            &protected_collection(),
            &BlockMetadata::normal("doc1", "col1", "creator", Some("peer"), false),
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        Some(MergeOutcome::retryable_skip(
            "encrypted replicated document is not yet registered in local ACP",
        ))
    );
}

#[tokio::test]
async fn update_to_unregistered_document_is_not_judged() {
    assert_eq!(judge(&hook(false), update(Some(ATTACKER))).await, None);
    assert_eq!(judge(&hook(false), update(None)).await, None);
}

#[tokio::test]
async fn genesis_is_not_judged() {
    let hook = hook_with_acp(false, registered_acp().await);
    let genesis = CompositeFrame {
        is_genesis: true,
        ..update(Some(ATTACKER))
    };

    assert_eq!(judge(&hook, genesis).await, None);
}

#[tokio::test]
async fn update_signed_without_update_permission_is_rejected() {
    let hook = hook_with_acp(false, registered_acp().await);

    assert_eq!(
        judge(&hook, update(Some(ATTACKER))).await,
        Some(MergeOutcome::rejected(format!(
            "signer {ATTACKER} lacks update permission on protected document doc1"
        )))
    );
}

#[tokio::test]
async fn update_signed_by_owner_merges() {
    let hook = hook_with_acp(false, registered_acp().await);

    assert_eq!(judge(&hook, update(Some(OWNER))).await, None);
}

#[tokio::test]
async fn update_signed_by_grantee_merges() {
    let acp = registered_acp().await;
    grant(&acp, ATTACKER, "updater").await;
    let hook = hook_with_acp(false, acp);

    assert_eq!(judge(&hook, update(Some(ATTACKER))).await, None);
}

#[tokio::test]
async fn update_signed_by_node_identity_merges() {
    let hook = hook_with_acp(false, registered_acp().await);

    assert_eq!(judge(&hook, update(Some(NODE))).await, None);
}

#[tokio::test]
async fn unsigned_update_is_rejected() {
    let hook = hook_with_acp(false, registered_acp().await);

    assert_eq!(
        judge(&hook, update(None)).await,
        Some(MergeOutcome::rejected(
            "signer unsigned lacks update permission on protected document doc1"
        ))
    );
}

#[tokio::test]
async fn delete_requires_delete_permission() {
    let acp = registered_acp().await;
    grant(&acp, ATTACKER, "updater").await;
    let hook = hook_with_acp(false, acp.clone());

    assert_eq!(
        judge(&hook, delete(Some(ATTACKER))).await,
        Some(MergeOutcome::rejected(format!(
            "signer {ATTACKER} lacks delete permission on protected document doc1"
        )))
    );

    grant(&acp, ATTACKER, "deleter").await;
    assert_eq!(judge(&hook, delete(Some(ATTACKER))).await, None);
}

#[tokio::test]
async fn strict_acp_does_not_judge_updates() {
    let hook = hook_with_acp(true, registered_acp().await);

    assert!(!hook.guards_protected_updates());
    assert_eq!(judge(&hook, update(Some(ATTACKER))).await, None);
}

#[tokio::test]
async fn local_acp_guards_updates_only_once_document_acp_is_wired() {
    let unwired = AcpCompositeMergeHook::new(None);
    assert!(!unwired.guards_protected_updates());
    assert!(hook(false).guards_protected_updates());
}

#[test]
fn strict_acp_registers_owner_only_for_a_did_creator() {
    let hook = hook(true);
    let collection = protected_collection();

    let peer_id = BlockMetadata::normal("doc1", "col1", "12D3KooWPeer", Some("peer"), false);
    assert!(hook
        .post_commit_action("doc1", &collection, &peer_id)
        .is_none());

    let did = BlockMetadata::normal("doc1", "col1", "did:key:z6MkOwner", Some("peer"), false);
    assert!(hook.post_commit_action("doc1", &collection, &did).is_some());
}
