//! Batch signing and verification over a set of CIDs.
//!
//! Computes a Merkle root from collected CIDs, signs the root with the
//! node's private key, and provides verification of batch signatures.
//!
//! This module is intentionally kept as a live compatibility surface for the
//! Rust HTTP and FFI batch-signing endpoints used by the indexer flow.

use cid::Cid;
use serde::{Deserialize, Serialize};

use crate::keys::{PrivateKey, PublicKey};
use crate::{Ed25519PrivateKey, Ed25519PublicKey, Secp256k1PrivateKey, Secp256k1PublicKey};
use defra_core::batch_signing::{
    compute_merkle_root_with, merkle_path, root_from_path, BatchOptions, BatchScope, MerklePath,
    MerkleScheme,
};
use defra_core::signing::SigningConfig;

/// Errors that can occur during batch signing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BatchSignError {
    #[error("signing key not configured")]
    MissingKey,
    #[error("unsupported key type: {0}")]
    UnsupportedKeyType(String),
    #[error("signing failed: {0}")]
    SigningFailed(String),
}

/// Errors that can occur while verifying a batch signature.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BatchVerifyError {
    #[error("invalid identity hex: {0}")]
    InvalidIdentity(String),
    #[error("invalid {key_type} public key: {reason}")]
    InvalidPublicKey {
        key_type: &'static str,
        reason: String,
    },
    #[error("{key_type} verify failed: {reason}")]
    VerifyFailed {
        key_type: &'static str,
        reason: String,
    },
    #[error("unsupported sig type: {0}")]
    UnsupportedSigType(String),
}

/// A batch signature covering a set of CIDs via their Merkle root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSignature {
    /// Signature algorithm: "EdDSA" or "ES256K"
    pub sig_type: String,
    /// Hex-encoded public key of the signer
    pub identity: String,
    /// Raw signature bytes over the Merkle root
    #[serde(with = "hex_bytes")]
    pub value: Vec<u8>,
    /// The 32-byte Merkle root that was signed
    #[serde(with = "hex_bytes")]
    pub merkle_root: Vec<u8>,
    /// Number of CIDs in the batch
    pub cid_count: usize,
    /// How the Merkle tree was built; absent for Go Shinzo's tree, so a
    /// Go-scheme signature serializes as before.
    #[serde(default, skip_serializing_if = "is_go_scheme")]
    pub merkle_scheme: MerkleScheme,
    /// Which blocks the CIDs were collected from; absent for Go Shinzo's
    /// composite-only scope, so such a signature serializes as before.
    #[serde(default, skip_serializing_if = "is_go_scope")]
    pub batch_scope: BatchScope,
}

fn is_go_scheme(scheme: &MerkleScheme) -> bool {
    *scheme == MerkleScheme::GoShinzo
}

fn is_go_scope(scope: &BatchScope) -> bool {
    *scope == BatchScope::Composite
}

/// Sign a batch of CIDs using the provided signing config, with Go Shinzo's
/// Merkle tree and scope.
pub fn sign_batch(cids: &[Cid], config: &SigningConfig) -> Result<BatchSignature, BatchSignError> {
    sign_batch_with(cids, config, BatchOptions::default())
}

