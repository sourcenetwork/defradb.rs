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
/// request digest the hand-written encoding produced for it before the
/// commands were typed.
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
    Subject::Entity(did.parse().expect("a DID"))
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
    let relationship = |subject| Relationship::new("users", "bae-1", "reader", subject);
    let actor = PolicySubject::Actor;
    let team = || Subject::entity_set("groups", "team", "");
    let members = || Subject::entity_set("groups", "team", "member");
    vec![
        Case {
            command: command(PolicyAction::RegisterObject),
            chain: PolicyCmd::RegisterObject(object("users", "bae-1")),
            golden: r#"{"RegisterObject":{"resource":"users","id":"bae-1"}}"#,
            digest: "11d65e11a72dac58353e23ae8e0c47b62b278a07cfc9f14b6340d9494c976fcb",
        },
        Case {
            command: set(actor(&actors.ed25519)),
            chain: PolicyCmd::SetRelationship(relationship(entity(ED25519))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp"}}}"#,
            digest: "19620516fcd142e9acbbff7b942af36457918ff46509bbaf82b64373b0d1601d",
        },
        Case {
            command: delete(actor(&actors.ed25519)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(ED25519))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp"}}}"#,
            digest: "3adecb653d046da73c1afd45ccb4f573bda4eb6dd9013d926e2aac70115ed235",
        },
        Case {
            command: set(actor(&actors.secp256k1_compressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "d93de0de335901ac79fc598d87dfd70f86ffabd5d5273280fc1baf84d6e5d1b2",
        },
        Case {
            command: delete(actor(&actors.secp256k1_compressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "dc99916f8664eb7f669be1d168582f27cc0ccba850c92c1da066c766af8758ac",
        },
        Case {
            command: set(actor(&actors.secp256k1_uncompressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "d93de0de335901ac79fc598d87dfd70f86ffabd5d5273280fc1baf84d6e5d1b2",
        },
        Case {
            command: delete(actor(&actors.secp256k1_uncompressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "dc99916f8664eb7f669be1d168582f27cc0ccba850c92c1da066c766af8758ac",
        },
        Case {
            command: set(actor(&actors.p256_uncompressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "3f438f217f9c3cf4596d8e942f8992da55f52f8da663edec6fa6a605fcf36c59",
        },
        Case {
            command: delete(actor(&actors.p256_uncompressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "3869b2742924f225c00fb0c4b6947e462fec136123968c8b8c85ae80014daa3d",
        },
        Case {
            command: set(actor(&actors.p256_compressed)),
            chain: PolicyCmd::SetRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "3f438f217f9c3cf4596d8e942f8992da55f52f8da663edec6fa6a605fcf36c59",
        },
        Case {
            command: delete(actor(&actors.p256_compressed)),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "3869b2742924f225c00fb0c4b6947e462fec136123968c8b8c85ae80014daa3d",
        },
        Case {
            command: set(actor(&actors.all)),
            chain: PolicyCmd::SetRelationship(relationship(Subject::Wildcard)),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":"Wildcard"}}"#,
            digest: "7875ab70057ff675336febfabf741ca2270b27d7ec526125a65a5a0c1ee628a5",
        },
        Case {
            command: delete(actor(&actors.all)),
            chain: PolicyCmd::DeleteRelationship(relationship(Subject::Wildcard)),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":"Wildcard"}}"#,
            digest: "b5c8b69124c9b0da7aa2b5f168fb5f1e24bc68917ae4454821e8642f2a0789e7",
        },
        Case {
            command: set(structured(0, "", SECP256K1_COMPRESSED, "")),
            chain: PolicyCmd::SetRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "d93de0de335901ac79fc598d87dfd70f86ffabd5d5273280fc1baf84d6e5d1b2",
        },
        Case {
            command: delete(structured(0, "", SECP256K1_COMPRESSED, "")),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(SECP256K1_UNCOMPRESSED))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:z7r8oqKHB4AwqvHNsPgHdGic7GYAWgVjrWsKwwazb9epJXZ3wBwVMJd5xrWTvrmDBFLJ1RZAx13earK9RyJzj8nqWbvG6"}}}"#,
            digest: "dc99916f8664eb7f669be1d168582f27cc0ccba850c92c1da066c766af8758ac",
        },
        Case {
            command: set(structured(0, "", P256_UNCOMPRESSED, "")),
            chain: PolicyCmd::SetRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "3f438f217f9c3cf4596d8e942f8992da55f52f8da663edec6fa6a605fcf36c59",
        },
        Case {
            command: delete(structured(0, "", P256_UNCOMPRESSED, "")),
            chain: PolicyCmd::DeleteRelationship(relationship(entity(P256_COMPRESSED))),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"Entity":"did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv"}}}"#,
            digest: "3869b2742924f225c00fb0c4b6947e462fec136123968c8b8c85ae80014daa3d",
        },
        Case {
            command: set(structured(1, "", "", "")),
            chain: PolicyCmd::SetRelationship(relationship(Subject::Wildcard)),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":"Wildcard"}}"#,
            digest: "7875ab70057ff675336febfabf741ca2270b27d7ec526125a65a5a0c1ee628a5",
        },
        Case {
            command: delete(structured(1, "", "", "")),
            chain: PolicyCmd::DeleteRelationship(relationship(Subject::Wildcard)),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":"Wildcard"}}"#,
            digest: "b5c8b69124c9b0da7aa2b5f168fb5f1e24bc68917ae4454821e8642f2a0789e7",
        },
        Case {
            command: set(structured(2, "groups", "team", "")),
            chain: PolicyCmd::SetRelationship(relationship(team())),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"EntitySet":{"resource":"groups","object_id":"team","relation":""}}}}"#,
            digest: "5bb3f847f21be0c00e58f0938201c03b7db70e0e2596980be5376b6f571893b2",
        },
        Case {
            command: delete(structured(2, "groups", "team", "")),
            chain: PolicyCmd::DeleteRelationship(relationship(team())),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"EntitySet":{"resource":"groups","object_id":"team","relation":""}}}}"#,
            digest: "38139e033c6d6ff0df1e5f16cc605047f6da723d0a4201c2f6b2541639a4e941",
        },
        Case {
            command: set(structured(3, "groups", "team", "member")),
            chain: PolicyCmd::SetRelationship(relationship(members())),
            golden: r#"{"SetRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"EntitySet":{"resource":"groups","object_id":"team","relation":"member"}}}}"#,
            digest: "517eb3a492fc12d737c3ba9cefdecf5abc8b7a7cacc3db0d59cde675c308c4f4",
        },
        Case {
            command: delete(structured(3, "groups", "team", "member")),
            chain: PolicyCmd::DeleteRelationship(relationship(members())),
            golden: r#"{"DeleteRelationship":{"resource":"users","object_id":"bae-1","relation":"reader","subject":{"EntitySet":{"resource":"groups","object_id":"team","relation":"member"}}}}"#,
            digest: "4f241342adf35419d0527aa66de108d440bf8b8de82e7081d772a2065c78e63f",
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
fn the_bytes_sent_are_those_sent_before_the_commands_were_typed() {
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
