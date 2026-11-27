use vera_modules::acp::delegated_operation::DelegatedOperation;

use super::*;
use crate::request_digest;

const POLICY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
const ED25519: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
/// The compressed and uncompressed spellings of the secp256k1 key `[42; 32]`.
const SECP256K1_COMPRESSED: &str = "did:key:zQ3shkpqZaRR4P8KsTE1vXHRtjAAjKPfHFeu5B8vmPnW79A51";
const SECP256K1_UNCOMPRESSED: &str = "did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6";
/// The W3C did:key vector for P-256, compressed, and its uncompressed spelling.
const P256_COMPRESSED: &str = "did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv";
const P256_UNCOMPRESSED: &str = "did:key:z4oJ8cYF2JwS84CUKnKrnNW6hAhUzH3BNfybZEa87TkErqCeqTScZ4TFF565pwTYuoHbHbP6sR544QJf5tgQe13tFvfRt";

/// The actor subjects a command can name.
struct Actors {
    ed25519: SubjectRef,
    secp256k1_compressed: SubjectRef,
    secp256k1_uncompressed: SubjectRef,
    p256_compressed: SubjectRef,
    p256_uncompressed: SubjectRef,
    all: SubjectRef,
}

impl Actors {
    fn new() -> Self {
        let actor = |did: &str| SubjectRef::Actor(did.into());
        Self {
            ed25519: actor(ED25519),
            secp256k1_compressed: actor(SECP256K1_COMPRESSED),
            secp256k1_uncompressed: actor(SECP256K1_UNCOMPRESSED),
            p256_compressed: actor(P256_COMPRESSED),
            p256_uncompressed: actor(P256_UNCOMPRESSED),
            all: SubjectRef::AllActors,
        }
    }
}

/// One command, the chain value it must be sent as, and the bytes and
/// request digest it is sent with, pinned at vera.rs's acp-rs relationship
/// encoding (the hand-written encoding's bytes held until vera.rs moved to
/// acp-rs; a registration without relationships keeps them).
struct Case<'a> {
    command: PolicyCommand<'a>,
    chain: PolicyCmd,
    golden: &'static str,
    digest: &'static str,
}

fn structured<'a>(
    kind: u8,
    resource: &'a str,
    object_id: &'a str,
    relation: &'a str,
) -> PolicySubject<'a> {
    PolicySubject::Structured {
        kind,
        resource,
        object_id,
        relation,
    }
}

fn entity(did: &str) -> Subject {
    relationship_values::actor_subject(&vera_identity::Did::new(did).expect("a DID"))
        .expect("an actor")
}

/// What a document carries through one `@policy(relation:)` field.
fn derived() -> &'static [acp::DerivedRelationship] {
    Box::leak(Box::new([acp::DerivedRelationship {
        relation: "author".into(),
        target_resource: "authors".into(),
        target_doc_id: "bae-2".into(),
    }]))
}