/// Sign a batch of CIDs, collected under `options.scope`, over the Merkle
/// tree `options.scheme` builds.
pub fn sign_batch_with(
    cids: &[Cid],
    config: &SigningConfig,
    options: BatchOptions,
) -> Result<BatchSignature, BatchSignError> {
    let root = compute_merkle_root_with(cids, options.scheme);

    let (sig_type, sig_bytes) = match config.key_type {
        defra_core::signing::SigningKeyType::Ed25519 => {
            let key = Ed25519PrivateKey::from_bytes(&config.private_key_bytes).map_err(|e| {
                BatchSignError::SigningFailed(format!("invalid ed25519 key: {}", e))
            })?;
            let sig = key.sign(&root).map_err(|e| {
                BatchSignError::SigningFailed(format!("ed25519 sign failed: {}", e))
            })?;
            ("EdDSA".to_string(), sig)
        }
        defra_core::signing::SigningKeyType::Secp256k1 => {
            let key = Secp256k1PrivateKey::from_bytes(&config.private_key_bytes).map_err(|e| {
                BatchSignError::SigningFailed(format!("invalid secp256k1 key: {}", e))
            })?;
            let sig = key.sign(&root).map_err(|e| {
                BatchSignError::SigningFailed(format!("secp256k1 sign failed: {}", e))
            })?;
            ("ES256K".to_string(), sig)
        }
        defra_core::signing::SigningKeyType::Secp256r1
        | defra_core::signing::SigningKeyType::BlsAugV1 => {
            return Err(BatchSignError::UnsupportedKeyType(
                config.key_type.to_string(),
            ))
        }
        other => return Err(BatchSignError::UnsupportedKeyType(other.to_string())),
    };

    Ok(BatchSignature {
        sig_type,
        identity: config.public_key_hex.clone(),
        value: sig_bytes,
        merkle_root: root.to_vec(),
        cid_count: cids.len(),
        merkle_scheme: options.scheme,
        batch_scope: options.scope,
    })
}

/// Verify a batch signature against a set of CIDs, with Go Shinzo's Merkle
/// tree and scope.
pub fn verify_batch_signature(sig: &BatchSignature, cids: &[Cid]) -> Result<bool, String> {
    verify_batch_signature_with(sig, cids, BatchOptions::default()).map_err(|e| e.to_string())
}

/// Verify a batch signature against a set of CIDs, collected under
/// `options.scope`, over the tree `options.scheme` builds. The options are
/// the verifier's, never the signature's: a signature naming another tree or
/// scope fails, so a signer cannot pick a weaker one. The signed CID count
/// must equal the number of CIDs, since Go's tree gives lists of different
/// lengths the same root.
pub fn verify_batch_signature_with(
    sig: &BatchSignature,
    cids: &[Cid],
    options: BatchOptions,
) -> Result<bool, BatchVerifyError> {
    if !scheme_matches(sig, options)
        || sig.batch_scope != options.scope
        || sig.cid_count != cids.len()
        || has_duplicates(cids)
    {
        return Ok(false);
    }
    let root = compute_merkle_root_with(cids, options.scheme);
    verify_signed_root(sig, &root)
}

/// One CID of a signed batch and the path that shows it is in the batch,
/// so a block can carry its batch signature without the whole batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProof {
    pub signature: BatchSignature,
    pub path: MerklePath,
}

/// A proof that `cid` is in the batch `signature` covers.
pub fn prove_inclusion(
    signature: &BatchSignature,
    cids: &[Cid],
    cid: &Cid,
    options: BatchOptions,
) -> Option<BatchProof> {
    Some(BatchProof {
        signature: signature.clone(),
        path: merkle_path(cids, cid, options.scheme)?,
    })
}

/// Verify that `cid` is in the batch `proof.signature` signed. As for a
/// whole batch, the options are the verifier's, and the path must be for a
/// batch of exactly the signed count.
pub fn verify_batch_inclusion(
    proof: &BatchProof,
    cid: &Cid,
    options: BatchOptions,
) -> Result<bool, BatchVerifyError> {
    let sig = &proof.signature;
    if !scheme_matches(sig, options)
        || sig.batch_scope != options.scope
        || sig.cid_count != proof.path.count
    {
        return Ok(false);
    }
    match root_from_path(cid, &proof.path, options.scheme) {
        Some(root) => verify_signed_root(sig, &root),
        None => Ok(false),
    }
}

/// Whether the signature's scheme is the one the verifier chose. A signature
/// made with Rust's old tree carries no scheme and reads back as Go Shinzo's,
/// so a verifier that selects `RustLegacy` takes that too.
fn scheme_matches(sig: &BatchSignature, options: BatchOptions) -> bool {
    sig.merkle_scheme == options.scheme
        || (options.scheme == MerkleScheme::RustLegacy
            && sig.merkle_scheme == MerkleScheme::GoShinzo)
}

/// Whether a CID appears twice. The count is not signed, and Go's tree gives
/// `[a, b, c]` and `[a, b, c, c]` one root, so a repeat is refused outright.
fn has_duplicates(cids: &[Cid]) -> bool {
    let mut sorted: Vec<Vec<u8>> = cids.iter().map(Cid::to_bytes).collect();
    sorted.sort();
    sorted.windows(2).any(|pair| pair[0] == pair[1])
}

