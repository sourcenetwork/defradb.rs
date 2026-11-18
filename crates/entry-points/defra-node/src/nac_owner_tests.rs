use super::nac_owner;
use crate::error::Error;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const NODE: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

#[test]
fn a_named_owner_does_not_enable_nac() {
    let owner = nac_owner(false, Some(OWNER), &None, false).unwrap();
    assert!(owner.is_none(), "NAC not enabled has no owner: {owner:?}");
}

#[test]
fn a_named_owner_owns_nac_without_a_node_identity() {
    let owner = nac_owner(true, Some(OWNER), &None, false).unwrap();
    assert_eq!(owner.map(|did| did.to_string()), Some(OWNER.to_owned()));
}

#[test]
fn a_named_owner_must_be_a_did() {
    let error = nac_owner(true, Some("not a did"), &None, false).unwrap_err();
    assert!(
        matches!(error, Error::InvalidNodeAcpIdentity(_)),
        "{error:?}"
    );
}

#[test]
fn without_a_named_owner_the_node_identity_owns_nac() {
    let owner = nac_owner(true, None, &Some(NODE.to_owned()), true).unwrap();
    assert_eq!(owner.map(|did| did.to_string()), Some(NODE.to_owned()));
}

#[test]
fn without_a_named_owner_nac_needs_a_node_identity_and_its_key() {
    let error = nac_owner(true, None, &None, true).unwrap_err();
    assert!(matches!(error, Error::NodeAcpNeedsIdentity), "{error:?}");
    let error = nac_owner(true, None, &Some(NODE.to_owned()), false).unwrap_err();
    assert!(matches!(error, Error::NodeAcpNeedsLocalKey), "{error:?}");
}
