//! Delegations bound to one request.
//!
//! A plain delegation lets a node send any policy command for its signer
//! until it expires. One bound to a request carries vera.rs's `request`
//! claim: the digest of one command, the deployment's genesis identity and
//! an operation identity that expires with the token. vera.rs executes
//! exactly that command under it, within 600 seconds of its expiry, and
//! keeps the outcome under the operation identity, so sending it a second
//! time changes nothing.
//!
//! A node checks the same binding before it sends anything, against the
//! digest of the command it is about to send and, when its provider knows
//! it, the deployment's genesis identity. A provider that does not know it
//! leaves that check to vera.rs.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vera_crypto::jwt::{DelegationScope, JwtClaims};
use vera_crypto::operation::{OperationClaim, OperationId, MAX_OPERATION_TTL};

use super::{base64url_encode, header, BearerError};
use crate::signer::{admission, DelegationSigner};
use crate::{did_spelling, request_digest, PolicyCommand};

/// The longest a delegation bound to one request lasts: as long as vera.rs
/// keeps a request open, less the [`acp::CLOCK_SKEW_ALLOWANCE`]. vera.rs
/// refuses an operation identity that expires more than
/// [`MAX_OPERATION_TTL`] after the block that executes it, so a longer one
/// from a signer whose clock is ahead would be refused.
pub const MAX_REQUEST_LIFETIME: Duration =
    Duration::from_secs(MAX_OPERATION_TTL - acp::CLOCK_SKEW_ALLOWANCE.as_secs());

/// The one request a delegation is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestBinding {
    /// The command's digest
    /// ([`request_digest`](crate::request_digest)).
    pub digest: [u8; 32],
    /// The genesis identity of the deployment the command is for, as the
    /// node reports it (`DelegationAudience::genesis_id`).
    pub genesis_id: [u8; 32],
    /// Bytes the signer chooses once for this request, not all zero. With
    /// the expiry they are the operation identity vera.rs keeps the
    /// request's outcome under.
    pub nonce: [u8; 24],
}

impl RequestBinding {
    /// The binding to `command` on the deployment `audience` names, the
    /// audience a node reports (`LightClientStatus::delegation`), under
    /// `nonce`. Refused when the node does not know its deployment's
    /// genesis identity.
    pub fn for_command(
        audience: &acp::DelegationAudience,
        command: &PolicyCommand<'_>,
        nonce: [u8; 24],
    ) -> Result<Self, BearerError> {
        let genesis_id = audience.genesis_id.ok_or(BearerError::NoGenesisIdentity)?;
        let digest = request_digest(command).map_err(|e| BearerError::Command(e.to_string()))?;
        Ok(Self {
            digest,
            genesis_id,
            nonce,
        })
    }
}

/// The delegation `signer` gives `audience.worker` for the one request
/// `binding` names, on `audience.deployment`, for `lifetime`: at most
/// [`MAX_REQUEST_LIFETIME`]. It is issued at the signer's clock; a signer that
/// knows the provider's time issues it there with [`request_delegation_at`].
pub async fn request_delegation(
    signer: &dyn DelegationSigner,
    audience: &acp::DelegationAudience,
    binding: &RequestBinding,
    lifetime: Duration,
) -> Result<String, BearerError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| BearerError::Time(e.to_string()))?
        .as_secs();
    issue_request_at(signer, audience, binding, lifetime.as_secs(), now).await
}

/// [`request_delegation`], issued at `issued_at` in seconds since the epoch:
/// the provider's time a node reports
/// ([`LightClientStatus::chain_time`](acp::LightClientStatus::chain_time)), or
/// the one a refusal names
/// ([`DelegationWindow::provider_time`](acp::DelegationWindow::provider_time)),
/// so that the signer's own clock does not decide whether it holds.
pub async fn request_delegation_at(
    signer: &dyn DelegationSigner,
    audience: &acp::DelegationAudience,
    binding: &RequestBinding,
    lifetime: Duration,
    issued_at: u64,
) -> Result<String, BearerError> {
    issue_request_at(signer, audience, binding, lifetime.as_secs(), issued_at).await
}

