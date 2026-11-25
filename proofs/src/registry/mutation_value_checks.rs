use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "document, defra-core, query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Mutation value checks",
        name: "Rfc3339Nano.no_trailing_zero / trim_restores / all_zero_trims_to_nothing / nonzero_keeps_a_digit",
        axis: Lean,
        anchor: "crates/building-blocks/document/src/encoding.rs trim_rfc3339_trailing_zeros, format_time_rfc3339_nano",
        model_ref: "Rfc3339Nano (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Mutation value checks",
        name: "DocIdText.length_refusal_only_for_unaccepted / accepted_length_never_says_length / digits_length / parse_accepts",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/doc_id.rs parse_uuid, ParsedDocId::from_string",
        model_ref: "DocIdText (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Mutation value checks",
        name: "SecondaryRelation.primary_never_refused / secondary_id_refused / same_answer_for_relation_and_id / checks_agree",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/mutation_inputs.rs normalize_mutation_input_fields, input_field_name; crates/building-blocks/document/src/document.rs Document::set",
        model_ref: "SecondaryRelation (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
