use std::process::Command;
use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity, MemoryZanzibarStore, ZanzibarDocumentACP};
use datastore::multistore::GenesisReadAccess;
use db::merge::merge_handler::genesis_access::{
    creator_reader, genesis_reader, register_signed_genesis, GENESIS_READER_RELATION,
};
use identity::Did;

const POLICY: &str = "policy-genesis";
const RESOURCE: &str = "records";
const DOC: &str = "bae-genesis";
const FIELD: &str = "addressee";

fn cbor(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(&value, &mut bytes).unwrap();
    bytes
}

fn field<'a>(bytes: &'a [u8]) -> impl FnOnce(&str) -> Option<&'a [u8]> + 'a {
    move |name| (name == FIELD).then_some(bytes)
}

fn access(name: &str) -> Option<GenesisReadAccess> {
    match name {
        "off" => None,
        "public" => Some(GenesisReadAccess::Public),
        "addressed" => Some(GenesisReadAccess::AddressedBy(FIELD)),
        other => panic!("unknown model access {other}"),
    }
}

/// The Zanzibar ACP is what a transaction binds the merge to.
fn zanzibar() -> ZanzibarDocumentACP<MemoryZanzibarStore> {
    ZanzibarDocumentACP::new(Arc::new(MemoryZanzibarStore::new()))
}

async fn reads(acp: &dyn DocumentACP, reader: &Did) -> bool {
    acp.check_doc_access(
        &Identity::Authenticated(reader.clone()),
        DocumentPermission::Read,
        POLICY,
        RESOURCE,
        DOC,
    )
    .await
    .unwrap()
}

#[test]
fn only_an_immutable_address_naming_this_node_selects_it() {
    let node = Did::new("did:key:receiver").unwrap();
    let address = cbor(node.as_str());
    let other = cbor("did:key:other");
    let undecodable = [0xff_u8];
    let addressed = || Some(GenesisReadAccess::AddressedBy(FIELD));

    assert_eq!(
        genesis_reader(addressed(), field(&address), Some(&node)),
        Some(node.clone())
    );
    assert_eq!(
        genesis_reader(addressed(), field(&other), Some(&node)),
        None
    );
    assert_eq!(genesis_reader(addressed(), |_| None, Some(&node)), None);
    assert_eq!(genesis_reader(addressed(), field(&address), None), None);
    assert_eq!(
        genesis_reader(addressed(), field(&undecodable), Some(&node)),
        None
    );
    assert_eq!(genesis_reader(None, field(&address), Some(&node)), None);
    assert_eq!(
        genesis_reader(Some(GenesisReadAccess::Public), |_| None, None),
        Some(Did::wildcard())
    );
}

#[test]
fn a_revoked_initial_reader_stays_revoked_when_the_genesis_arrives_again() {
    workers::block_on(async {
        let acp = zanzibar();
        let owner = Did::new("did:key:z6Mkowner").unwrap();
        let reader = Did::new("did:key:receiver").unwrap();

        register_signed_genesis(&acp, &owner, POLICY, RESOURCE, DOC, Some(&reader))
            .await
            .unwrap();
        assert!(reads(&acp, &reader).await);

        acp.delete_actor_relationship(
            &owner,
            &reader,
            POLICY,
            RESOURCE,
            DOC,
            GENESIS_READER_RELATION,
            &[],
        )
        .await
        .unwrap();
        register_signed_genesis(&acp, &owner, POLICY, RESOURCE, DOC, Some(&reader))
            .await
            .unwrap();

        assert!(!reads(&acp, &reader).await);
        assert_eq!(
            acp.get_doc_owner(POLICY, RESOURCE, DOC).await.unwrap(),
            Some(owner)
        );
    })
    .unwrap()
}

#[test]
fn a_genesis_without_a_reader_registers_only_its_owner() {
    workers::block_on(async {
        let acp = zanzibar();
        let owner = Did::new("did:key:z6Mkowner").unwrap();

        register_signed_genesis(&acp, &owner, POLICY, RESOURCE, DOC, None)
            .await
            .unwrap();

        assert!(!reads(&acp, &Did::new("did:key:receiver").unwrap()).await);
        assert_eq!(
            acp.get_doc_owner(POLICY, RESOURCE, DOC).await.unwrap(),
            Some(owner)
        );
    })
    .unwrap()
}

#[derive(serde::Deserialize)]
struct Case {
    access: String,
    address: Option<String>,
    receiver: Option<String>,
    reader: Option<String>,
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn genesis_reader_matches_executed_lean_model() {
    // From the Lean project, so its `lean-toolchain` selects the toolchain.
    let output = Command::new("lake")
        .args(["env", "lean", "--run", "EnrollmentGenesis.lean", "cases"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../proofs/lean"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let address = case.address.as_deref().map(cbor);
        let receiver = case.receiver.as_deref().map(|did| Did::new(did).unwrap());
        let decided = genesis_reader(
            access(&case.access),
            |name| address.as_deref().filter(|_| name == FIELD),
            receiver.as_ref(),
        );
        assert_eq!(
            decided.map(|did| did.to_string()),
            case.reader,
            "access {} address {:?} receiver {:?}",
            case.access,
            case.address,
            case.receiver
        );
    }
}

fn named<'a>(address: Option<&'a str>) -> impl FnOnce(&str) -> Option<&'a str> + 'a {
    move |name| address.filter(|_| name == FIELD)
}

#[test]
fn a_creator_grants_exactly_the_address_its_document_names() {
    let addressed = || Some(GenesisReadAccess::AddressedBy(FIELD));
    let receiver = Did::new("did:key:receiver").unwrap();

    assert_eq!(
        creator_reader(addressed(), named(Some(receiver.as_str()))),
        Some(receiver)
    );
    assert_eq!(creator_reader(addressed(), named(None)), None);
    assert_eq!(creator_reader(addressed(), named(Some("not a did"))), None);
    assert_eq!(
        creator_reader(addressed(), named(Some("*"))),
        None,
        "an address spelled as everyone names no node"
    );
    assert_eq!(
        creator_reader(Some(GenesisReadAccess::Public), named(None)),
        Some(Did::wildcard())
    );
    assert_eq!(creator_reader(None, named(Some("did:key:receiver"))), None);
}

#[derive(serde::Deserialize)]
struct CreatorCase {
    access: String,
    address: Option<String>,
    reader: Option<String>,
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn creator_reader_matches_executed_lean_model() {
    // From the Lean project, so its `lean-toolchain` selects the toolchain.
    let output = Command::new("lake")
        .args([
            "env",
            "lean",
            "--run",
            "EnrollmentGenesis.lean",
            "creator-cases",
        ])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../proofs/lean"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<CreatorCase> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 21);
    for case in cases {
        let decided = creator_reader(access(&case.access), named(case.address.as_deref()));
        assert_eq!(
            decided.map(|did| did.to_string()),
            case.reader,
            "access {} address {:?}",
            case.access,
            case.address
        );
    }
}
