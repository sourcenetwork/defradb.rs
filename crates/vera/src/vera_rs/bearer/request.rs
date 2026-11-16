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
//! The node checks the same binding before it sends anything, against the
//! digest of the command it is about to send. It does not know the
//! deployment's genesis identity, which vera.rs checks.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vera_client::DelegationScope;
use vera_crypto::jwt::JwtClaims;
use vera_crypto::operation::{OperationClaim, OperationId, MAX_OPERATION_TTL};

use super::{base64url_encode, header, BearerError, CallerDelegationError};
use crate::provider::ProviderError;
use crate::vera_rs::did_spelling;
use crate::vera_rs::signer::{admission, DelegationSigner};

/// The one request a delegation is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestBinding {
    /// The command's digest
    /// ([`request_digest`](crate::vera_rs::request_digest)).
    pub digest: [u8; 32],
    /// The genesis identity of the deployment the command is for.
    pub genesis_id: [u8; 32],
    /// Bytes the signer chooses once for this request, not all zero. With
    /// the expiry they are the operation identity vera.rs keeps the
    /// request's outcome under.
    pub nonce: [u8; 24],
}

/// The delegation `signer` gives `audience.worker` for the one request
/// `binding` names, on `audience.deployment`, for `lifetime`: at most 600
/// seconds, which is as long as vera.rs keeps a request open.
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

