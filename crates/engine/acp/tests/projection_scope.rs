use acp::projection::{in_scope, Rule, TargetKind};
use acp::Subject;
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRule {
    collection: String,
    resource: String,
    relation: String,
    target_kind: String,
}

#[derive(Deserialize)]
struct Case {
    rules: Vec<ModelRule>,
    candidate: ModelRule,
    allowed: bool,
}

fn target(kind: &str) -> (TargetKind, Subject) {
    match kind {
        "entity" => (
            TargetKind::Entity,
            Subject::entity(identity::Did::new("did:key:actor").unwrap()),
        ),
        "entity_set" => (
            TargetKind::EntitySet,
            Subject::entity_set("nodes", "node", ""),
        ),
        "wildcard" => (TargetKind::Wildcard, Subject::wildcard()),
        "typed_wildcard" => (TargetKind::TypedWildcard, Subject::typed_wildcard("nodes")),
        other => panic!("unknown model target kind {other}"),
    }
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn projection_scope_matches_executed_lean_model() {
    // From the Lean project, so its `lean-toolchain` selects the toolchain.
    let output = Command::new("lake")
        .args([
            "env",
            "lean",
            "--run",
            "ProjectionRelationship.lean",
            "scope",
        ])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../proofs/lean"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    for case in cases {
        let rules = case
            .rules
            .into_iter()
            .map(|rule| Rule {
                collection: rule.collection,
                resource: rule.resource,
                relation: rule.relation,
                target_kind: target(&rule.target_kind).0,
            })
            .collect::<Vec<_>>();
        let subject = target(&case.candidate.target_kind).1;
        assert_eq!(
            in_scope(
                &rules,
                &case.candidate.collection,
                &case.candidate.resource,
                &case.candidate.relation,
                &subject
            ),
            case.allowed
        );
    }
}
