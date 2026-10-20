//! The conformance registry: every modeled family's headline property, the
//! model that proves it, the source it is anchored to, and how it is bound to
//! the real binary.
//!
//! This table is the spine. `matrix::every_modeled_family_is_bound` asserts that
//! each family in `proofs/README.md`'s *Modeled* list appears here, so a new
//! model cannot land without declaring how it is kept honest against the code.

/// Which proof tool establishes the property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// TLA+ / TLC temporal-safety model (`proofs/tla/`).
    Tla,
    /// Lean 4 functional/algebraic proof (`proofs/lean/`).
    Lean,
}

/// How a property is checked for conformance with the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Exercised against the running release binary (`tests/tla_conformance.rs`).
    Behavioral,
    /// A model vocabulary/contract asserted against the live Rust types
    /// (`tests/lean_conformance.rs`) — anti-drift, no binary needed.
    Contract,
    /// An assumed boundary (crypto / connectivity / bounded-N / foreign
    /// substrate): surfaced, never asserted, so a green matrix never reads as
    /// "this was proven against the artifact."
    Boundary,
}

pub struct Property {
    /// Family name — must match a row in `proofs/README.md`'s Modeled table.
    pub family: &'static str,
    /// The headline invariant or theorem.
    pub name: &'static str,
    pub axis: Axis,
    /// `file:symbol` the model is grounded to.
    pub anchor: &'static str,
    /// The TLC config (`*.cfg`) or Lean theorem that establishes it.
    pub model_ref: &'static str,
    /// How this property is bound to the implementation.
    pub tiers: &'static [Tier],
}

use Axis::{Lean, Tla};
use Tier::{Behavioral, Boundary, Contract};

