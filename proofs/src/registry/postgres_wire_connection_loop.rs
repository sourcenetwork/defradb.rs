use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "pg-compat";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // One task serves a pg-compat connection: one deadline read at the
        // accept bounds the peek, the SSL and GSS refusals and the startup;
        // an extended-protocol error skips to the Sync; a fatal error or a
        // cancel request closes it and the loop returns. A copy's CopyDone
        // leaves the copy, so an Execute copy's Sync reaches on_sync.
        family: "Postgres wire connection loop",
        name: "INV_StartupWithinDeadline / INV_StartedNeverTimedOut / INV_NoQueryBeforeStartup / INV_SkipUntilSync / INV_SyncResumes / INV_CopyEnds / INV_NothingAfterEnd / LIVE_Ends: no connection outlives the startup deadline unstarted, nothing but Sync is handled during a skip, nothing is read after the end",
        axis: Tla,
        anchor: "crates/database/pg-compat/src/wire/process.rs serve, run, refuse_ssl, dispatch, report; crates/database/pg-compat/src/wire/connection.rs Connection::next, Connection::is_closed, Connection::poll_close; crates/database/pg-compat/src/handler/auth.rs DIDAuthHandler::on_startup",
        model_ref: "MC_PgWire_{Green_Startup,Green_Protocol,Red_PeekNoDeadline,Red_SslNoDeadline,Red_StartupNoDeadline,Red_FreshDeadline,Red_DeadlineAfterStartup,Red_NoAwaitingSync,Red_HandleWhileSkipping,Red_SyncKeepsSkipping,Red_CopyFlagFromMessage,Red_CopyDoneStays,Red_ExtendedCopyDoneStays,Red_LoopAfterClose,Red_LoopAfterCloseRuns,Red_LoopAfterCloseHangs,Red_FatalStaysOpen,Red_StartupErrorNotFatal}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/pg-compat/tests/wire.rs ssl_is_refused_then_queries_run_and_a_fatal_error_closes_the_connection, a_client_that_starts_tls_directly_is_closed, a_client_that_sends_nothing_is_closed_at_the_startup_deadline",
    },
];