/// Whether `root` is the signature's root and the signature over it holds.
fn verify_signed_root(sig: &BatchSignature, root: &[u8; 32]) -> Result<bool, BatchVerifyError> {
    if root.as_slice() != sig.merkle_root.as_slice() {
        return Ok(false);
    }
    let root = *root;

    let pub_bytes =
        hex::decode(&sig.identity).map_err(|e| BatchVerifyError::InvalidIdentity(e.to_string()))?;

    match sig.sig_type.as_str() {
        "EdDSA" => {
            let pubkey = Ed25519PublicKey::from_bytes(&pub_bytes).map_err(|e| {
                BatchVerifyError::InvalidPublicKey {
                    key_type: "ed25519",
                    reason: e.to_string(),
                }
            })?;
            pubkey
                .verify(&root, &sig.value)
                .map_err(|e| BatchVerifyError::VerifyFailed {
                    key_type: "ed25519",
                    reason: e.to_string(),
                })
        }
        "ES256K" => {
            let pubkey = Secp256k1PublicKey::from_bytes(&pub_bytes).map_err(|e| {
                BatchVerifyError::InvalidPublicKey {
                    key_type: "secp256k1",
                    reason: e.to_string(),
                }
            })?;
            pubkey
                .verify(&root, &sig.value)
                .map_err(|e| BatchVerifyError::VerifyFailed {
                    key_type: "secp256k1",
                    reason: e.to_string(),
                })
        }
        other => Err(BatchVerifyError::UnsupportedSigType(other.to_string())),
    }
}

/// Serde helper for hex-encoding `Vec<u8>` fields.
mod hex_bytes {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Key;
    use defra_core::batch_signing::compute_merkle_root;
    use defra_core::DAG_CBOR_CODEC;
    use multihash_codetable::{Code, MultihashDigest};

    fn make_cid(data: &[u8]) -> Cid {
        let hash = Code::Sha2_256.digest(data);
        Cid::new_v1(DAG_CBOR_CODEC, hash)
    }