pub const PROPERTIES: &[Property] = &[
    Property {
        family: "Vector index link maintenance",
        name: "INV_NoLostLinks — blind additions preserve links and stale structural pruning aborts",
        axis: Tla,
        anchor: "crates/storage/src/backends/regolith/merge/vector.rs; crates/db/src/index/vector/engine/hnsw/insert.rs; crates/db/tests/vector/concurrent_hnsw.rs",
        model_ref: "MC_VectorLinks_Green.cfg / MC_VectorLinks_Red_Puts.cfg / MC_VectorLinks_Red_Prune.cfg",
        tiers: &[Contract],
    },
    Property {
        // Two writers appending to one collection without seeing each other must
        // both land, leaving two tips for a later merge: that is what makes the
        // head set a CRDT rather than a last-writer-wins cell. The eager-delete
        // strategy makes them write the same key, which regolith refuses at
        // every isolation level, so the RED configuration is the defect and the
        // GREEN one is the derived-head fix.
        family: "Concurrent collection-head transitions",
        name: "INV_NoWriteConflict — concurrent appends to one collection both commit and form siblings",
        axis: Tla,
        anchor: "crates/db/src/block/builder/collection.rs write_collection_block; crates/storage/src/backends/regolith/transaction.rs",
        model_ref: "MC_HeadSet_Green.cfg (GREEN) / MC_HeadSet_Red_EagerDelete.cfg (RED) / MC_HeadSet_Red_PerKeyScan.cfg (RED)",
        tiers: &[Behavioral],
    },
    Property {
        // A replicated block is published into its document's head set once:
        // its priority index entry records the publication, so a replay never
        // makes a superseded head live again, and a parent held ahead is read
        // by its own part of the set, so the parent's concurrent publication
        // (and nothing unrelated) refuses that decision. No guard key exists.
        family: "Concurrent collection-head transitions",
        name: "INV_LiveExact / INV_AheadExact / INV_NoUnrelatedAbort — document head publication without guard keys",
        axis: Tla,
        anchor: "crates/db/src/block/heads/document.rs publish/was_published; crates/storage/src/backends/regolith/merge/head_set.rs part/touches; crates/db/tests/block/document_heads.rs, crates/db/tests/merge/head_publication.rs",
        model_ref: "MC_HeadPublication_Green.cfg (GREEN) / MC_HeadPublication_Red_{NoRecord,OwnUnvalidated,NoPartRead,WholeSet}.cfg (RED)",
        tiers: &[Contract],
    },
    Property {
        // The algebra the temporal model rests on: every key a writer writes
        // names that writer, so two writers cannot collide (disjointness), and
        // the eager strategy demonstrably does collide (non-vacuity). Appending
        // a block makes it a head and un-heads the parents it named, which is
        // what the delete used to achieve.
        family: "Concurrent collection-head transitions",
        name: "derived_writeSets_disjoint / eager_writeSets_overlap / applyDerived_parents_not_head",
        axis: Lean,
        anchor: "crates/db/src/block/heads/document.rs iterator/supersede; crates/db/src/block/heads.rs live_collection_heads; crates/db/src/block/builder/collection.rs write_collection_block",
        model_ref: "HeadSet.Core (lake build)",
        tiers: &[Contract],
    },
    Property {
        family: "Commit-ordered document arrivals",
        name: "dense positions / commit order / at-most-once / snapshot prefix",
        axis: Tla,
        anchor: "crates/db/src/event/arrivals.rs record; crates/storage/src/backends/regolith/transaction.rs append; crates/db/tests/read/arrival_transactions.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/CommitOrderedAppend.tla (GREEN Groups/Pipeline/Faults, RED Counter/OnceFromView/AssignBeforeValidation)",
        tiers: &[Behavioral],
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_ProviderOncePerContext / INV_NewRegistrationSurvivesOldAck / INV_NewRegistrationSurvivesOldClear",
        axis: Tla,
        anchor: "crates/db/src/write/autocommit/{create,update,delete}.rs NeedsInput resolver; crates/query/src/prepared.rs; crates/storage/src/stores/peerstore.rs register/complete/clear retry scope",
        model_ref: "MC_RetryPreparedInputs_{Green,Red_*}.cfg / MC_RetryMarkerGeneration_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_QueuedPushOwnsLiveMarker — admission requires current durable ownership and elapsed backpressure",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore.rs admit_push_scope; crates/p2p-adapter/src/retry.rs spawn_failure_recorder; crates/p2p/src/sync/coordinator/push_worker.rs run_push_job",
        model_ref: "MC_RetryPushAdmission_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_RegistrationCannotCrossFence — allocated registrations preserve peer incarnation and marker order",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore.rs RegisterScopeMarkerAttempt; create_replicator",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Collection schema-version OCC",
        name: "INV_NoStaleSchemaWrite — a writer cannot commit against an obsolete structural version",
        axis: Tla,
        anchor: "crates/db/src/txn/mod.rs validate_collection_for_write; crates/db/src/write/autocommit/{create,update,delete}.rs; crates/db/src/definition structural updates",
        model_ref: "MC_CollectionVersionOCC_Green.cfg / MC_CollectionVersionOCC_Red_NoValidation.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Explicit transaction statement atomicity",
        name: "INV_AbortedEffectsRemoved / INV_SuccessfulEffectsRetained / INV_NoCommitWhileCleanupPending",
        axis: Tla,
        anchor: "crates/query/src/runner/mutation.rs explicit transaction mutation execution; crates/db/src/txn/mod.rs statement savepoint; crates/storage/src/backends/regolith/transaction.rs savepoint callbacks",
        model_ref: "MC_StatementAtomicity_Green.cfg / MC_StatementAtomicity_Red_*.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "B3 filtered replication",
        name: "INV_DagComplete — filtered push still converges per-document",
        axis: Tla,
        anchor: "crates/p2p filtered replication; crates/db (merge)",
        model_ref: "MC_S4_ModelB.cfg / M1Convergence.cfg",
        tiers: &[Behavioral],
    },
    Property {
        // Two behavioral legs: live-forward convergence (replication.rs) and
        // convergence across a real partition (partition.rs) — node1 is restarted
        // to sever the link, each side writes independently, and both converge to
        // hold every write once reconnected.
        family: "DAG convergence (partition / eviction / restart)",
        name: "INV_Converged — every node receives every delta under eventual connectivity",
        axis: Tla,
        anchor: "crates/db (merge) merge handler; crates/blockstore",
        model_ref: "MC_Conv_Eventual.cfg",
        tiers: &[Behavioral],
    },
    Property {
        family: "CRDT merge laws",
        // Generic CrdtField core (DefraConvergence.CrdtField): comm+assoc => the merge
        // fold is order-independent (every field). Idempotence is the dividing line:
        // LWW (lwwMerge) is idempotent => a join-semilattice, re-delivery-safe, no dedup
        // (lww_dup_safe); the counter (Int +, including PNCounter negative deltas)
        // is NOT idempotent (counter_not_idempotent) => it must apply each delta
        // exactly once, the algebraic root of the #4935 double-apply. Counter, LWW,
        // and the mixed Counter×LWW product fully instantiate the core (counterCM /
        // lwwCM / mixedCM). PNCounter is the same signed-delta algebra with
        // decrement enabled, covered by singleStore_pncounter_converges and exact
        // live/restart/storm behavioral tests. The mixed product's cross-field
        // materialization hazard is checked by MixedFieldMaterialization.
        name: "CrdtField: comm+assoc => order-independent; PNCounter refines to signed Counter; mixed Counter×LWW inherits dedup",
        axis: Lean,
        anchor: "crates/crdt/src/lww.rs set_value; crates/crdt/src/counter.rs; crates/crdt/src/composite.rs componentwise field merge",
        model_ref: "DefraConvergence.MixedField (lake build); MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Contract, Behavioral],
    },
    Property {
        // Two behavioral legs: backfill (a write that pre-dates the replicator is
        // delivered) and full resume across a real disconnect (node1 restarted —
        // its state survives on disk and a post-disconnect write arrives after
        // reconnect). The resume leg needs an actual partition: config-level
        // replicator delete does not gate sync between already-connected peers, so
        // it restarts node1 via the harness `restart_node` (fixed upstream in
        // backbone 025d396 to respawn the configured binary, not the debug path).
        family: "Replicator lifecycle (no-loss / resume)",
        name: "INV_NoLoss — reconnect recomputes the target gap, no block dropped",
        axis: Tla,
        anchor: "crates/p2p/src/replicator.rs; crates/db/src/merge/push_docs_transport.rs",
        model_ref: "MC_Replicator_Resumable_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ObligationConservation / INV_PendingServiceable / INV_PendingHasAuthenticatedProvider / INV_SingleFlight / INV_SingleMergeWriter / INV_SenderMarkersOnly",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) push replay; crates/storage peerstore retry markers",
        model_ref: "MC_SyncOwnership_Green.cfg",
        // The deterministic A/B and restart/fanout witnesses live in the
        // integration-test crate, outside this conformance binary's behavioral
        // harness. Keep this binding honest until that harness drives them.
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — document scope",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) document marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Doc.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — collection scope",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) collection marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Collection.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_FetchHasQualifiedProvider / LIVE_EventualCurrency — immutable origin plus durable same-root alternate",
        axis: Tla,
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs; crates/p2p/src/sync/pending_store.rs; crates/p2p/src/sync/coordinator/mod.rs",
        model_ref: "MC_SyncOwnership_Green_Liveness_IrohRelay.cfg",
        tiers: &[Boundary],
    },
    Property {
        // A second PushLog for a CID already being processed joins that
        // processing instead of being nacked: it waits, bounded by a deadline,
        // looks again once the owner lets go, and either acks a block already
        // merged or registered, or takes the guard and processes its own copy.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesDurable / INV_OneProcessor / LIVE_EveryJoinEnds: same-CID PushLog joins the in-flight owner",
        axis: Tla,
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence, join_in_flight; crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire",
        model_ref: "MC_PushLogJoin_{Green,Red_NoRecheck,Red_NoGuard,Red_NoDeadline}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "acquire_exclusive / join_acks_only_durable / join_processes_only_unowned: the CID guard is exclusive and a join acks only a safe block",
        axis: Lean,
        anchor: "crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire, release_sync; crates/p2p/src/sync/manager/process/pushlog.rs join_in_flight",
        model_ref: "SingleFlight.Join (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Multi-instance claim",
        name: "INV_EventualUnique — claim CAS converges to a single winner",
        axis: Tla,
        anchor: "gents:crates/gents/src/lifecycle/claim.rs:claim_inner (foreign substrate, not the defradb.rs binary)",
        model_ref: "MC_Claim_Filtered_Eventual.cfg",
        tiers: &[Boundary],
    },
    Property {
        // Verify-before-merge fires only on the P2P merge path; injecting a
        // forged/unsigned block needs an adversarial peer, not reachable through
        // the public CLI. Rust makes signature verification mandatory (model->code
        // audit) and EUF-CMA is a named crypto boundary, so this is structural.
        family: "Block integrity / signatures",
        name: "INV_OnlyVerifiedMerged — verify-before-merge, author bound to verified DID",
        axis: Tla,
        anchor: "crates/db (merge) sig verify; crates/defra-core/src/batch_signing.rs",
        model_ref: "MC_Integrity_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        // The unauthorized-denied leg is now observable: a serve-side
        // `tracing::warn!("DEK release DENIED")` at the KMS policy check makes the
        // previously-silent denial loggable, and the behavioral test reads node0's
        // log to confirm node0 denies the DEK to an unauthorized node1's fetch.
        // ECIES secrecy of a released envelope remains a crypto Boundary (proven
        // by crates/kms ecies_envelope unit tests).
        family: "KMS key distribution",
        name: "INV_AuthorizedEventuallyGets / no-unauthorized-usable",
        axis: Tla,
        anchor: "crates/kms/src/defra_kms.rs serve_request; crates/kms/src/nac_dac_policy.rs",
        model_ref: "MC_Kms_Green.cfg",
        tiers: &[Behavioral, Boundary],
    },
    Property {
        // Encryption reuses PriorityReconcile.lwwCM unchanged: ciphertext and
        // key timing affect when a value can materialize, not which value wins.
        // The behavioral legs preserve encrypted fields selected for filtered
        // replication and receive both encrypted siblings after a real restart,
        // proving identical DAGs and the decrypted winner.
        family: "Encrypted LWW restart/replay",
        name: "INV_NoFilteredLoss / INV_LwwWinner — encrypted delivery remains complete and LWW-convergent",
        axis: Tla,
        anchor: "crates/db/src/merge/push_docs.rs; crates/db/src/merge/push_docs_transport.rs; crates/crdt/src/lww.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        // The model and focused KMS/merge unit tests cover the retry mechanics,
        // but the external harness cannot deterministically hold a DEK request
        // unavailable between ciphertext acknowledgement and receiver restart.
        family: "Encrypted LWW restart/replay",
        name: "INV_AckBacked — acknowledged encrypted replay remains re-drivable across transient KMS failure and restart",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/composite_fields.rs; crates/p2p/src/sync/pending_store.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Management-channel auth (NAC gate)",
        name: "INV_OnlyAuthorizedManages — management ops require a valid scoped token",
        axis: Tla,
        anchor: "crates/http manage channel; crates/db (nac)",
        model_ref: "MC_Auth_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_RevocationConsistent + both User and _commits paths gated",
        axis: Tla,
        anchor: "crates/acp; crates/zanzibar; crates/query dag-scan",
        model_ref: "MC_Acp_Green.cfg / MC_Commits_Green.cfg",
        tiers: &[Behavioral, Contract],
    },
    Property {
        // The light client checks an answer at the newest root it has
        // authenticated, and refuses one that a block finalized during the
        // round trip overtook. The vera provider asks such a read again, at
        // most ATTEMPTS times: the refusal reaches a caller only after a block
        // overtook every ask, and a stale answer is never accepted.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NeverStale / INV_FailsOnlyWhenEveryAskOvertaken / INV_AttemptsBounded / LIVE_Answers: a proven read the chain overtook is asked again, bounded, never accepted stale",
        axis: Tla,
        anchor: "crates/vera/src/vera_rs/provider/verified.rs verified, ATTEMPTS; crates/vera/src/vera_rs/provider.rs create_policy, query_policy, query_object_owner, verify_access, create_access_decision; acp-light-client HeaderChain::accept_response",
        model_ref: "MC_VerifiedRead_{Green,Green_Exhausted,Red_AcceptStale,Red_NoRetry,Red_Uncapped}.cfg",
        tiers: &[Boundary],
    },
    Property {
        // Registrations of one document that race each write their own owner
        // key, so only the commit can refuse all but one. The deciding prefix
        // scan is a validated scan: a registration that commits under the
        // prefix after another's snapshot makes the other conflict, and it
        // returns false. The sentinel byte it replaces never conflicted on
        // regolith, which commits a write that leaves a key as it was.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_OneOwner / INV_WinnersOwn / INV_SomeoneWins: racing registrations of one document leave exactly one owner",
        axis: Tla,
        anchor: "crates/acp/src/persistent.rs PersistentAcpStore::register_doc_atomic, delete_doc_tuples; crates/acp/src/store.rs AcpStore::register_doc_atomic",
        model_ref: "MC_RegisterDocOnce_{Green,Red_NoValidation,Red_Sentinel}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Storage transaction validation",
        name: "Validated point reads reject crossing writes",
        axis: Tla,
        anchor: "crates/storage/src/backends/regolith/transaction.rs; proofs/tests/behavioral/ssi.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/DefraLevel.tla",
        tiers: &[Behavioral, Boundary],
    },
    Property {
        // The replay-capability gate lives inside the P2P replication wire
        // protocol; presenting a forged/expired capability needs a custom peer,
        // not the public CLI. Rust-only hardening Go lacks; structural.
        family: "P2P explicit-replay capability gate",
        name: "INV_OnlyLegitAccepted — forged/expired/wrong-target capability rejected",
        axis: Tla,
        anchor: "crates/p2p capability replay gate",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "NAC lifecycle privilege-escalation",
        name: "INV_NoPrivEsc — no escalation across enable/disable/restart",
        axis: Tla,
        anchor: "crates/db (nac) lifecycle",
        model_ref: "MC_Nac_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        // No-loss / no-double-apply under concurrent same-document mutation is now
        // BEHAVIORAL: `partition::convergence_concurrent_same_doc_merge_storm`
        // storms one PCounter doc from a 3-node mesh and asserts the exact sum
        // (below => a delta dropped, above => double-applied). Mixed-field legs
        // assert the product state (LWW name + counter views) after live, restart,
        // and 3-node full-mesh replay; `MC_MixedFieldMaterialization` proves the
        // stale whole-document commit hazard RED/GREEN.
        // The process-local document queue and counter-finalize gate are gone.
        // Ordinary application markers validate exactly-once decisions; counter
        // operands fold in the engine. Materialized mirrors retain validated
        // reads. The adapter test `interactive_counter_increment_conflicts_with_concurrent_same_doc_merge`
        // checks conflict/rollback/retry convergence. Engine OCC is a boundary;
        // the historical serialized MergeQueue model is deliberately retired.
        family: "Transaction & merge-queue concurrency",
        name: "INV_NoLoss / INV_NoDoubleApply under concurrent same-doc mutation",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/counter.rs; crates/db/src/write/doc.rs; crates/db/tests/merge/merge_handler_tests.rs; crates/db txn registry",
        model_ref: "MC_TxnRegistry_Green.cfg / MC_TwoStoreCounter_Green.cfg / MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Behavioral, Boundary],
    },
    Property {
        // A commit begins when it is handed to its transaction's owner, or,
        // if a request is running the transaction then, when the owner takes
        // it; a caller that stops waiting before that abandons it. One that
        // began still lands after its caller stops waiting. The caller raises
        // the database's fence count
        // and swaps the commit's state word; the owner swaps it once it is
        // finished, and whichever swap loses lowers the count. Every new
        // transaction and read waits in DB::new_txn while the count is above
        // zero, so it sees the commit.
        family: "Transaction & merge-queue concurrency",
        name: "INV_ReaderSeesCancelledCommit / INV_NeverNegative / INV_UncancelledUntouched / INV_SettledExact / INV_BegunEndRuns / INV_AbandonedEndDiscarded / LIVE_NoFenceLeak / LIVE_ReaderProceeds: a cancelled commit that began is fenced until it lands; one that had not begun is abandoned",
        axis: Tla,
        anchor: "crates/db/src/txn/fence.rs CommitFences, Fence::abandon, Fence::finish; crates/db/src/txn/owner/mod.rs Owners::run, Owners::finish, FenceOnDrop; crates/db/src/txn/owner/actor.rs own; crates/db/src/database/mod.rs DB::new_txn",
        model_ref: "MC_CommitFence_{Green,Red_RaiseAfterSwap,Red_OwnerNoSwap,Red_ReaderNoWait,Red_ForgetFailed,Red_UndoNoWake,Red_SkipBegun,Red_RunAbandoned}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "exactly_one_wins / one_lowering_per_raise / caller_win_means_owner_lowers / uncancelled_owner_wins: the two swaps settle each fence once",
        axis: Lean,
        anchor: "crates/db/src/txn/fence.rs Fence::abandon, Fence::finish",
        model_ref: "CommitFence.Swap (lake build)",
        tiers: &[Boundary],
    },
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
        anchor: "crates/telemetry/src/engine_tickers.rs Source::publish, EngineTickerExport::drop, engine_ticker_totals; crates/storage/src/backends/shared.rs TransactionMetrics",
        model_ref: "MC_EngineTickers_{Green,Green_TwoSources,Red_Backward,Red_BlindStore,Red_GiveUp,Red_StaleSeen,Red_RemoveFirst,Red_WeakClose,Red_Reset,Red_SharedStats,Red_CountAfterClose,Finding_ClaimGap}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Engine ticker export",
        name: "failed_attempt_changes_nothing / run_gain / run_total_eq_gains / publishAlone_comm / closed_export_adds_exactly: a mark moved by compare-and-swap adds each count once",
        axis: Lean,
        anchor: "crates/telemetry/src/engine_tickers.rs Source::publish",
        model_ref: "EngineTickers.Mark, EngineTickers.Race (lake build)",
        tiers: &[Boundary],
    },
    Property {
        // A placed pool claims its CPUs in the process's ledger, one counter
        // per CPU: alone, moving each counter from 0 to 1 with one
        // compare-and-swap and giving back what it took when one CPU is
        // held; or shared, adding one whatever the counter was. After its
        // capped claims alone fail it shares and says so. Its Hold gives back
        // exactly what it raised once the pool and every thread placed
        // through it are gone.
        family: "Worker pool CPU ledger",
        name: "INV_AloneTakesDisjoint / INV_AloneRunsAlone / INV_TakenAlone / INV_AllOrNothing / INV_HeldNeverFree / INV_CountExact / INV_DoneLeavesNothing / LIVE_StartEnds: placed pools never share a CPU unless one of them says it shares",
        axis: Tla,
        anchor: "crates/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop; crates/workers/src/placement/reserve.rs reserve, shared; crates/workers/src/pool/placer.rs Placer::holding; crates/workers/src/pool/start.rs Pool::start_in",
        model_ref: "MC_CpuLedger_{Green,Red_SplitTake,Red_NoRollback,Red_ReleaseLoser,Red_ShareUncounted,Red_NoAttemptCap,Red_ReleaseEarly,Red_RunOutsideHold,Red_DoubleRelease,Red_ForgetHold}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Worker pool CPU ledger",
        name: "hold_all_or_nothing / rollback_restores / hold_won_alone / hold_step / share_step / release_alone_step / release_shared_step: a claim alone takes all or nothing, the counters stay exact, Holds claimed alone stay apart",
        axis: Lean,
        anchor: "crates/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop",
        model_ref: "CpuLedger.{Hold,Count,Exclusive} (lake build)",
        tiers: &[Boundary],
    },
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
        anchor: "crates/workers/src/pool/default.rs default_pool, Pool::make_default, DefaultPool::register, DefaultPool::get, DefaultPool::started; crates/workers/src/pool/mod.rs Pool::stop; crates/workers/src/pool/deadline.rs delay_on",
        model_ref: "MC_DefaultPool_{Green,Red_AnswerYes,Red_BlindRegister,Red_FailKeepsTurn,Red_NoClosingCheck,Red_NoRecheck,Red_NoRetry,Red_NoTurn,Red_RacyCell,Red_RegisterClosing,Red_RegisterOverwrites,Red_ShutStarted,Red_StartedFirst,Red_StopAsksFirst,Red_StrongSlot,Red_WorkerRefusedFirst,Red_WorkerWaits}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Worker default pool",
        name: "first_running_wins / stopped_never_selected / later_registrations_change_nothing / first_calls_agree / registered_wins / stop_deadline_elsewhere: the first running registered pool wins, a stopped one never does, every caller gets one started pool",
        axis: Lean,
        anchor: "crates/workers/src/pool/default.rs DefaultPool::register, DefaultPool::get",
        model_ref: "DefaultPool.{Slot,Cell,Get} (lake build)",
        tiers: &[Boundary],
    },
    Property {
        // `partition::convergence_delete_update_race_preserves_tombstone`
        // partitions a replicated doc, deletes on one side, updates a mutable field
        // on the other, then heals and asserts the merged materialized view stays
        // tombstoned on both replicas. The TLA red leg captures the bad policy:
        // committing an active update from a stale whole-document snapshot clears
        // the delete marker.
        family: "Document materialization status convergence",
        name: "INV_DeletedMarkerAbsorbs — active rematerialization never clears a tombstone",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/composite_persist.rs handle_deletion / persist_merged_document; crates/crdt/src/composite.rs status",
        model_ref: "MC_DocumentMaterialization_Green.cfg; DefraConvergence.DocumentMaterialization.delete_active_age_converge",
        tiers: &[Behavioral],
    },
    Property {
        // The DID-binding half (a distinct identity cannot impersonate another)
        // IS observed behaviorally by the ACP test (ungranted bob denied). The
        // signer-binding half — forging a malformed / alg-confused token, or one
        // whose iss/signer DID mismatches its signature — is unreachable through
        // the public CLI (the client always signs correctly), so it is Boundary.
        family: "JWT issuer / algorithm binding",
        name: "INV_TokenBindsGenuineDid — alg/iss/sig bound to did(pubkey)",
        axis: Tla,
        anchor: "crates/identity token verification",
        model_ref: "MC_Jwt_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "CID content-addressing determinism + Block canonicalization",
        name: "cid_injective_mod_hash — same normal-form content yields same CID",
        axis: Lean,
        anchor: "crates/defra-core/src/block.rs Block::generate_cid",
        model_ref: "Cid (lake build)",
        tiers: &[Behavioral, Contract],
    },
    Property {
        family: "Deferred-ACP overlay consistency",
        name: "INV_FailClosedActive — txn-local ACP projection gates as committed state would",
        axis: Tla,
        anchor: "crates/query/src/txn/context.rs; crates/db/src/write/autocommit/batch.rs",
        model_ref: "MC_DeferredAcp_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        // Lean marker values (mNull=0..mIntMax=253) verified to match the live
        // `crates/storage/src/encoding/mod.rs` constants by inspection, but those
        // are `pub(crate)` (no static assert without widening prod visibility) and
        // a plain ordered query may sort in memory rather than exercise the index
        // key encoding — so neither tier is cleanly realizable. Boundary.
        family: "Order-preserving key encoding",
        name: "asc_strictly_order_preserving_* — encoding is a strict-order embedding",
        axis: Lean,
        anchor: "crates/storage/src/encoding/mod.rs type-marker constants",
        model_ref: "OrderEncoding (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Index-maintenance consistency",
        name: "onDocumentUpdate_correct / _no_stale / _none_missing",
        axis: Lean,
        anchor: "crates/db (index) index maintenance",
        model_ref: "IndexMaintenance (lake build)",
        tiers: &[Behavioral, Contract],
    },
    Property {
        // `index::index_reconciles_lww_merge_after_restart` drives a restart
        // partition where two replicas write different values for the same indexed
        // LWW field. Once the DAG converges, indexed filters must expose exactly
        // the winning value and no stale seed/loser keys.
        family: "Index-maintenance consistency",
        name: "INV_IndexMatchesWinner — indexed filters equal the converged CRDT value",
        axis: Tla,
        anchor: "crates/db (merge) LWW materialization; crates/db (index) on_document_update",
        model_ref: "MC_IndexReconciliation_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        family: "Inbound stream admission",
        name: "INV_ResidentWithinBudget / INV_ShareBound / LIVE_HealthyHandled / LIVE_WaitEnds",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_streams.rs admit_stream; crates/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, body_deadline; crates/p2p/src/sync/dispatch_queue.rs dispatch_queue; crates/p2p/src/sync/event_dispatcher.rs run_event_dispatcher",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Inbound stream admission",
        name: "acquire_conserves / release_all_restores / admit_failure_keeps_nothing",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundLease",
        model_ref: "ByteBudget (lake build)",
        tiers: &[Boundary],
    },
];
