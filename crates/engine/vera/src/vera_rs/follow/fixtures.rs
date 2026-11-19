//! Signed requests and a deployment for the term tests.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use alloy_primitives::B256;
use base64::Engine as _;
use defra_core::thread_bounds::MaybeBoxFuture;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use vera_domain::NativeTx;

use super::{Chain, Confirmation, Replayed};
use crate::provider::ProviderError;
use crate::vera_rs::{abi::ACP_ADDRESS, policy_cmd};
use crate::{PolicyAction, PolicyCommand, PolicySubject, SubjectRef};

pub(super) const POLICY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
pub(super) const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
pub(super) const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

/// A delegation issued by `actor`, as far as the term check reads one.
pub(super) fn token(actor: &str) -> String {
    let encode = |value: serde_json::Value| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
    };
    format!(
        "{}.{}.signature",
        encode(serde_json::json!({ "typ": "vera-delegation-v1+jwt" })),
        encode(serde_json::json!({ "iss": actor }))
    )
}

/// The request with `sequence` that sends `action` on `object`.
pub(super) fn wire(sequence: u64, object: &str, action: PolicyAction<'_>) -> Vec<u8> {
    let command = PolicyCommand {
        policy_id: POLICY,
        resource: "users",
        object_id: object,
        action,
    };
    let typed = policy_cmd::typed(&command).expect("command");
    let calldata = policy_cmd::calldata(&token(OWNER), POLICY, &typed).expect("calldata");
    sign(sequence, calldata)
}

/// The request with `sequence` that archives `object`.
pub(super) fn archive(sequence: u64, object: &str) -> Vec<u8> {
    let calldata = policy_cmd::calldata(
        &token(OWNER),
        POLICY,
        &policy_cmd::archive_object("users", object),
    )
    .expect("calldata");
    sign(sequence, calldata)
}

fn sign(sequence: u64, calldata: alloy_primitives::Bytes) -> Vec<u8> {
    vera_client::BlsSigner::new(7u64.into(), 9001)
        .expect("signer")
        .sign_native_tx_with_sequence(ACP_ADDRESS, calldata, sequence)
        .expect("signed")
}

pub(super) fn register() -> PolicyAction<'static> {
    PolicyAction::RegisterObject
}

pub(super) fn grant(subject: &SubjectRef) -> PolicyAction<'_> {
    PolicyAction::SetRelationship {
        relation: "reader",
        subject: PolicySubject::Actor(subject),
    }
}

pub(super) fn revoke(subject: &SubjectRef) -> PolicyAction<'_> {
    PolicyAction::DeleteRelationship {
        relation: "reader",
        subject: PolicySubject::Actor(subject),
    }
}

/// The confirmation of `wire`, at `height`.
pub(super) fn confirmed(wire: Vec<u8>, height: u64) -> Confirmation {
    let tx = NativeTx::decode_wire(&wire).expect("wire");
    Confirmation {
        sequence: tx.nonce,
        submission: tx.tx_id().0,
        height,
        wire,
    }
}

/// A deployment after a switch: the submissions it holds, how it answers a
/// replay of each one, and what it was sent.
#[derive(Default)]
pub(super) struct Deployment {
    held: HopscotchMap<B256, (), RandomState>,
    refuses: HopscotchMap<B256, (), RandomState>,
    /// A replay after this many fails, as an unreachable node does.
    pub unreachable_after: Option<usize>,
    /// The replay of this index is executed, and its answer is lost.
    pub answer_lost_at: Option<usize>,
    replays: AtomicUsize,
    pub sent: kovan_queue::seg_queue::SegQueue<u64>,
}

impl Deployment {
    pub(super) fn holding<'a>(held: impl IntoIterator<Item = &'a Confirmation>) -> Self {
        let deployment = Self::default();
        for confirmation in held {
            deployment.held.insert(confirmation.submission, ());
        }
        deployment
    }

    /// A replay of `confirmation` is refused.
    pub(super) fn refuse(self, confirmation: &Confirmation) -> Self {
        self.refuses.insert(confirmation.submission, ());
        self
    }

    /// The sequences sent again, in order.
    pub(super) fn replayed(&self) -> Vec<u64> {
        std::iter::from_fn(|| self.sent.pop()).collect()
    }
}

impl Chain for Deployment {
    fn holds(&self, submission: B256) -> MaybeBoxFuture<'_, Result<bool, ProviderError>> {
        Box::pin(async move { Ok(self.held.get(&submission).is_some()) })
    }

    fn replay<'a>(&'a self, wire: &'a [u8]) -> MaybeBoxFuture<'a, Result<Replayed, ProviderError>> {
        Box::pin(async move {
            let done = self.replays.fetch_add(1, Ordering::Relaxed);
            if self.unreachable_after.is_some_and(|limit| done >= limit) {
                return Err(ProviderError::Unavailable("no route".into()));
            }
            let tx = NativeTx::decode_wire(wire).expect("wire");
            let submission = tx.tx_id().0;
            self.sent.push(tx.nonce);
            self.held.insert(submission, ());
            if self.answer_lost_at == Some(done) {
                return Err(ProviderError::Unavailable("connection reset".into()));
            }
            Ok(Replayed {
                executed: self.refuses.get(&submission).is_none(),
                height: 100 + tx.nonce,
            })
        })
    }
}

/// The journal's entries, by sequence.
pub(super) fn entries(journal: &super::Journal) -> BTreeMap<u64, Confirmation> {
    journal
        .sequences()
        .map(|sequence| (sequence, journal.entry(sequence).expect("entry")))
        .collect()
}
