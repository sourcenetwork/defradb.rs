//! The bearer token an actor's policy commands are sent under.

/// The signer of a registered identity's local key, for the key types a
/// delegation can be signed with.
fn registered_signer(
    signing_config: &defra_core::signing::SigningConfig,
) -> Option<Result<Box<dyn super::signer::DelegationSigner>, super::signer::SignerError>> {
    use defra_core::signing::SigningKeyType;

    use super::signer::{DelegationSigner, Ed25519Signer, P256Signer, Secp256k1Signer};

    let key = signing_config.private_key_bytes.as_slice();
    match signing_config.key_type {
        SigningKeyType::Secp256k1 => {
            Some(Secp256k1Signer::from_slice(key).map(|s| Box::new(s) as Box<dyn DelegationSigner>))
        }
        SigningKeyType::Ed25519 => {
            Some(Ed25519Signer::from_slice(key).map(|s| Box::new(s) as Box<dyn DelegationSigner>))
        }
        SigningKeyType::Secp256r1 => {
            Some(P256Signer::from_slice(key).map(|s| Box::new(s) as Box<dyn DelegationSigner>))
        }
        _ => None,
    }
}

/// Whether the node holds, for `did`, a registered key it signs a delegation
/// with: what [`resolve_registered_or_passthrough_bearer_token_as`] signs
/// with before it falls back to a request's token.
pub(crate) fn holds_registered_key(did: &str) -> bool {
    registered_key(did)
        .is_some_and(|signing_config| matches!(registered_signer(&signing_config), Some(Ok(_))))
}

/// The local key registered for the actor `did` names, under whichever
/// spelling of its key it was registered. A caller may name the actor in
/// another spelling than the registration's, and a staged change keeps the
/// actor in one spelling only.
fn registered_key(did: &str) -> Option<defra_core::signing::SigningConfig> {
    super::did_spelling::spellings(did)
        .iter()
        .filter_map(|spelling| defra_core::signing::get_identity(spelling))
        .find(defra_core::signing::SigningConfig::has_local_private_key)
}

/// [`resolve_registered_or_passthrough_bearer_token_as`] in
/// [`defra_core::RequestContext::ambient`].
#[cfg(test)]
pub(crate) async fn resolve_registered_or_passthrough_bearer_token(
    did: &str,
    submitter: &str,
    deployment_id: u64,
) -> Result<Option<String>, crate::provider::ProviderError> {
    resolve_registered_or_passthrough_bearer_token_as(
        &defra_core::RequestContext::ambient(),
        did,
        submitter,
        deployment_id,
    )
    .await
}

/// The bearer token `did` acts with in the request `context` serves: one
/// built from its registered secp256k1, Ed25519 or P-256 key when the node
/// holds it, the token
/// the request carries for it otherwise.
pub(crate) async fn resolve_registered_or_passthrough_bearer_token_as(
    context: &defra_core::RequestContext,
    did: &str,
    submitter: &str,
    deployment_id: u64,
) -> Result<Option<String>, crate::provider::ProviderError> {
    if let Some(signer) = registered_key(did).as_ref().and_then(registered_signer) {
        let signer = signer.map_err(|e| {
            crate::provider::ProviderError::Config(format!("invalid signing key: {}", e))
        })?;
        let issuer = super::did_spelling::wire(did);
        return super::bearer::create_bearer_token(
            signer.as_ref(),
            &issuer,
            submitter,
            deployment_id,
            300,
        )
        .await
        .map(Some)
        .map_err(|e| {
            crate::provider::ProviderError::Config(format!("bearer token creation failed: {}", e))
        });
    }

    Ok(context.bearer_token_for(did))
}

#[cfg(test)]
mod request_bearer_tests {
    use super::*;

    #[nuclei::test]
    async fn a_remote_identity_acts_with_the_token_its_request_carries() {
        let did = "did:key:z6MkVeraRsRequestBearer";
        defra_core::signing::set_request_bearer_token(did, "stored");
        let request = defra_core::RequestContext::default()
            .with_bearer(Some(did.to_string()), Some("request".to_string()));

        let own = resolve_registered_or_passthrough_bearer_token_as(
            &request,
            did,
            "did:key:submitter",
            9001,
        )
        .await
        .unwrap();
        let unscoped = resolve_registered_or_passthrough_bearer_token_as(
            &defra_core::RequestContext::default(),
            did,
            "did:key:submitter",
            9001,
        )
        .await
        .unwrap();
        defra_core::signing::clear_request_bearer_token(did);

        assert_eq!(own.as_deref(), Some("request"));
        assert_eq!(
            unscoped, None,
            "a request that carries no bearer presents none"
        );
    }

    #[test]
    fn the_node_holds_a_registered_local_key_and_not_a_request_token() {
        use defra_core::signing::{SigningConfig, SigningKeyType};
        use identity::Identity as _;

        let identity =
            identity::RawIdentity::from_secp256k1(crypto::generate_secp256k1().expect("key"))
                .expect("identity");
        let held = identity.did().expect("did").to_string();
        defra_core::signing::store_identity(
            &held,
            SigningConfig {
                key_type: SigningKeyType::Secp256k1,
                private_key_bytes: SigningConfig::private_key_bytes_from_vec(
                    identity.private_key_bytes().to_vec(),
                ),
                public_key_bytes: identity.public_key_bytes().to_vec(),
                public_key_hex: hex::encode(identity.public_key_bytes()),
                remote_signer: None,
                signing_authorization: None,
            },
        )
        .expect("the store takes the identity");
        let carried = "did:key:z6MkVeraRsCarriedToken";
        defra_core::signing::set_request_bearer_token(carried, "stored");

        assert!(holds_registered_key(&held));
        // The same key named in its other spelling is the same held key.
        let other = crate::vera_rs::did_spelling::spellings(&held)
            .pop()
            .expect("a spelling")
            .into_owned();
        assert_ne!(other, held);
        assert!(holds_registered_key(&other));
        assert!(!holds_registered_key(carried));
        assert!(!holds_registered_key("did:key:z6MkVeraRsUnknown"));

        defra_core::signing::clear_request_bearer_token(carried);
        defra_core::signing::release_identity(&held);
    }
}
