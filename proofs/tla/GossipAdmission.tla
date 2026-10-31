---- MODULE GossipAdmission ----
\* The story: peers write documents and announce each one exactly once over
\* gossip. Gossip has no reply and no retry: a peer never says it again, and
\* a write-once document never gets a later head that would announce it anew.
\* Our node hears each announcement at its gossip edge and must either take it
\* on now (a slot in the in-memory pending map, which is small) or refuse it,
\* because the peer is over its rate, the workers are saturated, or the map is
\* full. Our node can also crash and restart between announcements.
\*
\* Who acts: the peers announcing, our gossip edge admitting or refusing, the
\* resync sweep moving durable records into free map slots, the fetch and merge
\* path finishing a slotted document, and a crash wiping memory.
\*
\* What must never happen: an announcement our node heard is forgotten: owned
\* by nothing that survives a crash, and never reported to the application.
\* With budgets at least the burst, nothing is even reported: all is kept.
\* What must always happen: every announced document is merged, or reported.
\*
\* The fix: a refused announcement is written as a durable pending record
\* (root, document, collection, origin peer) instead of being dropped. The
\* resync sweep promotes it once a slot is free, and the fetch path fetches its
\* root from the origin when the block never arrived. Durable deferrals are
\* bounded by a budget per origin peer and a budget for all peers together;
\* past either, the announcement is dropped and reported as a node event that
\* names the peer and the root, so the application can resync the document.
\*
\* Code it models (crates/p2p/src/sync):
\*   coordinator/event_handler/mod.rs     handle_transport_event, handle_transport_event_with_admission  (rate and saturation refusals)
\*   coordinator/event_handler/gossip.rs  handle_gossip_message, defer_refused_gossip  (capacity refusal, durable deferral)
\*   manager/process/gossip_deferral.rs   defer_gossip_head  (the budgets and the durable-only record)
\*   manager/process/gossip_deferral.rs   drop_gossip_head  (the reported drop past a budget)
\*   manager/process/gossip_deferral.rs   pending_has_room_for  (the sweep's room check before its block walk)
\*   manager/process/pending_dag.rs       resync_persisted_pending_dags  (promotion into free slots)
\*   coordinator/mod.rs                   run_pending_dag_resync  (wakes the sweep when a slot frees)
\* Budgets count records here; the code counts their bytes, which the
\* records' measured footprint turns into the same bound.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Docs,        \* the documents the peers write, each announced once
  Peers,       \* the peers that announce them
  PeerOf,      \* [Docs -> Peers] the origin peer of each document
  MemCap,      \* slots in the in-memory pending map (the per-peer quota)
  PeerCap,     \* durable records one origin peer may hold (its byte budget)
  TotalCap,    \* durable records all peers may hold together (the total budget)
  MaxCrashes,  \* how many times our node may crash in one run
  Mode,        \* "Durable" (the fix) | "SilentShed" | "Drop" (main) | "MemoryDefer"
  Promote      \* TRUE: the resync sweep promotes durable records; FALSE: it never does

\* There is something to announce, from somebody.
ASSUME Docs # {} /\ Peers # {}
\* Every document has one origin peer.
ASSUME PeerOf \in [Docs -> Peers]
\* The map has at least one slot.
ASSUME MemCap \in Nat /\ MemCap >= 1
\* The budgets are numbers of records.
ASSUME PeerCap \in Nat /\ TotalCap \in Nat
\* Crashes are a bounded count, so the state space stays finite.
ASSUME MaxCrashes \in Nat
\* The refusal handling is one of the four we compare.
ASSUME Mode \in {"Durable", "SilentShed", "Drop", "MemoryDefer"}
\* Promotion is on or off.
ASSUME Promote \in BOOLEAN

VARIABLES
  sent,      \* documents the peers have announced (never announced again)
  inflight,  \* announcements heard at the edge and not yet decided
  mem,       \* documents holding an in-memory pending slot
  durable,   \* documents with a durable pending record
  sideq,     \* documents parked in an in-memory side queue (MemoryDefer only)
  merged,    \* documents merged on our node
  reported,  \* documents dropped past a budget and reported as a node event
  crashes    \* crashes so far

\* Every variable, for stuttering steps.
vars == <<sent, inflight, mem, durable, sideq, merged, reported, crashes>>

\* The durable records whose origin is peer p.
DurableOf(p) == {d \in durable : PeerOf[d] = p}

\* Room for one more deferred record from peer p, under both budgets.
HasRoom(p) == Cardinality(DurableOf(p)) < PeerCap /\ Cardinality(durable) < TotalCap

\* Each variable holds the kind of value it should, and the slot bound holds.
TypeOK ==
  \* Announced and in-flight are documents, in-flight ones were announced.
  /\ sent \subseteq Docs /\ inflight \subseteq sent
  \* The pending map holds documents and never more than its slots.
  /\ mem \subseteq Docs /\ Cardinality(mem) <= MemCap
  \* The store and the side queue hold documents.
  /\ durable \subseteq Docs /\ sideq \subseteq Docs
  \* Merged and reported are documents.
  /\ merged \subseteq Docs /\ reported \subseteq Docs
  \* The crash count is a number.
  /\ crashes \in Nat

\* Nothing is announced, held or merged at the start.
Init ==
  \* The peers have said nothing yet.
  /\ sent = {} /\ inflight = {}
  \* Our node holds nothing.
  /\ mem = {} /\ durable = {} /\ sideq = {}
  \* Nothing merged, nothing reported, no crash yet.
  /\ merged = {} /\ reported = {} /\ crashes = 0

\* A peer announces a document, once: a document already announced is never
\* announced again (gossip keeps no retry ledger).
Announce(d) ==
  \* Not announced before.
  /\ d \notin sent
  \* It is now announced, and our edge has heard it.
  /\ sent' = sent \cup {d}
  /\ inflight' = inflight \cup {d}
  \* Nothing else moves.
  /\ UNCHANGED <<mem, durable, sideq, merged, reported, crashes>>

\* The edge takes the announcement on now: a free map slot, and its durable
\* record (an admitted head is persisted before it counts as owned; pushed
\* registrations are bounded by the slots, not the deferral budgets).
Admit(d) ==
  \* The announcement is waiting at the edge, and a map slot is free.
  /\ d \in inflight
  /\ Cardinality(mem) < MemCap
  \* It takes the slot and gets its record; the edge is done with it.
  /\ mem' = mem \cup {d}
  /\ durable' = durable \cup {d}
  /\ inflight' = inflight \ {d}
  \* Nothing else moves.
  /\ UNCHANGED <<sent, sideq, merged, reported, crashes>>

\* The edge refuses the announcement. The rate limiter or a saturated worker
\* pool may refuse at any moment, and a full map refuses too, so this step is
\* always open for a waiting announcement. What happens next is the Mode.
Refuse(d) ==
  \* The announcement is waiting at the edge; the edge finishes with it.
  /\ d \in inflight
  /\ inflight' = inflight \ {d}
  \* The rest depends on how a refusal is handled.
  /\ CASE Mode \in {"Durable", "SilentShed"} ->
            \* The fix: a durable record while both budgets have room...
            IF HasRoom(PeerOf[d])
            THEN /\ durable' = durable \cup {d}
                 /\ UNCHANGED <<reported, sideq>>
            \* ...and past one, a reported drop; SilentShed drops it unreported.
            ELSE /\ reported' = IF Mode = "Durable" THEN reported \cup {d} ELSE reported
                 /\ UNCHANGED <<durable, sideq>>
       [] Mode = "Drop" ->
            \* Main: the announcement is simply gone.
            UNCHANGED <<durable, sideq, reported>>
       [] Mode = "MemoryDefer" ->
            \* A side queue in memory, under the same budgets, reported past them.
            IF Cardinality(sideq) < TotalCap
            THEN /\ sideq' = sideq \cup {d}
                 /\ UNCHANGED <<durable, reported>>
            ELSE /\ reported' = reported \cup {d}
                 /\ UNCHANGED <<durable, sideq>>
  \* Nothing else moves.
  /\ UNCHANGED <<sent, mem, merged, crashes>>

\* The resync sweep moves a durable record with no slot into a free slot.
PromoteDurable(d) ==
  \* The sweep is in use.
  /\ Promote
  \* The document has a record, no slot, and is not merged.
  /\ d \in durable \ (mem \cup merged)
  \* A slot is free.
  /\ Cardinality(mem) < MemCap
  \* It takes the slot; its record stays.
  /\ mem' = mem \cup {d}
  \* Nothing else moves.
  /\ UNCHANGED <<sent, inflight, durable, sideq, merged, reported, crashes>>

\* The side queue's drain moves a parked document into a free slot, writing
\* its durable record as an admission would.
PromoteSide(d) ==
  \* The drain is in use.
  /\ Promote
  \* The document is parked.
  /\ d \in sideq
  \* A slot is free.
  /\ Cardinality(mem) < MemCap
  \* It leaves the queue, takes a slot and gets its record.
  /\ sideq' = sideq \ {d}
  /\ mem' = mem \cup {d}
  /\ durable' = durable \cup {d}
  \* Nothing else moves.
  /\ UNCHANGED <<sent, inflight, merged, reported, crashes>>

\* The fetch path completes a slotted document (fetching its root from the
\* origin if the block never arrived) and merges it; the slot and the record go.
Resolve(d) ==
  \* The document holds a slot.
  /\ d \in mem
  \* It merges; its slot and record are released.
  /\ merged' = merged \cup {d}
  /\ mem' = mem \ {d}
  /\ durable' = durable \ {d}
  \* Nothing else moves.
  /\ UNCHANGED <<sent, inflight, sideq, reported, crashes>>

\* Our node crashes and restarts. Memory is wiped: map slots and the side
\* queue are gone, durable records survive. A crash is taken between
\* announcements: an announcement lost on the wire while the node is down
\* is the node being offline, which gossip never promised to cover.
Crash ==
  \* Crashes left, and nothing waiting at the edge.
  /\ crashes < MaxCrashes
  /\ inflight = {}
  \* Memory is wiped.
  /\ mem' = {}
  /\ sideq' = {}
  \* One more crash.
  /\ crashes' = crashes + 1
  \* The store, the merged state and the reports survive.
  /\ UNCHANGED <<sent, inflight, durable, merged, reported>>

\* Every document is merged or reported: the run is over.
Done == sent = Docs /\ inflight = {} /\ merged \cup reported = Docs

\* One step of the system.
Next ==
  \* Some document takes a step...
  \/ \E d \in Docs :
       \/ Announce(d) \/ Admit(d) \/ Refuse(d)
       \/ PromoteDurable(d) \/ PromoteSide(d) \/ Resolve(d)
  \* ...or the node crashes...
  \/ Crash
  \* ...or the run is over and nothing changes.
  \/ (Done /\ UNCHANGED vars)

\* The behaviours: start in Init, take Next steps.
Spec == Init /\ [][Next]_vars

\* Fairness: the peers finish announcing, the edge decides every
\* announcement, the sweep keeps promoting, the fetch path keeps resolving.
FairSpec ==
  /\ Spec
  /\ \A d \in Docs :
       \* Announcing and deciding keep happening.
       /\ WF_vars(Announce(d))
       /\ WF_vars(Admit(d) \/ Refuse(d))
       \* Promotion and resolution keep happening.
       /\ WF_vars(PromoteDurable(d))
       /\ WF_vars(PromoteSide(d))
       /\ WF_vars(Resolve(d))

\* The type invariant, by name for the configs.
INV_TypeOK == TypeOK

\* Every decided announcement is owned by something that survives a crash
\* (or by the side queue, which the MemoryDefer RED shows does not), or is
\* merged. Holds when the budgets are at least the burst.
INV_HeardIsOwned ==
  \A d \in sent \ inflight : d \in merged \cup durable \cup sideq

\* Past a budget, every decided announcement is owned as above or was
\* reported to the application: none is lost in silence.
INV_HeardIsOwnedOrReported ==
  \A d \in sent \ inflight : d \in merged \cup durable \cup sideq \cup reported

\* With budgets at least the burst, nothing is dropped at all.
INV_NothingDropped == reported = {}

\* Every announced document is merged in the end.
LIVE_AllMerged == <>(merged = Docs)

\* Every announced document is merged or reported in the end.
LIVE_AllSettled == <>(merged \cup reported = Docs)
====
