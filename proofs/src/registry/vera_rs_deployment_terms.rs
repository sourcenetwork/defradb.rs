use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "vera";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "vera.rs deployment terms",
        name: "missing_is_suffix / takeWhile_is_filter / scan_finds_missing / floor_covers_pruned: the submissions a takeover voided are exactly the journal's suffix from the first void one, found by asking newest first, and the newest pruned entry vouches for every older one",
        axis: Lean,
        anchor: "crates/engine/vera/src/vera_rs/follow/mod.rs reconcile, uncovered; crates/engine/vera/src/vera_rs/follow/journal.rs Journal::record, Journal::prune, Floor",
        model_ref: "VeraConfirmationJournal (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs deployment terms",
        name: "deny_wins / settled_reads_confirmed / confirmed_outranks_grant / settled_is_proven / unanswered_grant_is_unproven / refused_restores / applied_reads_fact / confirmed_reads_fact / last_iff_end / walk_covers: an unanswered denial wins a recovery slot, an unanswered grant neither outranks what was confirmed nor counts as confirmed, a refused change leaves the slot as it was, the newest answer replaces what was unanswered, and pages read one after the other hand over every slot once and end at the last",
        axis: Lean,
        anchor: "crates/engine/acp/src/recovery.rs resolve, RecoveryLedger::confirm, RecoveryLedger::prepare, RecoveryLedger::settle, RecoveryLedger::page; crates/engine/acp/src/external/recovery.rs RecoverySource::next",
        model_ref: "VeraRecoveryLedger (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs deployment terms",
        name: "foldl_push / applied_is_filter / owed_is_filter / unrestorable_is_filter / every_replay_in_one_place / existing_policy_is_applied / owed_is_a_change / unrestorable_is_policy / event_keeps_every_entry / event_keeps_names_and_actors / closed_is_told: a takeover report puts every replay in one place and keeps its order, a policy that is there is not reported lost, and the event keeps every entry with its name and actor and says when the check failed closed",
        axis: Lean,
        anchor: "crates/engine/vera/src/vera_rs/follow/lost.rs held_again, lost_effects; crates/engine/vera/src/vera_rs/follow/report.rs TakeoverReport, Applied, Unrestorable; crates/engine/vera/src/vera_rs/provider/terms.rs takeover_report",
        model_ref: "VeraTakeoverReport (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
