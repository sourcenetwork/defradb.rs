//! Go scenarios that do not apply to defradb.rs, by scenario ID: the
//! `NotApplying` variant and the reason each generated test carries.

const ENTRIES: &[(&str, &str, &str)] = &[
    (
        "mutation.update.crdt.pncounter-float-overflow-increment",
        "Interop",
        NON_FINITE,
    ),
    (
        "mutation.update.crdt.pncounter-float-overflow-decrement",
        "Interop",
        NON_FINITE,
    ),
];

const NON_FINITE: &str =
    "The result holds an infinite Float, which JSON cannot carry, so Go's own \
     HTTP and CLI clients cannot read it back (defradb#2569) and Go runs this only in-process. \
     defradb.rs refuses it on every client with Go's encoding error, \
     \"json: unsupported value: +Inf\" (tests/query/non_finite_float.rs).";

/// The variant and reason for scenario `id`, when it does not apply.
pub fn lookup(id: &str) -> Option<(&'static str, &'static str)> {
    ENTRIES
        .iter()
        .find(|(entry, _, _)| *entry == id)
        .map(|(_, kind, reason)| (*kind, *reason))
}
