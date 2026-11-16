//! Delegations bound to one request, as document ACP admits them.
//!
//! A token an actor bound to one request lets the node send exactly the
//! command it names. Every place a token meets a command asks here first:
//! when a transaction asks what lets the node send what it owes, when a
//! command goes to the provider directly, and when an owed change is
//! submitted. A token bound to another command is refused on this node,
//! before anything is sent.

use acp::{OwedOp, OwedSubject, ProviderCommand};

use super::VeraDocumentACP;
use crate::policy_command::{PolicyAction, PolicyCommand, PolicySubject};
use crate::provider::SubjectRef;

impl VeraDocumentACP {
    /// Whether `token`, bound to one request, lets the node send `command`
    /// as `actor` now, or why not. `None` when `token` is bound to no
    /// request.
    pub(super) fn request_bound(
        &self,
        actor: &str,
        token: &str,
        command: &PolicyCommand<'_>,
    ) -> Option<Result<(), String>> {
        Some(
            match self.provider.caller_of_request_delegation(token, command)? {
                Ok(caller)
                    if self.provider.canonical_actor(&caller)
                        == self.provider.canonical_actor(actor) =>
                {
                    Ok(())
                }
                Ok(caller) => Err(format!("the request's delegation is signed by {caller}")),
                Err(error) => Err(error.to_string()),
            },
        )
    }

    /// Refuse `token` for `command` when it is bound to another request.
    pub(super) fn admit_for(
        &self,
        actor: &str,
        token: &str,
        command: &PolicyCommand<'_>,
    ) -> acp::Result<()> {
        match self.request_bound(actor, token, command) {
            Some(Err(reason)) => Err(acp::Error::OwedAuthorityUnavailable {
                actor: actor.to_owned(),
                reason,
            }),
            _ => Ok(()),
        }
    }
}

/// `command` as a provider is sent it, handed to `then`; `None` for a
/// relationship whose subject no provider command carries.
pub(super) fn with_policy_command<T>(
    command: ProviderCommand<'_>,
    then: impl FnOnce(Option<&PolicyCommand<'_>>) -> T,
) -> T {
    match command {
        ProviderCommand::RegisterObject {
            policy_id,
            resource,
            object,
        } => then(Some(&PolicyCommand {
            policy_id,
            resource,
            object_id: object,
            action: PolicyAction::RegisterObject,
        })),
        ProviderCommand::Relationship { tuple, op } => {
            let relation = tuple.relation.as_str();
            let action = |subject| match op {
                OwedOp::Add => PolicyAction::SetRelationship { relation, subject },
                OwedOp::Delete => PolicyAction::DeleteRelationship { relation, subject },
            };
            let sent = |action| PolicyCommand {
                policy_id: &tuple.policy_id,
                resource: &tuple.resource,
                object_id: &tuple.object,
                action,
            };
            let actor = match &tuple.subject {
                OwedSubject::Actor { did } => Some(SubjectRef::Actor(did.to_string())),
                OwedSubject::Wildcard => Some(SubjectRef::AllActors),
                OwedSubject::EntitySet { .. } | OwedSubject::TypedWildcard { .. } => None,
            };
            if let Some(actor) = &actor {
                return then(Some(&sent(action(PolicySubject::Actor(actor)))));
            }
            let subject = zanzibar::Subject::from(tuple.subject.clone());
            match zanzibar::encode_subject(&subject) {
                Ok((kind, resource, object_id, relation))
                    if matches!(subject, zanzibar::Subject::EntitySet { .. }) =>
                {
                    then(Some(&sent(action(PolicySubject::Structured {
                        kind,
                        resource: &resource,
                        object_id: &object_id,
                        relation: &relation,
                    }))))
                }
                _ => then(None),
            }
        }
    }
}
