//! Vera delegations, made without the provider.
//!
//! A client that acts through a node's vera.rs ACP signs delegations for the
//! node's worker: a plain one for every policy command
//! ([`caller_delegation`]), or one bound to a single command
//! ([`request_delegation`], [`RequestBinding`]) by its digest
//! ([`request_digest`]). This crate builds both, checks each signature as
//! vera.rs would before a token leaves it ([`signer::admission`]), and holds
//! the typed policy commands those digests are computed from. It depends on
//! neither the vera.rs client nor a network runtime; the `vera` crate's
//! provider builds on it and re-exports these names.

pub mod abi;
pub mod bearer;
mod command_digest;
mod command_error;
pub mod did_spelling;
pub mod policy_cmd;
mod policy_command;
mod policy_id;
pub mod signer;
mod subject_ref;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use bearer::{
    caller_delegation, request_delegation, request_delegation_at, BearerError, RequestBinding,
    MAX_REQUEST_LIFETIME,
};
pub use command_digest::request_digest;
pub use command_error::CommandError;
pub use policy_command::{PolicyAction, PolicyCommand, PolicySubject};
pub use policy_id::{parse_policy_id, PolicyIdError};
pub use signer::{
    signature_from_der, DelegationAlgorithm, DelegationSigner, Ed25519Signer, P256Signer,
    Secp256k1Signer, SignerError,
};
pub use subject_ref::SubjectRef;
pub use vera_crypto::jwt::DelegationScope;