    fn test_ed25519_config() -> SigningConfig {
        let private_key = crate::keys::generation::generate_ed25519().unwrap();
        let public_key = private_key.public_key();
        SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(private_key.raw_owned()),
            public_key_bytes: public_key.raw_owned(),
            public_key_hex: public_key.to_hex_string(),
            remote_signer: None,
            signing_authorization: None,
        }
    }

    fn test_secp256k1_config() -> SigningConfig {
        let private_key = crate::keys::generation::generate_secp256k1().unwrap();
        let public_key = private_key.public_key();
        SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Secp256k1,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(private_key.raw_owned()),
            public_key_bytes: public_key.raw_owned(),
            public_key_hex: public_key.to_hex_string(),
            remote_signer: None,
            signing_authorization: None,
        }
    }

    #[test]
    fn sign_verify_ed25519_roundtrip() {
        let config = test_ed25519_config();
        let cids = vec![make_cid(b"a"), make_cid(b"b"), make_cid(b"c")];
        let sig = sign_batch(&cids, &config).unwrap();
        assert_eq!(sig.sig_type, "EdDSA");
        assert_eq!(sig.cid_count, 3);
        assert!(verify_batch_signature(&sig, &cids).unwrap());
    }

    #[test]
    fn sign_verify_secp256k1_roundtrip() {
        let config = test_secp256k1_config();
        let cids = vec![make_cid(b"x"), make_cid(b"y")];
        let sig = sign_batch(&cids, &config).unwrap();
        assert_eq!(sig.sig_type, "ES256K");
        assert!(verify_batch_signature(&sig, &cids).unwrap());
    }

    #[test]
    fn verify_fails_wrong_cids() {
        let config = test_ed25519_config();
        let cids = vec![make_cid(b"a"), make_cid(b"b")];
        let sig = sign_batch(&cids, &config).unwrap();
        let wrong_cids = vec![make_cid(b"a"), make_cid(b"c")];
        assert!(!verify_batch_signature(&sig, &wrong_cids).unwrap());
    }

    /// Batch signatures from Go shinzo-develop's `SignBatch` over the CIDs of
    /// "doc0".."doc2", with the Ed25519 seed SHA256("shinzo-ed25519") and the
    /// secp256k1 secret SHA256("shinzo-secp256k1").
    const GO_ED25519_SEED: &str =
        "e447512b417e54f102eac9e2d0ac8b3fae5d9111d381e493b24720d07b0adf0a";
    const GO_ED25519_IDENTITY: &str =
        "fabd7aa83c1055a21d6c176138011a96035dbc2ef1af10708067e8af2b15a427";
    const GO_ED25519_VALUE: &str = "00eebb9a3d93372651acc87f05bf2f04e39660064f432e12faa1fcbcbcf7e9b29870c281326621f80afe02b3f36d0f866e2a7bd7798e91f058bb2376a7278f04";
    const GO_SECP256K1_SECRET: &str =
        "1233ff67e1cbaeabd49e0c2a3976746e019276392249314c08bb088e05918e4b";
    const GO_SECP256K1_IDENTITY: &str =
        "038cab60f242dc148f3ade868850621f544ce4ae89b22e77653f4482e12538af71";
    const GO_SECP256K1_VALUE: &str = "3045022100a763a5c583e70f9285fb20610b6ed5a6557dcf864570e0618d864cb7528060b502203782e375cdc90b19e79be0fc58ec5f5d0f614fb3316536d0e836320fa278a697";
    const GO_ROOT: &str = "0ffb57f4c557c42494a782e71b2d234ec526603ac639bee54ad50a326377777b";

    fn go_cids() -> Vec<Cid> {
        (0..3)
            .map(|i| make_cid(format!("doc{i}").as_bytes()))
            .collect()
    }

    fn go_signature(sig_type: &str, identity: &str, value: &str) -> BatchSignature {
        BatchSignature {
            sig_type: sig_type.into(),
            identity: identity.into(),
            value: hex::decode(value).unwrap(),
            merkle_root: hex::decode(GO_ROOT).unwrap(),
            cid_count: 3,
            merkle_scheme: MerkleScheme::GoShinzo,
            batch_scope: BatchScope::Composite,
        }
    }

    #[test]
    fn domain_separated_roots_differ_where_go_roots_collide() {
        let mut cids = [make_cid(b"a"), make_cid(b"b"), make_cid(b"c")];
        cids.sort_by_key(Cid::to_bytes);
        let [a, b, c] = cids;
        let separated = MerkleScheme::DomainSeparated;
        assert_ne!(
            compute_merkle_root_with(&[a, b, c], separated),
            compute_merkle_root_with(&[a, b, c, c], separated)
        );
        // A single CID's root is its tagged leaf, never the bare CID hash.
        assert_ne!(
            compute_merkle_root_with(&[a], separated),
            compute_merkle_root_with(&[a], MerkleScheme::GoShinzo)
        );
    }

    #[test]
    fn domain_separated_signatures_verify_only_under_that_scheme() {
        let config = test_ed25519_config();
        let cids = go_cids();
        let separated = BatchOptions {
            scheme: MerkleScheme::DomainSeparated,
            ..BatchOptions::default()
        };
        let sig = sign_batch_with(&cids, &config, separated).unwrap();
        assert!(verify_batch_signature_with(&sig, &cids, separated).unwrap());
        // A Go-scheme verifier refuses it, and a Go-scheme signature is
        // refused by a domain-separated verifier: the scheme is never the
        // signer's choice.
        assert!(!verify_batch_signature(&sig, &cids).unwrap());
        let go = sign_batch(&cids, &config).unwrap();
        assert!(!verify_batch_signature_with(&go, &cids, separated).unwrap());
        // A Go-scheme signature serializes without the scheme field.
        assert!(!serde_json::to_string(&go)
            .unwrap()
            .contains("merkle_scheme"));
    }

    /// Every CID of batches of 1 to 9 has a path to the signed root, under
    /// either tree; a CID outside the batch, or a path for another count,
    /// does not.
    #[test]
    fn every_cid_of_a_batch_proves_its_inclusion() {
        let config = test_ed25519_config();
        let outsider = make_cid(b"outsider");
        for scheme in [MerkleScheme::GoShinzo, MerkleScheme::DomainSeparated] {
            let options = BatchOptions {
                scheme,
                ..BatchOptions::default()
            };
            for count in 1..=9 {
                let cids: Vec<Cid> = (0..count)
                    .map(|i| make_cid(format!("doc{i}").as_bytes()))
                    .collect();
                let sig = sign_batch_with(&cids, &config, options).unwrap();
                for cid in &cids {
                    let proof = prove_inclusion(&sig, &cids, cid, options).unwrap();
                    assert!(
                        verify_batch_inclusion(&proof, cid, options).unwrap(),
                        "{scheme:?} {count}"
                    );
                    assert!(!verify_batch_inclusion(&proof, &outsider, options).unwrap());
                    let mut longer = proof.clone();
                    longer.path.count += 1;
                    assert!(!verify_batch_inclusion(&longer, cid, options).unwrap());
                }
                assert!(prove_inclusion(&sig, &cids, &outsider, options).is_none());
            }
        }
    }

    /// Go's tree gives `[a, b, c]` and `[a, b, c, c]` one root; a path that
    /// claims the fourth leaf of a batch signed with three is refused.
    #[test]
    fn a_path_into_the_duplicated_leaf_is_refused() {
        let config = test_ed25519_config();
        let mut cids = [make_cid(b"a"), make_cid(b"b"), make_cid(b"c")];
        cids.sort_by_key(Cid::to_bytes);
        let sig = sign_batch(&cids, &config).unwrap();
        let options = BatchOptions::default();
        let mut forged = prove_inclusion(&sig, &cids, &cids[2], options).unwrap();
        forged.path.index = 3;
        forged.path.count = 4;
        assert!(!verify_batch_inclusion(&forged, &cids[2], options).unwrap());
    }

    #[test]
    fn the_verifier_scope_decides() {
        let config = test_ed25519_config();
        let cids = go_cids();
        let full = BatchOptions {
            scope: BatchScope::Full,
            ..BatchOptions::default()
        };
        let sig = sign_batch_with(&cids, &config, full).unwrap();
        assert!(verify_batch_signature_with(&sig, &cids, full).unwrap());
        // The same CIDs, labelled with another scope, do not verify.
        assert!(!verify_batch_signature(&sig, &cids).unwrap());
        let go = sign_batch(&cids, &config).unwrap();
        assert!(!verify_batch_signature_with(&go, &cids, full).unwrap());
        assert!(!serde_json::to_string(&go).unwrap().contains("batch_scope"));
    }

    #[test]
    fn go_batch_signatures_verify() {
        for sig in [
            go_signature("EdDSA", GO_ED25519_IDENTITY, GO_ED25519_VALUE),
            go_signature("ES256K", GO_SECP256K1_IDENTITY, GO_SECP256K1_VALUE),
        ] {
            assert!(
                verify_batch_signature(&sig, &go_cids()).unwrap(),
                "{}",
                sig.sig_type
            );
        }
    }

    /// Both runtimes sign deterministically (Ed25519, and RFC 6979 nonces
    /// for secp256k1), so equal bytes show Go verifies what Rust signs.
    #[test]
    fn rust_batch_signatures_match_go_bytes() {
        let mut ed25519 = hex::decode(GO_ED25519_SEED).unwrap();
        ed25519.extend(hex::decode(GO_ED25519_IDENTITY).unwrap());
        let ed25519 = SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(ed25519),
            public_key_bytes: hex::decode(GO_ED25519_IDENTITY).unwrap(),
            public_key_hex: GO_ED25519_IDENTITY.into(),
            remote_signer: None,
            signing_authorization: None,
        };
        let sig = sign_batch(&go_cids(), &ed25519).unwrap();
        assert_eq!(hex::encode(&sig.value), GO_ED25519_VALUE);
        assert_eq!(hex::encode(&sig.merkle_root), GO_ROOT);

        let secp256k1 = SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Secp256k1,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(
                hex::decode(GO_SECP256K1_SECRET).unwrap(),
            ),
            public_key_bytes: hex::decode(GO_SECP256K1_IDENTITY).unwrap(),
            public_key_hex: GO_SECP256K1_IDENTITY.into(),
            remote_signer: None,
            signing_authorization: None,
        };
        let sig = sign_batch(&go_cids(), &secp256k1).unwrap();
        assert_eq!(hex::encode(&sig.value), GO_SECP256K1_VALUE);
    }

    /// Go's tree pairs an odd last leaf with itself, so `[a, b, c]` and
    /// `[a, b, c, c]`, with `c` last in sorted order, share a root; the
    /// signed count tells them apart.
    #[test]
    fn verify_rejects_a_cid_set_of_another_size_with_the_same_root() {
        let config = test_ed25519_config();
        let mut cids = [make_cid(b"a"), make_cid(b"b"), make_cid(b"c")];
        cids.sort_by_key(Cid::to_bytes);
        let [a, b, c] = cids;
        let sig = sign_batch(&[a, b, c], &config).unwrap();
        assert_eq!(
            compute_merkle_root(&[a, b, c]),
            compute_merkle_root(&[a, b, c, c])
        );
        assert!(verify_batch_signature(&sig, &[a, b, c]).unwrap());
        assert!(!verify_batch_signature(&sig, &[a, b, c, c]).unwrap());
        // The count is not signed: forging it alongside the repeat still fails.
        let forged = BatchSignature {
            cid_count: 4,
            ..sig.clone()
        };
        assert!(!verify_batch_signature(&forged, &[a, b, c, c]).unwrap());
    }

    /// A signature that the base revision of #2454 made over "doc0".."doc2"
    /// with the Go fixture's Ed25519 key: Rust's old tree, no scheme field.
    const LEGACY_SIGNATURE: &str = r#"{"sig_type":"EdDSA","identity":"fabd7aa83c1055a21d6c176138011a96035dbc2ef1af10708067e8af2b15a427","value":"ba768beb92db641e431301c51ee83661e4dc7c2d90ca9e456817bac810407cc30ad69f506adfef8aa5b401522ec76b6b6de1cb2e7a3c6646e18359cb72b8c300","merkle_root":"fc8f2a4d773c6db591bdb6b6e122a3fb44274049c8d7d4b0b68212a1dcc8bfc9","cid_count":3}"#;

    /// An old-tree signature reads back as Go's scheme, so the default
    /// verifier refuses it; a verifier that selects `RustLegacy` accepts it.
    #[test]
    fn a_signature_from_rusts_old_tree_verifies_when_selected() {
        let sig: BatchSignature = serde_json::from_str(LEGACY_SIGNATURE).unwrap();
        assert_eq!(sig.merkle_scheme, MerkleScheme::GoShinzo);
        assert!(!verify_batch_signature(&sig, &go_cids()).unwrap());
        let legacy = BatchOptions {
            scheme: MerkleScheme::RustLegacy,
            ..Default::default()
        };
        assert!(verify_batch_signature_with(&sig, &go_cids(), legacy).unwrap());
        assert!(!verify_batch_signature_with(&sig, &go_cids()[..2], legacy).unwrap());
    }

    /// Go signs an empty batch over SHA256 of nothing; so does Rust.
    #[test]
    fn sign_empty_signs_the_empty_root() {
        let config = test_ed25519_config();
        let sig = sign_batch(&[], &config).unwrap();
        assert_eq!(sig.cid_count, 0);
        assert!(verify_batch_signature(&sig, &[]).unwrap());
    }

    #[test]
    fn batch_signature_json_roundtrip() {
        let config = test_ed25519_config();
        let cids = vec![make_cid(b"json")];
        let sig = sign_batch(&cids, &config).unwrap();
        let json = serde_json::to_string(&sig).unwrap();
        let deserialized: BatchSignature = serde_json::from_str(&json).unwrap();
        assert_eq!(sig.sig_type, deserialized.sig_type);
        assert_eq!(sig.identity, deserialized.identity);
        assert_eq!(sig.value, deserialized.value);
        assert_eq!(sig.merkle_root, deserialized.merkle_root);
        assert_eq!(sig.cid_count, deserialized.cid_count);
    }
}
