#!/usr/bin/env bash
# Run every TLA+ model and check its verdict against the expected red/green oracle.
# Each run gets a unique -metadir (TLC's default scratch dir is per-second; a tight
# loop would otherwise collide). Exits non-zero if any verdict does not match.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# rows: <cfg> <module> <GREEN|RED>
RUNS=(
  "MC_ReplicatorIncarnation_Green.cfg ReplicatorIncarnation.tla GREEN"
  "MC_ReplicatorIncarnation_Red_Error.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_Rollback.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorInstall_Green.cfg ReplicatorInstall.tla GREEN"
  "MC_ReplicatorInstall_Red_Validation.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Active.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Fence.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Duplicate.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Settle.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Durable.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Deadline.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Ordered.cfg ReplicatorInstall.tla RED"
  "MC_ReplicatorInstall_Red_Recovery.cfg ReplicatorInstall.tla RED"
  "MC_PeerConnection_Green.cfg PeerConnection.tla GREEN"
  "MC_PeerConnection_Red_StaleObservation.cfg PeerConnection.tla RED"
  "MC_SchemaPublication_Green.cfg SchemaPublication.tla GREEN"
  "MC_SchemaPublication_Red_EarlyReply.cfg SchemaPublication.tla RED"
  "MC_RetryPreparedInputs_Green.cfg RetryPreparedInputs.tla GREEN" # exact-context external answer survives commit retry
  "MC_RetryPreparedInputs_Red_Recompute.cfg RetryPreparedInputs.tla RED" # clearing prepared memo repeats provider work
  "MC_RetryPreparedInputs_Red_InAttempt.cfg RetryPreparedInputs.tla RED" # provider work runs under active transaction
  "MC_RetryMarkerGeneration_Green.cfg RetryMarkerGeneration.tla GREEN" # old ack and peer clear are fenced
  "MC_RetryMarkerGeneration_Red_StaleAck.cfg RetryMarkerGeneration.tla RED" # stale ack erases newer registration
  "MC_RetryMarkerGeneration_Red_Readd.cfg RetryMarkerGeneration.tla RED" # stale peer clear erases re-added retry state
  "MC_RetryPushAdmission_Green.cfg RetryPushAdmission.tla GREEN"
  "MC_RetryPushAdmission_Red_NoPresence.cfg RetryPushAdmission.tla RED"
  "MC_RetryPushAdmission_Red_NoGeneration.cfg RetryPushAdmission.tla RED"
  "MC_RetryPushAdmission_Red_NoDeadline.cfg RetryPushAdmission.tla RED"
  "MC_RetryRegistrationFence_Green.cfg RetryRegistrationFence.tla GREEN"
  "MC_RetryRegistrationFence_Red_NoEpoch.cfg RetryRegistrationFence.tla RED"
  "MC_RetryRegistrationFence_Red_NoOrder.cfg RetryRegistrationFence.tla RED"
  "MC_CollectionVersionOCC_Green.cfg CollectionVersionOCC.tla GREEN" # schema-version dependency rejects stale writer
  "MC_CollectionVersionOCC_Red_NoValidation.cfg CollectionVersionOCC.tla RED" # stale schema-dependent write is admitted
  "MC_StatementAtomicity_Green.cfg StatementAtomicity.tla GREEN" # statement failure/cancellation restores only its staged effects before next op/commit
  "MC_StatementAtomicity_Red_NoWriteRollback.cfg StatementAtomicity.tla RED" # failed statement leaves staged storage writes
  "MC_StatementAtomicity_Red_NoSideEffectRollback.cfg StatementAtomicity.tla RED" # failed statement leaves callback side effects
  "MC_StatementAtomicity_Red_NextOperationSkipsCleanup.cfg StatementAtomicity.tla RED" # next statement crosses pending cancellation cleanup
  "MC_StatementAtomicity_Red_CommitSkipsCleanup.cfg StatementAtomicity.tla RED" # transaction commits before cancellation cleanup
  "MC_EventOrder_Green.cfg EventOrder.tla GREEN"
  "MC_EventOrder_Red_Arrival.cfg EventOrder.tla RED"
  "MC_TransactionDecisions_Green.cfg TransactionDecisions.tla GREEN"
  "MC_TransactionDecisions_Red_OutsideRead.cfg TransactionDecisions.tla RED"
  "MC_DefraKeyClassifier_Green.cfg DefraKeyClassifier.tla GREEN"
  "MC_DefraKeyClassifier_Red_BroadBlockPrefix.cfg DefraKeyClassifier.tla RED"
  "MC_DefraKeyClassifier_Red_BlockConcurrency.cfg DefraKeyClassifier.tla RED"
  "MC_RootSingleFlight_Green.cfg RootSingleFlight.tla GREEN" # #2079: one fetch per root and trust however many peers reply, and a reply after a failed fetch starts the next
  "MC_RootSingleFlight_Red_NoGuard.cfg RootSingleFlight.tla RED" # every reply fetches, as before (INV_OneFetchPerRoot)
  "MC_RootSingleFlight_Red_Keep.cfg RootSingleFlight.tla RED" # a failed fetch keeps its claim: the next reply starts nothing (INV_NoReplyLost)
  "MC_RootSingleFlight_Red_RootOnly.cfg RootSingleFlight.tla RED" # claim keyed by root alone: a trusted reply waits behind a plain fetch (INV_NoReplyLost)
  "MC_ProviderLookup_Green.cfg ProviderLookup.tla GREEN" # #2121: an empty lookup asks a connected peer, once per interval, and finds the owner
  "MC_ProviderLookup_GreenOwner.cfg ProviderLookup.tla GREEN" # the connected peer is the owner: its own record answers
  "MC_ProviderLookup_Red_NoAsk.cfg ProviderLookup.tla RED" # an empty lookup asks nobody, as before (LIVE_Found)
  "MC_ProviderLookup_Red_BookOnly.cfg ProviderLookup.tla RED" # the answer leaves out the answerer's own record (LIVE_Found)
  "MC_ProviderLookup_Red_Always.cfg ProviderLookup.tla RED" # every lookup asks, even one the book answers (INV_AskOnlyWhenEmpty)
  "M1Convergence.cfg            M1Convergence.tla            GREEN"  # B3 control (S1')
  "M1Naive.cfg                  M1Convergence.tla            RED"    # B3 #2721 (S1)
  "MC_S2.cfg                    MC_S2.tla                    GREEN"  # B3 ideal (S2)
  "MC_S3.cfg                    MC_S3.tla                    RED"    # B3 split ownership (S3)
  "MC_S3_Fixed.cfg              MC_S3.tla                    GREEN"  # B3 immutable key (S3)
  "MC_S4_Naive.cfg              MC_S4.tla                    RED"    # B3 field-grain #2721 (S4)
  "MC_S4_FullWalkA.cfg          MC_S4.tla                    RED"    # B3 Model A over-fetch (S4)
  "MC_S4_ModelB.cfg             MC_S4.tla                    GREEN"  # B3 Model B (S4)
  "MC_Conv_Eventual.cfg         MC_Conv_Eventual.tla         GREEN"  # convergence, eventual connectivity
  "MC_Conv_RestartEviction.cfg  MC_Conv_RestartEviction.tla  GREEN"  # convergence, eviction+restart
  "MC_Conv_NoHeadRediscovery.cfg MC_Conv_NoHeadRediscovery.tla RED"  # convergence fails w/o rediscovery
  "MC_Claim_Unfiltered_Eventual.cfg  MC_Claim_Common.tla     GREEN"  # claim eventual-unique
  "MC_Claim_Filtered_Eventual.cfg    MC_Claim_Common.tla     GREEN"  # claim eventual-unique (filtered)
  "MC_Claim_Unfiltered_Execution.cfg MC_Claim_Common.tla     RED"    # exec-unique fails (CAS race)
  "MC_Claim_Filtered_Execution.cfg   MC_Claim_Common.tla     RED"    # exec-unique fails (filtered)
  "MC_Claim_Split_Eventual.cfg       MC_Claim_Common.tla     RED"    # split same-DID breaks convergence
  "MC_Kms_Green.cfg             MC_Kms_Gossip.tla            GREEN"  # KMS policy-gated
  "MC_Kms_NoPolicy_Red.cfg      MC_Kms_Gossip.tla            RED"    # KMS unauthorized obtains key
  "MC_Kms_BroadcastCiphertext_Red.cfg MC_Kms_Gossip.tla     RED"    # KMS ciphertext to non-recipient
  "MC_Kms_RevokeReplay_Green.cfg MC_Kms_Replay.tla          GREEN"  # KMS revoke/replay gated
  "MC_Kms_Revoke_Red.cfg        MC_Kms_Replay.tla            RED"    # KMS revoked obtains key
  "MC_Kms_Replay_Red.cfg        MC_Kms_Replay.tla            RED"    # KMS replay grants key
  "MC_EncryptedLwwReplay_Green.cfg MC_EncryptedLwwReplay_Common.tla GREEN" # #1049: durable ciphertext/key replay converges across restart
  "MC_EncryptedLwwReplay_Red_TerminalUnavailable.cfg MC_EncryptedLwwReplay_Common.tla RED" # transient KMS miss is terminally acknowledged
  "MC_EncryptedLwwReplay_Red_VolatilePending.cfg MC_EncryptedLwwReplay_Common.tla RED" # acked ciphertext loses its retry obligation on restart
  "MC_EncryptedLwwReplay_Red_FilterDropsField.cfg MC_EncryptedLwwReplay_Common.tla RED" # replayed composite omits the encrypted LWW field link
  "MC_EncryptedLwwReplay_Red_ArrivalWins.cfg MC_EncryptedLwwReplay_Common.tla RED" # decrypted stale replay overwrites the LWW winner
  "MC_Auth_Green.cfg            MC_Auth_Green.tla            GREEN"  # auth gated
  "MC_Auth_Red_PeerOnly.cfg     MC_Auth_Red_PeerOnly.tla     RED"   # auth: PeerID-only executes
  "MC_Auth_Red_Stale.cfg        MC_Auth_Red_Stale.tla        RED"   # auth: stale token authorizes
  "MC_Auth_Red_WrongScope.cfg   MC_Auth_Red_WrongScope.tla   RED"   # auth: wrong-permission executes
  "MC_Replicator_Naive_Red.cfg       MC_Replicator_Naive_Red.tla       RED"   # replicator drops doc on disconnect
  "MC_Replicator_Resumable_Green.cfg MC_Replicator_Resumable_Green.tla GREEN" # replicator resumes -> no loss
  "MC_Commits_Red_UserOnly.cfg          MC_Commits_Red_UserOnly.tla          RED"   # commits leak (only User gated)
  "MC_Commits_Red_ReplicationUngated.cfg MC_Commits_Red_ReplicationUngated.tla RED" # commit block replicated to unauth peer
  "MC_Commits_Green.cfg                 MC_Commits_Green.tla                 GREEN" # both paths + replication gated
  "MC_Integrity_Red_NoCheck.cfg       MC_Integrity_Attacks.tla     RED"   # no sig check -> forged merges
  "MC_Integrity_Red_SigOnly.cfg       MC_Integrity_Attacks.tla     RED"   # sig-only (no author bind) -> spoof merges
  "MC_Integrity_Red_ReplayNoCheck.cfg MC_Integrity_Attacks.tla     RED"   # replayed sig over new content merges
  "MC_Integrity_Green.cfg             MC_Integrity_Attacks.tla     GREEN" # VerifyThenMerge gate
  "MC_Integrity_HonestConvergence.cfg MC_Integrity_Attacks.tla     GREEN" # gate doesn't block honest convergence
  "MC_Acp_Green.cfg            MC_Acp_Green.tla            GREEN" # ACP revocation propagates
  "MC_Acp_StaleCache_Red.cfg   MC_Acp_StaleCache_Red.tla   RED"   # stale cache grants revoked permission
  "MC_VerifiedRead_Green.cfg VerifiedRead.tla GREEN" # a proven read a block overtook is asked again and answered at the newest root
  "MC_VerifiedRead_Green_Exhausted.cfg VerifiedRead.tla GREEN" # a block overtakes every ask: the refusal returns only after the last one
  "MC_VerifiedRead_Red_AcceptStale.cfg VerifiedRead.tla RED" # light client skips the root check: a stale answer is accepted (INV_NeverStale)
  "MC_VerifiedRead_Red_NoRetry.cfg VerifiedRead.tla RED" # refused read never asked again: errors with asks left (INV_FailsOnlyWhenEveryAskOvertaken)
  "MC_VerifiedRead_Red_Uncapped.cfg VerifiedRead.tla RED" # retry without a cap asks past ATTEMPTS (INV_AttemptsBounded)
  "MC_RegisterDocOnce_Green.cfg RegisterDocOnce.tla GREEN" # racing registrations of one document: one owner, the winner owns it, one wins
  "MC_RegisterDocOnce_Red_NoValidation.cfg RegisterDocOnce.tla RED" # deciding scan not validated: two owners (INV_OneOwner)
  "MC_RegisterDocOnce_Red_Sentinel.cfg RegisterDocOnce.tla RED" # main before the fix: the identical sentinel write commits, two owners (INV_OneOwner)
  "MC_Capability_Green.cfg         MC_Capability_Common.tla   GREEN" # capability: only legit tokens authorized
  "MC_Capability_Red_Forge.cfg     MC_Capability_Common.tla   RED"   # capability: forged token accepted (INV_OnlyLegitAccepted)
  "MC_Capability_Red_Ttl.cfg       MC_Capability_Common.tla   RED"   # capability: over-cap TTL accepted (INV_TtlCapped)
  "MC_Capability_Red_WrongTarget.cfg MC_Capability_Common.tla RED"   # capability: wrong peer/collection accepted (INV_TargetBound)
  "MC_Capability_Red_Revoked.cfg   MC_Capability_Common.tla   RED"   # capability: revoked token accepted (INV_RevokedNeverAccepted)
  "MC_Nac_Green.cfg                MC_Nac_Green.tla                GREEN" # NAC lifecycle: no priv-esc across enable/disable
  "MC_Nac_Red_NoPersist.cfg        MC_Nac_Red_NoPersist.tla        RED"   # NAC: disabled-flag not persisted across restart
  "MC_Nac_Red_ReEnableLive.cfg     MC_Nac_Red_ReEnableLive.tla     RED"   # NAC: re-enable from stale live admin (escalation)
  "MC_Nac_Red_WriteWhileDisabled.cfg MC_Nac_Red_WriteWhileDisabled.tla RED" # NAC: admin write accepted while disabled
  "MC_TxnRegistry_Green.cfg        MC_TxnRegistry_Green.tla        GREEN" # txn cleanup never evicts a live transaction
  "MC_TxnRegistry_Red_NaiveSweep.cfg MC_TxnRegistry_Red_NaiveSweep.tla RED" # naive sweep evicts a still-live txn
  "MC_TwoStoreCounter_Red_Split.cfg TwoStoreCounter.tla           RED"   # counter reconcile-from-blob clobbers a concurrent local increment (INV_NoLoss)
  "MC_TwoStoreCounter_Red_DoubleApply.cfg TwoStoreCounter.tla     RED"   # #4935: re-delivered delta merged twice w/o dedup -> double-apply (INV_NoDoubleApply)
  "MC_TwoStoreCounter_Green.cfg     TwoStoreCounter.tla           GREEN" # unified RMW + merged-set dedup -> exact (no loss, no double)
  "MC_MixedFieldMaterialization_Red_WholeDoc.cfg MixedFieldMaterialization.tla RED" # stale whole-doc field merge clobbers another field
  "MC_MixedFieldMaterialization_Green.cfg MixedFieldMaterialization.tla GREEN" # componentwise field materialization preserves mixed product state
  "MC_IndexReconciliation_Red_SaveOnly.cfg IndexReconciliation.tla RED" # stale index key survives CRDT winner rematerialization
  "MC_IndexReconciliation_Green.cfg IndexReconciliation.tla GREEN" # index keys equal the winning CRDT materialized value
  "MC_DocumentMaterialization_Red_Overwrite.cfg DocumentMaterialization.tla RED" # active rematerialization must not clear delete marker
  "MC_DocumentMaterialization_Green.cfg DocumentMaterialization.tla GREEN" # deletion marker is componentwise/absorbing
  "MC_PushLogAdmission_Green.cfg MC_PushLogAdmission_Common.tla GREEN" # #1088 W1: capacity overflow nacks -> success implies registered-or-merged + all docs merge
  "MC_PushLogAdmission_Red_SuccessOnFull.cfg MC_PushLogAdmission_Common.tla RED" # fa4a84f7 regression: overflow drops registration but acks success (INV_SuccessImpliesRegisteredOrMerged)
  "MC_PushLogJoin_Green.cfg PushLogJoin.tla GREEN" # same-CID PushLog joins the in-flight owner: success implies durable, one processor, every join ends
  "MC_PushLogJoin_Red_NoRecheck.cfg PushLogJoin.tla RED" # waiter acks when the owner lets go without looking again (INV_SuccessImpliesDurable)
  "MC_PushLogJoin_Red_NoGuard.cfg PushLogJoin.tla RED" # waiter processes its copy without the CID guard (INV_OneProcessor)
  "MC_PushLogJoin_Red_NoDeadline.cfg PushLogJoin.tla RED" # waiter waits on a stuck owner with no deadline (LIVE_EveryJoinEnds)
  "MC_CommitFence_Green.cfg CommitFence.tla GREEN" # a commit whose caller stopped waiting is seen by every later reader; the count is exact and clears, readers wake
  "MC_CommitFence_Red_RaiseAfterSwap.cfg CommitFence.tla RED" # caller raises after its swap: the count dips and a reader slips past (INV_ReaderSeesCancelledCommit)
  "MC_CommitFence_Red_OwnerNoSwap.cfg CommitFence.tla RED" # owner lowers without its swap: double lowering (INV_NeverNegative)
  "MC_CommitFence_Red_ReaderNoWait.cfg CommitFence.tla RED" # reader skips the fence and misses the landing commit (INV_ReaderSeesCancelledCommit)
  "MC_CommitFence_Red_ForgetFailed.cfg CommitFence.tla RED" # owner forgets a failed commit's fence: it leaks (LIVE_NoFenceLeak)
  "MC_CommitFence_Red_UndoNoWake.cfg CommitFence.tla RED" # caller's undo reaches zero without a wake: a reader sleeps forever (LIVE_ReaderProceeds)
  "MC_CommitFence_Red_SkipBegun.cfg CommitFence.tla RED" # owner abandons a commit that began at hand-off once its caller left (INV_BegunEndRuns)
  "MC_CommitFence_Red_RunAbandoned.cfg CommitFence.tla RED" # owner commits an end whose caller left before it began (INV_AbandonedEndDiscarded)
  "MC_EngineTickers_Green.cfg EngineTickers.tla GREEN" # two reads race the close of one export: each count reaches the total once, the total and the mark never go back, every call ends
  "MC_EngineTickers_Green_TwoSources.cfg EngineTickers.tla GREEN" # one read walks two exports while both close: the same promises across the map
  "MC_EngineTickers_Red_Backward.cfg EngineTickers.tla RED" # loop runs while now != seen: a stale reading swaps the mark back and takes the total back (PROP_TotalNeverBack)
  "MC_EngineTickers_Red_BlindStore.cfg EngineTickers.tla RED" # plain store instead of the compare-and-swap: two publishers add one count twice (INV_NeverOvercount)
  "MC_EngineTickers_Red_GiveUp.cfg EngineTickers.tla RED" # publish gives up after a lost swap: the close leaves its last count unclaimed (INV_ClosedComplete)
  "MC_EngineTickers_Red_StaleSeen.cfg EngineTickers.tla RED" # a lost swap keeps its old seen mark: the close spins forever (LIVE_EveryCallEnds)
  "MC_EngineTickers_Red_RemoveFirst.cfg EngineTickers.tla RED" # the close leaves the map before it publishes: the export is gone with a count unclaimed (INV_LeftClaimed)
  "MC_EngineTickers_Red_WeakClose.cfg EngineTickers.tla RED" # the close reaches the statistics through the weak handle: the store let go first and its last count is lost (INV_ClosedComplete)
  "MC_EngineTickers_Red_Reset.cfg EngineTickers.tla RED" # contract broken, exported statistics reset: a count below the old mark is never added (INV_QuiescentExact)
  "MC_EngineTickers_Red_SharedStats.cfg EngineTickers.tla RED" # contract broken, two exports of one statistics: one count is added twice (INV_NeverOvercount)
  "MC_EngineTickers_Red_CountAfterClose.cfg EngineTickers.tla RED" # contract broken, the engine counts after its guard dropped: that count never reaches the total (INV_QuiescentExact)
  "MC_EngineTickers_Finding_ClaimGap.cfg EngineTickers.tla RED" # finding, no planted bug: a read that claimed but has not added lets a closed export leave the map with its count not yet in the total (INV_LeftInTotals)
  "MC_CpuLedger_Green.cfg CpuLedger.tla GREEN" # placed pools claim CPUs alone all or nothing, or share counted; no two alone on one CPU, counters exact, every start ends
  "MC_CpuLedger_Red_SplitTake.cfg CpuLedger.tla RED" # claim alone looks then adds in two steps: two pools take one CPU alone (INV_AloneTakesDisjoint)
  "MC_CpuLedger_Red_NoRollback.cfg CpuLedger.tla RED" # failed claim returns without giving back what it took (INV_AllOrNothing)
  "MC_CpuLedger_Red_ReleaseLoser.cfg CpuLedger.tla RED" # rollback also gives back the CPU it lost on: a held CPU reads free (INV_HeldNeverFree)
  "MC_CpuLedger_Red_ShareUncounted.cfg CpuLedger.tla RED" # a sharing pool takes no hold: a claim alone wins its CPU (INV_TakenAlone)
  "MC_CpuLedger_Red_NoAttemptCap.cfg CpuLedger.tla RED" # reserve claims alone forever: a start never ends (LIVE_StartEnds)
  "MC_CpuLedger_Red_ReleaseEarly.cfg CpuLedger.tla RED" # caller drops the hold while its threads run: two alone pools on one CPU (INV_AloneRunsAlone)
  "MC_CpuLedger_Red_RunOutsideHold.cfg CpuLedger.tla RED" # caller runs a thread on a CPU its hold does not cover (INV_AloneRunsAlone)
  "MC_CpuLedger_Red_DoubleRelease.cfg CpuLedger.tla RED" # caller gives its hold back twice: a counter drops below its holders (INV_CountExact)
  "MC_CpuLedger_Red_ForgetHold.cfg CpuLedger.tla RED" # caller never gives its hold back: CPUs stay held after every pool is gone (INV_DoneLeavesNothing)
  "MC_DefaultPool_Green.cfg DefaultPool.tla GREEN" # the first registered pool that runs is the default, else one pool started once; never a closing pool, a stop's deadline goes elsewhere, every call returns
  "MC_DefaultPool_Red_NoClosingCheck.cfg DefaultPool.tla RED" # running() keeps only the upgrade: a closing registered pool is handed out (INV_NeverHandsOutClosing)
  "MC_DefaultPool_Red_RegisterOverwrites.cfg DefaultPool.tla RED" # make_default swaps without looking: a running default is replaced (INV_FirstRunningWins)
  "MC_DefaultPool_Red_BlindRegister.cfg DefaultPool.tla RED" # make_default stores instead of its compare-and-swap: a lost update replaces a running default (INV_FirstRunningWins)
  "MC_DefaultPool_Red_AnswerYes.cfg DefaultPool.tla RED" # make_default answers yes without Arc::ptr_eq: yes while another pool is the default (INV_MakeDefaultTellsTruth)
  "MC_DefaultPool_Red_NoRetry.cfg DefaultPool.tla RED" # make_default swaps once with no loop: no although no running pool holds the slot (INV_MakeDefaultTellsTruth)
  "MC_DefaultPool_Red_StartedFirst.cfg DefaultPool.tla RED" # get reads the started cell first: a running registered pool is passed over (INV_RegisteredWins)
  "MC_DefaultPool_Red_StrongSlot.cfg DefaultPool.tla RED" # the slot holds an Arc: a pool its owner let go stays alive (INV_RegistrationKeepsNothingAlive)
  "MC_DefaultPool_Red_WorkerWaits.cfg DefaultPool.tla RED" # block_on does not refuse a worker: the worker parks waiting for the turn (INV_WorkerNeverWaits)
  "MC_DefaultPool_Red_WorkerRefusedFirst.cfg DefaultPool.tla RED" # a worker is refused before the cell is read: an error while a started pool runs (INV_ErrorHonest)
  "MC_DefaultPool_Red_NoTurn.cfg DefaultPool.tla RED" # no initializer turn: two default pools are started (INV_OneStart)
  "MC_DefaultPool_Red_NoRecheck.cfg DefaultPool.tla RED" # no look after the turn: a waiter starts a second pool after the publish (INV_OneStart)
  "MC_DefaultPool_Red_FailKeepsTurn.cfg DefaultPool.tla RED" # a failed start keeps the turn: the next caller waits forever (LIVE_EveryCallReturns)
  "MC_DefaultPool_Red_RacyCell.cfg DefaultPool.tla RED" # a check-then-store cell: two calls get two started pools (INV_OneDefault)
  "MC_DefaultPool_Red_StopAsksFirst.cfg DefaultPool.tla RED" # contract C1 broken, stop asks before it shuts down: its deadline lands on its own pool (INV_StopDeadlineElsewhere)
  "MC_DefaultPool_Red_ShutStarted.cfg DefaultPool.tla RED" # contract C2 broken, a caller shuts the started pool down: it is still handed out (INV_NeverHandsOutClosing)
  "MC_DefaultPool_Red_RegisterClosing.cfg DefaultPool.tla RED" # contract C3 broken, a closing pool registers: make_default says yes and is not the default (INV_MakeDefaultTellsTruth)
  "MC_PushBacklog_Green.cfg MC_PushBacklog_Common.tla GREEN" # #1099: bounded queue + fixed workers + per-peer cap -> all bounds hold, healthy peers progress
  "MC_PushBacklog_Red_SpawnPerItem.cfg MC_PushBacklog_Common.tla RED" # current main: task spawned per (write,peer) before the semaphore -> resident work unbounded (INV_QueueBounded)
  "MC_PushBacklog_Red_PermitLeak.cfg MC_PushBacklog_Common.tla RED" # completing job keeps its worker slot -> pool decays (INV_PermitConservation)
  "MC_PushBacklog_Red_RetainHandles.cfg MC_PushBacklog_Common.tla RED" # SyncShutdownHandle retains every completed JoinHandle (INV_HandlesBounded)
  "MC_PushBacklog_Red_NoPeerCap.cfg MC_PushBacklog_Common.tla RED" # no per-peer cap: slow peer's stuck sends hold every worker -> healthy peers starve (LIVE_HealthyProgress)
  "MC_PushCoalescing_Green.cfg MC_PushCoalescing_Common.tla GREEN" # #1102: latest head retained while queued/persisted predecessors retire
  "MC_PushCoalescing_Red_AppendEvery.cfg MC_PushCoalescing_Common.tla RED" # no live coalescing -> two heads occupy one (doc,peer) queue
  "MC_PushCoalescing_Red_StaleRetry.cfg MC_PushCoalescing_Common.tla RED" # failed superseded active head re-enters persisted retry
  "MC_PendingDagRestart_Green.cfg MC_PendingDagRestart_Common.tla GREEN" # #1099: pending-DAG registrations persisted + restored after crash -> acks stay backed, all docs merge
  "MC_PendingDagRestart_Red_ProcessLocal.cfg MC_PendingDagRestart_Common.tla RED" # process-local registrations: hub crash after success-ack -> silent permanent loss (INV_AckBacked)
  "MC_SyncOwnership_Green.cfg SyncOwnership.tla GREEN" # #1116 stage 3: one head hint transfers durable completion ownership to the receiver
  "MC_SyncOwnership_Green_IrohOrigin.cfg SyncOwnership.tla GREEN" # Iroh binds durable recovery to the signed, routable origin that owns the linked DAG
  "MC_SyncOwnership_Green_Liveness_IrohRelay.cfg SyncOwnership.tla GREEN" # unreachable signed origin remains bound while a durable qualified same-root provider completes recovery
  "MC_SyncOwnership_Green_Liveness_Doc.cfg SyncOwnership.tla GREEN" # document supersession eventually reaches currency and quiescence
  "MC_SyncOwnership_Green_Liveness_Collection.cfg SyncOwnership.tla GREEN" # collection supersession eventually reaches currency and quiescence
  "MC_SyncOwnership_Red_DocOnlyMarkers.cfg SyncOwnership.tla RED" # no collection marker loses a dropped collection-head obligation
  "MC_SyncOwnership_Red_PayloadLedger.cfg SyncOwnership.tla RED" # current main stores CID/version delivery state instead of scope markers
  "MC_SyncOwnership_Red_VolatileRegistration.cfg SyncOwnership.tla RED" # success ack followed by restart loses a volatile receiver obligation
  "MC_SyncOwnership_Red_DuplicateFetch.cfg SyncOwnership.tla RED" # alternate triggers claim two fetch owners for one root
  "MC_SyncOwnership_Red_StaleAckClears.cfg SyncOwnership.tla RED" # stale ack clears the marker for a newer current head
  "MC_SyncOwnership_Red_VolatileServeAuthority.cfg SyncOwnership.tla RED" # success ack survives receiver restart but sender restart loses CAR-serving authority
  "MC_SyncOwnership_Red_RelayOnlyProvider.cfg SyncOwnership.tla RED" # an unverified payload relay is not an authenticated recovery provider
  "MC_SyncOwnership_Red_ProviderRebind.cfg SyncOwnership.tla RED" # same-root replay cannot overwrite the durably bound recovery origin
  "MC_SyncOwnership_Red_UnroutableOrigin.cfg SyncOwnership.tla RED" # publisher identity without a direct-or-relayed CAR route cannot discharge ownership
  "MC_SyncOwnership_Red_UnsignedIrohOrigin.cfg SyncOwnership.tla RED" # unsigned Iroh payload SourcePeerID is not an authenticated recovery provider
  "MC_SyncOwnership_Red_RootOnlyHop.cfg SyncOwnership.tla RED" # authenticated gossip hop owns only the root, not the linked DAG promised by the hint
  "MC_SyncOwnership_Red_UnauthorizedOrigin.cfg SyncOwnership.tla RED" # a complete authenticated origin that cannot authorize the requester still cannot discharge ownership
  "MC_SyncOwnership_Red_CancelOnProgress.cfg SyncOwnership.tla RED" # first arriving CAR block truncates the receiver-owned response before DAG completion
  "MC_SyncOwnership_Red_RecursiveFirst.cfg SyncOwnership.tla RED" # known missing frontier is delayed behind a recursive full-DAG CAR
  "MC_SyncOwnership_Red_EveryRoot.cfg SyncOwnership.tla RED" # successive heads from one sender/scope accumulate obsolete durable roots
  "MC_SyncOwnership_Red_ParallelMerge.cfg SyncOwnership.tla RED" # frontend-selected parallel writers violate the receiver's single merge-owner boundary
  "MC_SyncOwnership_Red_DuplicateTerminal.cfg SyncOwnership.tla RED" # concurrent same-root terminal cleanup violates the single durable metadata writer
  "MC_SyncOwnership_Red_EdgeTriggeredCompletion.cfg SyncOwnership.tla RED" # a fast transport failure races ahead of waiter registration and strands the fetch owner
  "MC_SyncOwnership_Red_WorkerSaturatedCompletion.cfg SyncOwnership.tla RED" # saturated spawned workers stop draining the CAR completion needed by the fetch owner
  "MC_SyncOwnership_Red_SharedServeWorkers.cfg SyncOwnership.tla RED" # ownership admission consumes every worker needed to serve receiver-owned CAR recovery
  "MC_SyncOwnership_Red_EagerIdentityLookup.cfg SyncOwnership.tla RED" # durable replicator authority is delayed behind an unnecessary reverse identity challenge
  "MC_SyncOwnership_Red_BlockingHostEvent.cfg SyncOwnership.tla RED" # full event delivery blocks the host command needed to reply and release admitted work
  "MC_SyncOwnership_Red_BusyExhaustion.cfg SyncOwnership.tla RED" # local shared-CID ingest contention is deferred, never counted as terminal provider exhaustion
  "MC_Jwt_Green.cfg                MC_Jwt_Green.tla                GREEN" # token->DID binds genuine signer DID
  "MC_Jwt_Red_NoAlgBinding.cfg     MC_Jwt_Red_NoAlgBinding.tla     RED"   # alg-confusion: header alg not bound to key type
  "MC_Jwt_Red_NoIssBinding.cfg     MC_Jwt_Red_NoIssBinding.tla     RED"   # iss not bound to did(pubkey)
  "MC_Jwt_Red_NoSig.cfg            MC_Jwt_Red_NoSig.tla            RED"   # signature not verified -> forged token
  "MC_DeferredAcp_Green.cfg        MC_DeferredAcp_Green.tla        GREEN" # txn-local ACP projection gates as committed state would
  "MC_DeferredAcp_Red_OwnerBypass.cfg MC_DeferredAcp_Red_OwnerBypass.tla RED" # projection grants what committed state denies
  "MC_DeferredAcp_Red_RollbackHooks.cfg MC_DeferredAcp_Red_RollbackHooks.tla RED" # rollback leaves hooks applied (not a no-op)
  "MC_DeferredAcp_Red_SharedOverlay.cfg MC_DeferredAcp_Red_SharedOverlay.tla RED" # one txn observes another's uncommitted projection
  "MC_PendingDagQuarantine_Green.cfg PendingDagQuarantine.tla GREEN" # #1128: deterministic rejection quarantines durably; sound docs merge, poison docs quarantine
  "MC_PendingDagQuarantine_Red_RetryForever.cfg PendingDagQuarantine.tla RED" # pre-#1128 wedge, still live for any unclassified Rejected producer: Rejected treated as retryable skip -> poison root swept forever (LIVE_PoisonQuiesces)
  "MC_PendingDagQuarantine_Red_OvereagerQuarantine.cfg PendingDagQuarantine.tla RED" # forbidden overcorrection: sound doc's transient failure also quarantines it (LIVE_SoundEventuallyMerged)
  "MC_VectorLinks_Green.cfg VectorLinks.tla GREEN"
  "MC_VectorLinks_Red_Puts.cfg VectorLinks.tla RED"
  "MC_VectorLinks_Red_Prune.cfg VectorLinks.tla RED"
  "MC_HeadSet_Green.cfg            HeadSet.tla                  GREEN" # derived heads: writers only write keys naming themselves, so concurrent appends never conflict and siblings form
  "MC_HeadSet_Red_EagerDelete.cfg  HeadSet.tla                  RED"   # eager delete: both writers delete the same observed head key, so regolith refuses one (INV_NoWriteConflict)
  "MC_HeadSet_Red_MarkersOnly.cfg  HeadSet.tla                  RED"   # reclamation that drops a head's markers but keeps its head key resurrects a superseded head (INV_HeadsExact)
  "MC_HeadSet_Red_PerKeyScan.cfg   HeadSet.tla                  RED"   # a head scan validated per key: a sweep reclaiming a key the scan yielded aborts an in-flight append (INV_NoWriteConflict)
  "MC_HeadPublication_Green.cfg              HeadPublication.tla GREEN" # the priority index entry records a publication, read validated; a parent held ahead is read by its part of the set
  "MC_HeadPublication_Red_NoRecord.cfg       HeadPublication.tla RED"   # nothing records a publication: a replay of a superseded block makes it a head again (INV_LiveExact)
  "MC_HeadPublication_Red_OwnUnvalidated.cfg HeadPublication.tla RED"   # the record read unchecked: a duplicate publication undoes a child's supersede (INV_LiveExact)
  "MC_HeadPublication_Red_NoPartRead.cfg     HeadPublication.tla RED"   # a parent held ahead without a part read stays ahead after it is published (INV_AheadExact)
  "MC_HeadPublication_Red_WholeSet.cfg       HeadPublication.tla RED"   # touches that says yes to every part: an unrelated publication turns the reader away (INV_NoUnrelatedAbort)
  "MC_InboundAdmission_Green.cfg MC_InboundAdmission_Common.tla GREEN" # #2076: bodies stay within their class budget, a healthy control message is handled, no wait is endless
  "MC_InboundAdmission_Red_SharedBudget.cfg MC_InboundAdmission_Common.tla RED" # one budget for all classes: stuck bulk bodies starve control (LIVE_HealthyHandled)
  "MC_InboundAdmission_Red_SharedDrain.cfg MC_InboundAdmission_Common.tla RED" # current main: one queue, a blocked completion head starves control (LIVE_HealthyHandled)
  "MC_InboundAdmission_Red_NoShare.cfg MC_InboundAdmission_Common.tla RED" # no per-peer share: one peer holds a whole class (INV_ShareBound)
  "MC_InboundAdmission_Red_PerConnection.cfg MC_InboundAdmission_Common.tla RED" # a share per connection: a peer with two connections holds a whole class (INV_ShareBound)
  "MC_InboundAdmission_Red_ReleaseOnQueue.cfg MC_InboundAdmission_Common.tla RED" # lease dropped on enqueue: memory exceeds the budget (INV_ResidentWithinBudget)
  "MC_InboundAdmission_Red_ReadFirst.cfg MC_InboundAdmission_Common.tla RED" # current main: body read before admission (INV_ResidentWithinBudget)
  "MC_InboundAdmission_Red_NoDeadline.cfg MC_InboundAdmission_Common.tla RED" # no deadline: a stream waits for budget forever (LIVE_WaitEnds)
  "MC_InboundAdmission_Red_NoReadDeadline.cfg MC_InboundAdmission_Common.tla RED" # no read deadline: a stalled body holds control forever (LIVE_HealthyHandled)
  "MC_InboundAdmission_Red_IdleOnly.cfg MC_InboundAdmission_Common.tla RED" # idle deadline only: a trickling body holds control forever (LIVE_HealthyHandled)
  "MC_GossipOwnership_Green.cfg MC_GossipOwnership_Common.tla GREEN" # #2078: gossip rides the mux connection; a revoked peer has no gossip, gossip never closes a shared connection
  "MC_GossipOwnership_Red_OwnConnection.cfg MC_GossipOwnership_Common.tla RED" # current main: gossip dials its own connection and re-reaches a revoked peer (INV_RevokedHasNoGossip)
  "MC_GossipOwnership_Red_NoRecheck.cfg MC_GossipOwnership_Common.tla RED" # no admission check after connecting: a revoke during the dial is missed (INV_RevokedHasNoGossip)
  "MC_GossipOwnership_Red_GossipCloses.cfg MC_GossipOwnership_Common.tla RED" # forgetting a peer closes the shared connection (INV_GossipNeverClosesShared)
  "MC_GossipOwnership_Red_ForgetForever.cfg MC_GossipOwnership_Common.tla RED" # gossip may drop the healthy peer forever: it is never reached (LIVE_HealthyReached)
  "MC_GossipOwnership_Green_Legacy.cfg MC_GossipOwnership_Common.tla GREEN" # a dial that lands on the older mux answers gossip's ask as a failure
  "MC_GossipOwnership_Red_SilentLegacy.cfg MC_GossipOwnership_Common.tla RED" # an older-mux connection is dropped silently: gossip waits on the peer forever (LIVE_AskAnswered)
  "MC_GossipInbox_Green.cfg GossipInbox.tla GREEN" # control input never waits and received gossip is dropped when full: every endpoint request is answered
  "MC_GossipInbox_Red_Shared.cfg GossipInbox.tla RED" # one bounded inbox: peers fill it while the driver waits on events, the endpoint loop stalls (LIVE_RequestAnswered)
  "MC_GossipInbox_Red_LossyShared.cfg GossipInbox.tla RED" # received dropped when full but control waits in the same queue: same stall (LIVE_RequestAnswered)
  "MC_ServiceRecords_Green.cfg ServiceRecords.tla GREEN" # #2080: a record replaces only a lower seq, an expired record is never used, a stale pointer triggers a query
  "MC_ServiceRecords_Red_AnySeq.cfg ServiceRecords.tla RED" # the last record in wins: a late older record replaces a newer one (INV_SeqNeverGoesBack)
  "MC_ServiceRecords_Red_NoExpiryCheck.cfg ServiceRecords.tla RED" # peers picked from an expired record (INV_NoExpiredChosen)
  "MC_ServiceRecords_Red_GossipOnly.cfg ServiceRecords.tla RED" # no query on a stale pointer: lost gossip leaves an old record forever (LIVE_NewestReached)
  "MC_ServiceRecords_Red_QueryOnConnect.cfg ServiceRecords.tla RED" # pointer looked up only at connect: gossip lost later leaves an old record forever (LIVE_NewestReached)
  "MC_SolicitedReplies_Green.cfg SolicitedReplies.tla GREEN" # #2081: a reply stream is admitted only while a reply is owed, and an owed one is never refused
  "MC_SolicitedReplies_Red_NoMatch.cfg SolicitedReplies.tla RED" # every reply stream read, as before: one nobody asked for is admitted (INV_NoUnsolicitedAdmitted)
  "MC_SolicitedReplies_Red_Keep.cfg SolicitedReplies.tla RED" # admitting does not use up the expectation: a second reply is admitted (INV_NoUnsolicitedAdmitted)
  "MC_SolicitedReplies_Red_Carry.cfg SolicitedReplies.tla RED" # an expired entry's leftovers carry into the next window (INV_NoUnsolicitedAdmitted)
  "MC_SolicitedReplies_Red_PruneLive.cfg SolicitedReplies.tla RED" # prune removes a live entry: the owed reply is refused (INV_SolicitedAdmitted)
  "MC_SolicitedReplies_Red_PruneRace.cfg SolicitedReplies.tla RED" # prune between an expect's read and write loses it (INV_SolicitedAdmitted)
  "MC_SolicitedReplies_Red_SendFirst.cfg SolicitedReplies.tla RED" # send before expect: a fast reply is refused (INV_SolicitedAdmitted)
  "MC_SolicitedReplies_GreenV2.cfg SolicitedReplies.tla GREEN" # mux 0.2: replies on the request stream, every reply stream refused
  "MC_SolicitedReplies_Red_GateOnV2.cfg SolicitedReplies.tla RED" # 0.2 still gates reply streams: a forged second reply is admitted (INV_NoReplyStreamOnV2)
  "MC_SolicitedReplies_Red_NewStreamOnV2.cfg SolicitedReplies.tla RED" # the peer answers a 0.2 request on a new stream: the reply is refused (INV_SolicitedAdmitted)
  # ---- GovernanceContract.tla: what a merge validator may assume; GovernanceMerge.tla: the merge path checked to refine it ----
  "MC_GovernanceContract_Green.cfg            MC_GovernanceContract_Orphan.tla       GREEN" # re-drive + sweep: every settleable composite settles
  "MC_GovernanceContract_Green_Nameable.cfg   MC_GovernanceContract_Nameable.tla     GREEN" # nameable awaits: arrival re-drive alone settles them, no sweep
  "MC_GovernanceContract_Green_FieldAwait.cfg MC_GovernanceContract_Orphan.tla       GREEN" # immutable-field keys settle the orphan, no sweep
  "MC_GovernanceContract_Green_Recovery.cfg   MC_GovernanceContract_Nameable.tla     GREEN" # restart followed by a pass over the unmerged composites
  "MC_GovernanceContract_Green_IndexFull.cfg  MC_GovernanceContract_TwoDeferred.tla  GREEN" # index at capacity: the sweep covers the unindexed defer
  "MC_GovernanceContract_Green_Withheld.cfg   MC_GovernanceContract_Withheld.tla     GREEN" # a withheld input defers its composite forever and rejects nothing
  "MC_GovernanceContract_Red_NoRetry.cfg      MC_GovernanceContract_Orphan.tla       RED"   # a Defer naming nothing strands without the sweep (L1 L2 L3)
  "MC_GovernanceContract_Red_NoDrain.cfg      MC_GovernanceContract_NoDrain.tla      RED"   # drained re-drive budget, no backstop: waiter never settles (L1 L2 L3)
  "MC_GovernanceContract_Red_NoRecovery.cfg   MC_GovernanceContract_Nameable.tla     RED"   # restart loses the defer index and nothing re-drives (L1 L2)
  "MC_GovernanceContract_Red_IndexFull.cfg    MC_GovernanceContract_TwoDeferred.tla  RED"   # index at capacity with no sweep: unindexed defer stranded (L1 L2)
  "MC_GovernanceContract_Red_Absence.cfg      MC_GovernanceContract_Nameable.tla     RED"   # purity violated, reject on absence -> replicas split (INV_NoSplit)
  "MC_GovernanceContract_Green_GC.cfg         MC_GovernanceContract_Nameable.tla     GREEN" # disposition with the floor: forgetting strands nothing
  "MC_GovernanceContract_Red_GCNoFloor.cfg    MC_GovernanceContract_Nameable.tla     RED"   # disposition below the floor: a settleable composite is stranded (L1 L2 L3)
  "MC_GovernanceMerge_Today.cfg               MC_GovernanceMerge_Orphan.tla          GREEN" # the merge path as coded refines the contract: field keys, unmerged-scoped sweep, in-memory index
  "MC_GovernanceMerge_Green_CidOnly.cfg       MC_GovernanceMerge_Orphan.tla          GREEN" # an unmerged-scoped sweep suffices alone: no field keys, nothing durable
  "MC_GovernanceMerge_Green_Augmented.cfg     MC_GovernanceMerge_Orphan.tla          GREEN" # field keys + a persisted index would also refine, without the sweep
  "MC_GovernanceMerge_Red_NoSweep.cfg         MC_GovernanceMerge_Orphan.tla          RED"   # the tree before sweep.rs: index-only re-drive, in-memory index -> orphan never merges
  "MC_GovernanceMerge_Red_SweepIndexed.cfg    MC_GovernanceMerge_Orphan.tla          RED"   # the refactor to refuse: a sweep over the index never sees an unindexed composite
  "MC_GovernanceMerge_Red_Mutant.cfg          MC_GovernanceMerge_Mutant.tla          RED"   # teeth check: a silent merge must break the refinement
  "MC_Arrivals_Green.cfg             Arrivals.tla GREEN" # commit-sequence stamps: no skip, commit order, no disjoint conflict, one commit per write
  "MC_Arrivals_Red_Counter.cfg       Arrivals.tla RED"   # head key in the document txn: disjoint creates conflict (INV_NoDisjointConflict)
  "MC_Arrivals_Red_Detached.cfg      Arrivals.tla RED"   # #1897 post-commit numbering: second durable commit (INV_OneCommitPerWrite)
  "MC_Arrivals_Red_DetachedOrder.cfg Arrivals.tla RED"   # #1897 short-ID numbering leaves commit order (INV_CommitOrder)
  "MC_Arrivals_Red_StampLatest.cfg   Arrivals.tla RED"   # bound by latest allocated seq: unapplied commit skipped (INV_NoSkip)
  "MC_NodeShutdown_Green.cfg NodeShutdown.tla GREEN" # held tasks stop and are awaited before regolith closes; shutdown returns
  "MC_NodeShutdown_Red_RedriveDetached.cfg NodeShutdown.tla RED" # detached ACP redrive outlives the wait for tasks
  "MC_NodeShutdown_Red_ServicesNotAwaited.cfg NodeShutdown.tla RED" # stopped service still in storage when regolith closes
  "MC_NodeShutdown_Red_NoDrainDeadline.cfg NodeShutdown.tla RED" # hung request keeps shutdown waiting forever
  "MC_NodeShutdown_Red_FlushOnCaller.cfg NodeShutdown.tla RED" # refused flush runs the exporter join on the caller
)

fails=0; n=0
for row in "${RUNS[@]}"; do
  read -r cfg mod want <<<"$row"
  n=$((n+1))
  out=$(./tools/tlc -metadir "states/run$n" -config "$cfg" "$mod" 2>&1)
  # Here-strings, not pipes: `grep -q` stops at the first match, and under
  # pipefail the writer's broken pipe would turn a long trace into ERROR.
  if grep -q "No error has been found" <<<"$out"; then got=GREEN
  elif grep -qE "is violated|was violated|properties were violated|Deadlock reached|Deadlock" <<<"$out"; then got=RED
  else got=ERROR; fi
  rm -rf "states/run$n"
  if [ "$got" = "$want" ]; then printf "  ok   %-6s %-34s %s\n" "$got" "$cfg" "$mod"
  else printf "  FAIL want=%s got=%s  %-30s %s\n" "$want" "$got" "$cfg" "$mod"; fails=$((fails+1)); fi
done

echo "----"
if [ "$fails" -eq 0 ]; then echo "all $n TLA+ runs matched their expected verdict"; else echo "$fails/$n MISMATCHED"; fi
exit "$fails"
