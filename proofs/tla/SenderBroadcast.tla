---- MODULE SenderBroadcast ----
\* The story: our node commits writes, and each committed write (a batch, a
\* create, an update, a delete) must announce its heads over gossip so peers
\* that follow its collection hear about it. Gossip has no retry, and a peer
\* that only follows gossip holds no durable marker on our node, so an
\* announcement our node never sends is never sent at all.
\*
\* An announcement first waits out a short coalescing window (a newer head of
\* the same document may replace it), then publishes. Two bounds hold the
\* work in: a byte budget for all announcements waiting to publish (the
\* pending set), and a few publish slots.
\*
\* Who acts: the writers committing, a committed write taking room in the
\* pending set, the coalescing window ending, an announcement taking a
\* publish slot, a publish finishing, and the deadline passing on a write
\* still waiting for room or for a slot.
\*
\* What must never happen: a committed write's announcement is forgotten
\* (neither pending, nor published, nor reported to the application as
\* dropped); the pending set grows past its budget; a publish slot is held
\* through a coalescing window, which is what capped gossip near 128 writes
\* a second.
\* What must always happen: every committed write is published, or reported.
\*
\* The fix (#2489): a committed write waits for room in the pending set, the
\* window runs without a slot, and the slot is taken only for the publish.
\* Each wait has a deadline; when it passes, the announcement is dropped,
\* counted and reported as the node event `p2p-broadcast-dropped`.
\*
\* Code it models:
\*   crates/p2p/src/sync/coordinator/mod.rs  spawn_non_authoritative_broadcast_task  (wait for room, deadline)
\*   crates/p2p/src/sync/coordinator/broadcast_room.rs  BroadcastRoom::reserve  (the pending set's byte budget)
\*   crates/p2p/src/sync/coordinator/broadcast.rs  broadcast_local_update_with_creator  (the slot after the window)
\*   crates/p2p/src/sync/coordinator/mod.rs  publish_slot, acquire_non_authoritative_broadcast_slot  (the publish slots)
\*   crates/db/src/merge/broadcast_mutator/broadcast.rs  spawn_gossip_or_report, broadcast_with_retry_with_creator  (the reports)
\* Room counts announcements here; the code counts their bytes, which the
\* same bound covers.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Writes,           \* the writes our node commits, each announced once
  Room,             \* how many announcements the pending set holds
  Slots,            \* how many publishes may run at once
  Shed,             \* TRUE: a full pending set drops the work at once (main)
  ReportExpiry,     \* TRUE: a wait that passes its deadline is reported
  SlotAfterWindow,  \* TRUE: the slot is taken after the window, not before
  BoundRoom         \* TRUE: a write waits for room in the pending set

\* There is something to commit.
ASSUME Writes # {}
\* The pending set and the slots each hold at least one.
ASSUME Room \in Nat /\ Room >= 1 /\ Slots \in Nat /\ Slots >= 1
\* The four guards are each on or off.
ASSUME {Shed, ReportExpiry, SlotAfterWindow, BoundRoom} \subseteq BOOLEAN

\* Where one write's announcement is.
Phases == {
  "none",         \* not committed yet
  "roomwait",     \* committed, waiting for room in the pending set
  "window",       \* pending, in its coalescing window, holding no slot
  "slotwait",     \* pending, its window over, waiting for a slot
  "slotwindow",   \* pending, holding a slot through its window (the bug)
  "publishing",   \* pending, holding a slot, publishing
  "published",    \* announced over gossip
  "reported",     \* dropped past a deadline, reported as a node event
  "lost"          \* dropped and never reported
}

VARIABLES
  phase  \* [Writes -> Phases] where each write's announcement is

\* Every variable, for stuttering steps.
vars == <<phase>>

\* The writes whose announcement takes room in the pending set.
Pending == {w \in Writes : phase[w] \in {"window", "slotwait", "slotwindow", "publishing"}}

\* The writes whose announcement holds a publish slot.
Slotted == {w \in Writes : phase[w] \in {"slotwindow", "publishing"}}

\* The pending set has no room left.
RoomFull == Cardinality(Pending) >= Room

\* Every slot is taken.
SlotsFull == Cardinality(Slotted) >= Slots

\* What a wait that passed its deadline becomes.
Expired == IF ReportExpiry THEN "reported" ELSE "lost"

\* Each write is in one known phase.
TypeOK == phase \in [Writes -> Phases]

\* Nothing has been committed.
Init == phase = [w \in Writes |-> "none"]

\* A writer commits write w; its announcement waits for room.
Commit(w) ==
  \* The write was not committed before.
  /\ phase[w] = "none"
  \* Now it waits for room.
  /\ phase' = [phase EXCEPT ![w] = "roomwait"]

\* A waiting announcement takes room and starts its coalescing window. The
\* fix waits while the set is full; without the bound it never waits.
Reserve(w) ==
  \* It waits for room.
  /\ phase[w] = "roomwait"
  \* There is room, or nothing bounds the set.
  /\ (~BoundRoom \/ ~RoomFull)
  \* The fix starts the window holding no slot; the bug takes a slot first.
  /\ IF SlotAfterWindow
       \* Into the window with no slot.
       THEN phase' = [phase EXCEPT ![w] = "window"]
       \* A slot first, so it must be free.
       ELSE /\ ~SlotsFull
            \* Through the window holding the slot.
            /\ phase' = [phase EXCEPT ![w] = "slotwindow"]

\* The bug on main: an announcement that finds the pool full is dropped at
\* once, with no wait and no report.
ShedFull(w) ==
  \* Only the shedding code does this.
  /\ Shed
  \* It waits for room.
  /\ phase[w] = "roomwait"
  \* The pending set is full.
  /\ RoomFull
  \* It is gone, and nobody is told.
  /\ phase' = [phase EXCEPT ![w] = "lost"]

\* The deadline passes while an announcement still waits on a full set.
ExpireRoom(w) ==
  \* The shedding code never waits, so it has no deadline.
  /\ ~Shed
  \* It waits for room.
  /\ phase[w] = "roomwait"
  \* The set is still full.
  /\ RoomFull
  \* It stops waiting: reported by the fix, forgotten by the broken variant.
  /\ phase' = [phase EXCEPT ![w] = Expired]

\* The coalescing window ends; the announcement now needs a slot.
EndWindow(w) ==
  \* It is in its window without a slot.
  /\ phase[w] = "window"
  \* It waits for a slot.
  /\ phase' = [phase EXCEPT ![w] = "slotwait"]

\* The window ends for an announcement that already holds a slot (the bug).
EndSlotWindow(w) ==
  \* It holds a slot through its window.
  /\ phase[w] = "slotwindow"
  \* It publishes.
  /\ phase' = [phase EXCEPT ![w] = "publishing"]

\* An announcement whose window ended takes a free slot.
AcquireSlot(w) ==
  \* It waits for a slot.
  /\ phase[w] = "slotwait"
  \* A slot is free.
  /\ ~SlotsFull
  \* It publishes.
  /\ phase' = [phase EXCEPT ![w] = "publishing"]

\* The deadline passes while an announcement still waits for a slot.
ExpireSlot(w) ==
  \* It waits for a slot.
  /\ phase[w] = "slotwait"
  \* Every slot is still taken.
  /\ SlotsFull
  \* It stops waiting: reported by the fix, forgotten by the broken variant.
  /\ phase' = [phase EXCEPT ![w] = Expired]

\* A publish finishes; its slot and its room free.
Publish(w) ==
  \* It is publishing.
  /\ phase[w] = "publishing"
  \* Its announcement went out.
  /\ phase' = [phase EXCEPT ![w] = "published"]

\* Every write committed and settled: the run is over.
Done ==
  \* No write is uncommitted or in flight.
  /\ \A w \in Writes : phase[w] \in {"published", "reported", "lost"}
  \* And it stays that way.
  /\ UNCHANGED vars

\* One step: some actor acts, or the run is over.
Next ==
  \* A commit, a reservation, a shed, a deadline, a window ending, a slot or a publish.
  \/ \E w \in Writes :
       \/ Commit(w) \/ Reserve(w) \/ ShedFull(w) \/ ExpireRoom(w)
       \/ EndWindow(w) \/ EndSlotWindow(w) \/ AcquireSlot(w) \/ ExpireSlot(w)
       \/ Publish(w)
  \* Or the run is over.
  \/ Done

\* Writers keep committing, free room and free slots get taken, windows end
\* and publishes finish; deadlines need not pass, so they have no fairness.
Fairness ==
  \A w \in Writes :
    \* Each write is committed eventually.
    /\ WF_vars(Commit(w))
    \* A waiting announcement takes room that stays free.
    /\ WF_vars(Reserve(w))
    \* A coalescing window ends.
    /\ WF_vars(EndWindow(w))
    \* So does one held with a slot.
    /\ WF_vars(EndSlotWindow(w))
    \* A waiting announcement takes a slot that stays free.
    /\ WF_vars(AcquireSlot(w))
    \* A publish finishes.
    /\ WF_vars(Publish(w))

\* The whole behaviour.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Invariant: no committed write's announcement is forgotten.
INV_CommittedIsOwnedOrReported == \A w \in Writes : phase[w] # "lost"

\* Invariant: the pending set never holds more than its budget.
INV_PendingBounded == Cardinality(Pending) <= Room

\* Invariant: no announcement holds a publish slot through its window.
INV_WindowHoldsNoSlot == \A w \in Writes : phase[w] # "slotwindow"

\* Liveness: every committed write is eventually published or reported.
LIVE_AllSettled ==
  \A w \in Writes : <>(phase[w] \in {"published", "reported"})
====
