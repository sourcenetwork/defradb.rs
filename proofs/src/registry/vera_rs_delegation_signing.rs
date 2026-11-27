use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "vera";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "vera.rs delegation signing",
        name: "admitted_iff_vera_accepts / sent_is_accepted / refused_is_not_sent / honest_answer_admitted / honest_ed25519_answer_admitted / ed25519_refusal_is_length_or_verification / refusal_names_the_first_failure / ed25519_refusal_names_the_first_failure",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/signer/admission.rs admit, actor_key; crates/engine/vera-delegation/src/bearer.rs create_scoped_bearer_token_at",
        model_ref: "VeraDelegationSigner (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs delegation signing",
        name: "names_iff_vera_takes_own_key / every_secp256k1_spelling_names_its_key / base64url_of_another_type_names_nothing / another_key_is_never_named / old_check_refused_a_taken_spelling / wire_names_the_signer / defra_one_spelling_per_key / defra_idempotent",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/signer/admission.rs ActorKey::names; crates/engine/vera-delegation/src/did_spelling.rs defra, wire; crates/engine/vera-delegation/src/bearer.rs create_scoped_bearer_token_at",
        model_ref: "VeraIssuerSpelling (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
