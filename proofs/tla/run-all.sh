#!/usr/bin/env bash
# Run every TLA+ model and check its verdict against the expected red/green oracle.
# Each invocation gets private TLC state and output directories. Exits non-zero
# if any verdict does not match, including a worker that fails to run TLC.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# rows: <cfg> <module> <GREEN|RED|TRACE:<depth>|DEADLOCK:<l>>
# A trace replay walks one path, so its verdict also says where it ends:
# TRACE:<depth> finds no error from exactly one initial state and walks every
# event to the settled rest (depth = events + 1), and DEADLOCK:<l> stops with
# event l next (past the end when l = events + 1).
RUNS=(
  "MC_DocumentUpdateJournal_Green.cfg DocumentUpdateJournal.tla GREEN"
  "MC_DocumentUpdateJournal_Green_Outside.cfg DocumentUpdateJournal.tla GREEN"
  "MC_DocumentUpdateJournal_Red_Detached.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Duplicate.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Superseded.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Authorization.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Deleted.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Stale.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Unordered.cfg DocumentUpdateJournal.tla RED"
  "MC_DocumentUpdateJournal_Red_Skip.cfg DocumentUpdateJournal.tla RED"
  "MC_UsersetRelationManager_Green.cfg UsersetRelationManager.tla GREEN"
  "MC_UsersetRelationManager_Red_Owner.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_Direct.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_Wildcard.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_Node.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_NodeOwner.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_NodeAdmin.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_Target.cfg UsersetRelationManager.tla RED"
  "MC_UsersetRelationManager_Red_Conflict.cfg UsersetRelationManager.tla RED"
  "MC_ProjectionAccessNotification_Green.cfg ProjectionAccessNotification.tla GREEN"
  "MC_ProjectionAccessNotification_Red_Grant.cfg ProjectionAccessNotification.tla RED"
  "MC_ProjectionAccessNotification_Red_Revoke.cfg ProjectionAccessNotification.tla RED"
  "MC_ProjectionAccessNotification_Red_Abort.cfg ProjectionAccessNotification.tla RED"
  "MC_ProjectionAccessNotification_Red_Noop.cfg ProjectionAccessNotification.tla RED"
  "MC_HostMutationProjection_Green.cfg HostMutationProjection.tla GREEN"
  "MC_HostMutationProjection_Red_Atomic.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Raw.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Replicated.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Schema.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Direct.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Other.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Authority.cfg HostMutationProjection.tla RED"
  "MC_HostMutationProjection_Red_Owner.cfg HostMutationProjection.tla RED"
  "MC_ProjectorMutationOrigin_Green.cfg ProjectorMutationOrigin.tla GREEN"
  "MC_ProjectorMutationOrigin_Red_Actor.cfg ProjectorMutationOrigin.tla RED"
  "MC_ProjectorMutationOrigin_Red_Revocation.cfg ProjectorMutationOrigin.tla RED"
  "MC_ProjectorMutationOrigin_Red_Atomic.cfg ProjectorMutationOrigin.tla RED"
  "MC_HostSourceNoop_Green.cfg HostSourceNoop.tla GREEN"
  "MC_HostSourceNoop_Red_Authority.cfg HostSourceNoop.tla RED"
  "MC_HostSourceNoop_Red_Validation.cfg HostSourceNoop.tla RED"
  "MC_HostNoopProjection_Green.cfg HostNoopProjection.tla GREEN"
  "MC_HostNoopProjection_Red_Skipped.cfg HostNoopProjection.tla RED"
  "MC_HostNoopProjection_Red_Authority.cfg HostNoopProjection.tla RED"
  "MC_HostNoopProjection_Red_Rollback.cfg HostNoopProjection.tla RED"
  "MC_LocalFetchPermit_Green.cfg LocalFetchPermit.tla GREEN"
  "MC_LocalFetchPermit_Red_Hold.cfg LocalFetchPermit.tla RED"
  "MC_HeldPubsubRequest_Green.cfg HeldPubsubRequest.tla GREEN"
  "MC_HeldPubsubRequest_Red_NoHold.cfg HeldPubsubRequest.tla RED" # a refused publish fails the sync at once (Delivered)
  "MC_HeldPubsubRequest_Red_NoResend.cfg HeldPubsubRequest.tla RED" # a later subscriber is never sent the request (Delivered)
  "MC_HeldPubsubRequest_Red_LateWatch.cfg HeldPubsubRequest.tla RED" # the wake-up is armed after the publish (Delivered)
  "MC_HeldPubsubRequest_Red_NoDedupe.cfg HeldPubsubRequest.tla RED" # a resend's duplicate reply reaches the caller (OneReplyEach)
  "MC_HeldPubsubRequest_Red_ResendLate.cfg HeldPubsubRequest.tla RED" # a resend goes out after the deadline (NoResendAfterDeadline)
  "MC_VerifiedClientGraph_Green.cfg VerifiedClientGraph.tla GREEN"
  "MC_VerifiedClientGraph_Red_SingleMutation.cfg VerifiedClientGraph.tla RED" # a multi-mutation request is accepted (INV_SingleMutation)
  "MC_VerifiedClientGraph_Red_Unencrypted.cfg VerifiedClientGraph.tla RED" # the request adds encryption outside the approved graph (INV_Unencrypted)
  "MC_VerifiedClientGraph_Red_Input.cfg VerifiedClientGraph.tla RED" # a different input is accepted (INV_Input)
  "MC_VerifiedClientGraph_Red_Selection.cfg VerifiedClientGraph.tla RED" # the physical document ID is not selected correctly (INV_Selection)
  "MC_VerifiedClientGraph_Red_Confirmation.cfg VerifiedClientGraph.tla RED" # an unconfirmed response is accepted (INV_Confirmation)
  "MC_VerifiedClientGraph_Red_Writable.cfg VerifiedClientGraph.tla RED" # a read-only transaction publishes a mutation (INV_Writable)
  "MC_VerifiedClientGraph_Red_Canonical.cfg VerifiedClientGraph.tla RED" # typed operation overrides the checked GraphQL (INV_Canonical)
  "MC_VerifiedClientGraph_Red_Actor.cfg VerifiedClientGraph.tla RED" # effect actor differs from the signer (INV_Actor)
  "MC_VerifiedClientGraph_Red_Signature.cfg VerifiedClientGraph.tla RED" # an invalid signature is accepted (INV_Signature)
  "MC_VerifiedClientGraph_Red_Target.cfg VerifiedClientGraph.tla RED" # a different physical target is accepted (INV_Target)
  "MC_VerifiedClientGraph_Red_Heads.cfg VerifiedClientGraph.tla RED" # a stale update or delete graph is accepted (INV_Heads)
  "MC_VerifiedClientGraph_Red_HeadConflict.cfg VerifiedClientGraph.tla RED" # a concurrent head change is not validated (INV_HeadConflict)
  "MC_VerifiedClientGraph_Red_Wave.cfg VerifiedClientGraph.tla RED" # the execution reports success without all signed blocks (INV_Wave)
  "MC_VerifiedClientGraph_Red_Schema.cfg VerifiedClientGraph.tla RED" # an outside schema supplies the decision (INV_Schema)
  "MC_VerifiedClientGraph_Red_Bypass.cfg VerifiedClientGraph.tla RED" # the request carries DAC bypass (INV_Bypass)
  "MC_VerifiedClientGraph_Red_Owner.cfg VerifiedClientGraph.tla RED" # the Node does not own the transaction (INV_Owner)
  "MC_VerifiedClientGraph_Red_Nac.cfg VerifiedClientGraph.tla RED" # the active NAC permission is ignored (INV_Nac)
  "MC_VerifiedClientGraph_Red_Dac.cfg VerifiedClientGraph.tla RED" # the active DAC permission is ignored (INV_Dac)
  "MC_VerifiedClientGraph_Red_NacBound.cfg VerifiedClientGraph.tla RED" # active NAC is not transaction bound (INV_NacBound)
  "MC_VerifiedClientGraph_Red_DacBound.cfg VerifiedClientGraph.tla RED" # active DAC is not transaction bound (INV_DacBound)
  "MC_VerifiedClientGraph_Red_RevokeNac.cfg VerifiedClientGraph.tla RED" # NAC revocation is not validated at commit (INV_NacRevocation)
  "MC_VerifiedClientGraph_Red_RevokeDac.cfg VerifiedClientGraph.tla RED" # DAC revocation is not validated at commit (INV_DacRevocation)
  "MC_VerifiedClientGraph_Red_Rollback.cfg VerifiedClientGraph.tla RED" # a rejected graph publishes the Agent effect (INV_Atomic)
  "MC_SourceScopedNac_Green.cfg SourceScopedNac.tla GREEN"
  "MC_SourceScopedNac_Red_Authority.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Owner.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Readonly.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Store.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Expired.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_InvalidSource.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Autocommit.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Conflict.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Sources.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Membership.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Unchanged.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_NoReplay.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_Partial.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_BorrowNode.cfg SourceScopedNac.tla RED"
  "MC_SourceScopedNac_Red_UntrustedReplay.cfg SourceScopedNac.tla RED"
  "MC_EmbeddedSigner_Green.cfg EmbeddedSigner.tla GREEN"
  "MC_EmbeddedSigner_Red_Actor.cfg EmbeddedSigner.tla RED"
  "MC_EmbeddedSigner_Red_Match.cfg EmbeddedSigner.tla RED"
  "MC_EmbeddedSigner_Red_Key.cfg EmbeddedSigner.tla RED"
  "MC_EmbeddedSigner_Red_Owner.cfg EmbeddedSigner.tla RED"
  "MC_EmbeddedSigner_Red_Acp.cfg EmbeddedSigner.tla RED"
  "MC_EmbeddedSigner_Red_Bypass.cfg EmbeddedSigner.tla RED"
  "MC_SigningAdmission_Green.cfg SigningAdmission.tla GREEN" # bounded connections release expired slots
  "MC_SigningAdmission_Red_Capacity.cfg SigningAdmission.tla RED" # admission without a cap (INV_Capacity)
  "MC_SigningAdmission_Red_Cleanup.cfg SigningAdmission.tla RED" # expiration leaks a slot (LIVE_Reaped)
  "MC_ClientSignedCreate_ExpiryFinality.cfg ClientSignedCreate.tla RED" # an already-started commit may finish after token expiry (INV_NoCompletionAfterExpiry)
  "MC_ClientSignedCreate_ReplayBoundary.cfg ClientSignedCreate.tla RED" # ordinary signatures may be reused across sessions (INV_NoReplay)
  "MC_ClientSignedCreate_Green.cfg ClientSignedCreate.tla GREEN" # the bounded conversation preserves every guard and reaps ended sessions
  "MC_ClientSignedCreate_Red_Actor.cfg ClientSignedCreate.tla RED" # another caller answers the session (INV_Actor)
  "MC_ClientSignedCreate_Red_Context.cfg ClientSignedCreate.tla RED" # changed context reuses an answer (INV_Context)
  "MC_ClientSignedCreate_Red_Signature.cfg ClientSignedCreate.tla RED" # an unverified signature enters the memo (INV_Signature)
  "MC_ClientSignedCreate_Red_Client.cfg ClientSignedCreate.tla RED" # the signer fails to validate the wave (INV_Client)
  "MC_ClientSignedCreate_Red_Live.cfg ClientSignedCreate.tla RED" # an ended session consumes an answer (INV_Live)
  "MC_ClientSignedCreate_Red_Acp.cfg ClientSignedCreate.tla RED" # a revoked actor commits (INV_Acp)
  "MC_ClientSignedCreate_Red_Inside.cfg ClientSignedCreate.tla RED" # the client is awaited inside an attempt (INV_Outside)
  "MC_ClientSignedCreate_Red_Reclaim.cfg ClientSignedCreate.tla RED" # expiry leaves its capsule allocated forever (LIVE_Reaped)
  "MC_ClientSignedUpdate_Green.cfg ClientSignedUpdate.tla GREEN" # fixed intent, relevant field and transactional authorization
  "MC_ClientSignedUpdate_Disjoint.cfg ClientSignedUpdate.tla RED" # a disjoint write commits beside the signed update
  "MC_ClientSignedUpdate_Red_CheckIntent.cfg ClientSignedUpdate.tla RED" # a wrong graph receives a signature
  "MC_ClientSignedUpdate_Red_CheckLive.cfg ClientSignedUpdate.tla RED" # a cancelled conversation receives a signature
  "MC_ClientSignedUpdate_Red_ValidateField.cfg ClientSignedUpdate.tla RED" # a changed written field commits unseen
  "MC_ClientSignedUpdate_Red_ValidateAcp.cfg ClientSignedUpdate.tla RED" # a revoked actor commits unseen
  "MC_SseDelivery_Green.cfg SseDelivery.tla GREEN"
  "MC_SseDelivery_Red_Overflow.cfg SseDelivery.tla RED"
  "MC_SseDelivery_Red_Error.cfg SseDelivery.tla RED"
  "MC_SseDelivery_Red_Wakeup.cfg SseDelivery.tla RED"
  "MC_HttpRouterComposition_Green.cfg HttpRouterComposition.tla GREEN"
  "MC_HttpRouterComposition_Red_Early.cfg HttpRouterComposition.tla RED"
  "MC_HttpRouterComposition_Red_Fallback.cfg HttpRouterComposition.tla RED"
  "MC_HttpRouterComposition_Red_Twice.cfg HttpRouterComposition.tla RED"
  "MC_HttpRouterComposition_Red_Overdeny.cfg HttpRouterComposition.tla RED"
  "MC_EndpointBind_Green.cfg EndpointBind.tla GREEN"
  "MC_EndpointBind_Red_Retain.cfg EndpointBind.tla RED"
  "MC_EndpointBind_Red_Address.cfg EndpointBind.tla RED"
  "MC_EndpointBind_Red_Port.cfg EndpointBind.tla RED"
  "MC_EndpointBind_Red_Defaults.cfg EndpointBind.tla RED"
  "MC_EndpointBind_Red_MappingScope.cfg EndpointBind.tla RED"
  "MC_EndpointBind_Red_MappingDefaults.cfg EndpointBind.tla RED"
  "MC_SelectiveCarCapacity_Green.cfg SelectiveCarCapacity.tla GREEN"
  "MC_SelectiveCarCapacity_Red_NoReclaim.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_ActiveEviction.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_NoPeerBound.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_NoGlobalBound.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_CrossPeer.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_CrossRoot.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_ExpiredAuthority.cfg SelectiveCarCapacity.tla RED"
  "MC_SelectiveCarCapacity_Red_EarlyEviction.cfg SelectiveCarCapacity.tla RED"
  "MC_RootedAuthorization_Green.cfg RootedAuthorization.tla GREEN"
  "MC_RootedAuthorization_Red_DuplicateOwner.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_LostResume.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_StaleEviction.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_OverReserve.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_Slots.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_ActiveBudget.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_CancelNoWake.cfg RootedAuthorization.tla RED"
  "MC_RootedAuthorization_Red_UnreachedGrant.cfg RootedAuthorization.tla RED"
  "MC_HistoryDriver_Red_UnpacedRetry.cfg HistoryDriver.tla RED"
  "MC_HistoryDriver_Red_TimerDelay.cfg HistoryDriver.tla RED"
  "MC_HistoryDriver_Green.cfg HistoryDriver.tla GREEN"
  "MC_HistoryDriver_Red_LocalLoop.cfg HistoryDriver.tla RED"
  "MC_HistoryDriver_Red_SwallowedAbort.cfg HistoryDriver.tla RED"
  "MC_ContinuationSlot_Green.cfg ContinuationSlot.tla GREEN"
  "MC_ContinuationSlot_Red_LateClaim.cfg ContinuationSlot.tla RED"
  "MC_PendingDagOwner_Green.cfg PendingDagOwner.tla GREEN" # only an owner's own generation releases its pending-DAG slot, and a release wakes the retry clock
  "MC_PendingDagOwner_Red_NoGeneration.cfg PendingDagOwner.tla RED" # a losing claimant's drop frees the winner's slot: more roots work than slots (INV_Capacity)
  "MC_PendingDagOwner_Red_NoWake.cfg PendingDagOwner.tla RED" # a release does not wake the clock: a due root waits for the tick beside a free slot (INV_NoIdleCapacity)
  "MC_PendingDagOwner_Red_RollbackWake.cfg PendingDagOwner.tla RED" # a full-lane rollback wakes the clock itself: it spins through dispatches that cannot progress (INV_RollbackNeedsCause)
  "MC_HistoryTurns_Green.cfg HistoryTurns.tla GREEN"
  "MC_HistoryTurns_Red_Admission.cfg HistoryTurns.tla RED"
  "MC_HistoryTurns_Red_Atomic.cfg HistoryTurns.tla RED"
  "MC_HistoryTurns_Red_Fence.cfg HistoryTurns.tla RED"
  "MC_HistoryTurns_Red_Events.cfg HistoryTurns.tla RED"
  "MC_HistoryTurns_Red_Reclaim.cfg HistoryTurns.tla RED"
  "MC_HistoryTurns_Red_Blocks.cfg HistoryTurns.tla RED"
  "MC_ReplicatorIncarnation_Green.cfg ReplicatorIncarnation.tla GREEN"
  "MC_ReplicatorIncarnation_Red_Error.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_Rollback.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_ReplayIdentity.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_ReplayResurrect.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_ReplayStale.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_ReplayMarker.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_ReplaySuperseded.cfg ReplicatorIncarnation.tla RED"
  "MC_ReplicatorIncarnation_Red_ReplayCompare.cfg ReplicatorIncarnation.tla RED" # a read-then-compare flip skips after a same-incarnation rewrite (ACT_FlipNotSkipped)
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
  "MC_DocumentPush_Green.cfg DocumentPush.tla GREEN"
  "MC_SelectedCarAuthority_Green.cfg SelectedCarAuthority.tla GREEN"
  "MC_SelectedCarAuthority_Red_send.cfg SelectedCarAuthority.tla RED"
  "MC_SelectedCarAuthority_Red_durability.cfg SelectedCarAuthority.tla RED"
  "MC_SelectedCarAuthority_Red_expiry.cfg SelectedCarAuthority.tla RED"
  "MC_SelectedCarAuthority_Red_subscription.cfg SelectedCarAuthority.tla RED"
  "MC_SelectedCarAuthority_Red_scope.cfg SelectedCarAuthority.tla RED"
  "MC_SelectedCarAuthority_Red_revoke.cfg SelectedCarAuthority.tla RED"
  "MC_SelectedPushIsolation_Green.cfg SelectedPushIsolation.tla GREEN"
  "MC_SelectedPushIsolation_Red_shared_budget.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_pause.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_filter.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_backoff.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_hint.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_drain.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_stall.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_cancel.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_schedule.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_admission.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_partial.cfg SelectedPushIsolation.tla RED"
  "MC_SelectedPushIsolation_Red_expiry_stop.cfg SelectedPushIsolation.tla RED"
  "MC_DocumentPush_Red_Admission.cfg DocumentPush.tla RED"
  "MC_PeerStreamCap_Green.cfg PeerStreamCap.tla GREEN"
  "MC_PeerStreamCap_Red_NoCheck.cfg PeerStreamCap.tla RED"
  "MC_PeerStreamCap_Red_Leak.cfg PeerStreamCap.tla RED"
  "MC_PeerStreamCap_Red_EvictLive.cfg PeerStreamCap.tla RED"
  "MC_ReceiveGate_Green.cfg ReceiveGate.tla GREEN" # a refused push stores and imports nothing, a kept one is merged
  "MC_ReceiveGate_Red_StoreFirst.cfg ReceiveGate.tla RED" # the head is stored before the first check (INV_ArrivalRefusalStoresNothing)
  "MC_ReceiveGate_Red_NoAfter.cfg ReceiveGate.tla RED" # a fetched CAR is imported unchecked (INV_FetchRefusalImportsNothing)
  "MC_ReceiveGate_Red_NoBackstop.cfg ReceiveGate.tla RED" # the merge stops checking (INV_RefusedNeverMerged)
  "MC_ReceiveGate_Red_Eager.cfg ReceiveGate.tla RED" # a check refuses what it cannot see (INV_KeptIsKept)
  "MC_DepthCutFetch_Green.cfg DepthCutFetch.tla GREEN" # a depth-cut DAG fetch re-sends little held history, in logarithmic rounds, and completes
  "MC_DepthCutFetch_Red_Unbounded.cfg DepthCutFetch.tla RED" # the provider ignores the cut (INV_HistoryNotResent)
  "MC_DepthCutFetch_Red_NoWiden.cfg DepthCutFetch.tla RED" # the receiver never widens the cut (INV_RoundsLogarithmic)
  "MC_ExactDagFetch_Green.cfg ExactDagFetch.tla GREEN" # a fetch naming its haves is sent only what it lacks, in one request, and merges
  "MC_ExactDagFetch_Green_Liar.cfg ExactDagFetch.tla GREEN" # a lying provider gets no forgery stored and no incomplete DAG merged
  "MC_ExactDagFetch_Red_NoHaves.cfg ExactDagFetch.tla RED" # the provider ignores the haves (INV_HeldNotResent)
  "MC_ExactDagFetch_Red_Fifo.cfg ExactDagFetch.tla RED" # the walk pops out of priority order (INV_HeldNotResent)
  "MC_ExactDagFetch_Red_FlipWant.cfg ExactDagFetch.tla RED" # held overrides a block asked for by name (INV_RoundsBounded)
  "MC_ExactDagFetch_Red_TrustProvider.cfg ExactDagFetch.tla RED" # the receiver merges on the provider's word (INV_MergeComplete)
  "MC_ExactDagFetch_Red_SkipCid.cfg ExactDagFetch.tla RED" # the receiver skips the CID check (INV_NoForged)
  "MC_HeldBlockIngest_Green.cfg HeldBlockIngest.tla GREEN" # ingest owns only the blocks it stores; one writer per mark, no contention on held blocks
  "MC_HeldBlockIngest_Red_NoOwners.cfg HeldBlockIngest.tla RED" # ingest owns nothing (INV_OneMarkWriter)
  "MC_HeldBlockIngest_Red_OwnHeld.cfg HeldBlockIngest.tla RED" # ingest also owns held blocks (INV_HeldNeverContends)
  "MC_DocumentPush_Red_Ownership.cfg DocumentPush.tla RED"
  "MC_DocumentPush_Red_Selection.cfg DocumentPush.tla RED"
  "MC_DocumentPush_Red_Grant.cfg DocumentPush.tla RED"
  "MC_DocumentPush_Red_Fence.cfg DocumentPush.tla RED"
  "MC_DocumentPush_Red_Marker.cfg DocumentPush.tla RED"
  "MC_DocumentPush_Red_Acknowledgement.cfg DocumentPush.tla RED"
  "MC_DocumentPush_Red_Retained.cfg DocumentPush.tla RED"
  "MC_StorageFailure_Green.cfg StorageFailure.tla GREEN"
  "MC_StorageFailure_Red_LostCause.cfg StorageFailure.tla RED"
  "MC_StorageFailure_Red_Text.cfg StorageFailure.tla RED"
  "MC_StorageFailure_Red_Retry.cfg StorageFailure.tla RED"
  "MC_UnifiedAccess_Green.cfg OwnerObservation.tla GREEN"
  "MC_UnifiedAccess_Red_Actor.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Bound.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Nacbound.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Context.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Principal.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_App.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Read.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Member.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Registered.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Supported.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Active.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Node.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Permission.cfg OwnerObservation.tla RED"
  "MC_UnifiedAccess_Red_Atomic.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Green.cfg OwnerObservation.tla GREEN"
  "MC_OwnerObservation_Red_Actor.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Node.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_App.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Document.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Member.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Registered.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Bound.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Snapshot.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_RegistrationSnapshot.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_Visibility.cfg OwnerObservation.tla RED"
  "MC_OwnerObservation_Red_VisibilitySnapshot.cfg OwnerObservation.tla RED"
  "MC_PeerConnection_Green.cfg PeerConnection.tla GREEN"
  "MC_PeerConnection_Red_StaleObservation.cfg PeerConnection.tla RED"
  "MC_SharedRpcConnection_Green.cfg SharedRpcConnection.tla GREEN"
  "MC_LegacyRpcCache_Green.cfg LegacyRpcCache.tla GREEN"
  "MC_RpcCacheEviction_Green.cfg RpcCacheEviction.tla GREEN"
  "MC_RpcCacheEviction_Red_Reinsert.cfg RpcCacheEviction.tla RED"
  "MC_LegacyRpcCache_Red_NoCache.cfg LegacyRpcCache.tla RED"
  "MC_LegacyRpcCache_Red_Preference.cfg LegacyRpcCache.tla RED"
  "MC_SharedRpcConnection_Red_NoAdoption.cfg SharedRpcConnection.tla RED"
  "MC_SharedRpcConnection_Red_NoPostCheck.cfg SharedRpcConnection.tla RED"
  "MC_SharedRpcConnection_Red_NoOutboundCheck.cfg SharedRpcConnection.tla RED"
  "MC_SharedRpcConnection_Red_StaleEviction.cfg SharedRpcConnection.tla RED"
  "MC_SharedRpcConnection_Red_WrongMux.cfg SharedRpcConnection.tla RED"
  "MC_SharedRpcConnection_Red_RetainedClosed.cfg SharedRpcConnection.tla RED"
  "MC_ConnectionAdoption_Green.cfg ConnectionAdoption.tla GREEN"
  "MC_ConnectionAdoption_Red_OpenUncounted.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_RevokeCacheOnly.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_NoRpcAdoption.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_PumpLate.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_NoCleanup.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_HitAdopts.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_PeerWide.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_NoRestore.cfg ConnectionAdoption.tla RED"
  "MC_ConnectionAdoption_Red_AdoptAnnounces.cfg ConnectionAdoption.tla RED"
  "MC_PeerDisconnect_Green.cfg PeerDisconnect.tla GREEN" # Disconnect answers only once the peer's last connection closed
  "MC_PeerDisconnect_Red_EarlyAck.cfg PeerDisconnect.tla RED" # answering before the closes (INV_AnsweredMeansClosed)
  "MC_SchemaPublication_Green.cfg SchemaPublication.tla GREEN"
  "MC_SchemaPublication_Red_EarlyReply.cfg SchemaPublication.tla RED"
  "MC_RetryPreparedInputs_Green.cfg RetryPreparedInputs.tla GREEN" # exact-context external answer survives commit retry
  "MC_RetryPreparedInputs_Red_Recompute.cfg RetryPreparedInputs.tla RED" # clearing prepared memo repeats provider work
  "MC_RetryPreparedInputs_Red_InAttempt.cfg RetryPreparedInputs.tla RED" # provider work runs under active transaction
  "MC_RetryMarkerGeneration_Green.cfg RetryMarkerGeneration.tla GREEN" # old ack and peer clear are fenced; a tick reads one page; a vanished document's marker is discharged
  "MC_RetryMarkerGeneration_Red_StaleAck.cfg RetryMarkerGeneration.tla RED" # stale ack erases newer registration
  "MC_RetryMarkerGeneration_Red_Readd.cfg RetryMarkerGeneration.tla RED" # stale peer clear erases re-added retry state
  "MC_RetryMarkerGeneration_Red_UnboundedReads.cfg RetryMarkerGeneration.tla RED" # every tick loads every marker before asking whether the peer is due (INV_SweepReadsBounded)
  "MC_RetryMarkerGeneration_Red_KeepVanished.cfg RetryMarkerGeneration.tla RED" # a purged document's marker errors on every replay and is never discharged (LIVE_VanishedMarkerDischarged)
  "MC_RetryMarkerGeneration_Red_NoRotation.cfg RetryMarkerGeneration.tla RED" # a failing head marker is replayed first forever and the tail is never reached (LIVE_VanishedMarkerDischarged)
  "MC_RetryPushAdmission_Green.cfg RetryPushAdmission.tla GREEN"
  "MC_RetryPushAdmission_Red_NoPresence.cfg RetryPushAdmission.tla RED"
  "MC_RetryPushAdmission_Red_NoGeneration.cfg RetryPushAdmission.tla RED"
  "MC_RetryPushAdmission_Red_NoDeadline.cfg RetryPushAdmission.tla RED"
  "MC_RetryRegistrationFence_Green.cfg RetryRegistrationFence.tla GREEN"
  "MC_RetryRegistrationFence_Red_NoEpoch.cfg RetryRegistrationFence.tla RED"
  "MC_RetryRegistrationFence_Red_NoOrder.cfg RetryRegistrationFence.tla RED"
  "MC_RetryRegistrationFence_Red_FanOutAbort.cfg RetryRegistrationFence.tla RED" # first rejected peer aborts the fan-out (INV_FanOutPeerLocal)
  "MC_RetrySchedule_Green.cfg MC_RetrySchedule_Common.tla GREEN" # an ack, live, superseded or replayed, restarts the peer clock at the first rung; nothing without a delivery does; no deadline holds past the longest rung
  "MC_RetrySchedule_Red_KeepRung.cfg MC_RetrySchedule_Common.tla RED" # old rule: an ack leaves the escalated clock (ACT_DeliveryRestarts)
  "MC_RetrySchedule_Red_SupersededKeepsRung.cfg MC_RetrySchedule_Common.tla RED" # an ack fenced by a newer marker generation leaves the escalated clock (ACT_DeliveryRestarts)
  "MC_RetrySchedule_Red_ReplayKeepsRung.cfg MC_RetrySchedule_Common.tla RED" # a pass cut short after replays landed keeps the rung (ACT_DeliveryRestarts)
  "MC_RetrySchedule_Red_RegisterRestarts.cfg MC_RetrySchedule_Common.tla RED" # a new marker restarts the clock of a peer that keeps failing (ACT_SilentPeerBacksOff)
  "MC_RetrySchedule_Red_ActivateRestarts.cfg MC_RetrySchedule_Common.tla RED" # a reconnect restarts the clock of a peer that keeps failing (ACT_SilentPeerBacksOff)
  "MC_RetrySchedule_Red_ClockBackHolds.cfg MC_RetrySchedule_Common.tla RED" # F69: a backward clock step holds a connected peer past the longest rung (INV_HeldWithinLongestRung)
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
  "MC_DagFetchBudget_Green.cfg DagFetchBudget.tla GREEN" # one attempt waits at most a rotation of stalls plus the free refusals, however many batches
  "MC_DagFetchBudget_Red_SlowIsRefusal.cfg DagFetchBudget.tla RED" # a failure at the response timeout counts as a free refusal (INV_WaitBounded)
  "MC_DagFetchBudget_Red_Uncapped.cfg DagFetchBudget.tla RED" # every fast refusal is free (INV_WaitBounded)
  "MC_DagFetchBudget_Red_NoCheck.cfg DagFetchBudget.tla RED" # providers are asked after the budget is spent (INV_WaitBounded)
  "MC_Purge_Green_Prune.cfg Purge.tla GREEN" # a purge deletes the history only the purged documents own, keeps shared blocks, counts once on commit, and commits despite a sync re-storing a block
  "MC_Purge_Green_Branchable.cfg Purge.tla GREEN" # a purge in a branchable collection keeps the whole history and drops the purged owner edges
  "MC_Purge_Red_Blind.cfg Purge.tla RED" # the prune deletes a block another document owns (INV_OthersIntact)
  "MC_Purge_Red_PruneBranchable.cfg Purge.tla RED" # the purge deletes a branchable collection's history (INV_CollectionIntact)
  "MC_Purge_Red_KeepEdges.cfg Purge.tla RED" # a purge that keeps the blocks keeps owner edges of gone documents, as Go does (INV_NoStaleOwner)
  "MC_Purge_Red_KeepIndex.cfg Purge.tla RED" # index entries survive the purge (INV_Gone)
  "MC_Purge_Red_CountRequests.cfg Purge.tla RED" # a DocID named twice is counted twice (INV_Count)
  "MC_Purge_Red_CountAttempt.cfg Purge.tla RED" # a conflicted attempt is counted (INV_Count)
  "MC_Purge_Red_Fail.cfg Purge.tla RED" # a conflict fails the purge instead of re-running it (PROP_Commits)
  "MC_Purge_Red_PlainScan.cfg Purge.tla RED" # a plain owner scan misses a merge adopting a block, which the prune deletes (INV_OthersIntact)
  "MC_OrphanGc_Green.cfg OrphanGc.tla GREEN" # #2479: the orphan sweep reclaims an abandoned block and never a merged, owned or untimed one; merged blocks carry no mark
  "MC_OrphanGc_Red_SnapshotRead.cfg OrphanGc.tla RED" # the decision trusts the nomination, so a merge committing in between loses its block (INV_NoMergedLoss)
  "MC_OrphanGc_Red_NoOwnerCheck.cfg OrphanGc.tla RED" # an owned block with a stale mark is deleted (INV_NoOwnedLoss)
  "MC_OrphanGc_Red_BlindRefresh.cfg OrphanGc.tla RED" # sending a merged block again marks it again (INV_MergedUnmarked)
  "MC_OrphanGc_Red_ReclaimLegacy.cfg OrphanGc.tla RED" # a mark with no time is swept (INV_LegacyKept)
  "MC_OrphanGc_Red_SigMarked.cfg OrphanGc.tla RED" # merging leaves the signature block marked (INV_MergedUnmarked)
  "MC_OrphanGc_Red_SigUnowned.cfg OrphanGc.tla RED" # merging records no owner for the signature block (INV_SigOwned)
  "MC_ReplicationFilter_Green.cfg ReplicationFilter.tla GREEN" # #2451: a refused document stores nothing, kept ones merge alone or in a batch
  "MC_ReplicationFilter_Green_Branchable.cfg ReplicationFilter.tla GREEN" # #2451: a branchable collection is not filtered
  "MC_ReplicationFilter_Red_AfterPersist.cfg ReplicationFilter.tla RED" # asked after phase 2 wrote the fields (INV_RefusedNothingStored)
  "MC_ReplicationFilter_Red_WholeBatch.cfg ReplicationFilter.tla RED" # one refusal skips its batch (INV_KeptMerged)
  "MC_ReplicationFilter_Red_Retry.cfg ReplicationFilter.tla RED" # a refused head is retried and stored later (INV_RefusedNothingStored)
  "MC_ReplicationFilter_Red_FilterBranchable.cfg ReplicationFilter.tla RED" # a branchable collection is filtered per document (INV_BranchableUnfiltered)
  "MC_ReplicationFilter_Red_Alone.cfg ReplicationFilter.tla RED" # a later commit of a refused document merges on its own fields (INV_RefusedNothingStored)
  "MC_IndexDropBackfill_Green.cfg IndexDropBackfill.tla GREEN" # #2233: a drop during a backfill leaves no record and no entry, and finishes after a restart
  "MC_IndexDropBackfill_Red_KeepRecord.cfg IndexDropBackfill.tla RED" # the drop leaves the backfill record (INV_NoRecordForGoneIndex)
  "MC_IndexDropBackfill_Red_BlindFail.cfg IndexDropBackfill.tla RED" # the backfill writes its failure for a gone index (INV_NoRecordForGoneIndex)
  "MC_IndexDropBackfill_Red_SameTxn.cfg IndexDropBackfill.tla RED" # entries deleted from an unvalidated scan in the drop commit (INV_NoOrphanEntries)
  "MC_IndexDropBackfill_Red_Forget.cfg IndexDropBackfill.tla RED" # a restart ignores an unfinished drop (LIVE_DropFinishes)
  "MC_IndexDropBackfill_Red_ReuseId.cfg IndexDropBackfill.tla RED" # a new index gets the dropped id and the drop deletes its entries (INV_NewIndexIntact)
  "MC_IndexBuildGateClose_Green.cfg IndexBuildGateClose.tla GREEN" # a close while a build waits at its gate finishes, and the reopen returns before the node resumes the build, which is then built
  "MC_IndexBuildGateClose_Red_NoRace.cfg IndexBuildGateClose.tla RED" # the gate wait ignores the close and the close hangs (LIVE_CloseFinishes)
  "MC_IndexBuildGateClose_Red_FailOnClose.cfg IndexBuildGateClose.tla RED" # the close marks the build failed and it is never built (LIVE_BuildFinishes)
  "MC_IndexBuildGateClose_Red_Forget.cfg IndexBuildGateClose.tla RED" # the reopen forgets the stopped build (LIVE_BuildFinishes)
  "MC_IndexBuildGateClose_Red_OpenAwaits.cfg IndexBuildGateClose.tla RED" # the reopen waits for the resumed build held at the shut gate, so it is never built (LIVE_BuildFinishes)
  "MC_WritePipeline_Green.cfg WritePipeline.tla GREEN" # #1890: creates and updates store blocks once the composite exists, one short ID and one key mint per field across attempts
  "MC_WritePipeline_Red_AsComputed.cfg WritePipeline.tla RED" # each block stored as made (INV_NoStoreBeforeComposite)
  "MC_WritePipeline_Red_PerAttempt.cfg WritePipeline.tla RED" # fresh short ID per attempt: a key minted twice (INV_OneKeyPerField)
  "MC_WritePipeline_Red_Direct.cfg WritePipeline.tla RED" # key minted while the attempt runs (INV_NoMintInAttempt)
  "MC_WritePipeline_Red_FirstMiss.cfg WritePipeline.tla RED" # the first missing key ends the attempt: a second wave (INV_OneWave)
  "MC_AddCollection_Green.cfg AddCollection.tla GREEN" # #2319: add_collection checks nac, parse, validate, policy in order, then creates all collections or none
  "MC_AddCollection_Red_Nac.cfg AddCollection.tla RED" # NAC check left out: a caller without CollectionPatch creates collections (INV_NoCollectionUnlessAllChecksPass)
  "MC_AddCollection_Red_Parse.cfg AddCollection.tla RED" # parse check left out (INV_NoCollectionUnlessAllChecksPass)
  "MC_AddCollection_Red_Validate.cfg AddCollection.tla RED" # schema validation left out (INV_NoCollectionUnlessAllChecksPass)
  "MC_AddCollection_Red_Policy.cfg AddCollection.tla RED" # @policy check left out (INV_NoCollectionUnlessAllChecksPass)
  "MC_AddCollection_Red_OneByOne.cfg AddCollection.tla RED" # collections created one at a time: a later failure leaves earlier ones (INV_AllOrNone)
  "MC_IndexInTxn_Green.cfg IndexInTxn.tla GREEN" # #2374 follow-up: an index created or dropped in a txn is staged in it, listed in it, built after commit before the commit returns
  "MC_IndexInTxn_Red_WriteOutside.cfg IndexInTxn.tla RED" # the change commits on its own (INV_IsolatedUntilCommit)
  "MC_IndexInTxn_Red_EarlyBackfill.cfg IndexInTxn.tla RED" # backfill at create, before commit (INV_WorkOnlyAfterCommit)
  "MC_IndexInTxn_Red_StaleCatalog.cfg IndexInTxn.tla RED" # stale cached definition in the txn (INV_SeesOwnChange)
  "MC_IndexInTxn_Red_NoAwait.cfg IndexInTxn.tla RED" # commit returns before the backfill (INV_BuiltWhenCommitReturns)
  "MC_IndexInTxn_Red_NoSavepoint.cfg IndexInTxn.tla RED" # a failed change keeps its partial writes and the commit lands them (INV_FailedChangeLeavesNothing)
  "MC_Truncate_Green.cfg Truncate.tla GREEN" # #2321: truncate checks nac, then for a filter branchable, txn and shape, before a delete that is durable at once
  "MC_Truncate_Red_Nac.cfg Truncate.tla RED" # permission check left out: a caller without CollectionTruncate removes documents (INV_NoRemovalWithoutPermission)
  "MC_Truncate_Red_Branchable.cfg Truncate.tla RED" # branchable check left out: a filtered truncate splits shared history (INV_FilteredNeverOnBranchable)
  "MC_Truncate_Red_Txn.cfg Truncate.tla RED" # transaction check left out, as before: a filter on the txn view removes an uncommitted match (INV_FilteredRemovesOnlyCommittedMatches)
  "MC_Truncate_Red_Shape.cfg Truncate.tla RED" # filter shape check left out: a non-filter removes everything (INV_BadFilterRemovesNothing)
  "MC_Truncate_Red_ViewCache.cfg Truncate.tla RED" # #2703: view cache clear left out: a truncated view still shows its rows (INV_UnfilteredEmptiesView)
  "MC_NacOrder_Green.cfg NacOrder.tla GREEN" # #2680/#2700: a document write checks get-collection then its own permission, in its transaction, and a call with no identity acts as no one
  "MC_NacOrder_Red_SkipGet.cfg NacOrder.tla RED" # get-collection left out: a caller without it writes (INV_NoEffectWithoutPermission)
  "MC_NacOrder_Red_OwnFirst.cfg NacOrder.tla RED" # the write's own permission checked first: the refusal names it, not get-collection (INV_RefusalNamesGoFirst)
  "MC_NacOrder_Red_AnonAsNode.cfg NacOrder.tla RED" # no identity acts as the node: a caller with nothing writes (INV_NoEffectWithoutPermission)
  "MC_NacOrder_Red_OutsideTxn.cfg NacOrder.tla RED" # permissions read outside the transaction: a revoke that lands first is missed (INV_NoEffectWithoutPermission)
  "MC_NacDacBypass_Green.cfg NacDacBypass.tla GREEN" # #2681: a read bypasses DAC exactly when NAC is on and grants bypass-dac at its snapshot
  "MC_NacDacBypass_Red_NacOff.cfg NacDacBypass.tla RED" # a bare permission check: NAC off reads as granted and opens DAC (INV_BypassOnlyWhenGranted)
  "MC_NacDacBypass_Red_Never.cfg NacDacBypass.tla RED" # no bypass, as before: the node owner reads nothing it does not own (INV_BypassWhenGranted)
  "MC_NacDacBypass_Red_Live.cfg NacDacBypass.tla RED" # grant read outside the read's transaction: a revoke after the snapshot is missed (INV_BypassOnlyWhenGranted)
  "MC_RelationFilterScan_Green.cfg RelationFilterScan.tla GREEN" # #2099: a relation-filtered delete or update scans validated, so a concurrent change to a related document aborts it
  "MC_RelationFilterScan_Red_Unvalidated.cfg RelationFilterScan.tla RED" # scans left unvalidated: the delete commits on a stale related match (INV_DecisionHoldsAtCommit)
  "MC_DacPolicyAdd_Green.cfg DacPolicyAdd.tla GREEN" # #2321: add DAC policy checks nac, acp, creator, data, valid in Go's order; a remote ACP's policy is kept locally before its ID goes back
  "MC_DacPolicyAdd_Red_Nac.cfg DacPolicyAdd.tla RED" # permission check left out (INV_NoPolicyWithoutPermission)
  "MC_DacPolicyAdd_Red_Acp.cfg DacPolicyAdd.tla RED" # ACP check left out: no ACP, no error (INV_NoSilentFailure)
  "MC_DacPolicyAdd_Red_Creator.cfg DacPolicyAdd.tla RED" # creator check left out, as the CLI's Vera path sent an empty one (INV_NoPolicyWithoutCreator)
  "MC_DacPolicyAdd_Red_Data.cfg DacPolicyAdd.tla RED" # empty-policy check left out (INV_NoInvalidPolicy)
  "MC_DacPolicyAdd_Red_Valid.cfg DacPolicyAdd.tla RED" # policy not checked before the store or the remote create (INV_NoInvalidPolicy)
  "MC_DacPolicyAdd_Red_Mirror.cfg DacPolicyAdd.tla RED" # remote policy not kept locally, as the node's Vera path did (INV_ReturnedIsKeptLocally)
  "MC_DacPolicyAdd_Red_Order.cfg DacPolicyAdd.tla RED" # creator checked before ACP, as the HTTP handler did (INV_GoOrder)
  "MC_AddViewTransform_Green.cfg AddViewTransform.tla GREEN" # #2724: target design, NAC and the lens config both decided in the view's transaction; the accepted CID is stored as asked
  "MC_AddViewTransform_Red_NoEmpty.cfg AddViewTransform.tla RED" # empty-CID check left out (INV_NoEmptyTransform)
  "MC_AddViewTransform_Red_NoLens.cfg AddViewTransform.tla RED" # lens check left out (INV_StoredTransformExists)
  "MC_AddViewTransform_Red_NoStore.cfg AddViewTransform.tla RED" # accepted CID not stored on the query source (INV_StoredIsAccepted)
  "MC_AddViewTransform_Red_LensInMemory.cfg AddViewTransform.tla RED" # lens asked of the in-memory store, as before the fix: it lags the lens commit (INV_NoValidRejected)
  "MC_AddViewTransform_Red_SyncMemoryOnly.cfg AddViewTransform.tla RED" # P2P sync keeps a lens in memory only: a held lens is refused (INV_NoValidRejected)
  "MC_AddViewTransform_Red_SyncMemoryOnly_Restart.cfg AddViewTransform.tla RED" # before the fix: memory-only sync and a memory check store a lens a restart loses (INV_StoredTransformExists)
  "MC_AddViewTransform_Green_NacBefore.cfg AddViewTransform.tla GREEN" # the code as it is: lenses from either path decided in the transaction; NAC still before it, every other promise holds
  "MC_AddViewTransform_Red_KnownGap_NacBefore.cfg AddViewTransform.tla RED" # known gap (#2736): NAC ViewAdd read before the view's transaction (INV_NoViewWithoutPermission)
  "MC_LensIndexAnswer_Green.cfg LensIndexAnswer.tla GREEN" # #2379: an index answers only when no reindex is owed; schema changes and off-version merges owe a fresh record; a rebuild clears only its own
  "MC_LensIndexAnswer_Red_NoGate.cfg LensIndexAnswer.tla RED" # index answers while owed (INV_IndexMatchesReads)
  "MC_LensIndexAnswer_Red_LensGate.cfg LensIndexAnswer.tla RED" # main before #2379: no index once any lens exists (INV_SettledIndexAnswers)
  "MC_LensIndexAnswer_Red_NoSchemaOwe.cfg LensIndexAnswer.tla RED" # schema change owes nothing (INV_IndexMatchesReads)
  "MC_LensIndexAnswer_Red_NoMergeOwe.cfg LensIndexAnswer.tla RED" # off-version merge owes nothing (INV_IndexMatchesReads)
  "MC_LensIndexAnswer_Red_SkipIfOwed.cfg LensIndexAnswer.tla RED" # merge skips owing while a record stands (INV_IndexMatchesReads)
  "MC_LensIndexAnswer_Red_NoTokenCheck.cfg LensIndexAnswer.tla RED" # rebuild clears a newer record (INV_IndexMatchesReads)
  "MC_VerifySignature_Green_Pool.cfg VerifySignature.tla GREEN" # #2724: with no transaction, NAC, block and document ACP read one read-only view
  "MC_VerifySignature_Green_Txn.cfg VerifySignature.tla GREEN" # #2724: in the caller's transaction, all three read its view, pending writes included
  "MC_VerifySignature_Red_NoNac.cfg VerifySignature.tla RED" # NAC check left out (INV_VerifiedAtOnePoint)
  "MC_VerifySignature_Red_NoAcp.cfg VerifySignature.tla RED" # document ACP check left out (INV_VerifiedAtOnePoint)
  "MC_VerifySignature_Red_NoBlock.cfg VerifySignature.tla RED" # block presence check left out (INV_VerifiedAtOnePoint)
  "MC_VerifySignature_Red_NacLatest.cfg VerifySignature.tla RED" # NAC read before the snapshot (INV_VerifiedAtOnePoint)
  "MC_VerifySignature_Red_AcpLatest.cfg VerifySignature.tla RED" # document ACP unbound from the caller's transaction (INV_VerifiedAtOnePoint)
  "MC_VerifySignature_Red_BlockLatest.cfg VerifySignature.tla RED" # block read from the latest commit (INV_VerifiedAtOnePoint)
  "MC_NodeAcpOps_Green.cfg NodeAcpOps.tla GREEN" # #2321: NAC disable, re-enable and relationships check in Go's order; re-enable checks the stored relationships; a restart keeps NAC as it was
  "MC_NodeAcpOps_Red_Configured.cfg NodeAcpOps.tla RED" # set-up check left out (INV_NoChangeUnlessConfigured)
  "MC_NodeAcpOps_Red_Already.cfg NodeAcpOps.tla RED" # already-off/on checks left out (INV_NoSilentNoOp)
  "MC_NodeAcpOps_Red_Nac.cfg NodeAcpOps.tla RED" # live permission check left out (INV_NoDisableWithoutPermission)
  "MC_NodeAcpOps_Red_Stored.cfg NodeAcpOps.tla RED" # re-enable checks no permission (INV_NoReEnableWithoutStoredPermission)
  "MC_NodeAcpOps_Red_LiveReEnable.cfg NodeAcpOps.tla RED" # re-enable uses the live check, open while NAC is off (INV_NoReEnableWithoutStoredPermission)
  "MC_NodeAcpOps_Red_Acp.cfg NodeAcpOps.tla RED" # NAC-on check left out (INV_NoGrantUnlessAllChecksPass)
  "MC_NodeAcpOps_Red_Identity.cfg NodeAcpOps.tla RED" # identity check left out (INV_NoGrantUnlessAllChecksPass)
  "MC_NodeAcpOps_Red_Relation.cfg NodeAcpOps.tla RED" # relation check left out (INV_NoGrantUnlessAllChecksPass)
  "MC_NodeAcpOps_Red_Target.cfg NodeAcpOps.tla RED" # target check left out (INV_NoGrantUnlessAllChecksPass)
  "MC_NodeAcpOps_Red_Authority.cfg NodeAcpOps.tla RED" # admin-or-delegate authority check left out (INV_NoGrantUnlessAllChecksPass)
  "MC_NodeAcpOps_Red_StartReEnables.cfg NodeAcpOps.tla RED" # a restart re-enables a disabled NAC, as before (INV_RestartKeepsStatus)
  "MC_NodeAcpOps_Red_Order.cfg NodeAcpOps.tla RED" # re-enable checks already-on before the caller (INV_GoOrder)
  "MC_NodeAcpOps_Race_Green.cfg NodeAcpOps.tla GREEN" # #2521: two disables or re-enables race a grant or revocation; one transaction per attempt, re-decided after a lost commit
  "MC_NodeAcpOps_Red_SplitScope.cfg NodeAcpOps.tla RED" # permission read in its own transaction (INV_RaceNoChangeWithoutPermission)
  "MC_NodeAcpOps_Red_OutsideGuard.cfg NodeAcpOps.tla RED" # already guard read outside the write (INV_RaceOneWinner)
  "MC_NodeAcpOps_Red_NoRetry.cfg NodeAcpOps.tla RED" # lost commit reported as an error (INV_RaceLoserHearsAlready)
  "MC_NodeAcpOps_Rel_Green.cfg NodeAcpOps.tla GREEN" # #2521: a relationship change races a disable; NAC on is read in the write and re-decided after a lost commit
  "MC_NodeAcpOps_Red_RelOutsideGuard.cfg NodeAcpOps.tla RED" # NAC on checked only before the write (INV_RelOffHearsGo)
  "M1Convergence.cfg            M1Convergence.tla            GREEN"  # B3 control (S1')
  "M1Naive.cfg                  M1Convergence.tla            RED"    # B3 #2721 (S1)
  "MC_S2.cfg                    MC_S2.tla                    GREEN"  # B3 ideal (S2)
  "MC_S3.cfg                    MC_S3.tla                    RED"    # B3 split ownership (S3)
  "MC_S3_Fixed.cfg              MC_S3.tla                    GREEN"  # B3 immutable key (S3)
  "MC_S4_Naive.cfg              MC_S4.tla                    RED"    # B3 field-grain #2721 (S4)
  "MC_S4_FullWalkA.cfg          MC_S4.tla                    RED"    # B3 Model A over-fetch (S4)
  "MC_S4_ModelB.cfg             MC_S4.tla                    GREEN"  # B3 Model B (S4)
  "MC_Conv_Eventual.cfg         MC_Conv_Eventual.tla         GREEN"  # convergence, eventual connectivity, retry markers owe the heads
  "MC_Conv_RestartEviction.cfg  MC_Conv_RestartEviction.tla  GREEN"  # convergence, restarts evict memory, durable records re-drive
  "MC_Conv_NoHeadRediscovery.cfg MC_Conv_NoHeadRediscovery.tla RED"  # convergence fails when nothing owes the heads (no on-connect rediscovery)
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
  "MC_TextCrdt_Green.cfg TextCrdt.tla GREEN" # Text CRDT: same blocks same text, incremental equals replay, edits land at their position
  "MC_TextCrdt_Red_ArrivalOrder.cfg TextCrdt.tla RED" # siblings ordered by arrival: peers with the same blocks differ (INV_Converge)
  "MC_TextCrdt_Red_NoCausalBuffer.cfg TextCrdt.tla RED" # block merged before its parents: text differs from replay (INV_MatchesReplay)
  "MC_TextCrdt_Red_CurrentText.cfg TextCrdt.tla RED" # position read in the receiver's text: character misplaced (INV_PlacedAsIntended)
  "MC_TextCrdt_Red_NextNode.cfg TextCrdt.tla RED" # always left child of the next node: a concurrent run splits a typed run (INV_NoInterleave)
  "MC_TextCrdt_Red_CommonAncestor.cfg TextCrdt.tla RED" # concurrent merge replayed from a block merely under every head: text differs from a full replay (INV_CriticalReplay)
  "MC_TextCrdt_Red_NullLast.cfg TextCrdt.tla RED" # null flag taken from the block merged last: a peer reads null beside a concurrent insert (INV_NullMatchesReplay)
  "MC_JsonCrdt_Green_FirstValueOrMap.cfg JsonCrdt.tla GREEN" # JSON CRDT, first block sets a value or {}: same blocks same JSON, incremental equals replay, update beats delete, LWW per path
  "MC_JsonCrdt_Green_FirstListOrDelete.cfg JsonCrdt.tla GREEN" # JSON CRDT, first block sets [] or deletes: the other half of the same state space
  "MC_JsonCrdt_Red_DeleteWins.cfg JsonCrdt.tla RED" # a delete beats a concurrent edit below it: the edit vanishes (INV_UpdateWins)
  "MC_JsonCrdt_Red_NoCid.cfg JsonCrdt.tla RED" # priority ties kept by arrival: peers with the same blocks differ (INV_Converge)
  "MC_JsonCrdt_Red_DeleteImpliesMap.cfg JsonCrdt.tla RED" # a delete below a path implies a map: the path comes back with no set behind it (INV_DeleteCreatesNothing)
  "MC_JsonCrdt_Red_SharedId.cfg JsonCrdt.tla RED" # equal edits with no nonce share an ID: a delete that saw one wipes both on one peer only (INV_Converge)
  "MC_JsonCrdt_Green_MultiPath.cfg JsonCrdt.tla GREEN" # JSON CRDT, blocks editing several paths: same blocks same JSON, incremental equals replay, a write lands as it reads
  "MC_JsonCrdt_Red_NoAnchor.cfg JsonCrdt.tla RED" # a delete below a map only its sets hold goes out bare: the map vanishes (INV_WriteLandsAsRead)
  "MC_JsonCrdt_Red_NoSettle.cfg JsonCrdt.tla RED" # a block goes out with a value above a set it carries: its own value wins (INV_WriteLandsAsRead)
  "MC_JsonCrdt_Red_DeleteMakesParent.cfg JsonCrdt.tla RED" # the writer reads a delete through a missing path as making a map (INV_WriteLandsAsRead)
  "MC_JsonCrdt_Red_MapKeepsBelow.cfg JsonCrdt.tla RED" # the writer reads {} over a map as keeping what is below (INV_WriteLandsAsRead)
  "MC_UnknownCrdt_Green.cfg UnknownCrdt.tla GREEN" # #2183: a node without a custom type parks its blocks, merges the rest, and merges each parked block once when built with the type
  "MC_UnknownCrdt_Red_DropDeferral.cfg UnknownCrdt.tla RED" # a custom block acked with no record: never merged after registering (INV_ConvergeWhenDone)
  "MC_UnknownCrdt_Red_DoubleReplay.cfg UnknownCrdt.tla RED" # a parked record kept after its merge: merged again (INV_MergeOnce)
  "MC_UnknownCrdt_Red_Lww.cfg UnknownCrdt.tla RED" # the unknown type taken as LWW: replicas differ for good (INV_ConvergeWhenDone)
  "MC_UnknownCrdt_Red_RemoveStored.cfg UnknownCrdt.tla RED" # a node started without a type it stores: a rewrite loses the value for good (INV_ConvergeWhenDone)
  "MC_UnknownCrdt_Red_NoDocument.cfg UnknownCrdt.tla RED" # no document for a block whose only part was parked: its replay writes nowhere (INV_ConvergeWhenDone)
  "MC_UnknownCrdt_Red_FailStall.cfg UnknownCrdt.tla RED" # a store error or missing key sets a parked block aside for good (INV_ConvergeWhenDone)
  "MC_ReplicationAckScope_Green.cfg ReplicationAckScope.tla GREEN" # push and read pick one ack scope: an acknowledged document reads acknowledged, a never-pushed one never does
  "MC_ReplicationAckScope_Red_MainRule.cfg ReplicationAckScope.tla RED" # main before the fix: collection scope for a filtered replicator, its acks never read (INV_AckedReadsAcked)
  "MC_ReplicationAckScope_Red_PushToFiltered.cfg ReplicationAckScope.tla RED" # collection heads pushed to a filtered replicator: it gets unmatched documents (INV_FilterRespected)
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
  "MC_TxnContext_Green.cfg TxnContext.tla GREEN" # a transaction in the request context: only its opener runs in it, read-only writes nothing, finished runs nothing, discard leaves nothing, clashing commit fails
  "MC_TxnContext_Red_NoOwnerCheck.cfg TxnContext.tla RED" # another caller's transaction ID trusted (INV_OnlyOwnerRuns)
  "MC_TxnContext_Red_ReadOnlyWrites.cfg TxnContext.tla RED" # read-only flag unchecked at the write (INV_ReadOnlyNeverWrites)
  "MC_TxnContext_Red_RunAfterEnd.cfg TxnContext.tla RED" # a finished transaction still runs requests (INV_FinishedNeverRuns)
  "MC_TxnContext_Red_DiscardLeaks.cfg TxnContext.tla RED" # discard applies its writes (INV_DiscardLeavesNothing)
  "MC_TxnContext_Red_NoConflictCheck.cfg TxnContext.tla RED" # commit skips the conflict check (INV_NoLostUpdate)
  "MC_CoreDispatch_Green.cfg CoreDispatch.tla GREEN" # schema-evolution ops: NAC before any effect, a named transaction holds the write, a patch in one is refused
  "MC_CoreDispatch_Red_NoNac.cfg CoreDispatch.tla RED" # the core skips NAC (INV_NoEffectWithoutPermission)
  "MC_CoreDispatch_Red_IgnoreTxn.cfg CoreDispatch.tla RED" # the edge drops the transaction (INV_TxnNeverAutocommits)
  "MC_AdministrativeRelationship_Green.cfg AdministrativeRelationship.tla GREEN"
  "MC_AdministrativeRelationship_Red_Actor.cfg AdministrativeRelationship.tla RED" # INV_Actor
  "MC_AdministrativeRelationship_Red_Active.cfg AdministrativeRelationship.tla RED" # INV_Active
  "MC_AdministrativeRelationship_Red_NacBound.cfg AdministrativeRelationship.tla RED" # INV_NacBound
  "MC_AdministrativeRelationship_Red_Bypass.cfg AdministrativeRelationship.tla RED" # INV_Bypass
  "MC_AdministrativeRelationship_Red_Operation.cfg AdministrativeRelationship.tla RED" # INV_Operation
  "MC_AdministrativeRelationship_Red_DacBound.cfg AdministrativeRelationship.tla RED" # INV_DacBound
  "MC_AdministrativeRelationship_Red_Registered.cfg AdministrativeRelationship.tla RED" # INV_Registered
  "MC_AdministrativeRelationship_Red_Relation.cfg AdministrativeRelationship.tla RED" # INV_Relation
  "MC_AdministrativeRelationship_Red_Revocation.cfg AdministrativeRelationship.tla RED" # INV_Revocation
  "MC_AdministrativeRelationship_Red_Rollback.cfg AdministrativeRelationship.tla RED" # INV_Rollback
  "MC_AdministrativeRelationship_Red_Route.cfg AdministrativeRelationship.tla RED" # INV_EmbedderRoute
  "MC_TransactionalRelationship_Green.cfg TransactionalRelationship.tla GREEN" # a grant and marker share the authorized transaction
  "MC_DelegatedRelationship_Green.cfg DelegatedRelationship.tla GREEN" # the embedder owns the transaction and the selected actor passes NAC and DAC
  "MC_DelegatedRelationship_Red_Http.cfg DelegatedRelationship.tla RED" # an HTTP caller reaches the privileged embedding boundary (INV_ActorAuthority)
  "MC_DelegatedRelationship_Red_Owner.cfg DelegatedRelationship.tla RED" # a transaction owned by another caller is admitted (INV_ActorAuthority)
  "MC_DelegatedRelationship_Red_Actor.cfg DelegatedRelationship.tla RED" # an unauthenticated actor is accepted (INV_ActorAuthority)
  "MC_DelegatedRelationship_Red_Nac.cfg DelegatedRelationship.tla RED" # node authority replaces the actor's NAC (INV_ActorAuthority)
  "MC_DelegatedRelationship_Red_Dac.cfg DelegatedRelationship.tla RED" # node authority replaces the actor's DAC (INV_ActorAuthority)
  "MC_DelegatedRelationship_Red_Bound.cfg DelegatedRelationship.tla RED" # an unbound provider is accepted (INV_ActorAuthority)
  "MC_HttpTxnConfiguration_Green.cfg HttpTxnConfiguration.tla GREEN" # HTTP never shares the embedder-only transaction key
  "MC_HttpTxnConfiguration_Red_Allow.cfg HttpTxnConfiguration.tla RED" # HTTP uses the embedder-only key (INV_HttpOwnerIsolation)
  "MC_TransactionalRelationship_Red_Owner.cfg TransactionalRelationship.tla RED" # another actor reaches the transaction (INV_Authority)
  "MC_TransactionalRelationship_Red_Readonly.cfg TransactionalRelationship.tla RED" # a read-only transaction writes (INV_Authority)
  "MC_TransactionalRelationship_Red_Nac.cfg TransactionalRelationship.tla RED" # the node permission is skipped (INV_Authority)
  "MC_TransactionalRelationship_Red_Dac.cfg TransactionalRelationship.tla RED" # the document relationship manager check is skipped (INV_Authority)
  "MC_TransactionalRelationship_Red_Provider.cfg TransactionalRelationship.tla RED" # an external provider is treated as transaction-bound (INV_Authority)
  "MC_TransactionalRelationship_Red_Expiry.cfg TransactionalRelationship.tla RED" # an expired request starts its transaction turn (INV_Authority)
  "MC_TransactionalRelationship_Red_Backend.cfg TransactionalRelationship.tla RED" # an unsupported server accepts the request (INV_Authority)
  "MC_TransactionalRelationship_Red_Autocommit.cfg TransactionalRelationship.tla RED" # the edge drops the transaction (INV_Private)
  "MC_TransactionalRelationship_Red_Conflict.cfg TransactionalRelationship.tla RED" # the stale marker is not validated (INV_NoRevival)
  "MC_OwnerSelfObservation_Green.cfg OwnerSelfObservation.tla GREEN" # only a visible document owned by this caller can answer owned
  "MC_OwnerSelfObservation_Red_Auth.cfg OwnerSelfObservation.tla RED" # an anonymous caller is admitted (INV_NoFalseAuthority)
  "MC_OwnerSelfObservation_Red_Owner.cfg OwnerSelfObservation.tla RED" # the caller reaches another actor's transaction (INV_NoFalseAuthority)
  "MC_OwnerSelfObservation_Red_Bound.cfg OwnerSelfObservation.tla RED" # an unbound ACP is treated as a transaction snapshot (INV_NoFalseAuthority)
  "MC_OwnerSelfObservation_Red_Subject.cfg OwnerSelfObservation.tla RED" # the requester chooses another subject (INV_NoFalseAuthority)
  "MC_OwnerSelfObservation_Red_Visible.cfg OwnerSelfObservation.tla RED" # an unreadable document is reported owned (INV_NoFalseAuthority)
  "MC_TwoStoreCounter_Red_Split.cfg TwoStoreCounter.tla           RED"   # counter reconcile-from-blob clobbers a concurrent local increment (INV_NoLoss)
  "MC_TwoStoreCounter_Red_DoubleApply.cfg TwoStoreCounter.tla     RED"   # #4935: re-delivered delta merged twice w/o dedup -> double-apply (INV_NoDoubleApply)
  "MC_TwoStoreCounter_Red_LostDeferral.cfg TwoStoreCounter.tla    RED"   # standalone counter block before its document's value claims the applied key unadded (INV_NoLoss)
  "MC_TwoStoreCounter_Green.cfg     TwoStoreCounter.tla           GREEN" # unified RMW + merged-set dedup + standalone deferral -> exact (no loss, no double)
  "MC_MixedFieldMaterialization_Red_WholeDoc.cfg MixedFieldMaterialization.tla RED" # stale whole-doc field merge clobbers another field
  "MC_MixedFieldMaterialization_Green.cfg MixedFieldMaterialization.tla GREEN" # componentwise field materialization preserves mixed product state
  "MC_IndexReconciliation_Red_SaveOnly.cfg IndexReconciliation.tla RED" # stale index key survives CRDT winner rematerialization
  "MC_IndexReconciliation_Green.cfg IndexReconciliation.tla GREEN" # index keys equal the winning CRDT materialized value
  "MC_MergedFieldIndex_Green.cfg MergedFieldIndex.tla GREEN" # #2235: a merge moves the index of a merged field from its pre-merge value
  "MC_MergedFieldIndex_Red_ReadAfterPart.cfg MergedFieldIndex.tla RED" # the index update starts from the value the field merge already wrote (INV_IndexMatchesValue)
  "MC_MergedFieldIndex_Green_Clear.cfg MergedFieldIndex.tla GREEN" # a field write drops the kept read instead of updating it
  "MC_MergedFieldIndex_Red_StaleCache.cfg MergedFieldIndex.tla RED" # a field write leaves the kept read stale, so the next block loses an edit (INV_NoLostEdit)
  "MC_DocumentMaterialization_Red_Overwrite.cfg DocumentMaterialization.tla RED" # active rematerialization must not clear delete marker
  "MC_DocumentMaterialization_Green.cfg DocumentMaterialization.tla GREEN" # deletion marker is componentwise/absorbing
  "MC_PushLogAdmission_Green.cfg MC_PushLogAdmission_Common.tla GREEN" # #1088 W1: capacity overflow nacks -> success implies registered-or-merged + all docs merge
  "MC_PushLogAdmission_Red_SuccessOnFull.cfg MC_PushLogAdmission_Common.tla RED" # fa4a84f7 regression: overflow drops registration but acks success (INV_SuccessImpliesRegisteredOrMerged)
  "MC_PushLogAdmission_Green_Siblings.cfg MC_PushLogAdmission_Common.tla GREEN" # F1: a head retires only the scope head it links -> sibling heads each keep a registration + all merge
  "MC_PushLogAdmission_Red_PriorityCover.cfg MC_PushLogAdmission_Common.tla RED" # F1: (priority, CID) covers or supersedes a sibling (INV_AckedHeadReachable)
  "MC_PushLogAdmission_Red_ViewGuard.cfg MC_PushLogAdmission_Common.tla RED" # the ack guard reads only the lagging view and nacks a head on its committed durable record (INV_NackImpliesUnregistered)
  "MC_PushLogAdmission_Green_PeerShare.cfg MC_PushLogAdmission_Common.tla GREEN" # the store checks the peer's share in the registration's transaction: the peer never holds more than its share
  "MC_PushLogAdmission_Red_ViewShare.cfg MC_PushLogAdmission_Common.tla RED" # the share is checked against the lagging view: two of the peer's pushes pass it together (INV_PeerQuota)
  "MC_GossipAdmission_Green.cfg MC_GossipAdmission_Common.tla GREEN" # #2489: refused gossip heads kept durably and promoted -> none lost, all merge
  "MC_GossipAdmission_Green_PastCap.cfg MC_GossipAdmission_Common.tla GREEN" # past a byte budget every refused head is owned or reported as a node event
  "MC_GossipAdmission_Red_SilentShed.cfg MC_GossipAdmission_Common.tla RED" # past a budget, dropped without the event (INV_HeardIsOwnedOrReported)
  "MC_GossipAdmission_Red_Drop.cfg MC_GossipAdmission_Common.tla RED" # main: a refused gossip head is dropped and never re-announced (INV_HeardIsOwned)
  "MC_GossipAdmission_Red_MemoryDefer.cfg MC_GossipAdmission_Common.tla RED" # in-memory side queue: a crash loses it (INV_HeardIsOwned)
  "MC_GossipAdmission_Red_NoPromote.cfg MC_GossipAdmission_Common.tla RED" # durable records never promoted (LIVE_AllMerged)
  "MC_GossipAdmission_Red_LaneDrop.cfg MC_GossipAdmission_Common.tla RED" # dispatcher drops a head with every refusal worker busy (INV_HeardIsOwnedOrReported)
  "MC_SenderBroadcast_Green.cfg MC_SenderBroadcast_Common.tla GREEN" # #2489: a committed write waits for room, its window holds no slot, a passed deadline is reported
  "MC_SenderBroadcast_Red_Shed.cfg MC_SenderBroadcast_Common.tla RED" # main: a full pool sheds a committed write's gossip (INV_CommittedIsOwnedOrReported)
  "MC_SenderBroadcast_Red_SilentExpiry.cfg MC_SenderBroadcast_Common.tla RED" # a deadline drops it without the event (INV_CommittedIsOwnedOrReported)
  "MC_SenderBroadcast_Red_SlotAcrossWindow.cfg MC_SenderBroadcast_Common.tla RED" # a slot held through the coalescing window (INV_WindowHoldsNoSlot)
  "MC_SenderBroadcast_Red_UnboundedPending.cfg MC_SenderBroadcast_Common.tla RED" # no bound on the pending set (INV_PendingBounded)
  "MC_SenderBroadcast_Red_TaskPerWrite.cfg MC_SenderBroadcast_Common.tla RED" # #1099: one retained task per pending write (INV_TasksBounded)
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
  "MC_TxnAdmission_Green.cfg TxnAdmission.tla GREEN" # the embedder reaches its transactions as any identity; HTTP only its own
  "MC_TxnAdmission_Red_IdentityOnFfi.cfg TxnAdmission.tla RED" # embedded registry keys by identity, #2335 (INV_FfiReaches)
  "MC_TxnAdmission_Red_EmbedderOnHttp.cfg TxnAdmission.tla RED" # HTTP registry gives every request one key (INV_HttpOwnerOnly)
  "MC_TxnAdmission_Red_ActsAsOwner.cfg TxnAdmission.tla RED" # work runs as the transaction's beginner (INV_ActsAsCaller)
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
  "MC_CpuLedger_Green_Edge.cfg CpuLedger.tla GREEN" # a network runtime claims cores alone beside a pool, or gives up holding nothing
  "MC_CpuLedger_Red_EdgeNoCap.cfg CpuLedger.tla RED" # network runtime claims alone forever: its start never ends (LIVE_StartEnds)
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
  "MC_PushBacklog_Green.cfg MC_PushBacklog_Common.tla GREEN" # #1099: bounded queue + fixed workers + per-peer cap + timeout park, demote on a failed probe -> all bounds hold, healthy peers progress
  "MC_PushBacklog_Green_TwoDead.cfg MC_PushBacklog_Common.tla GREEN" # F24: two dead peers are parked on timeout and share one peer cap -> a worker always remains for the healthy peer
  "MC_PushBacklog_Green_SharedCap.cfg MC_PushBacklog_Common.tla GREEN" # F24 with 3 workers, cap 2: several probes per unreachable peer, a reply mid-probe -> the shared cap still leaves a worker
  "MC_PushBacklog_Red_TwoDead.cfg MC_PushBacklog_Common.tla RED" # F24, code before the fix: a timeout frees the worker and nothing else, two dead peers take the pool back (INV_UnreachableLeaveAWorker)
  "MC_PushBacklog_Red_DemoteEveryTimeout.cfg MC_PushBacklog_Common.tla RED" # F24 review: every timeout demotes the queue, a peer that answers its next send loses it (ACT_DemoteAfterFailedProbe)
  "MC_PushBacklog_Red_SpawnPerItem.cfg MC_PushBacklog_Common.tla RED" # old broadcast: task spawned per (write,peer) before the semaphore -> resident work unbounded (INV_QueueBounded)
  "MC_PushBacklog_Red_PermitLeak.cfg MC_PushBacklog_Common.tla RED" # completing job keeps its worker slot -> pool decays (INV_PermitConservation)
  "MC_PushBacklog_Red_RetainHandles.cfg MC_PushBacklog_Common.tla RED" # SyncShutdownHandle retains every completed JoinHandle (INV_HandlesBounded)
  "MC_PushBacklog_Red_NoPeerCap.cfg MC_PushBacklog_Common.tla RED" # no per-peer cap: the slow peer's sends hold every worker until they time out (INV_PeerLeavesAWorker)
  "MC_PushCoalescing_Green.cfg MC_PushCoalescing_Common.tla GREEN" # #1102: latest head retained while queued predecessors retire
  "MC_PushCoalescing_Red_AppendEvery.cfg MC_PushCoalescing_Common.tla RED" # no live coalescing -> two heads occupy one (doc,peer) queue
  "MC_PushCoalescing_Red_StaleRetry.cfg MC_PushCoalescing_Common.tla RED" # superseded failure registers the marker afresh, taking it from the newest job (INV_NoStaleRetry)
  "MC_PushCoalescing_Green_Siblings.cfg MC_PushCoalescing_Common.tla GREEN" # F1: a live ack clears the marker only for the whole frontier -> siblings reach the ladder
  "MC_PushCoalescing_Red_SiblingClear.cfg MC_PushCoalescing_Common.tla RED" # F1: the newest generation's ack clears the marker over a retired sibling (INV_ClearedScopeDelivered)
  "MC_PushCoalescing_Red_UnfencedAck.cfg MC_PushCoalescing_Common.tla RED" # F1: a sibling's live ack reported without its generation overwrites the ladder's acked frontier (INV_AckRecordCurrent)
  "MC_PendingDagRestart_Green.cfg MC_PendingDagRestart_Common.tla GREEN" # #1099: pending-DAG registrations persisted + restored after crash -> acks stay backed, all docs merge
  "MC_PendingDagRestart_Red_ProcessLocal.cfg MC_PendingDagRestart_Common.tla RED" # process-local registrations: hub crash after success-ack -> silent permanent loss (INV_AckBacked)
  "MC_SyncOwnership_Green.cfg SyncOwnership.tla GREEN" # #1116 stage 3: one head hint transfers durable completion ownership to the receiver
  "MC_SyncOwnership_Green_IrohOrigin.cfg SyncOwnership.tla GREEN" # Iroh binds durable recovery to the signed, routable origin that owns the linked DAG
  "MC_SyncOwnership_Green_Liveness_IrohRelay.cfg SyncOwnership.tla GREEN" # unreachable signed origin remains bound while a durable qualified same-root provider completes recovery
  "MC_SyncOwnership_Green_Liveness_Doc.cfg SyncOwnership.tla GREEN" # document supersession eventually reaches currency and quiescence
  "MC_SyncOwnership_Green_Liveness_Collection.cfg SyncOwnership.tla GREEN" # collection supersession eventually reaches currency and quiescence
  "MC_SyncOwnership_Red_DocOnlyMarkers.cfg SyncOwnership.tla RED" # no collection marker loses a dropped collection-head obligation
  "MC_SyncOwnership_Red_PayloadLedger.cfg SyncOwnership.tla RED" # superseded policy: CID/version delivery state instead of scope markers
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
  "MC_SyncOwnership_Red_DischargeBeforeForward.cfg SyncOwnership.tla RED" # a hub discharges the upstream record before its downstream marker is durable; a crash or failed forward between loses the head
  "MC_SyncOwnership_Red_OwedInMemory.cfg SyncOwnership.tla RED" # a hub forwards only roots its running process remembers merging; a restart before the terminal step loses the head
  "MC_SyncOwnership_Red_EdgeTriggeredCompletion.cfg SyncOwnership.tla RED" # a fast transport failure races ahead of waiter registration and strands the fetch owner
  "MC_SyncOwnership_Red_WorkerSaturatedCompletion.cfg SyncOwnership.tla RED" # saturated spawned workers stop draining the CAR completion needed by the fetch owner
  "MC_SyncOwnership_Red_SharedServeWorkers.cfg SyncOwnership.tla RED" # ownership admission consumes every worker needed to serve receiver-owned CAR recovery
  "MC_SyncOwnership_Red_EagerIdentityLookup.cfg SyncOwnership.tla RED" # durable replicator authority is delayed behind an unnecessary reverse identity challenge
  "MC_SyncOwnership_Red_BlockingHostEvent.cfg SyncOwnership.tla RED" # full event delivery blocks the host command needed to reply and release admitted work
  "MC_SyncOwnership_Red_BusyExhaustion.cfg SyncOwnership.tla RED" # local shared-CID ingest contention is deferred, never counted as terminal provider exhaustion
  "MC_SyncOwnership_Green_MarkerHandoff.cfg SyncOwnership.tla GREEN" # a transactional commit returns after its marker registration; P2P stops only after admitted writes drained
  "MC_SyncOwnership_Red_UndrainedMarker.cfg SyncOwnership.tla RED" # P2P stops with a marker registration pending and the dropped task acknowledges the commit
  "MC_SyncOwnership_Red_ShedAsStall.cfg SyncOwnership.tla RED" # a provider's busy CAR reply is rotated past and deferred, never counted as a stalled window or terminal exhaustion
  "MC_SyncOwnership_Green_Dispatch.cfg MC_SyncOwnership_Dispatch.tla GREEN" # one attempt per fetch slot, re-armed from its end, acked heads kept: due roots go in order and every acked head merges across a restart
  "MC_SyncOwnership_Red_HeldSlot.cfg MC_SyncOwnership_Dispatch.tla RED" # old rule: a fetch holds its slot through every attempt and starts the next attempt ahead of an acked root (ACT_AttemptsStartInDueOrder)
  "MC_SyncOwnership_Red_ClaimTimeRearm.cfg MC_SyncOwnership_Dispatch.tla RED" # next due time fixed at claim: a failed root ends due ahead of roots acked during its attempt (ACT_EndedAttemptQueuesBehind)
  "MC_SyncOwnership_Red_AckedTtl.cfg MC_SyncOwnership_Dispatch.tla RED" # the pending TTL drops an acked head its sender already forgot (ACT_AckedRootLeavesOwned)
  "MC_SyncOwnership_Green_Dispatch_DeadProvider.cfg MC_SyncOwnership_Dispatch.tla GREEN" # a provider that never serves keeps its head but does not stop the other head from merging
  "MC_SyncOwnership_Green_Dispatch_Trickle.cfg MC_SyncOwnership_Dispatch.tla GREEN" # a trickling attempt gives its slot back at its quantum; the other head still merges
  "MC_SyncOwnership_Red_UnboundedProductive.cfg MC_SyncOwnership_Dispatch.tla RED" # no quantum: a trickling attempt holds the only slot for good (LIVE_AckedRootMerged)
  "MC_SyncOwnership_Green_Dispatch_PeerPermits.cfg MC_SyncOwnership_Dispatch.tla GREEN" # a slot is taken with its source peer's permit: a busy peer's root is passed over and the slot goes to another peer's root
  "MC_SyncOwnership_Green_Dispatch_PullPermit.cfg MC_SyncOwnership_Dispatch.tla GREEN" # a pull fetch holds the peer's permit: its root is passed over, then claimed in its turn, and both heads merge
  "MC_SyncOwnership_Red_SlotBeforePermit.cfg MC_SyncOwnership_Dispatch.tla RED" # slot first, permit after: a fetch waits for its peer's permit in a slot another peer's root could use (INV_NoSlotWaitsForPermit)
  "MC_SyncOwnership_Green_Dispatch_Storm.cfg MC_SyncOwnership_Dispatch.tla GREEN" # the clock's own lane, emptied first: an acked head merges while blocked senders keep the merge loop's channel full
  "MC_SyncOwnership_Red_MergeFifoDispatch.cfg MC_SyncOwnership_Dispatch.tla RED" # old rule: the clock try_reserves the merge loop's channel and every freed cell goes to a waiting sender (LIVE_AckedRootMerged)
  "MC_SyncOwnership_Red_BlockingHandlerSend.cfg MC_SyncOwnership_Dispatch.tla RED" # old rule: a handler waits for room in the full channel in its admission lane slot (INV_AdmissionNeverWaitsOnMergeLoop)
  "MC_SyncOwnership_Red_LoopWaitsOnItself.cfg MC_SyncOwnership_Dispatch.tla RED" # old rule: the merge loop completes a root from a latched block-sync completion and waits for room in its own channel (INV_MergeLoopNeverWaitsOnItself)
  "MC_SyncOwnership_Green_Dispatch_LaneTurns.cfg MC_SyncOwnership_Dispatch.tla GREEN" # the lane goes first for a slot's worth of dispatches in a row: a DagReady still merges while a trickling root is re-dispatched at every quantum
  "MC_SyncOwnership_Red_LaneAlwaysFirst.cfg MC_SyncOwnership_Dispatch.tla RED" # the lane always goes first: a root re-dispatched before every receive keeps a DagReady in the channel for good (LIVE_AckedRootMerged)
  "MC_SyncOwnership_Green_Memory.cfg MC_SyncOwnership_Memory.tla GREEN" # durable roots past the in-memory cap take turns; a local continuation ends when it stops yielding
  "MC_SyncOwnership_Red_RetainedOverflow.cfg MC_SyncOwnership_Memory.tla RED" # a never-serving restored root holds the only entry for good (LIVE_DurableRootMerged)
  "MC_SyncOwnership_Red_KeyOrderResync.cfg MC_SyncOwnership_Memory.tla RED" # the sweep restarts at the first key and re-admits the root that gave its entry up (LIVE_DurableRootMerged)
  "MC_SyncOwnership_Red_ImmortalContinuation.cfg MC_SyncOwnership_Memory.tla RED" # a skipped local continuation keeps its entry for ever (LIVE_ContinuationEnds)
  "MC_SyncOwnership_Green_TwoTierQuota.cfg MC_SyncOwnership_Memory.tla GREEN" # a full map admits a head on its durable record alone; a peer's quota keeps its never-serving roots from filling the store
  "MC_SyncOwnership_Red_MemoryOnlyAdmission.cfg MC_SyncOwnership_Memory.tla RED" # a full map nacks a fresh head for ever while the store has room (LIVE_WrittenHeadMerged)
  "MC_SyncOwnership_Red_NoDurablePeerQuota.cfg MC_SyncOwnership_Memory.tla RED" # one peer's never-serving roots fill the durable store and every other peer's head is nacked (LIVE_WrittenHeadMerged)
  "MC_SyncOwnership_Green_Memory_Quota.cfg MC_SyncOwnership_Memory.tla GREEN" # a peer's places go to its roots in turn: a root waiting on its peer's quota takes the place its peer frees
  "MC_SyncOwnership_Red_QuotaPassOver.cfg MC_SyncOwnership_Memory.tla RED" # each place goes to its own root, so a root of the same peer that sorts first re-takes the place its peer frees (LIVE_DurableRootMerged)
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
  "MC_P2PDelivery_Green_Receiver.cfg MC_P2PDelivery_Receiver.tla GREEN" # target contract on a restarted hub with main's #2548 receiver rules: past a dead and a trickling root, a dropped connection, pull fetches, storms and continuations, x merges and stays owed
  "MC_P2PDelivery_Red_F8_SlotBeforePermit.cfg MC_P2PDelivery_Receiver.tla RED" # F8, main: a dispatch parks on the only slot waiting for a permit a pull fetch holds (LIVE_HeadDelivered)
  "MC_P2PDelivery_Red_F9_MergeFifo.cfg MC_P2PDelivery_Receiver.tla RED" # F9, main: dispatch through a merge channel catch-up traffic keeps full (LIVE_HeadDelivered)
  "MC_P2PDelivery_Finding_AdvisoryPermits.cfg MC_P2PDelivery_Receiver.tla RED" # finding, no planted bug: advisory pull fetches can take every global permit ahead of a due durable root (LIVE_HeadDelivered)
  "MC_P2PDelivery_Green_E2E.cfg MC_P2PDelivery_E2E.tla GREEN" # target contract end to end: through a crash and a failed registration or forward, every head stays owed on every edge, no request goes unanswered, and every head reaches every node
  "MC_P2PDelivery_Red_F1_PriorityCover.cfg MC_P2PDelivery_E2E.tla RED" # F1, main: a sibling is covered or retired by (priority, CID) and lost (INV_ObligationOwned)
  "MC_P2PDelivery_Red_F2_MarkerAfterCommit.cfg MC_P2PDelivery_E2E.tla RED" # F2, main: P2P stops before the drain, between a commit and its marker (INV_ObligationOwned)
  "MC_P2PDelivery_Red_F3_DischargeBeforeForward.cfg MC_P2PDelivery_E2E.tla RED" # F3, main: a crash between the hub's discharge and its forward (INV_ObligationOwned)
  "MC_P2PDelivery_Red_F4_LoopFirst.cfg MC_P2PDelivery_E2E.tla RED" # F4, main: a restarted hub merges a pushed head before its replicators are restored (INV_ObligationOwned)
  "MC_P2PDelivery_Red_F5_FanoutAbort.cfg MC_P2PDelivery_E2E.tla RED" # F5, main: a failed registration on the hub drops the intent of the peer not registered yet (INV_ObligationOwned)
  "MC_P2PDelivery_Red_F5_FailedDropped.cfg MC_P2PDelivery_E2E.tla RED" # F5, main: a failed registration's own intent is never retried (INV_ObligationOwned)
  "MC_P2PDelivery_Red_F27_HalfRegistered.cfg MC_P2PDelivery_E2E.tla RED" # F27, main only (the rc1 pin adopts): the accepting side sends on a connection its dialer does not serve (INV_ReverseBorrowServed)
  "MC_P2PDelivery_Trace_TwoNode.cfg MC_P2PDelivery_Trace_TwoNode.tla TRACE:20" # a recorded run, a -> b, create and update: all 19 events are model steps and the run settles
  "MC_P2PDelivery_Trace_HubRelay.cfg MC_P2PDelivery_Trace_HubRelay.tla TRACE:36" # a recorded run, a -> h -> b: all 35 events replay; the hub's merge lays its marker for b
  "MC_P2PDelivery_Trace_Red_Unscheduled.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:14" # the recorded run without its dispatches: a fetch with no slot (deadlock at the first Acquire)
  "MC_P2PDelivery_Trace_Red_PushBeforeMarker.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:11" # a push admitted before the sender's marker exists (deadlock at the Admit)
  "MC_P2PDelivery_Trace_Red_ExpireAfterAck.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:13" # #0b: the TTL drops an acknowledged root and its record (deadlock at the Expire: an acknowledged root never expires)
  "MC_P2PDelivery_Trace_Red_ExpireDropsRecord.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:13" # #0b: the TTL drops the acknowledged root's record too (deadlock at the Expire)
  "MC_P2PDelivery_Trace_Red_CoveredByNothing.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:12" # F1 class: a head acked as covered that no record brings (deadlock at the Admit)
  "MC_P2PDelivery_Trace_Red_RetiresNonAncestor.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:12" # F1 class: an admission retires a head that is no ancestor of it (deadlock at the Admit)
  "MC_P2PDelivery_Trace_Red_AckBeforeAdmit.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:12" # a sender drops its marker on an ack the receiver never gave (deadlock at the Ack)
  "MC_P2PDelivery_Trace_Red_StallHolds.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:16" # #0a: an attempt that got nothing keeps its fetch slot (deadlock at the Stall)
  "MC_P2PDelivery_Trace_Red_SlotBeforePermit.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:13" # F8: a dispatch parks on its slot waiting for permits (deadlock at the Schedule)
  "MC_P2PDelivery_Trace_Red_DialerOnly.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:7" # F27: a connection served only for its dialer (deadlock at the Reconnect)
  "MC_P2PDelivery_Trace_Red_Unsettled.cfg MC_P2PDelivery_Trace_Mutants.tla DEADLOCK:30" # the run ends with a forward intent never registered (deadlock past the end of 29 events)
  "MC_VectorLinks_Green.cfg VectorLinks.tla GREEN"
  "MC_VectorLinks_Red_Puts.cfg VectorLinks.tla RED"
  "MC_VectorLinks_Red_Prune.cfg VectorLinks.tla RED"
  "MC_PendingCache_Green.cfg PendingCache.tla GREEN" # no search hides a pending vector its snapshot holds
  "MC_PendingCache_Green_Evict.cfg PendingCache.tla GREEN" # forgetting the cache mid-search, as a drop does, breaks no promise
  "MC_PendingCache_Red_NoGeneration.cfg PendingCache.tla RED" # a cache of another generation scores truncated entries or lacks what a merge linked
  "MC_PendingCache_Red_TruncateThroughHead.cfg PendingCache.tla RED" # a merge truncating through the head drops vectors it never linked
  "MC_PendingCache_Red_OwnWrites.cfg PendingCache.tla RED" # a search leaves out its own transaction's appends
  "MC_PendingCache_Red_EveryVersion.cfg PendingCache.tla RED" # a search scores an obsolete entry of an updated vector
  "MC_PendingCache_Red_FutureVersion.cfg PendingCache.tla RED" # a search scores an entry past its head from a cache ahead of it
  "MC_PendingCache_Red_HeadBlindMarks.cfg PendingCache.tla RED" # a marker from a search's future hides the entry newest for it
  "MC_PendingCache_Red_NoGapCheck.cfg PendingCache.tla RED" # a copy loaded across a missing entry numbers the rest wrong
  "MC_SharedQuantizer_Green.cfg SharedQuantizer.tla GREEN" # a writer encodes only with its snapshot's codebooks
  "MC_SharedQuantizer_Red_NoTagCheck.cfg SharedQuantizer.tla RED" # a shared quantizer used whatever its tag encodes with a rebuilt index's old codebooks
  "MC_SharedQuantizer_Red_TagPerRequest.cfg SharedQuantizer.tla RED" # a tag drawn once per request names an aborted attempt's codebooks and the retry's
  "MC_SharedQuantizer_Red_OneSlot.cfg SharedQuantizer.tla RED" # one slot: writers on the two sides of a rebuild evict each other's quantizer
  "MC_HeadSet_Green.cfg            HeadSet.tla                  GREEN" # derived heads: writers only write keys naming themselves, so concurrent appends never conflict and siblings form
  "MC_HeadSet_Red_EagerDelete.cfg  HeadSet.tla                  RED"   # eager delete: both writers delete the same observed head key, so regolith refuses one (INV_NoWriteConflict)
  "MC_HeadSet_Red_MarkersOnly.cfg  HeadSet.tla                  RED"   # reclamation that drops a head's markers but keeps its head key resurrects a superseded head (INV_FieldExact)
  "MC_HeadSet_Red_PerKeyScan.cfg   HeadSet.tla                  RED"   # a head scan validated per key: a sweep reclaiming a key the scan yielded aborts an in-flight append (INV_NoWriteConflict)
  "MC_HeadPublication_Green.cfg              HeadPublication.tla GREEN" # the priority index entry records a publication, read validated; a parent held ahead is read by its part of the set
  "MC_HeadPublication_Red_NoRecord.cfg       HeadPublication.tla RED"   # nothing records a publication: a replay of a superseded block makes it a head again (INV_LiveExact)
  "MC_HeadPublication_Red_OwnUnvalidated.cfg HeadPublication.tla RED"   # the record read unchecked: a duplicate publication undoes a child's supersede (INV_LiveExact)
  "MC_HeadPublication_Red_NoPartRead.cfg     HeadPublication.tla RED"   # a parent held ahead without a part read stays ahead after it is published (INV_AheadExact)
  "MC_HeadPublication_Red_WholeSet.cfg       HeadPublication.tla RED"   # touches that says yes to every part: an unrelated publication turns the reader away (INV_NoUnrelatedAbort)
  "MC_InboundAdmission_Green.cfg MC_InboundAdmission_Common.tla GREEN" # #2076: bodies stay within their class budget, a healthy control message is handled, no wait is endless
  "MC_InboundAdmission_Red_SharedBudget.cfg MC_InboundAdmission_Common.tla RED" # one budget for all classes: stuck bulk bodies starve control (LIVE_HealthyHandled)
  "MC_InboundAdmission_Red_SharedDrain.cfg MC_InboundAdmission_Common.tla RED" # pre-#2076: one queue, a blocked completion head starves control (LIVE_HealthyHandled)
  "MC_InboundAdmission_Red_NoShare.cfg MC_InboundAdmission_Common.tla RED" # no per-peer share: one peer holds a whole class (INV_ShareBound)
  "MC_InboundAdmission_Red_PerConnection.cfg MC_InboundAdmission_Common.tla RED" # a share per connection: a peer with two connections holds a whole class (INV_ShareBound)
  "MC_InboundAdmission_Red_ReleaseOnQueue.cfg MC_InboundAdmission_Common.tla RED" # lease dropped on enqueue: memory exceeds the budget (INV_ResidentWithinBudget)
  "MC_InboundAdmission_Red_ReadFirst.cfg MC_InboundAdmission_Common.tla RED" # pre-#2076: body read before admission (INV_ResidentWithinBudget)
  "MC_InboundAdmission_Red_NoDeadline.cfg MC_InboundAdmission_Common.tla RED" # no deadline: a stream waits for budget forever (LIVE_WaitEnds)
  "MC_InboundAdmission_Red_NoReadDeadline.cfg MC_InboundAdmission_Common.tla RED" # no read deadline: a stalled body holds control forever (LIVE_HealthyHandled)
  "MC_InboundAdmission_Red_IdleOnly.cfg MC_InboundAdmission_Common.tla RED" # idle deadline only: a trickling body holds control forever (LIVE_HealthyHandled)
  "MC_GossipOwnership_Green.cfg MC_GossipOwnership_Common.tla GREEN" # #2078: gossip rides the mux connection; a revoked peer has no gossip, gossip never closes a shared connection
  "MC_GossipOwnership_Red_OwnConnection.cfg MC_GossipOwnership_Common.tla RED" # former design: gossip dials its own connection and re-reaches a revoked peer (INV_RevokedHasNoGossip)
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
  "MC_DetachedBegin_Green.cfg DetachedBegin.tla GREEN" # an unobserved begin is abandoned
  "MC_DetachedBegin_Red_NoResultGuard.cfg DetachedBegin.tla RED" # the unobserved transaction leaks
  "MC_GuardedCommit_Green.cfg GuardedCommit.tla GREEN" # the task owns the transaction guard
  "MC_GuardedCommit_Red_CallerOwns.cfg GuardedCommit.tla RED" # a cancelled waiter abandons the running commit
  "MC_SignedRequestDrain_Green.cfg SignedRequestDrain.tla GREEN" # a signed query keeps its permit until its worker ends
  "MC_SignedRequestDrain_Red_TaskDeadline.cfg SignedRequestDrain.tla RED" # the task expires while its worker keeps writing
  "MC_SignedRequestDrain_Red_Transform.cfg SignedRequestDrain.tla RED" # a transformed route escapes admission
  "MC_NodeShutdown_Green.cfg NodeShutdown.tla GREEN" # held tasks stop and are awaited before regolith closes; shutdown returns
  "MC_NodeShutdown_Red_RedriveDetached.cfg NodeShutdown.tla RED" # detached ACP redrive outlives the wait for tasks
  "MC_NodeShutdown_Red_ServicesNotAwaited.cfg NodeShutdown.tla RED" # stopped service still in storage when regolith closes
  "MC_NodeShutdown_Red_NoDrainDeadline.cfg NodeShutdown.tla RED" # hung request keeps shutdown waiting forever
  "MC_NodeShutdown_Red_FlushOnCaller.cfg NodeShutdown.tla RED" # refused flush runs the exporter join on the caller
  "MC_EdgeCompletion_Green.cfg EdgeCompletion.tla GREEN" # abort, stop and finish in any order: every completion says how it ended
  "MC_EdgeCompletion_Red_NoMap.cfg EdgeCompletion.tla RED" # the code before: a cut-off by the runtime's stop says Cancelled (INV_StopSaysClosed)
  "MC_EdgeCompletion_Red_CancelBeforeFlag.cfg EdgeCompletion.tla RED" # the handle cancels before it records its abort: its own abort reads as the stop (INV_AbortSaysCancelled)
  "MC_EdgeCompletion_Red_CancelBeforeStopMark.cfg EdgeCompletion.tla RED" # the runtime ends its tasks before its stopped mark: the cut-off says Cancelled (INV_StopSaysClosed)
  "MC_EdgeGateContinuation_Green.cfg EdgeGateContinuation.tla GREEN" # continuations of admitted work run through the drain, never after it, never from idle
  "MC_EdgeGateContinuation_Red_PlainSpawn.cfg EdgeGateContinuation.tla RED" # the code before (#2546): a request in flight is refused its connection (INV_InFlightContinues)
  "MC_EdgeGateContinuation_Red_NoSeal.cfg EdgeGateContinuation.tla RED" # no seal: a passing count lets a continuation in after the drain (INV_NothingAfterDrain)
  "MC_EdgeGateContinuation_Red_NoCas.cfg EdgeGateContinuation.tla RED" # no compare-and-swap: a continuation starts from an idle closed gate (INV_NoContinuationWhenIdle)
  "MC_IrohDialOutcome_Green.cfg IrohDialOutcome.tla GREEN" # a refused dial answers from its close reason; connect_peer ends with the answer
  "MC_IrohDialOutcome_Green_Admitted.cfg IrohDialOutcome.tla GREEN" # an admitted dial answers ok
  "MC_IrohDialOutcome_Red_Poll.cfg IrohDialOutcome.tla RED" # the code before: connect_peer polls the connected set and misses a refused connection (LIVE_DialAnswered)
  "MC_IrohDialOutcome_Red_NoCheck.cfg IrohDialOutcome.tla RED" # the dial answers ok without the close reason (INV_RefusalKnownIsSaid)
  "MC_ServerDrain_Green.cfg ServerDrain.tla GREEN" # requests in flight finish, idle connections close, subscription streams end, the server returns by the deadline
  "MC_ServerDrain_Red_Service.cfg ServerDrain.tla RED" # the code before: server tasks stopped as services, a request in flight dropped (INV_NoDropBeforeDeadline)
  "MC_ServerDrain_Red_CheckInside.cfg ServerDrain.tla RED" # closing looked at inside a request cuts it off (INV_NoDropBeforeDeadline)
  "MC_ServerDrain_Red_NoCheck.cfg ServerDrain.tla RED" # closing never looked at: new requests while draining (INV_NoNewWorkWhenClosing)
  "MC_ServerDrain_Red_NoDeadline.cfg ServerDrain.tla RED" # no deadline: a hung request keeps the server from returning (LIVE_ServerReturns)
  "MC_ServerDrain_Red_StreamIgnoresClosing.cfg ServerDrain.tla RED" # a subscription stream ignores closing: the drain waits out its deadline for it (INV_DeadlineStopsNoStream)
  "MC_DocTopicIngress_Green.cfg DocTopicIngress.tla GREEN" # updates queued past an unsubscribe are dropped, followed ones applied
  "MC_DocTopicIngress_Red_NoCheck.cfg DocTopicIngress.tla RED" # the code before (#2545): an update after the unsubscribe is applied (INV_NoApplyAfterUnsubscribe)
  "MC_DocTopicIngress_Red_OwnSet.cfg DocTopicIngress.tla RED" # the handler checks a set the unsubscribe never clears (INV_NoApplyAfterUnsubscribe)
  "MC_DocTopicIngress_Red_NoCollection.cfg DocTopicIngress.tla RED" # a followed collection does not admit the document's topic (INV_FollowedApplies)
  "MC_NodeShutdown_Red_RequestsDuringP2P.cfg NodeShutdown.tla RED" # requests still admitted while P2P stops commit without a head marker
  "MC_NodeShutdown_Red_CommitNotAdmitted.cfg NodeShutdown.tla RED" # an in-process commit outside the request scope commits after P2P stopped
  "MC_NodeShutdown_Red_OpenTransaction.cfg NodeShutdown.tla RED" # a transaction left open (a commit refused at shutdown) holds regolith's close for its timeout
  "MC_NodeDrop_Green.cfg NodeDrop.tla GREEN" # a node dropped without shutdown aborts its tasks and releases its pool; the drop waits for none
  "MC_NodeDrop_Red_SweepOutsideSet.cfg NodeDrop.tla RED" # the sweep in a slot of its own outlives a dropped node and keeps its pool alive
  "MC_NodeDrop_Red_NoAbortOnDrop.cfg NodeDrop.tla RED" # a shutdown set that does not abort on drop leaves every task holding the database
  "MC_NodeDrop_Red_DropAwaits.cfg NodeDrop.tla RED" # the dropping thread waits for an aborted task to end
  "MC_NodeStartup_Green.cfg NodeStartup.tla GREEN" # every config starts as asked or fails holding nothing; startup ends
  "MC_NodeStartup_Red_IrohDefaultLadder.cfg NodeStartup.tla RED" # iroh keeps the default retry ladder (INV_OneSchedule)
  "MC_NodeStartup_Red_ListenLeak.cfg NodeStartup.tla RED" # failed listen leaves the host running (INV_FailedHoldsNothing)
  "MC_NodeStartup_Red_LateNacCheck.cfg NodeStartup.tla RED" # NAC owner checked after the host started (INV_FailedHoldsNothing)
  "MC_NodeStartup_Red_NacUnowned.cfg NodeStartup.tla RED" # NAC enabled with no owner (INV_NacAsAsked)
  "MC_NodeStartup_Red_DialBeforeNac.cfg NodeStartup.tla RED" # bootstrap peer dialed before NAC (INV_NoPeerBeforeNac)
  "MC_NodeStartup_Red_BootstrapFatal.cfg NodeStartup.tla RED" # unreachable bootstrap peer fails startup (INV_BootstrapNonFatal)
  "MC_PgWire_Green_Startup.cfg PgWire.tla GREEN" # pg wire opening and startup: one deadline from the start, startup errors close, nothing read after the end, every connection ends
  "MC_PgWire_Green_Protocol.cfg PgWire.tla GREEN" # pg wire after startup: an extended error skips until Sync, the Sync resumes, a simple or extended copy ends on CopyDone, CopyFail or an error
  "MC_PgWire_Red_PeekNoDeadline.cfg PgWire.tla RED" # the reviewed bug, deadline read after the peek: a silent client stays forever (INV_StartupWithinDeadline)
  "MC_PgWire_Red_SslNoDeadline.cfg PgWire.tla RED" # SSL refusals outside the deadline: a client silent after its SSL request stays (INV_StartupWithinDeadline)
  "MC_PgWire_Red_StartupNoDeadline.cfg PgWire.tla RED" # startup reads with no deadline: a DID user that never sends its token stays (INV_StartupWithinDeadline)
  "MC_PgWire_Red_FreshDeadline.cfg PgWire.tla RED" # a new limit per wait: a trickling client runs past the limit from the start (INV_StartupWithinDeadline)
  "MC_PgWire_Red_DeadlineAfterStartup.cfg PgWire.tla RED" # the deadline covers reads after the startup: an idle started client is cut off (INV_StartedNeverTimedOut)
  "MC_PgWire_Red_NoAwaitingSync.cfg PgWire.tla RED" # report never enters AwaitingSync: the next extended message is handled before the Sync (INV_SkipUntilSync)
  "MC_PgWire_Red_HandleWhileSkipping.cfg PgWire.tla RED" # AwaitingSync handles every message: an extended message runs during the skip (INV_SkipUntilSync)
  "MC_PgWire_Red_SyncKeepsSkipping.cfg PgWire.tla RED" # the answered Sync leaves the state AwaitingSync (INV_SyncResumes)
  "MC_PgWire_Red_CopyFlagFromMessage.cfg PgWire.tla RED" # wait_for_sync from the message, not the copy: an Execute copy's error skips no Sync (INV_SkipUntilSync)
  "MC_PgWire_Red_CopyDoneStays.cfg PgWire.tla RED" # a simple copy's CopyDone leaves the state in the copy (INV_CopyEnds)
  "MC_PgWire_Red_ExtendedCopyDoneStays.cfg PgWire.tla RED" # an Execute copy's successful CopyDone stays in the copy and drops the Sync (INV_CopyEnds)
  "MC_PgWire_Red_LoopAfterClose.cfg PgWire.tla RED" # the CI bug, no is_closed check: the loop reads after a fatal error (INV_NothingAfterEnd)
  "MC_PgWire_Red_LoopAfterCloseRuns.cfg PgWire.tla RED" # the CI bug: a query sent after a wrong token runs with nobody logged in (INV_NoQueryBeforeStartup)
  "MC_PgWire_Red_LoopAfterCloseHangs.cfg PgWire.tla RED" # the CI bug: after a fatal error the loop waits for a client that waits for the close (LIVE_Ends)
  "MC_PgWire_Red_FatalStaysOpen.cfg PgWire.tla RED" # a fatal error or a cancel closes nothing: the loop reads on (INV_NothingAfterEnd)
  "MC_PgWire_Red_StartupErrorNotFatal.cfg PgWire.tla RED" # contract broken, a plain startup ERROR: the connection is ready with nobody logged in (INV_NoQueryBeforeStartup)
  "MC_ConnectFallback_Green.cfg ConnectFallback.tla GREEN" # two families under a connect timeout: one socket, the fallback after the delay or a failure, the last error, within the limit, every connect ends
  "MC_ConnectFallback_Green_NoTimeout.cfg ConnectFallback.tla GREEN" # no connect timeout: the OS ends every attempt, and the connect ends with one socket or the last error
  "MC_ConnectFallback_Green_Single.cfg ConnectFallback.tla GREEN" # one family: its attempt alone, under the connect timeout
  "MC_ConnectFallback_Red_NoDelay.cfg ConnectFallback.tla RED" # both families started at once (INV_FallbackWaits)
  "MC_ConnectFallback_Red_WaitAfterFailure.cfg ConnectFallback.tla RED" # an early failure waits the delay out with nothing under way (INV_NeverIdle)
  "MC_ConnectFallback_Red_FirstErrorWins.cfg ConnectFallback.tla RED" # the first failure is the answer while the other still runs (INV_ErrorOnlyWhenAllFail)
  "MC_ConnectFallback_Red_KeepFirstError.cfg ConnectFallback.tla RED" # both fail and the earlier error is answered (INV_LastError)
  "MC_ConnectFallback_Red_KeepLoser.cfg ConnectFallback.tla RED" # the loser left running: its socket stays open after the answer (INV_OneSocket)
  "MC_ConnectFallback_Red_TimeoutMissesRace.cfg ConnectFallback.tla RED" # the connect timeout covers only the lookup: a hanging race runs past it (INV_WithinLimit)
  "MC_ConnectFallback_Red_TimeoutMissesRaceHangs.cfg ConnectFallback.tla RED" # the same, with both attempts hanging: the connect never ends (LIVE_Ends)
  "MC_PubsubRpcReply_Green.cfg PubsubRpcReply.tla GREEN" # three peers, each request copy direct or relayed in every order: every answer reaches the asker, the deadline cuts off only an answer not yet sent or still on its way, the asker stops
  "MC_PubsubRpcReply_Red_AnswerRelay.cfg PubsubRpcReply.tla RED" # a peer answers the relay that handed it the request (INV_NoAnswerLost)
  "MC_PubsubRpcReply_Red_DeadlineCutsOff.cfg PubsubRpcReply.tla RED" # the same: every peer answered, one answer lost, the asker stops at its deadline (INV_AllAnswered)
  "MC_CoordinatorShutdown_Green.cfg CoordinatorShutdown.tla GREEN" # two callers, a late spawn racing the drain, the edge closing and the runtime torn down at any moment: one drain, no task left in the set, every caller returns
  "MC_CoordinatorShutdown_Green_TwoSpawns.cfg CoordinatorShutdown.tla GREEN" # two late spawns whose pruners race the drain and each other: no task left in the set, the caller returns
  "MC_CoordinatorShutdown_Red_DropRefused.cfg CoordinatorShutdown.tla RED" # the old code, a refused drain dropped: the tracked task leaks (INV_NoLeak)
  "MC_CoordinatorShutdown_Red_CheckThenSpawn.cfg CoordinatorShutdown.tla RED" # is_closed() then spawn: the edge closes in between and the drain is dropped (INV_NoLeak)
  "MC_CoordinatorShutdown_Red_NoAbort.cfg CoordinatorShutdown.tla RED" # no abort after the deadline: a task that never ends keeps every caller waiting (LIVE_CallersReturn)
  "MC_CoordinatorShutdown_Red_SenderOutsideDrain.cfg CoordinatorShutdown.tla RED" # the sender survives a cancelled drain: callers park forever (LIVE_CallersReturn)
  "MC_CoordinatorShutdown_Red_TwoWinners.cfg CoordinatorShutdown.tla RED" # the flag read then set: two callers both build a drain (INV_OneDrain)
  "MC_CoordinatorShutdown_Red_NoPrunerWait.cfg CoordinatorShutdown.tla RED" # the drain does not wait for pruners: a handle out with a pruner comes back after the drain (INV_NoLeak)
  "MC_CoordinatorShutdown_Red_PrunerIgnoresFlag.cfg CoordinatorShutdown.tla RED" # the code as it stands, the pruner never reads the flag: a pruner that starts after the drain's check puts a live handle back (INV_NoLeak)
  "MC_CoordinatorShutdown_Red_NoRecheck.cfg CoordinatorShutdown.tla RED" # register_task never reads the flag again: a handle pushed after the drain runs on (INV_NoLeak)
  "MC_DocumentSave_Green.cfg DocumentSave.tla GREEN" # a save refuses a deleted document and never forks
  "MC_DocumentSave_Red_NoDeletedCheck.cfg DocumentSave.tla RED" # a deleted document is written over and forks (INV_NoFork)
  "MC_DocumentSave_Red_SplitTxn.cfg DocumentSave.tla RED" # a delete between lookup and write forks the save (INV_NoFork)
)

