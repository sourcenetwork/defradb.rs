//! The read access a verified, signed document genesis grants when this node
//! registers the document for the first time. The host opts a collection in
//! through `DocumentMutationProjector::genesis_read_access`; the model is
//! `proofs/tla/GenesisReadAccess.tla` with `proofs/lean/EnrollmentGenesis.lean`.

use datastore::multistore::GenesisReadAccess;
use identity::Did;

/// The relation an initial reader is granted.
pub const GENESIS_READER_RELATION: &str = "reader";

/// The reader a genesis asks for, before the first-registration guard.
/// `immutable_field` returns a field's encoded value only when the genesis
/// holds it as immutable; `receiver` is this node's DID. An address that is
/// absent, mutable or not a CBOR string names nobody.
pub fn genesis_reader<'a, F>(
    access: Option<GenesisReadAccess>,
    immutable_field: F,
    receiver: Option<&Did>,
) -> Option<Did>
where
    F: FnOnce(&str) -> Option<&'a [u8]>,
{
    match access? {
        GenesisReadAccess::Public => Some(Did::wildcard()),
        GenesisReadAccess::AddressedBy(field) => {
            let address: String = ciborium::from_reader(immutable_field(field)?).ok()?;
            receiver
                .filter(|receiver| receiver.as_str() == address)
                .cloned()
        }
    }
}

/// The reader a creator grants for its own genesis when its provider cannot
/// join the create's transaction. Receivers share that provider and cannot
/// grant themselves, so the creator stages what the addressed node would
/// have granted itself (`creator_matches_addressed_receiver` in
/// `proofs/lean/EnrollmentGenesis.lean`). `address` returns the created
/// document's addressed field only when the collection holds it immutable;
/// an address that is not a DID (the wildcard included) names nobody
/// (`isDid` in the same Lean file).
pub fn creator_reader<'a, F>(access: Option<GenesisReadAccess>, address: F) -> Option<Did>
where
    F: FnOnce(&str) -> Option<&'a str>,
{
    match access? {
        GenesisReadAccess::Public => Some(Did::wildcard()),
        GenesisReadAccess::AddressedBy(field) => Did::new(address(field)?).ok(),
    }
}

/// Register a signed genesis's author as owner and, only when this is the
/// document's first registration here, grant the initial `reader`. A genesis
/// that arrives again finds the document registered and grants nothing, so a
/// revocation by the owner stands.
pub async fn register_signed_genesis(
    acp: &dyn acp::DocumentACP,
    signer: &Did,
    policy_id: &str,
    resource: &str,
    doc_id: &str,
    reader: Option<&Did>,
) -> acp::Result<()> {
    let initial_reader = match reader {
        Some(reader) if !acp.is_doc_registered(policy_id, resource, doc_id).await? => Some(reader),
        _ => None,
    };
    acp.register_doc_object(signer, policy_id, resource, doc_id)
        .await?;
    if let Some(reader) = initial_reader {
        acp.add_actor_relationship(
            signer,
            reader,
            policy_id,
            resource,
            doc_id,
            GENESIS_READER_RELATION,
            &[],
        )
        .await?;
    }
    Ok(())
}
