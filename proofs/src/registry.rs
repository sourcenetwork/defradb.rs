//! The conformance registry: every modeled family's headline property, the
//! model that proves it, the source it is anchored to, and how it is bound to
//! the real binary.
//!
//! One file per family under `registry/`, each with its own `PROPERTIES`
//! table; `build.rs` collects the family files into `FAMILIES`, in name
//! order. The family files are the spine: their distinct `family` values are
//! the modeled families.
//! `matrix::readme_lists_exactly_the_modeled_families` asserts that
//! `proofs/README.md`'s *Modeled families* table lists exactly these, so a new
//! model cannot land without declaring how it is kept honest against the code.

/// Which proof tool establishes the property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// TLA+ / TLC temporal-safety model (`proofs/tla/`).
    Tla,
    /// Lean 4 functional/algebraic proof (`proofs/lean/`).
    Lean,
}

/// How a property is checked for conformance with the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Exercised against the running release binary (`tests/tla_conformance.rs`).
    Behavioral,
    /// A model vocabulary/contract asserted against the live Rust types
    /// (`tests/lean_conformance.rs`) — anti-drift, no binary needed.
    Contract,
    /// An assumed boundary (crypto / connectivity / bounded-N / foreign
    /// substrate): surfaced, never asserted, so a green matrix never reads as
    /// "this was proven against the artifact."
    Boundary,
}

pub struct Property {
    /// Family name — must match a row in `proofs/README.md`'s Modeled families table.
    pub family: &'static str,
    /// The headline invariant or theorem.
    pub name: &'static str,
    pub axis: Axis,
    /// The source the model is grounded to: paths and the symbols in them,
    /// resolved against the tree by `anchor::every_registry_anchor_resolves`.
    pub anchor: &'static str,
    /// The TLC config (`*.cfg`) or Lean theorem that establishes it.
    pub model_ref: &'static str,
    /// How this property is bound to the implementation.
    pub tiers: &'static [Tier],
    /// The tests that bind a Contract or Behavioral property, in anchor
    /// syntax: each file and the test functions in it. Empty for a property
    /// that is only a Boundary.
    pub bound_by: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/registry_families.rs"));

/// Every property of every family file, family files in name order.
pub fn properties() -> impl Iterator<Item = &'static Property> {
    FAMILIES.iter().flat_map(|family| family.iter())
}
