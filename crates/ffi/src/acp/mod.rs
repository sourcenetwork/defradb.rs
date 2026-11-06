//! Access Control Policy (ACP) operations for FFI.
//!
//! This module exposes ACP management functions for both:
//! - NAC (Node Access Control) - node-level permissions
//! - DAC (Document Access Control) - document-level permissions

mod dac;
mod identity;
pub(crate) mod identity_ops;
mod nac;

pub use dac::{
    defra_acp_dac_policy_add, defra_acp_dac_relationship_add, defra_acp_dac_relationship_delete,
};
pub use identity::{defra_identity_new_from_bearer_token, defra_node_set_identity};
pub use nac::{
    defra_acp_nac_disable, defra_acp_nac_re_enable, defra_acp_nac_relationship_add,
    defra_acp_nac_relationship_delete, defra_acp_nac_status,
};

#[cfg(test)]
mod nac_tests;
#[cfg(test)]
mod tests;