/// The request-bound delegation as it is issued at `now`, in seconds since
/// the epoch.
async fn issue_request_at(
    signer: &dyn DelegationSigner,
    audience: &acp::DelegationAudience,
    binding: &RequestBinding,
    lifetime_secs: u64,
    now: u64,
) -> Result<String, BearerError> {
    let key = admission::actor_key(signer)?;
    if lifetime_secs == 0 || lifetime_secs > MAX_REQUEST_LIFETIME.as_secs() {
        return Err(BearerError::RequestLifetime);
    }
    if binding.nonce == [0; 24] || binding.genesis_id == [0; 32] {
        return Err(BearerError::Claims(
            "a request needs a nonce and a genesis identity that are not zero".into(),
        ));
    }
    let expiry = now
        .checked_add(lifetime_secs)
        .ok_or_else(|| BearerError::Time("invalid delegation lifetime".into()))?;
    let mut id = [0; 32];
    id[..8].copy_from_slice(&expiry.to_be_bytes());
    id[8..].copy_from_slice(&binding.nonce);
    let claims = JwtClaims {
        iss: did_spelling::wire(signer.did()).into_owned(),
        sub: audience.worker.clone(),
        exp: expiry,
        aud: format!("vera:{}", audience.deployment),
        scope: DelegationScope::PolicyCommands,
        // vera.rs's own builder writes an operation token with `nbf = iat`,
        // and checks its operation identity at `iat`: both are set back.
        iat: super::not_before(now),
        nbf: super::not_before(now),
        relay: None,
        request: Some(OperationClaim {
            id: OperationId(id),
            digest: binding.digest,
            genesis_id: binding.genesis_id,
        }),
    };
    let payload = serde_json::to_string(&claims).map_err(|e| BearerError::Claims(e.to_string()))?;
    let message = format!(
        "{}.{}",
        base64url_encode(header(signer.algorithm())),
        base64url_encode(payload.as_bytes())
    );
    let signature = signer.sign(message.as_bytes()).await?;
    admission::admit(signer, &key, message.as_bytes(), &signature)?;
    Ok(format!("{message}.{}", base64url_encode(&signature)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    const NOW: u64 = 1_900_000_000;
    const GENESIS: [u8; 32] = [3; 32];

    fn audience() -> acp::DelegationAudience {
        acp::DelegationAudience {
            worker: WORKER.into(),
            deployment: 9,
            genesis_id: Some(GENESIS),
        }
    }

    fn binding() -> RequestBinding {
        RequestBinding {
            digest: [1; 32],
            genesis_id: GENESIS,
            nonce: [5; 24],
        }
    }

    #[nuclei::test]
    async fn a_request_delegation_names_its_request_and_expires_with_its_operation_identity() {
        let signer = crate::Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let token = issue_request_at(&signer, &audience(), &binding(), 60, NOW)
            .await
            .expect("delegation");
        let claims = vera_crypto::jwt::verify_bearer_token(&token).expect("token verifies");
        let request = claims.request.expect("request claim");
        assert_eq!(request.digest, [1; 32]);
        assert_eq!(request.genesis_id, GENESIS);
        assert_eq!(request.id.expires_at(), NOW + 60);
        assert_eq!(claims.exp, NOW + 60);
        assert_eq!(claims.iat, NOW - acp::CLOCK_SKEW_ALLOWANCE.as_secs());
        assert_eq!(claims.nbf, claims.iat);
        assert_eq!(claims.scope, DelegationScope::PolicyCommands);
        assert_eq!(claims.sub, WORKER);
        assert_eq!(claims.aud, "vera:9");
    }

    #[nuclei::test]
    async fn a_request_delegation_outside_its_lifetime_or_without_its_binding_is_not_issued() {
        let signer = crate::Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let longest = MAX_REQUEST_LIFETIME.as_secs();
        assert_eq!(longest, MAX_OPERATION_TTL - 30);
        for lifetime in [0, longest + 1, MAX_OPERATION_TTL] {
            assert!(matches!(
                issue_request_at(&signer, &audience(), &binding(), lifetime, NOW).await,
                Err(BearerError::RequestLifetime)
            ));
        }
        assert!(
            issue_request_at(&signer, &audience(), &binding(), longest, NOW)
                .await
                .is_ok()
        );
        for unbound in [
            RequestBinding {
                nonce: [0; 24],
                ..binding()
            },
            RequestBinding {
                genesis_id: [0; 32],
                ..binding()
            },
        ] {
            assert!(matches!(
                issue_request_at(&signer, &audience(), &unbound, 60, NOW).await,
                Err(BearerError::Claims(_))
            ));
        }
    }

    #[test]
    fn a_binding_is_made_from_the_reported_audience_and_the_commands_digest() {
        let command = PolicyCommand {
            policy_id: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            resource: "users",
            object_id: "bae-first",
            action: crate::PolicyAction::RegisterObject,
        };
        let made = RequestBinding::for_command(&audience(), &command, [5; 24]).expect("binding");
        assert_eq!(made.genesis_id, GENESIS);
        assert_eq!(made.digest, request_digest(&command).unwrap());
        assert_eq!(made.nonce, [5; 24]);

        let unknown = acp::DelegationAudience {
            genesis_id: None,
            ..audience()
        };
        assert!(matches!(
            RequestBinding::for_command(&unknown, &command, [5; 24]),
            Err(BearerError::NoGenesisIdentity)
        ));
        let unparsable = PolicyCommand {
            policy_id: "not a policy",
            ..command
        };
        assert!(matches!(
            RequestBinding::for_command(&audience(), &unparsable, [5; 24]),
            Err(BearerError::Command(_))
        ));
    }
}