parallelism="${TLA_JOBS:-4}"
case "$parallelism" in
  ''|*[!0-9]*) echo "TLA_JOBS must be a positive integer" >&2; exit 2 ;;
esac
if [ "$parallelism" -lt 1 ]; then
  echo "TLA_JOBS must be a positive integer" >&2
  exit 2
fi
if ! workdir=$(mktemp -d "${TMPDIR:-/tmp}/defra-tla.XXXXXX"); then
  echo "Could not create TLA scratch directory" >&2
  exit 1
fi
trap 'rm -rf "$workdir"' EXIT

# The verdict of a trace replay: TRACE:<depth> when TLC found no error from
# exactly one initial state, DEADLOCK:<l> with the index l of the event the
# last state of the deadlocked behaviour waits on, ERROR otherwise.
trace_verdict() {
  local out="$1" depth last
  depth=$(sed -n 's/^The depth of the complete state graph search is \([0-9]*\)\.$/\1/p' <<<"$out")
  if grep -q "No error has been found" <<<"$out" &&
     grep -q "^Finished computing initial states: 1 distinct state generated" <<<"$out"; then
    echo "TRACE:$depth"
  elif grep -q "Deadlock reached" <<<"$out"; then
    last=$(grep -E '^/\\ l = [0-9]+$' <<<"$out" | tail -n 1)
    echo "DEADLOCK:${last##* }"
  else
    echo ERROR
  fi
}

