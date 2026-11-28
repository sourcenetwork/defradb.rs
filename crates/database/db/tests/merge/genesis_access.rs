use datastore::multistore::GenesisReadAccess;
use db::merge::merge_handler::genesis_access::creator_reader;
use identity::Did;
use std::process::Command;

const FIELD: &str = "addressee";

fn access(name: &str) -> Option<GenesisReadAccess> {
    match name {
        "off" => None,
        "public" => Some(GenesisReadAccess::Public),
        "addressed" => Some(GenesisReadAccess::AddressedBy(FIELD)),
        other => panic!("unknown model access {other}"),
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
