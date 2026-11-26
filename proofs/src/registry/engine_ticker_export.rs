use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "telemetry, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Every open store exports its engine's commit and policy tickers. A
        // read of the totals, or the store's close, adds what the engine
        // counted past the export's mark by swapping the mark forward with one
        // compare-and-swap; only the winner adds, exactly the stretch it
        // claimed. The close adds before it leaves the map. The finding
        // configuration shows the claim and the add are two steps: a read can
        // briefly miss a closed store's counts that another read claimed and
        // has not added yet. The race test exercises the code on real threads;
        // nothing drives this model against the binary.
        family: "Engine ticker export",
        name: "INV_Accounted / INV_NeverOvercount / INV_ClosedComplete / INV_LeftClaimed / INV_QuiescentExact / PROP_TotalNeverBack / PROP_MarkNeverBack / LIVE_EveryCallEnds: every count reaches the exported total once, and the total never goes back",
        axis: Tla,
        anchor: "crates/building-blocks/telemetry/src/engine_tickers.rs Source::publish, EngineTickerExport::drop, engine_ticker_totals; crates/engine/storage/src/backends/shared.rs TransactionMetrics",
        model_ref: "MC_EngineTickers_{Green,Green_TwoSources,Red_Backward,Red_BlindStore,Red_GiveUp,Red_StaleSeen,Red_RemoveFirst,Red_WeakClose,Red_Reset,Red_SharedStats,Red_CountAfterClose,Finding_ClaimGap}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Engine ticker export",
        name: "failed_attempt_changes_nothing / run_gain / run_total_eq_gains / publishAlone_comm / closed_export_adds_exactly: a mark moved by compare-and-swap adds each count once",
        axis: Lean,
        anchor: "crates/building-blocks/telemetry/src/engine_tickers.rs Source::publish",
        model_ref: "EngineTickers.Mark, EngineTickers.Race (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
