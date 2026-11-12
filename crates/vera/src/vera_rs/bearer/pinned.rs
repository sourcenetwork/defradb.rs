//! The exact bytes of a delegation token for a fixed key, claims and time.
//!
//! ES256K signing here is deterministic (RFC 6979), so a token is a function
//! of its key, its claims and the time it is issued at. vera.rs records and
//! verifies these bytes; a change to how a token is built or signed must not
//! move them.

use vera_client::{DelegationScope, Secp256k1SigningKey as SigningKey};

use super::*;
use crate::vera_rs::signer::Secp256k1Signer;

const ISSUED_AT: u64 = 1_700_000_000;
const WORKER: &str = "did:key:zWorker";

/// The token the key `[7; 32]` gives `WORKER` for five minutes on deployment
/// 9001, under the compressed or the uncompressed spelling of its DID.
async fn token(compressed: bool, scope: DelegationScope) -> String {
    let key = SigningKey::from_slice(&[7; 32]).expect("valid key");
    let issuer = did_from_signing_key(&key, compressed);
    let signer = Secp256k1Signer::from_slice(&[7; 32]).expect("valid key");
    issue_at(&signer, &issuer, WORKER, 9001, 300, scope, ISSUED_AT)
        .await
        .expect("token")
}

#[nuclei::test]
async fn a_delegation_token_is_these_exact_bytes() {
    assert_eq!(
        token(false, DelegationScope::PolicyCommands).await,
        concat!(
            "eyJhbGciOiJFUzI1NksiLCJ0eXAiOiJ2ZXJhLWRlbGVnYXRpb24tdjErand0In0.",
            "eyJhdWQiOiJ2ZXJhOjkwMDEiLCJleHAiOjE3MDAwMDAzMDAsImlhdCI6MTcwMDAwMDAwMCwiaXNzIjoiZGlkOmtleTp6N3I4b3JYZ1RFMVpZRGVNempRazlnNUZuc0h0WUYxTUJvOXc0UFVUZlBEeFZrb2c0Z3R5eGEzTmlMQXhrQUxYQnRMZnlDMnN0Rm1nZVZWa1VKUVNSUnNxdkdlS0giLCJuYmYiOjE2OTk5OTk5NzAsInNjb3BlIjoiYWNwOnBvbGljeSIsInN1YiI6ImRpZDprZXk6eldvcmtlciJ9.",
            "f5zzIzllneNive-9Kp7BRr9Pgt82UBdn9o8-l6E_ObtYjkDYBh0vJP3WO9EPg_4bLEdYbwYY6S8yiPoVQhCzZg",
        )
    );
    assert_eq!(
        token(false, DelegationScope::CreatePolicy).await,
        concat!(
            "eyJhbGciOiJFUzI1NksiLCJ0eXAiOiJ2ZXJhLWRlbGVnYXRpb24tdjErand0In0.",
            "eyJhdWQiOiJ2ZXJhOjkwMDEiLCJleHAiOjE3MDAwMDAzMDAsImlhdCI6MTcwMDAwMDAwMCwiaXNzIjoiZGlkOmtleTp6N3I4b3JYZ1RFMVpZRGVNempRazlnNUZuc0h0WUYxTUJvOXc0UFVUZlBEeFZrb2c0Z3R5eGEzTmlMQXhrQUxYQnRMZnlDMnN0Rm1nZVZWa1VKUVNSUnNxdkdlS0giLCJuYmYiOjE2OTk5OTk5NzAsInNjb3BlIjoiYWNwOnBvbGljeTpjcmVhdGUiLCJzdWIiOiJkaWQ6a2V5OnpXb3JrZXIifQ.",
            "xyn9GghpEE6SfjjsMJMAycq0yFIF1Kp5RyeRThtwE64ji7KZ9nzTkzUlhS2Imk0U0xvxgTLdPOke-v9eTCZWUA",
        )
    );
    assert_eq!(
        token(true, DelegationScope::PolicyCommands).await,
        concat!(
            "eyJhbGciOiJFUzI1NksiLCJ0eXAiOiJ2ZXJhLWRlbGVnYXRpb24tdjErand0In0.",
            "eyJhdWQiOiJ2ZXJhOjkwMDEiLCJleHAiOjE3MDAwMDAzMDAsImlhdCI6MTcwMDAwMDAwMCwiaXNzIjoiZGlkOmtleTp6UTNzaFhnV2pWc0NKc3Y5bUJtNmtWcUZTakFuRXJNZzN6RzlDY3l2bVVDQ2FGUkNyIiwibmJmIjoxNjk5OTk5OTcwLCJzY29wZSI6ImFjcDpwb2xpY3kiLCJzdWIiOiJkaWQ6a2V5OnpXb3JrZXIifQ.",
            "3gUSJns8MiQC1bpKT-v8CAOh7Bhg2TTSF_UG5NDVBwQ0f7IoLOhSPiBrMjHgFvL2y7M5Fjb8SCOB2sWJoe8i3Q",
        )
    );
}
