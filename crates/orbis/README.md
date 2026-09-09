# Orbis signing

Defra uses `orbis.v0.sign.SignService/StartSign` with a registered key derivation.
Configure an Ed25519 service identity authorized by that derivation's policy,
the derivation ID, and its independently provisioned BLS12-381 public key:

```sh
DEFRA_ALLOW_NON_GO_VERIFIABLE_SIGNING=1 defradb start --signer-type orbis \
  --signer-orbis-endpoint http://127.0.0.1:50051 \
  --signer-orbis-derivation-id "$DERIVATION_ID" \
  --signer-orbis-public-key "$DERIVED_PUBLIC_KEY" \
  --identity "$SERVICE_IDENTITY"
```

BLS-signed documents require Rust peers. The service identity is a hex-encoded
64-byte Ed25519 private key.

The derivation ID is 64 lowercase hexadecimal characters. The public key is hex
encoded; obtain it from authenticated ring and derivation metadata or trusted
operator provisioning. The signing endpoint cannot replace this key.

Each request carries a fresh Ed25519 token binding the derivation and message
digest. Defra verifies the returned signature before storing it. Authorization
comes from the registered derivation; per-request policy or access-decision
overrides are rejected. The former ring-ID/derivation-label options and
`UtilityService` protocol are no longer used.
