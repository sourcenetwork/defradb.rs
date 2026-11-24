use std::process::Command;
use std::sync::Arc;

use acp::policy_yaml::{build_policy, parse_policy_yaml};
use acp::{DocumentACP, PersistentZanzibarStore, RegistrationParent, ZanzibarDocumentACP};
use identity::Did;
use serde::Deserialize;
use storage::RegolithStore;
use zanzibar::{Subject, SubjectRestriction, ZanzibarStore};

const POLICY: &str = "name: gates\nresources:\n- name: node\n- name: job\n  relations: [{name: parent, types: [node]}]\n  create: {parent: parent, requires: owner}\n";

/// The cases the Lean model prints for `mode`, run from the Lean project so
/// its `lean-toolchain` selects the toolchain.
fn lean_cases<T>(mode: &str) -> Vec<T>
where
    T: serde::de::DeserializeOwned,
{
    let output = Command::new("lake")
        .args(["env", "lean", "--run", "ParentRegistration.lean", mode])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../proofs/lean"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn individual_publisher_rejects_wildcard() {
    workers::block_on(async {
        let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(RegolithStore::in_memory().unwrap())));
        let yaml = "name: gates\nresources:\n- name: node\n  relations: [{name: publisher, types: [actor:individual]}]\n- name: job\n  relations: [{name: parent, types: [node]}]\n  create: {parent: parent, requires: publisher}\n";
        let policy = build_policy(&parse_policy_yaml(yaml).unwrap(), 1).unwrap();
        let legacy = build_policy(&parse_policy_yaml(&yaml.replace("actor:individual", "actor")).unwrap(), 1).unwrap();
        assert_ne!(policy.id, legacy.id);
        store.store_policy(&legacy).await.unwrap();
        store.store_policy(&policy).await.unwrap();
        let acp = ZanzibarDocumentACP::new(store);
        let owner = Did::new("did:key:owner").unwrap();
        let publisher = Did::new("did:key:publisher").unwrap();
        acp.register_doc_object(&owner, &legacy.id, "node", "legacy").await.unwrap();
        acp.add_relationship(&owner, Subject::Wildcard, &legacy.id, "node", "legacy", "publisher", &[]).await.unwrap();
        acp.register_doc_object(&owner, &policy.id, "node", "a").await.unwrap();
        assert!(acp.add_relationship(&owner, Subject::Wildcard, &policy.id, "node", "a", "publisher", &[]).await.is_err());
        acp.add_relationship(&owner, Subject::entity(publisher.clone()), &policy.id, "node", "a", "publisher", &[]).await.unwrap();
        let parent = acp.registration_parent(&policy.id, "job", "a").await.unwrap();
        acp.register_under_parent(&publisher, &policy.id, "job", "b", &parent).await.unwrap();
    }).unwrap();
}

#[test]
fn parent_relation_requires_an_explicit_grant() {
    workers::block_on(async {
        let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(RegolithStore::in_memory().unwrap())));
        let yaml = "name: gates\nresources:\n- name: node\n  relations: [{name: publisher, types: [actor]}]\n- name: job\n  relations: [{name: parent, types: [node]}]\n  create: {parent: parent, requires: publisher}\n";
        let policy = build_policy(&parse_policy_yaml(yaml).unwrap(), 1).unwrap();
        store.store_policy(&policy).await.unwrap();
        let acp = ZanzibarDocumentACP::new(store);
        let owner = Did::new("did:key:owner").unwrap();
        let publisher = Did::new("did:key:publisher").unwrap();
        acp.register_doc_object(&owner, &policy.id, "node", "a").await.unwrap();
        let parent = acp.registration_parent(&policy.id, "job", "a").await.unwrap();
        assert!(acp.register_under_parent(&owner, &policy.id, "job", "b", &parent).await.is_err());
        assert!(acp.register_under_parent(&publisher, &policy.id, "job", "b", &parent).await.is_err());
        acp.add_relationship(&owner, Subject::entity(publisher.clone()), &policy.id, "node", "a", "publisher", &[]).await.unwrap();
        acp.register_under_parent(&publisher, &policy.id, "job", "b", &parent).await.unwrap();
    }).unwrap();
}

#[test]
fn local_registration_checks_parent_before_writing_child() {
    workers::block_on(async {
        let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(
            RegolithStore::in_memory().unwrap(),
        )));
        let definition = parse_policy_yaml(POLICY).unwrap();
        let policy = build_policy(&definition, 1).unwrap();
        store.store_policy(&policy).await.unwrap();
        let acp = ZanzibarDocumentACP::new(store.clone());
        let owner = Did::new("did:key:owner").unwrap();
        let other = Did::new("did:key:other").unwrap();
        let id = &policy.id;
        acp.register_doc_object(&owner, id, "node", "a")
            .await
            .unwrap();
        let parent = acp.registration_parent(id, "job", "a").await.unwrap();
        assert!(acp
            .register_doc_object(&owner, id, "job", "b")
            .await
            .is_err());
        assert!(acp
            .register_under_parent(&other, id, "job", "b", &parent)
            .await
            .is_err());
        assert!(!acp.is_doc_registered(id, "job", "b").await.unwrap());
        acp.register_under_parent(&owner, id, "job", "b", &parent)
            .await
            .unwrap();
        assert_eq!(
            acp.get_doc_owner(id, "job", "b").await.unwrap(),
            Some(owner.clone())
        );
        assert!(store
            .has_relationship(
                id,
                "job",
                "b",
                "parent",
                &Subject::entity_set("node", "a", "")
            )
            .await
            .unwrap());
        acp.register_doc_object(&other, id, "node", "c")
            .await
            .unwrap();
        assert!(acp
            .add_relationship(
                &owner,
                Subject::entity_set("node", "c", ""),
                id,
                "job",
                "b",
                "parent",
                &[]
            )
            .await
            .is_err());
    })
    .unwrap();
}

