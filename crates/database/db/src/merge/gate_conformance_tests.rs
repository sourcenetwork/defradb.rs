//! Drives the ACP merge hook through every case `proofs/lean/ReplicaGate.lean`
//! decides, and checks the hook's judgement and whether refusals park.

use std::process::Command;
use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity, ReplicaGrant, ReplicaOwed};
use async_trait::async_trait;
use identity::Did;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind, PolicyDescription};

use super::acp_merge_handler::AcpCompositeMergeHook;
use super::merge_handler::hook::{CompositeFrame, CompositeMergeHook};
use super::{ReplicaOwnership, ReplicaRegistration};
use defra_core::merge::MergeOutcome;

const SIGNER: &str = "did:key:gate-signer";
const OTHER: &str = "did:key:gate-other";

/// A provider whose every answer the case fixes.
struct Provider {
    remote: bool,
    registered: bool,
    owed: ReplicaOwed,
    holds: bool,
}

#[async_trait]
impl DocumentACP for Provider {
    fn supports_transactions(&self) -> bool {
        !self.remote
    }

    async fn replica_owed(&self, _: &str, _: &str, _: &str) -> acp::Result<ReplicaOwed> {
        Ok(self.owed.clone())
    }

    async fn register_doc_object(
        &self,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &[acp::DerivedRelationship],
    ) -> acp::Result<()> {
        Ok(())
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> acp::Result<bool> {
        Ok(self.registered)
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        _: DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> acp::Result<bool> {
        Ok(
            matches!(identity, Identity::Authenticated(did) if did.as_str() == SIGNER)
                && self.holds,
        )
    }

    async fn add_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn delete_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> acp::Result<()> {
        Ok(())
    }
}

struct NoOwnActors;

impl ReplicaOwnership for NoOwnActors {
    fn is_own_actor(&self, _: &str, _: &Did) -> bool {
        false
    }

    fn creator_grants(&self, _: &str, _: &Did, _: &str) -> Vec<ReplicaGrant> {
        Vec::new()
    }
}

fn users() -> CollectionVersion {
    CollectionVersion::new(
        "Users",
        "v1",
        "col-users",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "name", FieldKind::string(), CType::LwwRegister),
        ],
    )
    .with_policy(PolicyDescription::new("policy", "users"))
}

fn did(is_signer: bool) -> Did {
    Did::new(if is_signer { SIGNER } else { OTHER }).unwrap()
}

fn outcome_name(outcome: Option<MergeOutcome>) -> &'static str {
    match outcome {
        None => "merge",
        Some(MergeOutcome::Rejected { .. }) => "reject",
        Some(MergeOutcome::Skipped {
            terminal: false, ..
        }) => "retry",
        Some(outcome) => panic!("the hook gave an outcome the model has no name for: {outcome:?}"),
    }
}

/// One line of the model: the inputs, the hook's judgement, and whether
/// refusals park.
async fn check(line: &str) {
    let fields: Vec<&str> = line.split('\t').collect();
    let [provider, rest @ ..] = fields.as_slice() else {
        panic!("invalid model case: {line}")
    };
    let flags: Vec<bool> = rest[..10].iter().map(|f| f.parse().unwrap()).collect();
    let [strict, listed, registered, registration, creator_grants, adopted, signed, is_registrant, is_creator, holds] =
        flags[..]
    else {
        unreachable!()
    };
    let (expected_hook, expected_parks) = (rest[10], rest[11].parse::<bool>().unwrap());

    let collection = users();
    let hook = AcpCompositeMergeHook::new(None);
    hook.set_document_acp(Arc::new(Provider {
        remote: *provider == "remote",
        registered,
        owed: ReplicaOwed {
            registration: registration.then(|| did(is_registrant)),
            creator: Some(did(is_creator)),
            creator_grants,
            adopted,
        },
        holds,
    }));
    hook.set_strict_replicated_doc_access(strict);
    let listing: Vec<&str> = if listed { vec!["Users"] } else { vec![] };
    hook.set_replica_registration(Arc::new(ReplicaRegistration::new(
        listing,
        Arc::new(NoOwnActors),
    )));

    let signer = did(true);
    let judged = hook
        .on_protected_update(
            "doc",
            &collection,
            CompositeFrame {
                is_genesis: false,
                status: 1,
                signer: signed.then_some(&signer),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        outcome_name(judged),
        expected_hook,
        "hook judgement: {line}"
    );
    assert_eq!(
        hook.parks_refusals(&collection),
        expected_parks,
        "parks: {line}"
    );
}

#[test]
#[ignore = "requires Lean; executed explicitly by the Lean Conformance CI job"]
fn the_acp_hook_matches_the_executed_gate_model() {
    let output = Command::new("lake")
        .args(["env", "lean", "--run", "ReplicaGate.lean"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../proofs/lean"))
        .output()
        .expect("execute the gate model");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases = String::from_utf8(output.stdout).unwrap();
    let count = cases.lines().count();
    assert_eq!(count, 2048, "the model enumerates every combination");
    workers::block_on(async {
        for line in cases.lines() {
            check(line).await;
        }
    })
    .unwrap();
}
