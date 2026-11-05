-------------------- MODULE MC_P2PDelivery_Trace_Mutants --------------------
\* The recorded hub relay run, each copy with one defect planted in it, so
\* the replay is seen to refuse what the code must never do. Every mutant
\* fails with a deadlock at the event the model forbids, or past the end
\* when the run never settles; run-all.sh checks the index it stops at, so
\* a mutant that slips past its planted event and fails later is caught.
EXTENDS MC_P2PDelivery_Trace_HubRelay

\* The index of the first event of trace s that P holds for.
FirstWhere(s, P(_)) == CHOOSE i \in 1..Len(s) : P(s[i]) /\ \A k \in 1..(i - 1) : ~P(s[k])
\* Trace s without its i-th event.
Drop(s, i) == SubSeq(s, 1, i - 1) \o SubSeq(s, i + 1, Len(s))
\* Trace s with event x put right after its i-th event.
InsertAfter(s, i, x) == SubSeq(s, 1, i) \o <<x>> \o SubSeq(s, i + 1, Len(s))

\* The first push admitted as a durable record: h admits h1 from a.
FirstAdmit == FirstWhere(tTrace, LAMBDA x : x.ev = "Admit" /\ x.outcome = "registered")
\* The first marker registration: a registers h1 for h.
FirstRegister == FirstWhere(tTrace, LAMBDA x : x.ev = "Register")
\* The hub's merge of the update h2.
HubMergesUpdate == FirstWhere(tTrace, LAMBDA x : x.ev = "Merge" /\ x.node = "h" /\ x.head = "h2")
\* The first connection to come up: a's to h.
FirstReconnect == FirstWhere(tTrace, LAMBDA x : x.ev = "Reconnect")
\* The first dispatch: h gives h1 a fetch slot.
FirstSchedule == FirstWhere(tTrace, LAMBDA x : x.ev = "Schedule")
\* The first fetch to take its permits: h's for h1.
FirstAcquire == FirstWhere(tTrace, LAMBDA x : x.ev = "Acquire")
\* The first acknowledgement a sender acts on: a's for h1.
FirstAck == FirstWhere(tTrace, LAMBDA x : x.ev = "Ack")

\* A merge with no fetch slot: the receivers merge outside the bounded pool.
mUnscheduled == SelectSeq(tTrace, LAMBDA x : x.ev # "Schedule")
\* The sender registers its marker only after the receiver admitted the push.
mPushBeforeMarker ==
  InsertAfter(Drop(tTrace, FirstRegister), FirstAdmit - 1, tTrace[FirstRegister])
\* Right after the hub acknowledges h1, the pending TTL drops it with its record.
mExpireAfterAck ==
  InsertAfter(tTrace, FirstAdmit,
              [tTrace[FirstAdmit] EXCEPT !.ev = "Expire", !.peer = "none", !.outcome = "none", !.mode = "removed"])
\* The hub acknowledges h1 as covered, though no durable record brings it.
mCoveredByNothing == [tTrace EXCEPT ![FirstAdmit].outcome = "covered"]
\* The run stops once the hub merged h2: its forward intent never became a
\* marker, and b never got h2.
mUnsettled == SubSeq(tTrace, 1, HubMergesUpdate)
\* a's connection to h comes up dialed by a and served only for a.
mDialerOnly == [tTrace EXCEPT ![FirstReconnect].mode = "dialer"]
\* The hub's dispatch of h1 takes its slot before its permits are free.
mSlotBeforePermit == [tTrace EXCEPT ![FirstSchedule].mode = "wait"]
\* The hub's first attempt for h1 gets nothing and keeps its fetch slot.
mStallHolds ==
  InsertAfter(tTrace, FirstAcquire, [tTrace[FirstAcquire] EXCEPT !.ev = "Stall", !.mode = "hold"])
\* a acts on h's acknowledgement of h1 before h admitted the push.
mAckBeforeAdmit == InsertAfter(Drop(tTrace, FirstAck), FirstAdmit - 1, tTrace[FirstAck])
\* The hub's admission of h1 retires h2, which is no ancestor of h1.
mRetiresNonAncestor == [tTrace EXCEPT ![FirstAdmit].sup = {"h2"}]
=============================================================================
