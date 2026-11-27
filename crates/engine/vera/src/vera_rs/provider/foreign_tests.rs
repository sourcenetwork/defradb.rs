use super::*;

const GENESIS: [u8; 32] = [7; 32];

/// A home is read only once it was proven to serve the given deployment:
/// before that a read is retryably unavailable, and a home shown to serve
/// another deployment is refused for good.
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn a_home_is_read_only_once_its_genesis_is_proven() {
    assert!(matches!(
        serving(UNCHECKED, GENESIS),
        Err(ProviderError::Unavailable(message)) if message.contains(&hex::encode(GENESIS))
    ));
    assert!(serving(MATCHES, GENESIS).is_ok());
    assert!(matches!(
        serving(DIFFERS, GENESIS),
        Err(ProviderError::Config(_))
    ));
}
