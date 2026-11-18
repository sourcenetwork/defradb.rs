//! Delegations bound to one request, as document ACP admits them.
//!
//! A token an actor bound to one request lets the node send exactly the
//! command it names. Every place a token meets a command asks here first:
//! when a transaction asks what lets the node send what it owes, when a
//! command goes to the provider directly, and when an owed change is
//! submitted. A token bound to another command is refused on this node,
//! before anything is sent, and so is a plain delegation that is no longer
//! one the provider takes.

use acp::{OwedOp, OwedSubject, ProviderCommand};

use super::VeraDocumentACP;
use crate::provider::{ProviderError, SubjectRef};
use crate::{PolicyAction, PolicyCommand, PolicySubject};

impl VeraDocumentACP {
    /// Whether `token`, bound to one request, lets the node send `command`
    /// as `actor` now, or why not: [`acp::Error::DelegationOutsideWindow`]
    /// when it does not hold at the node's time, else
    /// [`acp::Error::OwedAuthorityUnavailable`]. `None` when `token` is bound
    /// to no request.
    pub(super) fn request_bound(
        &self,
        actor: &str,
        token: &str,
        command: &PolicyCommand<'_>,
    ) -> Option<acp::Result<()>> {
        let refused = |reason: String| acp::Error::OwedAuthorityUnavailable {
            actor: actor.to_owned(),
            reason,
        };
        Some(
            match self.provider.caller_of_request_delegation(token, command)? {
                Ok(caller)
                    if self.provider.canonical_actor(&caller)
                        == self.provider.canonical_actor(actor) =>
                {
                    Ok(())
                }
                Ok(caller) => Err(refused(format!(
                    "the request's delegation is signed by {caller}"
                ))),
                Err(error) => Err(delegation_refused(actor, error)),
            },
        )
    }

    /// Whether the node may send `command` as `actor` under `token` now:
    /// the checks made when the command was admitted, made again as it is
    /// sent. A key the node holds for `actor` signed `token` a moment ago.
    /// A token bound to one request must name `command`. Any other token
    /// must be a delegation the provider takes for `actor` now, and is not
    /// taken where the request presents one bound to a request. A provider
    /// that takes no delegation is passed its caller's token as it is.
    pub(super) fn admit_for(
        &self,
        actor: &str,
        token: &str,
        command: &PolicyCommand<'_>,
    ) -> acp::Result<()> {
        let canonical = self.provider.canonical_actor(actor);
        if self.provider.holds_key_for(actor) || self.provider.holds_key_for(&canonical) {
            return Ok(());
        }
        let refused = |reason: String| acp::Error::OwedAuthorityUnavailable {
            actor: actor.to_owned(),
            reason,
        };
        match self.request_bound(actor, token, command) {
            Some(bound) => return bound,
            None if defra_core::RequestContext::bound_or_ambient(self.context.as_ref())
                .bearer_names_request() =>
            {
                return Err(refused(
                    "the delegation presented as bound to one request is bound to none".into(),
                ))
            }
            None => {}
        }
        match self.provider.caller_of_delegation(token) {
            Some(Ok(caller)) if self.provider.canonical_actor(caller.did.as_str()) == canonical => {
                Ok(())
            }
            Some(Ok(caller)) => Err(refused(format!(
                "the request's delegation is signed by {}",
                caller.did
            ))),
            Some(Err(error)) => Err(delegation_refused(actor, error)),
            None if self.provider.delegation_audience().is_some() => Err(refused(
                "the request's token is not a delegation to this node".into(),
            )),
            None => Ok(()),
        }
    }
}

/// The refusal of `actor`'s delegation for `error`: one that does not hold
/// at the provider's time keeps its window, so the caller can issue it again.
pub(super) fn delegation_refused(actor: &str, error: ProviderError) -> acp::Error {
    match error {
        ProviderError::DelegationOutsideWindow(window) => acp::Error::DelegationOutsideWindow {
            actor: actor.to_owned(),
            window,
        },
        other => acp::Error::OwedAuthorityUnavailable {
            actor: actor.to_owned(),
            reason: other.to_string(),
        },
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
