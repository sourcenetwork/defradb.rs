-------------------- MODULE MC_P2PDelivery_Trace_Mutants --------------------
\* The recorded hub relay run, each copy with one defect planted in it, so
\* the replay is seen to refuse what the code must never do. Every mutant
\* fails with a deadlock at the event the model forbids, or past the end
\* when the run never settles.
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
=============================================================================
