use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // graphql-go checks a literal filter's keys against its input type
        // and treats each variable as valid there; the variable's value is
        // checked by coercion.
        family: "Filter input validation",
        name: "literal_walk_skips_variable_positions / variable_value_never_matters: a literal's unknown-field errors never depend on what its variables hold",
        axis: Lean,
        anchor: "crates/engine/query/src/query_parse/filter_shape/walk.rs literal, literal_fields; crates/engine/query/src/query_parse/filter_shape/mod.rs check; crates/engine/query/src/query_parse/validation.rs check_filter_shapes",
        model_ref: "FilterInputShape (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/tests/filter_source.rs a_nested_variable_is_recorded_at_its_position; tools/integration-test/tests/testo/query/simple/with_filter_unknown.rs a_bad_literal_is_reported_before_its_variables, unknown_operator_in_a_nested_variable_is_rejected",
    },
    Property {
        // An unknown key is one error: the walk does not look below it.
        family: "Filter input validation",
        name: "unknown_key_is_one_error: nothing below an unknown key is reported",
        axis: Lean,
        anchor: "crates/engine/query/src/query_parse/filter_shape/walk.rs literal_fields",
        model_ref: "FilterInputShape (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/simple/with_filter_unknown.rs unknown_field_in_a_literal_filter_is_rejected",
    },
    Property {
        // A variable that stands for a whole `_and`/`_or` list is found at
        // its position and checked as a list, never skipped.
        family: "Filter input validation",
        name: "whole_list_variable_is_checked: a variable at a list key is checked as a list",
        axis: Lean,
        anchor: "crates/engine/query/src/query_parse/filter_shape/walk.rs shape_at, variable_list; crates/engine/query/src/query_parse/filter_shape/mod.rs check",
        model_ref: "FilterInputShape (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/tests/filter_shape_checks.rs a_whole_list_variable_under_and_is_checked; tools/integration-test/tests/testo/query/simple/with_filter_unknown.rs unknown_field_in_a_whole_and_list_variable_is_rejected",
    },
    Property {
        // A literal error under any root, an excluded mutation included,
        // wins over every variable error.
        family: "Filter input validation",
        name: "literal_anywhere_wins / excluded_literal_is_reported: literal errors win across roots",
        axis: Lean,
        anchor: "crates/engine/query/src/query_parse/filter_shape/mod.rs check, Found; crates/engine/query/src/query_parse/validation.rs check_filter_shapes; crates/engine/query/src/query_parse/parser.rs parse_selection_to_mutations",
        model_ref: "FilterInputShape (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/simple/with_filter_unknown.rs a_literal_error_in_a_later_root_wins_over_a_variable_error; tools/integration-test/tests/testo/mutation/delete/with_filter_unknown.rs a_skipped_delete_with_an_unknown_filter_field_is_rejected, a_delete_in_an_excluded_inline_fragment_is_rejected",
    },
];