#[test]
fn a_registered_child_keeps_its_last_parent_link() {
    workers::block_on(async {
        let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(
            RegolithStore::in_memory().unwrap(),
        )));
        let definition = parse_policy_yaml(POLICY).unwrap();
        let policy = build_policy(&definition, 1).unwrap();
        store.store_policy(&policy).await.unwrap();
        let acp = ZanzibarDocumentACP::new(store.clone());
        let owner = Did::new("did:key:owner").unwrap();
        let id = &policy.id;
        for node in ["a", "c"] {
            acp.register_doc_object(&owner, id, "node", node)
                .await
                .unwrap();
        }
        let parent = acp.registration_parent(id, "job", "a").await.unwrap();
        acp.register_under_parent(&owner, id, "job", "b", &parent)
            .await
            .unwrap();
        let link = |node: &str| Subject::entity_set("node", node, "");
        let held = |node: &'static str| {
            let store = store.clone();
            async move {
                store
                    .has_relationship(id, "job", "b", "parent", &link(node))
                    .await
                    .unwrap()
            }
        };

        let refused = acp
            .delete_relationship(&owner, link("a"), id, "job", "b", "parent", &[])
            .await
            .unwrap_err();
        assert_eq!(
            refused.to_string(),
            "invalid relation: cannot delete the last parent relation"
        );
        assert!(held("a").await);

        assert!(acp
            .add_relationship(&owner, link("c"), id, "job", "b", "parent", &[])
            .await
            .unwrap());
        assert!(acp
            .delete_relationship(&owner, link("a"), id, "job", "b", "parent", &[])
            .await
            .unwrap());
        assert!(!held("a").await);
        assert!(acp
            .delete_relationship(&owner, link("c"), id, "job", "b", "parent", &[])
            .await
            .is_err());
        assert!(held("c").await);

        acp.unregister_doc_object(id, "job", "b").await.unwrap();
        assert!(!held("c").await);
    })
    .unwrap();
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveInput {
    has_rule: bool,
    restriction: String,
    resource: String,
    relation: String,
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelParent {
    resource: String,
    object_id: String,
}

#[derive(Deserialize)]
struct ResolveCase {
    input: ResolveInput,
    parent: Option<ModelParent>,
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn registration_parent_matches_executed_lean_model() {
    let cases: Vec<ResolveCase> = lean_cases("resolve");
    assert!(cases.iter().any(|case| case.parent.is_some()));
    for ResolveCase { input, parent } in cases {
        let mut policy = build_policy(&parse_policy_yaml(POLICY).unwrap(), 1).unwrap();
        let job = policy
            .resources
            .iter_mut()
            .find(|resource| resource.name == "job")
            .unwrap();
        if !input.has_rule {
            job.create = None;
        }
        let relation = job
            .relations
            .iter_mut()
            .find(|relation| relation.name == "parent")
            .unwrap();
        relation.subject_restriction = match input.restriction.as_str() {
            "none" => None,
            "entity" => Some(SubjectRestriction::Entity),
            "entity_set" => Some(SubjectRestriction::EntitySet {
                resource: input.resource,
                relation: input.relation,
            }),
            other => panic!("unknown model restriction {other}"),
        };
        let resolved = RegistrationParent::from_policy(&policy, "job", &input.id).ok();
        assert_eq!(
            resolved,
            parent.map(|parent| RegistrationParent {
                resource: parent.resource,
                object_id: parent.object_id,
            })
        );
    }
}

#[derive(Deserialize)]
struct UnlinkCase {
    links: Vec<String>,
    target: String,
    allowed: bool,
    after: Vec<String>,
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn parent_unlink_matches_executed_lean_model() {
    let cases: Vec<UnlinkCase> = lean_cases("unlink");
    assert!(cases.iter().any(|case| !case.allowed));
    workers::block_on(async {
        for case in cases {
            let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(
                RegolithStore::in_memory().unwrap(),
            )));
            let policy = build_policy(&parse_policy_yaml(POLICY).unwrap(), 1).unwrap();
            store.store_policy(&policy).await.unwrap();
            let acp = ZanzibarDocumentACP::new(store.clone());
            let owner = Did::new("did:key:owner").unwrap();
            let id = &policy.id;
            for node in ["a", "b", "c"] {
                acp.register_doc_object(&owner, id, "node", node)
                    .await
                    .unwrap();
            }
            let link = |node: &str| Subject::entity_set("node", node, "");
            let parent = acp
                .registration_parent(id, "job", &case.links[0])
                .await
                .unwrap();
            acp.register_under_parent(&owner, id, "job", "j", &parent)
                .await
                .unwrap();
            for node in &case.links[1..] {
                acp.add_relationship(&owner, link(node), id, "job", "j", "parent", &[])
                    .await
                    .unwrap();
            }
            let deleted = acp
                .delete_relationship(&owner, link(&case.target), id, "job", "j", "parent", &[])
                .await;
            assert_eq!(deleted.is_ok(), case.allowed, "{:?}", case.links);
            let mut held = Vec::new();
            for node in ["a", "b", "c"] {
                if store
                    .has_relationship(id, "job", "j", "parent", &link(node))
                    .await
                    .unwrap()
                {
                    held.push(node.to_owned());
                }
            }
            assert_eq!(held, case.after);
        }
    })
    .unwrap();
}
