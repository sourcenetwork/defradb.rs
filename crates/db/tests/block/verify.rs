use crypto::PrivateKey;
use db::block::verify::verified_signature_signer_did;
use defra_core::block::Block;
use defra_core::block::CrdtDelta;
use defra_core::block::LwwDeltaPayload;
use defra_core::block::Signature;
use defra_core::block::SignatureHeader;
use defra_core::block::SignatureType;

fn test_block() -> Block {
    Block {
        delta: CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            schema_version_id: "v1".to_string(),
            priority: 1,
            data: b"original".to_vec(),
        }),
        heads: None,
        links: None,
        encryption: None,
        signature: None,
    }
}

fn sign_block(block: &Block) -> (Signature, String) {
    let private_key = crypto::generate_ed25519().expect("generate Ed25519 key");
    let public_key = private_key.public_key();
    let signer_did = public_key.did().expect("derive signer DID");
    let signature = Signature::new(
        SignatureHeader::new(
            SignatureType::EdDSA,
            hex::encode(public_key.raw()).into_bytes(),
        ),
        private_key
            .sign(&block.to_dag_cbor().expect("encode block"))
            .expect("sign block"),
    );
    (signature, signer_did)
}

#[test]
fn verified_signer_returns_signer_did() {
    let block = test_block();
    let (signature, signer_did) = sign_block(&block);

    let verified_did =
        verified_signature_signer_did(&block, &signature).expect("valid signature must verify");
    assert_eq!(verified_did, signer_did);
}

#[test]
fn verified_signer_rejects_tampered_block() {
    let block = test_block();
    let (signature, _) = sign_block(&block);
    let mut tampered = block;
    let CrdtDelta::Lww(payload) = &mut tampered.delta else {
        panic!("expected LWW delta");
    };
    payload.data = b"tampered".to_vec();

    let error = verified_signature_signer_did(&tampered, &signature)
        .expect_err("tampered block must fail verification");
    assert!(error.contains("signature verification"), "{error}");
}

#[test]
fn verified_signer_rejects_invalid_identity() {
    let block = test_block();
    let (mut signature, _) = sign_block(&block);
    signature.header.identity = b"not hex".to_vec();

    let error = verified_signature_signer_did(&block, &signature)
        .expect_err("invalid identity must fail verification");
    assert!(error.contains("invalid signature identity"), "{error}");
}

#[test]
fn bls_block_signatures_require_the_declared_suite() {
    let key = blst::min_pk::SecretKey::key_gen(&[42; 32], &[]).unwrap();
    let public = key.sk_to_pk().to_bytes();
    let expected_did =
        crypto::PublicKey::did(&crypto::BlsPublicKey::from_bytes(&public).unwrap()).unwrap();
    let block = test_block();
    let bytes = block.to_dag_cbor().unwrap();
    let mut signature = Signature::new(
        SignatureHeader::new(SignatureType::BLSAugV1, hex::encode(public).into_bytes()),
        key.sign(
            &bytes,
            b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_AUG_",
            &public,
        )
        .to_bytes()
        .to_vec(),
    );
    assert_eq!(
        verified_signature_signer_did(&block, &signature).unwrap(),
        expected_did
    );
    let encoded = signature.to_dag_cbor().unwrap();
    assert_eq!(Signature::from_dag_cbor(&encoded).unwrap(), signature);
    signature.value = key
        .sign(&bytes, b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_", &[])
        .to_bytes()
        .to_vec();
    assert!(verified_signature_signer_did(&block, &signature).is_err());
}
