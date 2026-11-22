//! The registrations one commit owes an external provider, settled together.
//!
//! Each create that owes a registration (see [`crate::pending`]) adds a hook
//! to its transaction, as it always did, so a statement rolled back to its
//! savepoint drops its own hooks with its records. The hooks of one
//! transaction share what they owe ([`CommitRegistrations`]): the first to
//! run after the commit settles every registration kept by then, those one
//! creator owes in one call ([`DocumentACP::redrive_registrations`]), and the
//! later ones find nothing left. A registration whose record a rolled-back
//! statement took with it is owed nothing and settles as nothing.

use std::sync::Arc;

use identity::Did;
use regolith::sync::SegQueue;
use storage::corekv::CommitHook;

use crate::dac::{with_request_bearer, DocumentACP};
use crate::object_registration::AcpObject;

/// One registration a commit settles: the document, and the creator and the
/// bearer of the request that created it.
pub struct CommitRegistration {
    acp: Arc<dyn DocumentACP>,
    creator: Did,
    bearer_token: Option<String>,
    policy_id: String,
    resource_name: String,
    doc_id: String,
}

impl CommitRegistration {
    pub fn new(
        acp: Arc<dyn DocumentACP>,
        creator: Did,
        bearer_token: Option<String>,
        policy_id: String,
        resource_name: String,
        doc_id: String,
    ) -> Self {
        Self {
            acp,
            creator,
            bearer_token,
            policy_id,
            resource_name,
            doc_id,
        }
    }

    /// The hook that settles this registration alone, for a mutator that
    /// keeps no transaction's registrations together.
    pub fn into_hook(self) -> CommitHook {
        Box::new(move || Box::pin(settle(vec![self])))
    }
}

/// What one transaction's hooks owe, shared between them.
#[derive(Default)]
pub struct CommitRegistrations {
    owed: SegQueue<CommitRegistration>,
}

impl CommitRegistrations {
    /// Keep `registration` and return the hook its transaction runs for it:
    /// whichever of this transaction's hooks runs first settles everything
    /// kept by then.
    pub fn owe(self: &Arc<Self>, registration: CommitRegistration) -> CommitHook {
        self.owed.push(registration);
        let shared = Arc::clone(self);
        Box::new(move || {
            Box::pin(async move {
                let mut owed = Vec::new();
                while let Some(registration) = shared.owed.pop() {
                    owed.push(registration);
                }
                settle(owed).await
            })
        })
    }
}

/// One creator's registrations under one bearer, in the order they were
/// owed, each document once.
struct Group {
    acp: Arc<dyn DocumentACP>,
    creator: Did,
    bearer_token: Option<String>,
    objects: Vec<(String, String, String)>,
}

fn grouped(owed: Vec<CommitRegistration>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for registration in owed {
        let object = (
            registration.policy_id,
            registration.resource_name,
            registration.doc_id,
        );
        let group = groups.iter_mut().find(|group| {
            group.creator == registration.creator && group.bearer_token == registration.bearer_token
        });
        match group {
            Some(group) if group.objects.contains(&object) => {}
            Some(group) => group.objects.push(object),
            None => groups.push(Group {
                acp: registration.acp,
                creator: registration.creator,
                bearer_token: registration.bearer_token,
                objects: vec![object],
            }),
        }
    }
    groups
}

/// Settle `owed`, each creator's registrations in one call under the bearer
/// its request carried. Every failure is reported, each naming its document.
async fn settle(owed: Vec<CommitRegistration>) -> Result<(), String> {
    let mut failures = Vec::new();
    for group in grouped(owed) {
        let objects = group.objects;
        let outcomes = with_request_bearer(
            &group.acp,
            Some(group.creator.to_string()),
            group.bearer_token,
            |acp| {
                let objects = objects.clone();
                async move {
                    let refs: Vec<AcpObject<'_>> = objects
                        .iter()
                        .map(|(policy_id, resource, object_id)| AcpObject {
                            policy_id,
                            resource,
                            object_id,
                        })
                        .collect();
                    acp.redrive_registrations(&refs).await
                }
            },
        )
        .await;
        for ((_, _, doc_id), outcome) in objects.iter().zip(outcomes) {
            if let Err(error) = outcome {
                failures.push(format!("failed to register ACP document {doc_id}: {error}"));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
