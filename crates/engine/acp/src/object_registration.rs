//! Several objects registered to one owner in one go.
//!
//! An external provider applies a registration as one request, and a request
//! costs it a block. The registrations one transaction makes for one owner
//! go together ([`crate::DocumentACP::register_objects`]), as one list the
//! provider applies in order, all or none. A provider that cannot apply a
//! list sends them one at a time, so a failure names the prefix it applied.

use crate::error::Error;

/// One object an owner is registered for: a document (its DocID) or a
/// branchable collection (its collection ID).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcpObject<'a> {
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object_id: &'a str,
}

/// Where a list of registrations stopped: the objects before `index` are
/// registered, the one at `index` failed with `error`, and the ones after it
/// were not tried.
#[derive(Debug)]
pub struct RegistrationFailure {
    pub index: usize,
    pub error: Error,
}

impl RegistrationFailure {
    pub fn new(index: usize, error: Error) -> Self {
        Self { index, error }
    }
}
