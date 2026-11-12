--------------------------- MODULE PendingDagOwner ---------------------------
\* Story. A receiver keeps a bounded registry of pending-DAG work: at most Cap
\* roots may hold a slot at once. The retry clock reserves a slot for a due
\* root; then a fetch task, or a merge of a queued continuation, claims the
\* reservation by creating an owner with a fresh generation and installing that
\* generation in the root's entry. Two claimants can race for one reservation:
\* only one installs, and the loser's owner is dropped at once. Dropping an
\* owner (work finished, failed, panicked or was cancelled) releases the slot,
\* but only if the entry still holds that owner's own generation, and then
\* wakes the retry clock so a root waiting for room is dispatched now instead
\* of on the clock's next tick. The clock's dispatch can also meet a full event
\* lane: it hands the reservation back and waits for its tick, without waking
\* itself, because nothing it could do sooner would find room.
\* Never: more roots working than slots (a loser's drop releasing the winner's
\* slot), the clock asleep with no wake owed while a due root could take a
\* free slot, or the clock waking itself after a full lane and spinning
\* through dispatches that cannot make progress.
\* Rust: crates/p2p/src/sync/coordinator/mod.rs PendingDagWorkOwner::drop (the
\* generation check and the capacity wake), SyncShutdownHandle::reserve_pending_dag_fetch,
\* spawn_pending_dag_fetch and claim_pending_dag_merge (the claim race),
\* SyncCoordinator::dispatch_due_pending_dag_fetches with
\* release_pending_dag_fetch_reservation (the full-lane rollback, which does
\* not notify), and SyncCoordinator::run_pending_dag_retry_clock (the wait the
\* wake ends).
\* The tick is left out on purpose: the model asks whether freed room is used
\* before the next tick, so a clock that only ticks counts as asleep.
EXTENDS Naturals, FiniteSets
\* Bug: "none" keeps every guard; "no-generation" lets any owner of the root
\* release its entry; "no-wake" releases the slot without waking the clock;
\* "rollback-wake" makes a full-lane rollback notify the clock itself.
\* Cap is the slot count (max_concurrent_dag_fetches); MaxGen bounds owners made.
CONSTANTS Bug, Cap, MaxGen
\* Two roots are enough for one to wait while the other holds the only slot.
Roots == {1, 2}
\* Generation numbers an owner can be given.
Gens == 1..MaxGen
\* An entry value meaning the root holds no slot.
Free == 0
\* An entry value meaning the slot is reserved and not yet claimed.
Scheduled == MaxGen + 1
\* At most this many full-lane rollbacks, to keep the explored run finite.
MaxRollbacks == 3
\* entry[r]: Free, Scheduled, or the generation of the owner whose work holds the slot.
\* due: roots waiting for the clock to dispatch them.
\* arrived: how many times each root has become due, to bound the run.
\* next: the next generation to hand out.
\* oroot[g] / ostate[g]: owner g's root and its life: "none" not made,
\*   "made" created and racing to install, "working" installed and running,
\*   "lost" lost the race, "dropped" its destructor ran.
\* asleep: the retry clock is waiting in its select.
\* woken: a capacity-release or ready notification the clock has not yet taken
\*   (a Notify permit, kept until the next wait).
\* blocked: the clock's last pass ended on a full event lane, so its tick, not
\*   a wake, owns the retry until something wakes it.
\* rollbacks: how many dispatches a full lane has handed back.
\* causes: how many real wakes were sent (a root arriving, a slot released).
VARIABLES entry, due, arrived, next, oroot, ostate, asleep, woken, blocked, rollbacks, causes
\* Every variable, for steps that leave the rest unchanged.
vars == <<entry, due, arrived, next, oroot, ostate, asleep, woken, blocked, rollbacks, causes>>
\* Roots that hold a slot: reserved or worked on.
Occupied == {r \in Roots : entry[r] # Free}
\* Owners whose work is running in a slot.
Working == {g \in Gens : ostate[g] = "working"}
\* A due root the clock could reserve right now: its entry is free, a slot is
\* open, and the clock's last pass did not end on a full lane.
Ready == ~blocked /\ \E r \in due : entry[r] = Free /\ Cardinality(Occupied) < Cap
\* Nothing is due, no slot is held, no owner exists and the clock is awake.
Init == /\ entry = [r \in Roots |-> Free]
        \* No root is due yet.
        /\ due = {}
        \* No root has arrived yet.
        /\ arrived = [r \in Roots |-> 0]
        \* The first owner gets generation 1.
        /\ next = 1
        \* No owner has a root yet.
        /\ oroot = [g \in Gens |-> 0]
        \* No owner has been made.
        /\ ostate = [g \in Gens |-> "none"]
        \* The clock starts awake.
        /\ asleep = FALSE
        \* No notification is waiting.
        /\ woken = FALSE
        \* No pass has met a full lane.
        /\ blocked = FALSE
        \* No dispatch has been handed back, and no wake sent.
        /\ rollbacks = 0 /\ causes = 0
\* A root becomes due (a PushLog registered it); the pending-ready signal wakes the clock.
Arrive(r) == /\ entry[r] = Free /\ r \notin due /\ arrived[r] < 2
             \* The root joins the due set.
             /\ due' = due \cup {r}
             \* Count the arrival.
             /\ arrived' = [arrived EXCEPT ![r] = @ + 1]
             \* pending_dag_ready also ends the clock's wait.
             /\ woken' = TRUE
             \* That is one real wake.
             /\ causes' = causes + 1
             \* Nothing else changes.
             /\ UNCHANGED <<entry, next, oroot, ostate, asleep, blocked, rollbacks>>
\* The awake clock reserves a free slot for a due root.
Reserve(r) == /\ ~asleep /\ r \in due /\ entry[r] = Free
              \* A slot is open.
              /\ Cardinality(Occupied) < Cap
              \* The root's entry is now a reservation.
              /\ entry' = [entry EXCEPT ![r] = Scheduled]
              \* It is no longer waiting for dispatch.
              /\ due' = due \ {r}
              \* Nothing else changes.
              /\ UNCHANGED <<arrived, next, oroot, ostate, asleep, woken, blocked, rollbacks, causes>>
\* The clock's dispatch of a reserved root meets a full event lane
\* (manager.dispatch refuses it): it hands the reservation back, the root stays
\* due, and the pass ends with the clock waiting for its tick. The fix sends no
\* wake here; the "rollback-wake" bug notifies the clock itself.
Rollback(r) == /\ ~asleep /\ entry[r] = Scheduled /\ rollbacks < MaxRollbacks
               \* No claimant was handed this root: its dispatch never queued.
               /\ \A g \in Gens : ~(oroot[g] = r /\ ostate[g] = "made")
               \* The reservation is handed back.
               /\ entry' = [entry EXCEPT ![r] = Free]
               \* The root is due again.
               /\ due' = due \cup {r}
               \* The pass ends: the clock waits in its select again,
               /\ asleep' = TRUE
               \* and the full lane leaves the retry to its tick.
               /\ blocked' = TRUE
               \* Count the handed-back dispatch.
               /\ rollbacks' = rollbacks + 1
               \* No wake is owed for it, unless the bug notifies the clock itself.
               /\ woken' = (woken \/ Bug = "rollback-wake")
               \* Nothing else changes.
               /\ UNCHANGED <<arrived, next, oroot, ostate, causes>>
\* A claimant (the fetch spawn or a continuation's merge) sees the reservation
\* and makes an owner with a fresh generation; two can do this for one root.
Make(r) == /\ entry[r] = Scheduled /\ next <= MaxGen
           \* The new owner is for root r,
           /\ oroot' = [oroot EXCEPT ![next] = r]
           \* and is racing to install.
           /\ ostate' = [ostate EXCEPT ![next] = "made"]
           \* The next owner gets the next generation.
           /\ next' = next + 1
           \* Nothing else changes.
           /\ UNCHANGED <<entry, due, arrived, asleep, woken, blocked, rollbacks, causes>>
\* The owner installs its generation only if the entry is still a reservation;
\* otherwise another claimant won and this owner lost.
Install(g) == /\ ostate[g] = "made"
              \* Whether the reservation is still there to claim.
              /\ IF entry[oroot[g]] = Scheduled
                    \* It is: the entry now names this owner's generation,
                    THEN /\ entry' = [entry EXCEPT ![oroot[g]] = g]
                         \* and its work runs.
                         /\ ostate' = [ostate EXCEPT ![g] = "working"]
                    \* It is not: the entry stays, and this owner lost.
                    ELSE /\ ostate' = [ostate EXCEPT ![g] = "lost"]
                         \* The registry is untouched.
                         /\ UNCHANGED entry
              \* Nothing else changes.
              /\ UNCHANGED <<due, arrived, next, oroot, asleep, woken, blocked, rollbacks, causes>>
\* An owner is dropped: its work ended, or it lost the race. Its destructor
\* removes the entry only if the entry holds its own generation (the
\* "no-generation" bug removes any owner's entry for the root), and a release
\* wakes the clock (the "no-wake" bug stays silent).
Drop(g) == /\ ostate[g] \in {"working", "lost"}
           \* The entry this owner may release.
           /\ LET r == oroot[g]
                  \* Its own generation is there, or the bug ignores generations.
                  released == entry[r] = g \/ (Bug = "no-generation" /\ entry[r] \in Gens)
                  \* Whether this release sends the clock a wake.
                  wakes == released /\ Bug # "no-wake"
              IN /\ entry' = IF released THEN [entry EXCEPT ![r] = Free] ELSE entry
                 \* A release notifies the clock unless the bug is planted.
                 /\ woken' = (woken \/ wakes)
                 \* A sent wake is one real wake.
                 /\ causes' = IF wakes THEN causes + 1 ELSE causes
           \* The destructor has run.
           /\ ostate' = [ostate EXCEPT ![g] = "dropped"]
           \* Nothing else changes.
           /\ UNCHANGED <<due, arrived, next, oroot, asleep, blocked, rollbacks>>
\* The awake clock has nothing it can dispatch and no notification waiting, so
\* it waits.
Sleep == /\ ~asleep /\ ~woken /\ ~Ready
         \* It is now waiting in its select.
         /\ asleep' = TRUE
         \* Nothing else changes.
         /\ UNCHANGED <<entry, due, arrived, next, oroot, ostate, woken, blocked, rollbacks, causes>>
\* A waiting notification ends the wait, or is taken by an awake clock before
\* its next wait (Notify keeps one permit).
Wake == /\ woken
        \* The clock is awake to dispatch.
        /\ asleep' = FALSE
        \* The permit is used.
        /\ woken' = FALSE
        \* A woken clock tries its due roots again, full lane or not.
        /\ blocked' = FALSE
        \* Nothing else changes.
        /\ UNCHANGED <<entry, due, arrived, next, oroot, ostate, rollbacks, causes>>
\* Any of the steps above, in any order.
Next == \/ Sleep \/ Wake
        \/ \E r \in Roots : Arrive(r) \/ Reserve(r) \/ Rollback(r) \/ Make(r)
        \/ \E g \in Gens : Install(g) \/ Drop(g)
\* The whole behaviour: start from Init and take any Next step.
Spec == Init /\ [][Next]_vars
\* Every variable keeps a value of its intended kind.
TypeOK == /\ entry \in [Roots -> {Free, Scheduled} \cup Gens]
          \* Due roots are roots.
          /\ due \subseteq Roots
          \* Owners are in one of their five states.
          /\ ostate \in [Gens -> {"none", "made", "working", "lost", "dropped"}]
          \* The clock flags are booleans.
          /\ asleep \in BOOLEAN /\ woken \in BOOLEAN /\ blocked \in BOOLEAN
          \* The counters are natural numbers.
          /\ rollbacks \in Nat /\ causes \in Nat
\* No more owners are working than there are slots.
INV_Capacity == Cardinality(Working) <= Cap
\* Every working owner still holds its own root's entry.
INV_OwnedSlot == \A g \in Working : entry[oroot[g]] = g
\* The clock never waits with no wake owed while a due root could take a free slot.
INV_NoIdleCapacity == (asleep /\ ~woken) => ~Ready
\* Every pass a full lane ends was started by the clock's first run or by a real
\* wake, so the clock never spins on wakes it sent itself. Each rollback ends
\* one awake pass, and each pass after the first needs a wake.
INV_RollbackNeedsCause == rollbacks <= causes + 1
=============================================================================
