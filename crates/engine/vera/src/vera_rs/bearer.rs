//! A caller's delegation, as the node takes it before it acts under it.
//!
//! Delegations are made in `vera_delegation::bearer`; this module decides
//! whether the node acts under one it is given: the caller it names, for this
//! node's worker and deployment, at the node's time.

use std::time::{SystemTime, UNIX_EPOCH};

use vera_crypto::jwt::DelegationScope;

pub use vera_delegation::bearer::{
    caller_delegation, request_delegation, request_delegation_at, BearerError, RequestBinding,
    MAX_REQUEST_LIFETIME,
};
pub(super) use vera_delegation::bearer::{create_bearer_token, create_scoped_bearer_token};

#[cfg(test)]
mod key_types;
mod request;
mod window;

pub(super) use request::caller_of_request_delegation;

/// The `did:key` of `key` from its compressed or its uncompressed encoding.
#[cfg(test)]
pub(super) fn did_from_signing_key(
    key: &vera_client::Secp256k1SigningKey,
    compressed: bool,
) -> String {
    vera_delegation::bearer::did_from_key(key.verifying_key(), compressed)
}

/// The token type vera.rs gives a delegation.
const DELEGATION_TYPE: &str = "vera-delegation-v1+jwt";

/// Whether `token` declares itself a vera.rs delegation. Nothing is verified.
pub(super) fn is_delegation(token: &str) -> bool {
    use base64::Engine;

    #[derive(serde::Deserialize)]
    struct Header {
        typ: String,
    }
    token
        .split('.')
        .next()
        .and_then(|header| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(header)
                .ok()
        })
        .and_then(|header| serde_json::from_slice::<Header>(&header).ok())
        .is_some_and(|header| header.typ == DELEGATION_TYPE)
}

/// The DID that signed the delegation `token`, in DefraDB's spelling, when
/// vera.rs would act under it now for `worker` on `deployment_id`: a current
/// delegation for policy commands and nothing narrower or wider.
pub(super) fn caller_of_delegation(
    token: &str,
    worker: &str,
    deployment_id: u64,
) -> Result<acp::AuthenticatedCaller, CallerDelegationError> {
    let claims = vera_crypto::jwt::verify_bearer_token(token)?;
    if claims.relay.is_some()
        || claims.request.is_some()
        || claims.scope != DelegationScope::PolicyCommands
    {
        return Err(CallerDelegationError::Kind);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CallerDelegationError::Clock)?
        .as_secs();
    authorize_at(&claims, worker, deployment_id, now)?;
    Ok(acp::AuthenticatedCaller {
        did: identity::Did::new(super::did_spelling::defra(&claims.iss).into_owned())?,
        expires_at: claims.exp,
    })
}

/// Whether vera.rs would act under `claims` for `worker` on `deployment_id`
/// at `now`. A delegation for another worker or deployment is refused as
/// such whatever its times; one for this worker that does not hold at `now`
/// is refused with the window it holds in.
fn authorize_at(
    claims: &vera_crypto::jwt::JwtClaims,
    worker: &str,
    deployment_id: u64,
    now: u64,
) -> Result<(), CallerDelegationError> {
    // At `nbf` the times always pass, since a verified token has
    // `nbf ≤ iat < exp`: this asks about the worker and deployment alone.
    claims.authorize(worker, deployment_id, claims.nbf)?;
    window::within_window(claims, now).map_err(CallerDelegationError::Window)?;
    Ok(claims.authorize(worker, deployment_id, now)?)
}

