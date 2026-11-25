use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A Go-written config.yaml has each Go key moved onto its Rust key
        // before it is read: a Rust key already there wins, a file with no
        // Go keys is untouched, and no two Go keys feed one Rust key.
        family: "Go config layout",
        name: "rust_key_wins / rust_config_unchanged / table_injective / go_keys_are_not_rust_keys: a present Rust key is never overwritten, a config with no Go keys loads unchanged, and the table is injective",
        axis: Lean,
        anchor: "crates/entry-points/cli/src/config/go_layout/keys.rs KEYS, apply, take, put_default; crates/entry-points/cli/src/config/go_layout/mod.rs apply, set_default",
        model_ref: "GoConfigLayout (lake build)",
        tiers: &[Contract],
        bound_by: "crates/entry-points/cli/src/config/go_layout/keys.rs the_table_matches_the_lean_model",
    },
];
