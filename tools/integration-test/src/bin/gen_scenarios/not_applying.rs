//! Go scenarios that do not apply to defradb.rs, by scenario ID: the
//! `NotApplying` variant and the reason each generated test carries.

const ENTRIES: &[(&str, &str, &str)] = &[(
    "mutation.add.field_kinds.one_to_one_to_one.transactional-creation-and-linking-of-relational-documents-backward",
    "GoBugSuspect",
    "Go fails the second commit with \"transaction conflict\", an SSI anti-dependency on the \
     publisher each transaction read (go.mutation.txn.conflict). The transactions write \
     disjoint books and publishers, so regolith commits both and both books link correctly.",
)];

/// The variant and reason for scenario `id`, when it does not apply.
pub fn lookup(id: &str) -> Option<(&'static str, &'static str)> {
    ENTRIES
        .iter()
        .find(|(entry, _, _)| *entry == id)
        .map(|(_, kind, reason)| (*kind, *reason))
}
