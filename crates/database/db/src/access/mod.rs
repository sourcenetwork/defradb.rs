//! Access-control adapters: node access, NAC guards, DAC policies and KMS
//! wiring.

pub mod dac;
pub mod kms;
pub(crate) mod nac;
pub mod nac_ops;
pub mod node;
#[cfg(not(target_arch = "wasm32"))]
mod refused_create;
