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
//! digest of the command it is about to send and, when its provider knows
//! it, the deployment's genesis identity. A provider that does not know it
//! leaves that check to vera.rs.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vera_client::DelegationScope;
use vera_crypto::jwt::JwtClaims;
use vera_crypto::operation::{OperationClaim, OperationId, MAX_OPERATION_TTL};

use super::{base64url_encode, header, BearerError, CallerDelegationError};
use crate::provider::ProviderError;
use crate::vera_rs::signer::{admission, DelegationSigner};
use crate::vera_rs::{did_spelling, request_digest};
use crate::PolicyCommand;

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
    /// ([`request_digest`](crate::vera_rs::request_digest)).
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
pub(in crate::vera_rs) async fn issue_request_at(
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

/// The DID that signed `token`, in DefraDB's spelling, when `token` is bound
/// to one request and vera.rs would execute the command with `digest` under
/// it now for `audience`: its worker, its deployment and, when `audience`
/// knows it, its genesis identity. `None` when `token` does not verify or is
/// bound to no request.
pub(in crate::vera_rs) fn caller_of_request_delegation<D>(
    token: &str,
    audience: &acp::DelegationAudience,
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
    Some(now.and_then(|now| bound_caller(&claims, &request, audience, digest, now)))
}

/// [`caller_of_request_delegation`] for verified claims, at `now`.
fn bound_caller<D>(
    claims: &JwtClaims,
    request: &OperationClaim,
    audience: &acp::DelegationAudience,
    digest: D,
    now: u64,
) -> Result<String, CallerDelegationError>
where
    D: FnOnce() -> Result<[u8; 32], ProviderError>,
{
    if claims.relay.is_some() || claims.scope != DelegationScope::PolicyCommands {
        return Err(CallerDelegationError::Kind);
    }
    super::authorize_at(claims, &audience.worker, audience.deployment, now)?;
    request.id.validate(now)?;
    if audience
        .genesis_id
        .is_some_and(|genesis_id| genesis_id != request.genesis_id)
    {
        return Err(CallerDelegationError::OtherGenesis);
    }
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

    const GENESIS: [u8; 32] = [3; 32];

    fn audience() -> acp::DelegationAudience {
        acp::DelegationAudience {
            worker: WORKER.into(),
            deployment: 9,
            genesis_id: Some(GENESIS),
        }
    }

    fn elsewhere(worker: &str, deployment: u64) -> acp::DelegationAudience {
        acp::DelegationAudience {
            worker: worker.into(),
            deployment,
            ..audience()
        }
    }

    fn binding(digest: u8) -> RequestBinding {
        RequestBinding {
            digest: [digest; 32],
            genesis_id: GENESIS,
            nonce: [5; 24],
        }
    }

    fn caller_at(token: &str, digest: u8, now: u64) -> Result<String, CallerDelegationError> {
        let claims = vera_crypto::jwt::verify_bearer_token(token).expect("token verifies");
        let request = claims.request.clone().expect("request claim");
        bound_caller(&claims, &request, &audience(), || Ok([digest; 32]), now)
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
            Err(CallerDelegationError::Window(_))
        ));
        // It holds from the clock skew allowance before it was issued.
        assert_eq!(caller_at(&token, 1, NOW - 30).unwrap(), signer.did());
        assert!(matches!(
            caller_at(&token, 1, NOW - 31),
            Err(CallerDelegationError::Window(_))
        ));

        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        let request = claims.request.clone().unwrap();
        for (worker, deployment) in [(signer.did(), 9), (WORKER, 10)] {
            let other = elsewhere(worker, deployment);
            assert!(bound_caller(&claims, &request, &other, || Ok([1; 32]), NOW).is_err());
        }
        assert!(matches!(
            bound_caller(
                &claims,
                &request,
                &audience(),
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
            caller_of_request_delegation(&bound, &audience(), || Ok([1; 32]))
                .expect("bound to a request")
                .unwrap(),
            signer.did()
        );

        let plain = caller_delegation(&signer, &audience(), Duration::from_secs(60))
            .await
            .expect("delegation");
        assert!(caller_of_request_delegation(&plain, &audience(), || Ok([1; 32])).is_none());
        let mut forged = bound.clone();
        forged.push('A');
        assert!(caller_of_request_delegation(&forged, &audience(), || Ok([1; 32])).is_none());
    }

    #[nuclei::test]
    async fn a_request_delegation_for_another_genesis_is_refused_where_the_genesis_is_known() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let other = RequestBinding {
            genesis_id: [7; 32],
            ..binding(1)
        };
        let token = issue_request_at(&signer, &audience(), &other, 60, NOW)
            .await
            .expect("delegation");
        assert!(matches!(
            caller_at(&token, 1, NOW),
            Err(CallerDelegationError::OtherGenesis)
        ));

        // A provider that does not know its genesis identity leaves the
        // check to the chain, as before.
        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        let request = claims.request.clone().unwrap();
        let unknown = acp::DelegationAudience {
            genesis_id: None,
            ..audience()
        };
        assert_eq!(
            bound_caller(&claims, &request, &unknown, || Ok([1; 32]), NOW).unwrap(),
            signer.did()
        );
    }

    #[nuclei::test]
    async fn a_binding_made_from_the_reported_audience_is_taken_for_its_command() {
        let command = PolicyCommand {
            policy_id: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            resource: "users",
            object_id: "bae-first",
            action: crate::PolicyAction::RegisterObject,
        };
        let binding = RequestBinding::for_command(&audience(), &command, [5; 24]).expect("binding");
        assert_eq!(binding.genesis_id, GENESIS);
        assert_eq!(binding.digest, request_digest(&command).unwrap());
        assert_eq!(binding.nonce, [5; 24]);

        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let token = issue_request_at(&signer, &audience(), &binding, 60, NOW)
            .await
            .expect("delegation");
        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        let request = claims.request.clone().unwrap();
        assert_eq!(
            bound_caller(
                &claims,
                &request,
                &audience(),
                || request_digest(&command),
                NOW
            )
            .unwrap(),
            signer.did()
        );

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

    #[nuclei::test]
    async fn a_request_delegation_lasts_no_longer_than_vera_rs_keeps_a_request_open() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let longest_secs = MAX_REQUEST_LIFETIME.as_secs();
        assert_eq!(longest_secs, MAX_OPERATION_TTL - 30);
        for lifetime in [0, longest_secs + 1, MAX_OPERATION_TTL] {
            assert!(matches!(
                issue_request_at(&signer, &audience(), &binding(1), lifetime, NOW).await,
                Err(BearerError::RequestLifetime)
            ));
        }
        let longest = issue_request_at(&signer, &audience(), &binding(1), longest_secs, NOW)
            .await
            .expect("delegation");
        assert_eq!(caller_at(&longest, 1, NOW).unwrap(), signer.did());
        // The longest is still open to vera.rs when its signer's clock is a
        // whole allowance ahead.
        assert_eq!(caller_at(&longest, 1, NOW - 30).unwrap(), signer.did());
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
    async fn a_request_delegation_holds_from_the_clock_skew_allowance_before_it_was_issued() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let token = issue_request_at(&signer, &audience(), &binding(1), 60, NOW)
            .await
            .expect("delegation");
        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        assert_eq!(claims.iat, NOW - acp::CLOCK_SKEW_ALLOWANCE.as_secs());
        assert_eq!(claims.nbf, claims.iat);
        assert_eq!(claims.exp, NOW + 60);

        // A signer up to the allowance ahead of the node is taken.
        for ahead in [1, 5, 30] {
            let token = issue_request_at(&signer, &audience(), &binding(1), 60, NOW + ahead)
                .await
                .expect("delegation");
            assert_eq!(caller_at(&token, 1, NOW).unwrap(), signer.did());
        }
    }

    async fn ahead_token(signer: &Ed25519Signer) -> String {
        issue_request_at(signer, &audience(), &binding(1), 60, NOW + 1)
            .await
            .expect("delegation")
    }

    #[nuclei::test]
    async fn a_request_delegation_outside_its_window_names_the_window_and_the_provider_time() {
        let signer = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let window = |issued_at: u64, lifetime: u64, now: u64| {
            let signer = &signer;
            async move {
                let token = issue_request_at(signer, &audience(), &binding(1), lifetime, issued_at)
                    .await
                    .expect("delegation");
                match caller_at(&token, 1, now) {
                    Err(CallerDelegationError::Window(window)) => window,
                    other => panic!("not refused for its window: {other:?}"),
                }
            }
        };

        // Ahead by a minute: not valid yet. Set back by the allowance, its
        // start proves the signer at least 30 seconds ahead, no more.
        let ahead = window(NOW + 60, 120, NOW).await;
        assert_eq!(
            ahead,
            acp::DelegationWindow {
                not_before: NOW + 30,
                expires_at: NOW + 180,
                provider_time: NOW,
                refusal: acp::WindowRefusal::NotYetValid {
                    ahead_by_at_least: 30
                },
            }
        );
        // Behind by eleven minutes, or held past its end by a signer whose
        // clock is right: both expired, and nothing is said of the clock.
        let behind = window(NOW - 660, 570, NOW).await;
        let held = window(NOW - 20, 10, NOW).await;
        for expired in [behind, held] {
            assert_eq!(expired.refusal, acp::WindowRefusal::Expired);
            assert!(!expired.to_string().contains("clock"), "{expired}");
        }

        // From a signer that does not set its start back, one second ahead
        // is one second ahead: the allowance is not added.
        let mut unset = vera_crypto::jwt::verify_bearer_token(&ahead_token(&signer).await).unwrap();
        unset.iat = NOW + 1;
        unset.nbf = NOW + 1;
        assert_eq!(
            super::super::window::within_window(&unset, NOW)
                .unwrap_err()
                .refusal,
            acp::WindowRefusal::NotYetValid {
                ahead_by_at_least: 1
            }
        );
        // Started, but open longer past the node's time than vera.rs keeps a
        // request open.
        let mut long = unset.clone();
        long.nbf = NOW - 10;
        long.iat = NOW - 10;
        long.exp = NOW + MAX_OPERATION_TTL + 1;
        let mut id = [5; 32];
        id[..8].copy_from_slice(&long.exp.to_be_bytes());
        long.request.as_mut().unwrap().id = OperationId(id);
        assert_eq!(
            super::super::window::within_window(&long, NOW)
                .unwrap_err()
                .refusal,
            acp::WindowRefusal::OutlastsRequest
        );

        // A delegation for another worker is refused as such, whatever its times.
        let token = issue_request_at(&signer, &audience(), &binding(1), 60, NOW + 600)
            .await
            .expect("delegation");
        let claims = vera_crypto::jwt::verify_bearer_token(&token).unwrap();
        let request = claims.request.clone().unwrap();
        assert!(matches!(
            bound_caller(
                &claims,
                &request,
                &elsewhere(signer.did(), 9),
                || Ok([1; 32]),
                NOW
            ),
            Err(CallerDelegationError::Token(_))
        ));

        // Issued again at the provider time the refusal names, it is taken.
        let again = issue_request_at(&signer, &audience(), &binding(1), 120, ahead.provider_time)
            .await
            .expect("delegation");
        assert_eq!(caller_at(&again, 1, NOW).unwrap(), signer.did());
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
