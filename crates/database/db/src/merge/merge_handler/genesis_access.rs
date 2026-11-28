//! The read access a created document's genesis asks for when the creator's
//! provider cannot join the create's transaction. The host opts a collection
//! in through `DocumentMutationProjector::genesis_read_access`; the model is
//! `proofs/lean/EnrollmentGenesis.lean`.

use datastore::multistore::GenesisReadAccess;
use identity::Did;

/// The relation an initial reader is granted.
pub const GENESIS_READER_RELATION: &str = "reader";

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
