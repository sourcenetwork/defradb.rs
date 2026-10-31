//! The node's signing identity, from its [`NodeSigning`].

use identity::{Identity as _, IdentityKeyType, RawIdentity};

use crate::config::{NodeSigning, SigningKey};
use crate::{Error, Result};

/// The identity the node signs with and its DID. A registered identity with
/// only a remote signer has a DID and no local key.
pub(crate) fn create_node_identity(
    config: &NodeSigning,
) -> Result<(Option<RawIdentity>, Option<String>)> {
    match config {
        NodeSigning::Disabled => Ok((None, None)),
        NodeSigning::Enabled { key } => {
            let raw_identity = match key {
                Some(SigningKey::Secp256k1(bytes)) => {
                    let private_key =
                        crypto::Secp256k1PrivateKey::from_bytes(bytes).map_err(|source| {
                            Error::KeyLoad {
                                curve: "secp256k1",
                                source,
                            }
                        })?;
                    identity::RawIdentity::from_secp256k1(private_key)
                        .map_err(Error::IdentityCreate)?
                }
                Some(SigningKey::Secp256r1(bytes)) => {
                    let private_key =
                        crypto::Secp256r1PrivateKey::from_bytes(bytes).map_err(|source| {
                            Error::KeyLoad {
                                curve: "secp256r1",
                                source,
                            }
                        })?;
                    identity::RawIdentity::from_secp256r1(private_key)
                        .map_err(Error::IdentityCreate)?
                }
                Some(SigningKey::Ed25519(bytes)) => {
                    let private_key =
                        crypto::Ed25519PrivateKey::from_bytes(bytes).map_err(|source| {
                            Error::KeyLoad {
                                curve: "ed25519",
                                source,
                            }
                        })?;
                    identity::RawIdentity::from_ed25519(private_key)
                        .map_err(Error::IdentityCreate)?
                }
                None => {
                    let private_key = crypto::generate_secp256k1().map_err(Error::KeyGenerate)?;
                    identity::RawIdentity::from_secp256k1(private_key)
                        .map_err(Error::IdentityCreate)?
                }
            };

            let did = raw_identity.did().map_err(Error::DidDerive)?;
            let did_str = did.to_string();
            let key_type = match key {
                Some(SigningKey::Ed25519(_)) => defra_core::signing::SigningKeyType::Ed25519,
                Some(SigningKey::Secp256r1(_)) => defra_core::signing::SigningKeyType::Secp256r1,
                _ => defra_core::signing::SigningKeyType::Secp256k1,
            };

            defra_core::signing::store_identity(
                &did_str,
                defra_core::signing::SigningConfig {
                    key_type,
                    private_key_bytes:
                        defra_core::signing::SigningConfig::private_key_bytes_from_vec(
                            raw_identity.private_key_bytes(),
                        ),
                    public_key_bytes: raw_identity.public_key_bytes().to_vec(),
                    public_key_hex: hex::encode(raw_identity.public_key_bytes()),
                    remote_signer: None,
                    signing_authorization: None,
                },
            );

            Ok((Some(raw_identity), Some(did_str)))
        }
        NodeSigning::RegisteredIdentity { did } => {
            let registered = resolve_registered_node_identity(did)?;
            let raw_identity = local_raw_identity_from_registered_config(did, &registered)?;
            Ok((raw_identity, Some(did.clone())))
        }
    }
}

fn resolve_registered_node_identity(did: &str) -> Result<defra_core::signing::SigningConfig> {
    let config = defra_core::signing::get_identity(did)
        .ok_or_else(|| Error::NodeIdentityUnregistered(did.to_owned()))?;
    if !config.has_local_private_key() && !config.has_remote_signer() {
        return Err(Error::NodeIdentityUnusable(did.to_owned()));
    }
    Ok(config)
}

fn local_raw_identity_from_registered_config(
    did: &str,
    config: &defra_core::signing::SigningConfig,
) -> Result<Option<RawIdentity>> {
    if !config.has_local_private_key() {
        return Ok(None);
    }

    let key_type = identity_key_type_from_signing_key_type(config.key_type)?;
    let identity = RawIdentity::from_identity_key_type(key_type, &config.private_key_bytes)
        .map_err(|error| Error::NodeIdentityLoad {
            did: did.to_owned(),
            source: error,
        })?;
    let derived_did = identity
        .did()
        .map_err(Error::NodeIdentityDerive)?
        .to_string();
    if derived_did != did {
        return Err(Error::NodeIdentityMismatch {
            expected: did.to_owned(),
            derived: derived_did,
        });
    }
    Ok(Some(identity))
}

fn identity_key_type_from_signing_key_type(
    key_type: defra_core::signing::SigningKeyType,
) -> Result<IdentityKeyType> {
    match key_type {
        defra_core::signing::SigningKeyType::Ed25519 => Ok(IdentityKeyType::Ed25519),
        defra_core::signing::SigningKeyType::Secp256k1 => Ok(IdentityKeyType::Secp256k1),
        defra_core::signing::SigningKeyType::Secp256r1 => Ok(IdentityKeyType::Secp256r1),
        unsupported => Err(Error::UnsupportedKeyType(unsupported.to_string())),
    }
}
