//! Conformance: binds the formal models in `proofs/tla/` and `proofs/lean/` to
//! the real defradb.rs release binary.
//!
//! Two axes, matching the two proof tools:
//! - **Lean axis (auto):** `proofs/lean/Conformance.lean` emits a JSON contract
//!   that the live Rust types are asserted against — anti-drift, no binary.
//!   See [`lean_contract`] and `tests/lean_conformance.rs`.
//! - **TLA axis (behavioral):** each family's invariant is driven against the
//!   running release binary via the backbone `defra-harness`.
//!   See `tests/tla_conformance.rs`.
//!
//! [`registry`] is the spine — one file per family under `registry/` — and its
//! families are the modeled families; [`matrix::families_table`] prints them
//! (`cargo run -p conformance --example families`), and
//! `matrix::every_family_names_its_crates` makes each new family name the
//! crates its models cover, so a model cannot land without a conformance
//! hook. [`anchor`] resolves every registry anchor against the tree, and
//! [`constants`] holds the P2P constants the models depend on to the code
//! that sets them.

pub mod anchor;
pub mod constants;
pub mod lean_contract;
pub mod matrix;
pub mod model_trace;
pub mod registry;
