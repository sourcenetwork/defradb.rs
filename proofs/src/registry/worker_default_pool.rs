use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "workers";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // `default_pool()` answers the first registered pool that still runs,
        // else one pool started once for the process. Registration moves the
        // slot only off a pool that stopped, with one compare-and-swap; the
        // started pool is built under a one-permit turn and published once; a
        // worker thread never waits for that start; `Pool::stop` never times
        // its own deadline on the pool it stops.
        family: "Worker default pool",
        name: "INV_NeverHandsOutClosing / INV_FirstRunningWins / INV_RegisteredWins / INV_OneStart / INV_OneDefault / INV_ErrorHonest / INV_StopDeadlineElsewhere / INV_MakeDefaultTellsTruth / INV_WorkerNeverWaits / INV_RegistrationKeepsNothingAlive / LIVE_EveryCallReturns / LIVE_EveryRegisterReturns: one default pool, never a closing one",
        axis: Tla,
        anchor: "crates/foundation/workers/src/pool/default.rs default_pool, Pool::make_default, DefaultPool::register, DefaultPool::get, DefaultPool::started; crates/foundation/workers/src/pool/mod.rs Pool::stop; crates/foundation/workers/src/pool/deadline.rs delay_on",
        model_ref: "MC_DefaultPool_{Green,Red_AnswerYes,Red_BlindRegister,Red_FailKeepsTurn,Red_NoClosingCheck,Red_NoRecheck,Red_NoRetry,Red_NoTurn,Red_RacyCell,Red_RegisterClosing,Red_RegisterOverwrites,Red_ShutStarted,Red_StartedFirst,Red_StopAsksFirst,Red_StrongSlot,Red_WorkerRefusedFirst,Red_WorkerWaits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Worker default pool",
        name: "first_running_wins / stopped_never_selected / later_registrations_change_nothing / first_calls_agree / registered_wins / stop_deadline_elsewhere: the first running registered pool wins, a stopped one never does, every caller gets one started pool",
        axis: Lean,
        anchor: "crates/foundation/workers/src/pool/default.rs DefaultPool::register, DefaultPool::get",
        model_ref: "DefaultPool.{Slot,Cell,Get} (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
