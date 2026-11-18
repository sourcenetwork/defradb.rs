mod bearer;
mod client;
#[cfg(test)]
mod delegation_parity;
mod provider;
mod provider_commands;
mod transport;
mod worker;

use vera_delegation::{abi, did_spelling, policy_cmd, signer};

pub use acp_light_client::{AcpTransport, HeaderStream, TransportError};
pub use bearer::{
    caller_delegation, request_delegation, request_delegation_at, BearerError, RequestBinding,
    MAX_REQUEST_LIFETIME,
};
pub use client::{ClientError, VeraRsClient};
pub use provider::VeraRsProvider;
pub use transport::RpcTransport;
pub use vera_delegation::request_digest;
pub use vera_delegation::signer::{
    signature_from_der, DelegationAlgorithm, DelegationSigner, Ed25519Signer, P256Signer,
    Secp256k1Signer, SignerError,
};
pub use worker::NativeWorker;