/// The request-bound delegation as it is issued at `now`, in seconds since
/// the epoch.
pub(in crate::vera_rs) async fn issue_request_at(
    signer: &dyn DelegationSigner,
    audience: &acp::DelegationAudience,
    binding: &RequestBinding,
    lifetime_secs: u64,
    now: u64,
) -> Result<String, BearerError> {
    let key = admission::actor_key(signer)?;
    if lifetime_secs == 0 || lifetime_secs > MAX_OPERATION_TTL {
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
        iat: now,
        nbf: now,
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

/// The DID that signed `token`, in DefraDB's spelling, when `token` is bound
/// to one request and vera.rs would execute the command with `digest` under
/// it now for `worker` on `deployment_id`. `None` when `token` does not
/// verify or is bound to no request.
pub(in crate::vera_rs) fn caller_of_request_delegation<D>(
    token: &str,
    worker: &str,
    deployment_id: u64,
    digest: D,
) -> Option<Result<String, CallerDelegationError>>
where
    D: FnOnce() -> Result<[u8; 32], ProviderError>,
{
    let claims = vera_crypto::jwt::verify_bearer_token(token).ok()?;
    let request = claims.request.clone()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CallerDelegationError::Clock)
        .map(|now| now.as_secs());
    Some(now.and_then(|now| bound_caller(&claims, &request, worker, deployment_id, digest, now)))
}

/// [`caller_of_request_delegation`] for verified claims, at `now`.
fn bound_caller<D>(
    claims: &JwtClaims,
    request: &OperationClaim,
    worker: &str,
    deployment_id: u64,
    digest: D,
    now: u64,
) -> Result<String, CallerDelegationError>
where
    D: FnOnce() -> Result<[u8; 32], ProviderError>,
{
    if claims.relay.is_some() || claims.scope != DelegationScope::PolicyCommands {
        return Err(CallerDelegationError::Kind);
    }
    claims.authorize(worker, deployment_id, now)?;
    request.id.validate(now)?;
    let digest = digest().map_err(|error| CallerDelegationError::Command(error.to_string()))?;
    if request.genesis_id == [0; 32]
        || claims.exp > request.id.expires_at()
        || request.digest != digest
    {
        return Err(CallerDelegationError::OtherRequest);
    }
    Ok(did_spelling::defra(&claims.iss).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vera_rs::bearer::{caller_delegation, caller_of_delegation, is_delegation};
    use crate::vera_rs::signer::Ed25519Signer;

    const WORKER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    const NOW: u64 = 1_900_000_000;

    fn audience() -> acp::DelegationAudience {
        acp::DelegationAudience {
            worker: WORKER.into(),
            deployment: 9,
        }
    }

    fn binding(digest: u8) -> RequestBinding {
        RequestBinding {
            digest: [digest; 32],
            genesis_id: [3; 32],
            nonce: [5; 24],
        }
    }

    fn caller_at(token: &str, digest: u8, now: u64) -> Result<String, CallerDelegationError> {
        let claims = vera_crypto::jwt::verify_bearer_token(token).expect("token verifies");
        let request = claims.request.clone().expect("request claim");
        bound_caller(&claims, &request, WORKER, 9, || Ok([digest; 32]), now)
    }

    #[nuclei::test]
    async fn a_request_delegation_is_taken_for_its_request_and_no_other() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let token = issue_request_at(&signer, &audience(), &binding(1), 60, NOW)
            .await
            .expect("delegation");
        assert_eq!(caller_at(&token, 1, NOW).unwrap(), signer.did());
        assert_eq!(caller_at(&token, 1, NOW + 59).unwrap(), signer.did());
        assert!(matches!(
            caller_at(&token, 2, NOW),
            Err(CallerDelegationError::OtherRequest)
        ));
        // At its deadline the operation identity is closed.
        assert!(matches!(
            caller_at(&token, 1, NOW + 60),
            Err(CallerDelegationError::Token(_))
        ));
        assert!(matches!(
            caller_at(&token, 1, NOW - 1),
            Err(CallerDelegationError::Token(_))
        ));

        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        let request = claims.request.clone().unwrap();
        for (worker, deployment) in [(signer.did(), 9), (WORKER, 10)] {
            assert!(
                bound_caller(&claims, &request, worker, deployment, || Ok([1; 32]), NOW).is_err()
            );
        }
        assert!(matches!(
            bound_caller(
                &claims,
                &request,
                WORKER,
                9,
                || Err(ProviderError::Config("no such command".into())),
                NOW
            ),
            Err(CallerDelegationError::Command(_))
        ));
    }

    #[nuclei::test]
    async fn a_request_delegation_is_not_a_callers_credential() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let bound = request_delegation(&signer, &audience(), &binding(1), Duration::from_secs(60))
            .await
            .expect("delegation");
        assert!(is_delegation(&bound));
        assert!(matches!(
            caller_of_delegation(&bound, WORKER, 9),
            Err(CallerDelegationError::Kind)
        ));
        assert_eq!(
            caller_of_request_delegation(&bound, WORKER, 9, || Ok([1; 32]))
                .expect("bound to a request")
                .unwrap(),
            signer.did()
        );

        let plain = caller_delegation(&signer, &audience(), Duration::from_secs(60))
            .await
            .expect("delegation");
        assert!(caller_of_request_delegation(&plain, WORKER, 9, || Ok([1; 32])).is_none());
        let mut forged = bound.clone();
        forged.push('A');
        assert!(caller_of_request_delegation(&forged, WORKER, 9, || Ok([1; 32])).is_none());
    }

    #[nuclei::test]
    async fn a_request_delegation_lasts_no_longer_than_vera_rs_keeps_a_request_open() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        for lifetime in [0, MAX_OPERATION_TTL + 1] {
            assert!(matches!(
                issue_request_at(&signer, &audience(), &binding(1), lifetime, NOW).await,
                Err(BearerError::RequestLifetime)
            ));
        }
        let longest = issue_request_at(&signer, &audience(), &binding(1), MAX_OPERATION_TTL, NOW)
            .await
            .expect("delegation");
        assert_eq!(caller_at(&longest, 1, NOW).unwrap(), signer.did());
        for unbound in [
            RequestBinding {
                nonce: [0; 24],
                ..binding(1)
            },
            RequestBinding {
                genesis_id: [0; 32],
                ..binding(1)
            },
        ] {
            assert!(matches!(
                issue_request_at(&signer, &audience(), &unbound, 60, NOW).await,
                Err(BearerError::Claims(_))
            ));
        }
    }

    #[nuclei::test]
    async fn vera_rs_takes_the_token_as_its_own_builder_makes_it() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let token = issue_request_at(&signer, &audience(), &binding(1), 60, NOW)
            .await
            .expect("delegation");
        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        // vera.rs's own builder refuses claims its node would refuse.
        vera_client::UnsignedToken::operation(&claims).expect("an operation token");
        assert_eq!(claims.scope, DelegationScope::PolicyCommands);
        assert_eq!(claims.sub, WORKER);
        assert_eq!(claims.aud, "vera:9");
        assert_eq!(claims.request.unwrap().id.expires_at(), NOW + 60);
    }
}
