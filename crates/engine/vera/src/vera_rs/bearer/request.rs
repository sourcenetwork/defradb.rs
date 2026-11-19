//! Delegations bound to one request, as the node takes them.
//!
//! A delegation bound to one request (`vera_delegation::bearer::request`)
//! names the digest of one command, the deployment's genesis identity and an
//! operation identity. The node checks that binding before it sends anything,
//! against the digest of the command it is about to send and, when its
//! provider knows it, the deployment's genesis identity. A provider that does
//! not know it leaves that check to vera.rs.

use std::time::{SystemTime, UNIX_EPOCH};

use vera_crypto::jwt::{DelegationScope, JwtClaims};
use vera_crypto::operation::{OperationClaim, DEFAULT_OPERATION_LEAD};

use super::CallerDelegationError;
use crate::provider::ProviderError;
use crate::vera_rs::did_spelling;

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
    request.id.validate(now, DEFAULT_OPERATION_LEAD)?;
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
    use std::time::Duration;

    use vera_crypto::operation::{OperationId, DEFAULT_OPERATION_LEAD};

    use super::*;
    use crate::vera_rs::bearer::{
        caller_delegation, caller_of_delegation, is_delegation, request_delegation,
        request_delegation_at, BearerError, RequestBinding, MAX_REQUEST_LIFETIME,
    };
    use crate::vera_rs::request_digest;
    use crate::vera_rs::signer::{DelegationSigner, Ed25519Signer};
    use crate::PolicyCommand;

    /// The request-bound delegation `signer` issues at `now` for
    /// `lifetime_secs`.
    async fn issue_request_at(
        signer: &dyn DelegationSigner,
        audience: &acp::DelegationAudience,
        binding: &RequestBinding,
        lifetime_secs: u64,
        now: u64,
    ) -> Result<String, BearerError> {
        request_delegation_at(
            signer,
            audience,
            binding,
            Duration::from_secs(lifetime_secs),
            now,
        )
        .await
    }

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
        assert_eq!(longest_secs, DEFAULT_OPERATION_LEAD - 30);
        for lifetime in [0, longest_secs + 1, DEFAULT_OPERATION_LEAD] {
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
        long.exp = NOW + DEFAULT_OPERATION_LEAD + 1;
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
