------------------------ MODULE SettleBeforeObserving ------------------------
\* The story. A transaction commits a relationship change on a document; the
\* change is owed to the external provider until the node's drainer settles
\* it. Moments later an observation of that relationship arrives (a projection
\* of the next write asks for it, or a caller asks directly). The provider
\* does not hold the change yet, so its answer would predate it.
\*
\* Instead of refusing at once, the observation waits at the network edge
\* for the change to settle: it is woken whenever a record of something owed
\* is written or deleted, on any tuple, and it reads its own tuple again.
\* Then it takes the settling lock, which a settlement pass holds for its
\* whole run, and asks the provider, which now holds the change. One deadline,
\* set when the observation begins, covers all of it: the waits for a
\* notification and for the lock. Time passes whether or not anything is
\* notified. A change that is parked or refused never settles, so the wait
\* gives up on it at once; a provider that is down keeps the change owed, so
\* the deadline runs out.
\*
\* What must never happen: the observation answers from a provider state
\* older than the change it found owed; or it is still waiting once its
\* deadline has passed, however many notifications arrive and however long a
\* settlement pass holds the lock.
\*
\* The code: ExternalDocumentAcp::observe_actor_relationship,
\* ExternalDocumentAcp::await_tuple and next_wake in
\* crates/engine/acp/src/external.rs, with OBSERVATION_PATIENCE; the
\* projection's prepared relationship input is resolved through it at the
\* edge (crates/database/db/src/write/prepare.rs), and
\* DocumentMutationContext::projected_actor_relationship keys that input by
\* the tuple's latest staged change (ops::tuple_head).
\* Only the standard operators on numbers are used.
EXTENDS Naturals

\* Bug: "none"; "nowait" (the provider is asked while the change is owed);
\* "wakeFirst" (a pending notification is taken before the deadline is
\* checked, so a stream of them keeps the wait going); "lockUnbounded" (the
\* wait for the settling lock has no deadline); "noExpiry" (the deadline
\* never ends the wait). Patience: the deadline, in ticks of time.
\* Notifications: whether owed records notify at all; without them the wait
\* ends only by its deadline.
CONSTANTS Bug, Patience, Notifications

\* staged, settled: the latest change staged on the tuple and the one the
\* provider holds. up: the provider is reachable. parked: the latest change
\* is parked or refused. pending: a notification of some owed record has
\* arrived and not been taken. locked: a settlement pass holds the settling
\* lock. phase: where the observation is. needed: the change owed when the
\* observation began. seen: the change the provider answered at. elapsed:
\* time since the observation began. answer: the outcome.
VARIABLES staged, settled, up, parked, pending, locked, phase, needed, seen,
          elapsed, answer

\* Every variable, for stuttering steps.
vars == <<staged, settled, up, parked, pending, locked, phase, needed, seen,
          elapsed, answer>>

\* A change is owed: the provider does not hold the latest staged one.
Owed == settled < staged

\* The observation is waiting, for its tuple or for the lock.
Active == phase \in {"waiting", "locking"}

\* The deadline has passed while the observation still waits, and the code
\* honours it in the phase it is in.
Due == /\ Active /\ elapsed >= Patience
       /\ Bug # "noExpiry"
       /\ ~(Bug = "wakeFirst" /\ phase = "waiting" /\ pending)
       /\ ~(Bug = "lockUnbounded" /\ phase = "locking")

\* One change was just committed and is owed; the observation has not begun.
Init == /\ staged = 1 /\ settled = 0 /\ up \in BOOLEAN /\ parked \in BOOLEAN
        /\ pending = FALSE /\ locked = FALSE
        /\ phase = "idle" /\ needed = 0 /\ seen = 0 /\ elapsed = 0
        /\ answer = "none"

\* Time passes, whatever else happens, but never past a deadline that is due:
\* the timer ends the wait at its time. One tick past it is the bug showing.
Tick == /\ Active /\ ~Due /\ elapsed <= Patience
        /\ elapsed' = elapsed + 1
        /\ UNCHANGED <<staged, settled, up, parked, pending, locked, phase, needed, seen, answer>>

\* Some owed record, of this tuple or any other, is written or deleted.
Notify == /\ Notifications /\ ~pending /\ pending' = TRUE
          /\ UNCHANGED <<staged, settled, up, parked, locked, phase, needed, seen, elapsed, answer>>

\* A settlement pass takes the lock, settles the change if it can, and may
\* hold the lock on for other objects.
Settle == /\ ~locked /\ locked' = TRUE
          /\ settled' = IF up /\ ~parked THEN staged ELSE settled
          /\ pending' = (pending \/ Notifications)
          /\ UNCHANGED <<staged, up, parked, phase, needed, seen, elapsed, answer>>

\* The settlement pass lets the lock go.
Release == /\ locked /\ locked' = FALSE
           /\ UNCHANGED <<staged, settled, up, parked, pending, phase, needed, seen, elapsed, answer>>

\* The provider goes down or comes back.
Toggle == /\ up' = ~up
          /\ UNCHANGED <<staged, settled, parked, pending, locked, phase, needed, seen, elapsed, answer>>

\* The observation begins: it notes the change owed and sets its deadline.
Begin == /\ phase = "idle" /\ phase' = "waiting" /\ needed' = staged
         /\ UNCHANGED <<staged, settled, up, parked, pending, locked, seen, elapsed, answer>>

\* The deadline ends the wait, in either phase: refused as unsettled.
Expire == /\ Due
          /\ answer' = "unsettled" /\ phase' = "done"
          /\ UNCHANGED <<staged, settled, up, parked, pending, locked, needed, seen, elapsed>>

\* A notification is taken while patience remains (with the bug, regardless)
\* and the tuple is read again: settled, it goes for the lock.
Wake == /\ phase = "waiting" /\ pending /\ ~parked
        /\ (elapsed < Patience \/ Bug = "wakeFirst")
        /\ pending' = FALSE
        /\ phase' = IF Owed THEN "waiting" ELSE "locking"
        /\ UNCHANGED <<staged, settled, up, parked, locked, needed, seen, elapsed, answer>>

\* A parked change will not settle: the wait gives up at once.
GiveUp == /\ phase = "waiting" /\ Owed /\ parked
          /\ answer' = "unsettled" /\ phase' = "done"
          /\ UNCHANGED <<staged, settled, up, parked, pending, locked, needed, seen, elapsed>>

\* Nothing is owed and the lock is free (or, with the bug, regardless of what
\* is owed): the provider is asked.
Ask == /\ \/ phase = "locking" /\ ~locked /\ ~Owed
          \/ Bug = "nowait" /\ phase = "waiting"
       /\ seen' = settled /\ answer' = "answered" /\ phase' = "done"
       /\ UNCHANGED <<staged, settled, up, parked, pending, locked, needed, elapsed>>

\* Any of these, in any order.
Next == Tick \/ Notify \/ Settle \/ Release \/ Toggle \/ Begin \/ Expire
        \/ Wake \/ GiveUp \/ Ask

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* An answer comes from a provider state that holds the change the
\* observation found owed.
INV_NotOlderThanOwed == answer = "answered" => seen >= needed

\* Once the deadline has passed, the observation is no longer waiting: time
\* never runs past it while the observation waits.
INV_EndsInTime == Active => elapsed <= Patience
=============================================================================
