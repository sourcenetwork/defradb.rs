//! A lost change as the outbox owes it.
//!
//! The change is made again as the DID that issued the delegation it was
//! sent under. When the node holds that DID's key it signs a fresh one at
//! submission; otherwise the delegation is kept as the authority, and one
//! that has expired by then parks the change as awaiting authority, as the
//! outbox does for any change, until its caller hands in a fresh one.

use base64::Engine as _;

use super::super::did_spelling;
use super::effect::{Lost, LostSubject};

#[derive(serde::Deserialize)]
struct Claims {
    iss: String,
    #[serde(default)]
    request: Option<serde_json::Value>,
}

/// What the delegation `token` names, read without checking it: the
/// deployment checked it when it executed the request.
fn claims(token: &str) -> Option<Claims> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// `lost` as the outbox records it. `holds_key` says whether the node holds
/// an actor's key. `None` for a change whose delegation names no actor the
/// outbox can record.
pub(crate) fn owed_again<F>(lost: &Lost, holds_key: F) -> Option<acp::VoidedEffect>
where
    F: Fn(&str) -> bool,
{
    let claims = claims(lost.token())?;
    let actor = did_spelling::defra(&claims.iss).into_owned();
    let authority = if holds_key(&actor) {
        acp::OwedAuthority::NodeKey
    } else {
        acp::OwedAuthority::Delegation {
            token: lost.token().to_owned(),
        }
    };
    let actor = identity::Did::new(actor).ok()?;
    Some(match lost {
        Lost::Registration {
            policy_id,
            resource,
            object,
            relationships,
            ..
        } => acp::VoidedEffect::Registration {
            creator: actor,
            policy_id: policy_id.clone(),
            resource: resource.clone(),
            object: object.clone(),
            relationships: relationships.clone(),
            request_bound: claims.request.is_some()
                && matches!(authority, acp::OwedAuthority::Delegation { .. }),
            authority,
        },
        Lost::Archive {
            policy_id,
            resource,
            object,
            ..
        } => acp::VoidedEffect::Change(acp::NewOwedChange {
            txn: None,
            tuple: acp::OwedTuple::archive(policy_id, resource, object),
            op: acp::OwedOp::Archive,
            actor,
            authority,
            authority_checked: true,
        }),
        Lost::Relationship {
            add,
            policy_id,
            resource,
            object,
            relation,
            subject,
            ..
        } => acp::VoidedEffect::Change(acp::NewOwedChange {
            txn: None,
            tuple: acp::OwedTuple {
                policy_id: policy_id.clone(),
                resource: resource.clone(),
                object: object.clone(),
                relation: relation.clone(),
                subject: owed_subject(subject)?,
            },
            op: if *add {
                acp::OwedOp::Add
            } else {
                acp::OwedOp::Delete
            },
            actor,
            authority,
            authority_checked: true,
        }),
    })
}

/// The DID `lost`'s delegation names, without checking it: the deployment
/// checked it when the request was confirmed.
pub(crate) fn token_actor(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&claims).ok()?;
    Some(super::super::did_spelling::defra(claims.get("iss")?.as_str()?).into_owned())
}

/// The subject, as the permission engine writes it.
pub(crate) fn subject_string(subject: &LostSubject) -> Option<String> {
    owed_subject(subject).map(|subject| acp::Subject::from(subject).to_string())
}

fn owed_subject(subject: &LostSubject) -> Option<acp::OwedSubject> {
    Some(match subject {
        LostSubject::Actor { did } => acp::OwedSubject::Actor {
            did: identity::Did::new(did.as_str()).ok()?,
        },
        LostSubject::Wildcard => acp::OwedSubject::Wildcard,
        LostSubject::EntitySet {
            resource,
            object,
            relation,
        } => acp::OwedSubject::EntitySet {
            resource: resource.clone(),
            object: object.clone(),
            relation: relation.clone(),
        },
        LostSubject::TypedWildcard { resource } => acp::OwedSubject::TypedWildcard {
            resource: resource.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{token, OWNER, POLICY, READER};
    use super::*;

    fn grant(token: String) -> Lost {
        Lost::Relationship {
            add: true,
            policy_id: POLICY.into(),
            resource: "users".into(),
            object: "bae-1".into(),
            relation: "reader".into(),
            subject: LostSubject::Actor { did: READER.into() },
            token,
        }
    }

    #[test]
    fn a_lost_change_is_owed_as_its_delegations_issuer_under_the_key_the_node_holds_or_the_delegation(
    ) {
        let Some(acp::VoidedEffect::Change(held)) =
            owed_again(&grant(token(OWNER)), |did| did == OWNER)
        else {
            panic!("a change");
        };
        assert_eq!(held.actor.as_str(), OWNER);
        assert_eq!(held.authority, acp::OwedAuthority::NodeKey);
        assert_eq!(held.op, acp::OwedOp::Add);
        assert!(held.authority_checked);

        let Some(acp::VoidedEffect::Change(remote)) = owed_again(&grant(token(OWNER)), |_| false)
        else {
            panic!("a change");
        };
        assert_eq!(
            remote.authority,
            acp::OwedAuthority::Delegation {
                token: token(OWNER)
            }
        );
        assert_eq!(owed_again(&grant("not a token".into()), |_| true), None);
    }
}
