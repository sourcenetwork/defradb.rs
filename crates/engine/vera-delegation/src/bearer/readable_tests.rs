//! A token vera.rs would not read is refused before its signer is asked.

use super::*;
use crate::testing::{Recording, Returns};

const NOW: u64 = 1_900_000_000;

async fn minted(subject: &str) -> (Result<String, BearerError>, usize) {
    let signer = Recording::returning(Returns::Genuine);
    let outcome = create_scoped_bearer_token_at(
        signer.as_ref(),
        signer.did(),
        subject,
        7,
        60,
        DelegationScope::PolicyCommands,
        NOW,
    )
    .await;
    (outcome, signer.signed.len())
}

#[nuclei::test]
async fn a_delegation_with_no_subject_is_refused_unsigned() {
    let (outcome, signed) = minted("").await;
    assert!(matches!(outcome, Err(BearerError::Subject)));
    assert_eq!(signed, 0);
}

#[nuclei::test]
async fn a_delegation_is_signed_exactly_when_vera_rs_reads_all_of_it() {
    let (mut fits, mut too_long) = (1, MAX_TOKEN_BYTES);
    while too_long - fits > 1 {
        let middle = fits + (too_long - fits) / 2;
        match minted(&"s".repeat(middle)).await.0 {
            Ok(_) => fits = middle,
            Err(_) => too_long = middle,
        }
    }

    let (kept, signed) = minted(&"s".repeat(fits)).await;
    let kept = kept.expect("token");
    assert_eq!(signed, 1);
    assert!(kept.len() <= MAX_TOKEN_BYTES && kept.len() + 4 > MAX_TOKEN_BYTES);
    assert!(vera_crypto::jwt::verify_bearer_token(&kept).is_ok());

    let (refused, signed) = minted(&"s".repeat(too_long)).await;
    assert!(matches!(refused, Err(BearerError::Size { bytes }) if bytes > MAX_TOKEN_BYTES));
    assert_eq!(signed, 0);
}