run_model() {
  local index="$1" row="$2" cfg mod want out got
  read -r cfg mod want <<<"$row"
  mkdir -p "$workdir/java-$index"
  out=$(TLA_JAVA_TMPDIR="$workdir/java-$index" ./tools/tlc -metadir "$workdir/state-$index" -config "$cfg" "$mod" 2>&1)
  # Here-strings, not pipes: `grep -q` stops at the first match, and under
  # pipefail the writer's broken pipe would turn a long trace into ERROR.
  case "$want" in
    TRACE:*|DEADLOCK:*) got=$(trace_verdict "$out") ;;
    *)
      if grep -q "No error has been found" <<<"$out"; then got=GREEN
      elif grep -qE "is violated|was violated|properties were violated|Deadlock reached|Deadlock" <<<"$out"; then got=RED
      else got=ERROR; fi ;;
  esac
  if [ "$got" = "$want" ]; then
    printf "  ok   %-6s %-34s %s\n" "$got" "$cfg" "$mod"
  else
    printf "  FAIL want=%s got=%s  %-30s %s\n%s\n" "$want" "$got" "$cfg" "$mod" "$out"
    return 1
  fi
}

run_indexed() {
  local index="${1%% *}"
  run_model "$index" "${1#* }" >"$workdir/$index.result" 2>&1 || touch "$workdir/$index.fail"
}
export workdir
export -f run_model trace_verdict run_indexed

# A rolling pool: a lane takes the next run as soon as its last one ends, so
# one slow model never holds the other lanes idle the way a fixed batch did.
n=${#RUNS[@]}
for index in "${!RUNS[@]}"; do printf '%s %s\n' "$((index+1))" "${RUNS[$index]}"; done |
  xargs -P "$parallelism" -I{} bash -c 'run_indexed "$1"' _ {}

fails=0
for ((index=1; index<=n; index++)); do
  if [ ! -e "$workdir/$index.result" ] || [ -e "$workdir/$index.fail" ]; then fails=$((fails+1)); fi
  cat "$workdir/$index.result" 2>/dev/null || echo "  FAIL run $index never ran: ${RUNS[$((index-1))]}"
done

echo "----"
if [ "$fails" -eq 0 ]; then echo "all $n TLA+ runs matched their expected verdict"; else echo "$fails/$n MISMATCHED"; fi
exit "$((fails > 0))"
