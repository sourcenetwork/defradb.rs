//! Key type parsing and public key validation for FFI identity operations.

pub(crate) fn parse_signing_key_type(
    key_type: &str,
) -> Result<defra_core::signing::SigningKeyType, String> {
    key_type.parse()
}

pub(crate) fn parse_crypto_key_type(
    key_type: defra_core::signing::SigningKeyType,
) -> Result<crypto::KeyType, String> {
    match key_type {
        defra_core::signing::SigningKeyType::Ed25519 => Ok(crypto::KeyType::Ed25519),
        defra_core::signing::SigningKeyType::Secp256k1 => Ok(crypto::KeyType::Secp256k1),
        defra_core::signing::SigningKeyType::Secp256r1 => Ok(crypto::KeyType::Secp256r1),
        defra_core::signing::SigningKeyType::BlsAugV1 => Ok(crypto::KeyType::Bls12381),
        other => Err(format!("unsupported signing key type: {}", other)),
    }
}

pub(crate) fn decode_and_validate_public_key(
    did: &str,
    public_key_hex: &str,
    key_type: defra_core::signing::SigningKeyType,
) -> Result<Vec<u8>, String> {
    let public_key_bytes = hex::decode(public_key_hex)
        .map_err(|error| format!("invalid public key hex: {}", error))?;

    validate_public_key_bytes(did, &public_key_bytes, key_type)?;
    Ok(public_key_bytes)
}

pub(crate) fn validate_public_key_bytes(
    did: &str,
    public_key_bytes: &[u8],
    key_type: defra_core::signing::SigningKeyType,
) -> Result<(), String> {
    let crypto_key_type = parse_crypto_key_type(key_type)?;

    let public_key = crypto::public_key_from_bytes(crypto_key_type, public_key_bytes)
        .map_err(|error| format!("invalid public key bytes: {}", error))?;
    let derived_did = public_key
        .did()
        .map_err(|error| format!("failed to derive DID from public key: {}", error))?;

    if derived_did != did {
        return Err(format!(
            "public key DID mismatch: expected {}, derived {}",
            did, derived_did
        ));
    }

    Ok(())
}
