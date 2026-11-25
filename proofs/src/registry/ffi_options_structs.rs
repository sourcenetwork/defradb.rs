use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The C API's options structs grow only at their end and default to
        // zero: the library reads the caller's bytes up to the caller's size,
        // treats the rest as zero, and refuses a set field it does not know.
        family: "FFI options structs",
        name: "keeps_what_the_caller_gave / older_caller_gets_defaults / refuses_an_unknown_set_field / refuses_a_length_inside_a_field / unset_new_fields_read_as_absent / unset_new_fields_are_accepted",
        axis: Lean,
        anchor: "crates/entry-points/ffi/src/options.rs read, ZeroDefault",
        model_ref: "FfiOptions (lake build)",
        tiers: &[Contract],
        bound_by: "crates/entry-points/ffi/src/options_tests.rs null_options_are_every_default, an_older_callers_shorter_struct_gets_defaults_for_the_new_fields, a_newer_callers_longer_struct_is_read_when_its_new_fields_are_unset, a_set_field_the_library_does_not_know_is_refused, a_size_that_stops_inside_a_field_is_refused; crates/entry-points/ffi/src/api/sized_tests.rs options_this_library_does_not_know_are_refused, an_older_callers_shorter_options_get_defaults_for_the_rest, a_size_that_cuts_a_field_is_refused",
    },
];