/// Why a caller's delegation was refused.
#[derive(Debug, thiserror::Error)]
pub(super) enum CallerDelegationError {
    #[error("{0}")]
    Window(acp::DelegationWindow),
    #[error(transparent)]
    Identity(#[from] identity::Error),
    #[error(transparent)]
    Token(#[from] vera_crypto::jwt::JwtError),
    #[error("the delegation is not a plain delegation for policy commands")]
    Kind,
    #[error("the system clock is before the Unix epoch")]
    Clock,
    #[error("the delegation is bound to another request")]
    OtherRequest,
    #[error("the delegation names another deployment's genesis identity")]
    OtherGenesis,
    #[error("the command the delegation is presented for has no digest: {0}")]
    Command(String),
}

#[cfg(test)]
mod caller_tests {
    use vera_delegation::bearer::create_scoped_bearer_token_at;

    use super::*;
    use crate::vera_rs::signer::{DelegationSigner as _, Ed25519Signer};

    const WORKER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

    fn audience(deployment: u64) -> acp::DelegationAudience {
        acp::DelegationAudience {
            worker: WORKER.into(),
            deployment,
            genesis_id: None,
        }
    }

    #[nuclei::test]
    async fn a_delegation_names_its_signer_only_to_the_worker_and_deployment_it_was_made_for() {
        {
            let signer = Ed25519Signer::from_slice(&[7; 32]).unwrap();
            let lifetime = std::time::Duration::from_secs(60);
            let token = caller_delegation(&signer, &audience(9), lifetime)
                .await
                .unwrap();
            assert!(is_delegation(&token));
            assert_eq!(
                caller_of_delegation(&token, WORKER, 9)
                    .unwrap()
                    .did
                    .as_str(),
                signer.did()
            );
            assert!(caller_of_delegation(&token, WORKER, 10).is_err());
            assert!(caller_of_delegation(&token, signer.did(), 9).is_err());

            let mut forged = token.clone();
            forged.pop();
            forged.push(if token.ends_with('A') { 'B' } else { 'A' });
            assert!(is_delegation(&forged));
            assert!(caller_of_delegation(&forged, WORKER, 9).is_err());

            let narrower = create_scoped_bearer_token(
                &signer,
                signer.did(),
                WORKER,
                9,
                60,
                DelegationScope::CreatePolicy,
            )
            .await
            .unwrap();
            assert!(matches!(
                caller_of_delegation(&narrower, WORKER, 9),
                Err(CallerDelegationError::Kind)
            ));
        }
    }

    #[nuclei::test]
    async fn a_plain_delegation_outside_its_window_names_the_window_and_the_provider_time() {
        const NOW: u64 = 1_900_000_000;
        let signer = Ed25519Signer::from_slice(&[7; 32]).unwrap();
        let at = |issued_at: u64| {
            let signer = &signer;
            async move {
                let token = create_scoped_bearer_token_at(
                    signer,
                    signer.did(),
                    WORKER,
                    9,
                    60,
                    DelegationScope::PolicyCommands,
                    issued_at,
                )
                .await
                .unwrap();
                vera_crypto::jwt::verify_bearer_token(&token).unwrap()
            }
        };
        for ahead in [0, 30] {
            assert!(authorize_at(&at(NOW + ahead).await, WORKER, 9, NOW).is_ok());
        }
        let early = at(NOW + 31).await;
        assert!(matches!(
            authorize_at(&early, WORKER, 9, NOW),
            Err(CallerDelegationError::Window(window))
                if window.provider_time == NOW
                    && window.refusal == acp::WindowRefusal::NotYetValid { ahead_by_at_least: 1 }
        ));
        let late = at(NOW - 61).await;
        assert!(matches!(
            authorize_at(&late, WORKER, 9, NOW),
            Err(CallerDelegationError::Window(window))
                if window.refusal == acp::WindowRefusal::Expired
        ));
        assert!(matches!(
            authorize_at(&early, WORKER, 10, NOW),
            Err(CallerDelegationError::Token(_))
        ));
    }

    #[test]
    fn an_identity_token_is_not_a_delegation() {
        let identity =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let token = identity::new_token(
            &identity,
            std::time::Duration::from_secs(60),
            Some("127.0.0.1:9181".into()),
            None,
        )
        .unwrap();
        assert!(!is_delegation(std::str::from_utf8(&token).unwrap()));
        assert!(!is_delegation("not a token"));
    }
}
