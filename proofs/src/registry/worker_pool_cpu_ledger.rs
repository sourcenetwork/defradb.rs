use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A placed pool claims its CPUs in the process's ledger, one counter
        // per CPU: alone, moving each counter from 0 to 1 with one
        // compare-and-swap and giving back what it took when one CPU is
        // held; or shared, adding one whatever the counter was. After its
        // capped claims alone fail it shares and says so. Its Hold gives back
        // exactly what it raised once the pool and every thread placed
        // through it are gone. A network runtime sized from the machine
        // claims whole cores the same way, alone only, and gives up holding
        // nothing instead of sharing.
        family: "Worker pool CPU ledger",
        name: "INV_AloneTakesDisjoint / INV_AloneRunsAlone / INV_TakenAlone / INV_AllOrNothing / INV_HeldNeverFree / INV_CountExact / INV_DoneLeavesNothing / LIVE_StartEnds: placed pools never share a CPU unless one of them says it shares",
        axis: Tla,
        anchor: "crates/foundation/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop; crates/foundation/workers/src/placement/reserve.rs reserve, shared; crates/foundation/workers/src/placement/edge.rs EdgeCores::reserve_on, EdgeCores::reserve; crates/foundation/workers/src/pool/placer.rs Placer::holding; crates/foundation/workers/src/pool/start.rs Pool::start_in; crates/foundation/edge/src/platform/native.rs build",
        model_ref: "MC_CpuLedger_{Green,Green_Edge,Red_SplitTake,Red_NoRollback,Red_ReleaseLoser,Red_ShareUncounted,Red_NoAttemptCap,Red_ReleaseEarly,Red_RunOutsideHold,Red_DoubleRelease,Red_ForgetHold,Red_EdgeNoCap}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Worker pool CPU ledger",
        name: "hold_all_or_nothing / rollback_restores / hold_won_alone / hold_step / share_step / release_alone_step / release_shared_step: a claim alone takes all or nothing, the counters stay exact, Holds claimed alone stay apart",
        axis: Lean,
        anchor: "crates/foundation/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop",
        model_ref: "CpuLedger.{Hold,Count,Exclusive} (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
