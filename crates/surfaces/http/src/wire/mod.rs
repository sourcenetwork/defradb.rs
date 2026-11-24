//! The JSON shapes the HTTP surface serves, mirrored from the domain types.
//!
//! A mirror repeats the fields of the type it stands for and converts with
//! `From`, so the REST wire contract is defined in the crate that owns it and
//! `schemars` never reaches the data-path crates. A newtype cannot do this: the
//! derive forwards to the inner type, and `impl JsonSchema` for a foreign type
//! is blocked by the orphan rule.
//!
//! Each mirror has a round-trip test: a populated domain value and its mirror
//! must serialize to the same JSON. That is what keeps the two in step, since
//! nothing else compares them.

mod acp;
mod acp_status;
mod query;
mod version;

pub use acp::{DocumentRelationship, PermissionProvenance, PermissionVia};
pub use acp_status::{DelegationAudience, LightClientStatus};
pub use query::{DocumentsResult, SourcePermission};
pub use version::{GoCompat, VersionInfo};