fn cases(actors: &Actors) -> Vec<Case<'_>> {
    let command = |action| PolicyCommand {
        policy_id: POLICY,
        resource: "users",
        object_id: "bae-1",
        action,
    };
    let set = |subject| {
        command(PolicyAction::SetRelationship {
            relation: "reader",
            subject,
        })
    };
    let delete = |subject| {
        command(PolicyAction::DeleteRelationship {
            relation: "reader",
            subject,
        })
    };
    let relationship =
        |subject| relationship_values::relationship("users", "bae-1", "reader", subject);
    let actor = PolicySubject::Actor;
    let team = || relationship_values::object_subject("groups", "team");
    let members = || relationship_values::actor_set("groups", "team", "member");
    vec![
        Case {
            command: command(PolicyAction::RegisterObject),
            chain: PolicyCmd::RegisterObject(object("users", "bae-1")),
            golden: r#"{"RegisterObject":{"resource":"users","id":"bae-1"}}"#,
            digest: "11d65e11a72dac58353e23ae8e0c47b62b278a07cfc9f14b6340d9494c976fcb",
        },
        Case {
            command: command(PolicyAction::register(derived())),
            chain: PolicyCmd::RegisterObjectWithRelationships {
                object: object("users", "bae-1"),
                relationships: vec![relationship_values::relationship(
                    "users",
                    "bae-1",
                    "author",
                    relationship_values::object_subject("authors", "bae-2"),
                )],
            },
            golden: r#"{"RegisterObjectWithRelationships":{"object":{"resource":"users","id":"bae-1"},"relationships":[{"object":{"resource":"users","id":"bae-1"},"relation":"author","subject":{"object":{"resource":"authors","id":"bae-2"}}}]}}"#,
            digest: "8a1af91a4cddec6cb84c4b0644db93794d749644e439f60720b81f621d61585a",
        },
        Case {
            command: set(actor(&actors.ed25519)),
            chain: PolicyCmd::SetRelationship(relationship(entity(ED25519))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp"}}}"#,
            digest: "ac306441930a9c934839ad8247d553a14014cb5928ffac84a4d38d89e36410de",
        },
        Case {
            command: delete(actor(&actors.ed25519)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(ED25519))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp"}}}"#,
            digest: "78241797dcc9992135ed13d66960a739d355d9f8557092a54a97e585b40bcc2e",
        },
        Case {
            command: set(actor(&actors.secp256k1_compressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "4b4ab12bcf3f63cee2c05d04b7831cff9024f36a37ae77b151f3e51f60c249ab",
        },
        Case {
            command: delete(actor(&actors.secp256k1_compressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "d9140b7c7f6e42074d71efa48a7fcd009214cdcb0a8027ca76d25adce43f2d1d",
        },
        Case {
            command: set(actor(&actors.secp256k1_uncompressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "4b4ab12bcf3f63cee2c05d04b7831cff9024f36a37ae77b151f3e51f60c249ab",
        },
        Case {
            command: delete(actor(&actors.secp256k1_uncompressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "d9140b7c7f6e42074d71efa48a7fcd009214cdcb0a8027ca76d25adce43f2d1d",
        },
        Case {
            command: set(actor(&actors.p256_uncompressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "52d94456a03b89863ec2180a2936915085e5d7d580b68b38ebb8412065779430",
        },
        Case {
            command: delete(actor(&actors.p256_uncompressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "88cd9616feb3456ce76b83eef5a6a2bbbe804422ab075352a945ab122b0af104",
        },
        Case {
            command: set(actor(&actors.p256_compressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "52d94456a03b89863ec2180a2936915085e5d7d580b68b38ebb8412065779430",
        },
        Case {
            command: delete(actor(&actors.p256_compressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "88cd9616feb3456ce76b83eef5a6a2bbbe804422ab075352a945ab122b0af104",
        },
        Case {
            command: set(actor(&actors.all)),
            chain: PolicyCmd::SetRelationship(relationship(Subject::AllActors)),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":"all_actors"}}"#,
            digest: "707e4b33dcb4a5975775a771cd3cb36c00e8e865173cb034f2235bc597b5cae0",
        },
        Case {
            command: delete(actor(&actors.all)),
            chain: PolicyCmd::DeleteRelationship(relationship(Subject::AllActors)),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":"all_actors"}}"#,
            digest: "0ab1ebfc2b6e0eefcab16605141810ebe5fcd410306028835b92d4439ae97c8d",
        },
        Case {
            command: set(structured(0, "", SECP256K1_COMPRESSED, "")),
            chain: PolicyCmd::SetRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "4b4ab12bcf3f63cee2c05d04b7831cff9024f36a37ae77b151f3e51f60c249ab",
        },
        Case {
            command: delete(structured(0, "", SECP256K1_COMPRESSED, "")),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "d9140b7c7f6e42074d71efa48a7fcd009214cdcb0a8027ca76d25adce43f2d1d",
        },
        Case {
            command: set(structured(0, "", P256_UNCOMPRESSED, "")),
            chain: PolicyCmd::SetRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "52d94456a03b89863ec2180a2936915085e5d7d580b68b38ebb8412065779430",
        },
        Case {
            command: delete(structured(0, "", P256_UNCOMPRESSED, "")),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "88cd9616feb3456ce76b83eef5a6a2bbbe804422ab075352a945ab122b0af104",
        },
        Case {
            command: set(structured(1, "", "", "")),
            chain: PolicyCmd::SetRelationship(relationship(Subject::AllActors)),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":"all_actors"}}"#,
            digest: "707e4b33dcb4a5975775a771cd3cb36c00e8e865173cb034f2235bc597b5cae0",
        },
        Case {
            command: delete(structured(1, "", "", "")),
            chain: PolicyCmd::DeleteRelationship(relationship(Subject::AllActors)),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":"all_actors"}}"#,
            digest: "0ab1ebfc2b6e0eefcab16605141810ebe5fcd410306028835b92d4439ae97c8d",
        },
        Case {
            command: set(structured(2, "groups", "team", "")),
            chain: PolicyCmd::SetRelationship(relationship(team())),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"object":{"resource":"groups","id":"team"}}}}"#,
            digest: "33fa385833d89cf4fdc37a8d13f827f32c8aa5420b380db0f3ce2dfedfe7d89f",
        },
        Case {
            command: delete(structured(2, "groups", "team", "")),
            chain: PolicyCmd::DeleteRelationship(relationship(team())),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"object":{"resource":"groups","id":"team"}}}}"#,
            digest: "72e9faa0e79a6e180a3f206b2e0d6e071aab2941c1a22b6feff45551030b9b2e",
        },
        Case {
            command: set(structured(3, "groups", "team", "member")),
            chain: PolicyCmd::SetRelationship(relationship(members())),
            golden: r#"{"SetRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor_set":{"object":{"resource":"groups","id":"team"},"relation":"member"}}}}"#,
            digest: "c801549c74bf68af0a68ff015ff55572c0a429e962696723ff1d6ace84ea50b4",
        },
        Case {
            command: delete(structured(3, "groups", "team", "member")),
            chain: PolicyCmd::DeleteRelationship(relationship(members())),
            golden: r#"{"DeleteRelationship":{"object":{"resource":"users","id":"bae-1"},"relation":"reader","subject":{"actor_set":{"object":{"resource":"groups","id":"team"},"relation":"member"}}}}"#,
            digest: "3e160c87fef17206ff38600d1a2cceffe54642dc8593680caf2bda99e6a75063",
        },
    ]
}

/// The command bytes of the call `command` is submitted with.
fn sent(policy_id: &str, command: &PolicyCmd) -> Vec<u8> {
    let calldata = calldata("token", policy_id, command).expect("a call");
    let call = IAcp::bearerPolicyCmdCall::abi_decode(&calldata).expect("a policy command call");
    assert_eq!(call.bearerToken, "token");
    assert_eq!(call.policyId, parse_policy_id(policy_id).unwrap());
    call.cmd.to_vec()
}

#[test]
fn every_command_is_sent_as_the_chain_serializes_it() {
    let actors = Actors::new();
    for case in cases(&actors) {
        let typed = typed(&case.command).expect("a command");
        assert_eq!(
            sent(case.command.policy_id, &typed),
            serde_json::to_vec(&case.chain).unwrap(),
            "{}",
            case.golden
        );
    }
    let archive = PolicyCmd::ArchiveObject(object("users", "bae-1"));
    assert_eq!(
        sent(POLICY, &archive_object("users", "bae-1")),
        serde_json::to_vec(&archive).unwrap()
    );
}

#[test]
fn the_bytes_sent_are_the_pinned_ones() {
    let actors = Actors::new();
    for case in cases(&actors) {
        let typed = typed(&case.command).expect("a command");
        assert_eq!(
            String::from_utf8(sent(case.command.policy_id, &typed)).unwrap(),
            case.golden
        );
        assert_eq!(
            hex::encode(request_digest(&case.command).unwrap()),
            case.digest,
            "{}",
            case.golden
        );
    }
    assert_eq!(
        String::from_utf8(sent(POLICY, &archive_object("users", "bae-1"))).unwrap(),
        r#"{"ArchiveObject":{"resource":"users","id":"bae-1"}}"#
    );
}

#[test]
fn the_digest_names_the_command_as_sent() {
    let actors = Actors::new();
    for case in cases(&actors) {
        let typed = typed(&case.command).expect("a command");
        let received: PolicyCmd =
            serde_json::from_slice(&sent(case.command.policy_id, &typed)).unwrap();
        assert_eq!(
            request_digest(&case.command).unwrap(),
            DelegatedOperation::PolicyCommand(POLICY, &received)
                .digest()
                .unwrap(),
            "{}",
            case.golden
        );
    }
}

#[test]
fn a_registration_carrying_no_relationship_is_a_plain_one() {
    assert!(matches!(
        PolicyAction::register(&[]),
        PolicyAction::RegisterObject
    ));
}

#[test]
fn a_subject_vera_rs_has_no_command_for_is_refused() {
    let typed_wildcard = zanzibar::Subject::TypedWildcard {
        resource: "users".into(),
    };
    assert!(matches!(
        chain_subject(&typed_wildcard),
        Err(CommandError::Subject(_))
    ));

    let not_an_actor = SubjectRef::Actor("not a DID".into());
    for subject in [
        PolicySubject::Actor(&not_an_actor),
        structured(0, "", "not a DID", ""),
        structured(1, "groups", "", ""),
        structured(2, "groups", "team", "member"),
        structured(3, "groups", "team", ""),
        structured(4, "groups", "team", "member"),
    ] {
        let command = PolicyCommand {
            policy_id: POLICY,
            resource: "users",
            object_id: "bae-1",
            action: PolicyAction::SetRelationship {
                relation: "reader",
                subject,
            },
        };
        assert!(matches!(typed(&command), Err(CommandError::Subject(_))));
        assert!(request_digest(&command).is_err());
    }
}
