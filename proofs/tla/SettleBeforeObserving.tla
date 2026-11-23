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
\* notified. Changes on the tuple settle oldest first; once the oldest one
\* owed is parked or refused it never settles, so the wait gives up on it,
\* and an older change still to settle is waited for first. A provider
\* that is away keeps the change owed, so the deadline runs out.
\*
\* The drainer runs the settlement passes. A commit that stages changes
\* wakes it; a pass the provider fails ends in a back-off that doubles, and
\* a change failed MaxTries times is refused. The observation does not
\* wake the drainer, so waiting never spends a change's tries.
\*
\* The transaction then reads the answer as a prepared input keyed by the
\* tuple's latest staged change, and only while nothing is owed on the tuple
\* in its own view.
\*
\* What must never happen: the observation answers from a provider state
\* older than the change it found owed, or older than a change staged on the
\* tuple while it waited (a later change also renames the projection's
\* prepared input, so an answer about an earlier state is never reused for
\* it); a transaction reuses an answer older than a change staged and
\* settled after it; it is still waiting once its deadline has passed,
\* however many notifications arrive and however long a settlement pass
\* holds the lock; or the observation cuts the drainer's back-off, so a
\* provider failing for a moment spends all of a change's tries and it is
\* refused.
\*
\* The code: ExternalDocumentAcp::observe_actor_relationship,
\* ExternalDocumentAcp::await_tuple and before in
\* crates/engine/acp/src/external.rs, with OBSERVATION_PATIENCE; the
\* projection's prepared relationship input is resolved through it at the
\* edge (crates/database/db/src/write/prepare.rs), and
\* DocumentMutationContext::projected_actor_relationship keys that input by
\* the tuple's latest staged change (ops::tuple_head). The oldest change at
\* rest is ops::tuple_rest; the drainer and its back-off are
\* crates/entry-points/defra-node/src/owed_drainer.rs, with
\* MAX_FAILED_SUBMISSIONS in crates/engine/acp/src/external/settle.rs.
\* Only the standard operators on numbers are used.
EXTENDS Naturals

\* Bug: "none"; "nowait" (the provider is asked while the change is owed);
\* "earlierOnly" (the observation answers once the change it found owed has
\* settled, not every change staged since); "wakeFirst" (a pending
\* notification is taken before the deadline is
\* checked, so a stream of them keeps the wait going); "lockUnbounded" (the
\* wait for the settling lock has no deadline); "noExpiry" (the deadline
\* never ends the wait); "wakeDrainer" (the wait wakes the drainer whenever
\* it is woken); "noHead" (the prepared input is not keyed by the tuple's
\* latest staged change). Patience: the deadline, in ticks of time.
\* Notifications: whether owed records notify at all; without them the wait
\* ends only by its deadline. MaxTries: the failed passes after which a
\* change is refused.
CONSTANTS Bug, Patience, Notifications, MaxTries

\* staged, settled: the latest change staged on the tuple and the latest the
\* provider holds; changes settle in order. up: the provider takes changes;
\* otherwise it is away or answers with errors that may pass. parkedAt: the
\* change that is parked or refused, 0 for none. tries: failed passes of the
\* oldest owed change. due, gap: the time of the drainer's next turn and its
\* next back-off. pending: a notification of some owed record has
\* arrived and not been taken. locked: a settlement pass holds the settling
\* lock. phase: where the observation is. needed: the change owed when the
\* observation began. seen: the change the provider answered at. latest: the
\* latest change staged when it answered. elapsed: time since the
\* observation began. answer: the outcome. used, usedAt: a transaction read
\* the answer, and the latest change staged when it did.
VARIABLES staged, settled, up, parkedAt, tries, due, gap, pending, locked,
          phase, needed, seen, latest, elapsed, answer, used, usedAt

\* Every variable, for stuttering steps.
vars == <<staged, settled, up, parkedAt, tries, due, gap, pending, locked,
          phase, needed, seen, latest, elapsed, answer, used, usedAt>>

\* A time past every turn: the drainer waits for a commit.
Never == Patience + 1

\* The oldest change owed is parked or refused.
AtRest == parkedAt # 0 /\ parkedAt = settled + 1

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

\* One change was just committed and is owed, and its commit woke the
\* drainer; the observation has not begun.
Init == /\ staged = 1 /\ settled = 0 /\ up \in BOOLEAN /\ parkedAt \in {0, 1}
        /\ tries = 0 /\ due = 0 /\ gap = 1
        /\ pending = FALSE /\ locked = FALSE
        /\ phase = "idle" /\ needed = 0 /\ seen = 0 /\ latest = 0 /\ elapsed = 0
        /\ answer = "none" /\ used = FALSE /\ usedAt = 0

\* Time passes, whatever else happens, but never past a deadline that is due:
\* the timer ends the wait at its time. One tick past it is the bug showing.
Tick == /\ Active /\ ~Due /\ elapsed <= Patience
        /\ elapsed' = elapsed + 1
        /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, phase, needed, seen, latest, answer, used, usedAt>>

\* Some owed record, of this tuple or any other, is written or deleted.
Notify == /\ Notifications /\ ~pending /\ pending' = TRUE
          /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, locked, phase, needed, seen, latest, elapsed, answer, used, usedAt>>

\* A transaction stages a later change on the tuple, which the provider may
\* come to refuse; its record notifies, and its commit wakes the drainer.
StageAgain == /\ staged < 2 /\ staged' = staged + 1
              /\ parkedAt' \in IF parkedAt = 0 THEN {0, staged + 1} ELSE {parkedAt}
              /\ pending' = (pending \/ Notifications)
              /\ due' = elapsed
              /\ UNCHANGED <<settled, up, tries, gap, locked, phase, needed, seen, latest, elapsed, answer, used, usedAt>>

\* The drainer's turn: a settlement pass takes the lock and may hold it on for
\* other objects. It settles the changes up to the one at rest, oldest
\* first; a provider that fails it spends a try of the oldest owed change,
\* refuses it once its tries run out, and the drainer backs off. A change at
\* rest is no reason to turn again.
Settle == /\ ~locked /\ Owed /\ elapsed >= due
          /\ locked' = TRUE
          /\ pending' = (pending \/ Notifications)
          /\ IF AtRest
               THEN /\ due' = Never
                    /\ UNCHANGED <<settled, parkedAt, tries, gap>>
             ELSE IF up
               THEN /\ settled' = IF parkedAt > settled THEN parkedAt - 1 ELSE staged
                    /\ tries' = 0 /\ gap' = 1 /\ due' = Never
                    /\ UNCHANGED parkedAt
             ELSE /\ tries' = tries + 1
                  /\ parkedAt' = IF tries + 1 >= MaxTries THEN settled + 1 ELSE parkedAt
                  /\ due' = elapsed + gap
                  /\ gap' = IF gap < 4 THEN gap * 2 ELSE gap
                  /\ UNCHANGED settled
          /\ UNCHANGED <<staged, up, phase, needed, seen, latest, elapsed, answer, used, usedAt>>

\* The settlement pass lets the lock go.
Release == /\ locked /\ locked' = FALSE
           /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, phase, needed, seen, latest, elapsed, answer, used, usedAt>>

\* The provider goes down or comes back.
Toggle == /\ up' = ~up
          /\ UNCHANGED <<staged, settled, parkedAt, tries, due, gap, pending, locked, phase, needed, seen, latest, elapsed, answer, used, usedAt>>

\* The observation begins: it notes the change owed and sets its deadline.
Begin == /\ phase = "idle" /\ phase' = "waiting" /\ needed' = staged
         /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, seen, latest, elapsed, answer, used, usedAt>>

\* The deadline ends the wait, in either phase: refused as unsettled.
Expire == /\ Due
          /\ answer' = "unsettled" /\ phase' = "done"
          /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, needed, seen, latest, elapsed, used, usedAt>>

\* A notification is taken while patience remains (with the bug, regardless)
\* and the tuple is read again: settled, it goes for the lock. With the bug
\* the wait also wakes the drainer, ending its back-off.
Wake == /\ phase = "waiting" /\ pending /\ ~AtRest
        /\ (elapsed < Patience \/ Bug = "wakeFirst")
        /\ pending' = FALSE
        /\ phase' = IF Owed THEN "waiting" ELSE "locking"
        /\ due' = IF Bug = "wakeDrainer" THEN elapsed ELSE due
        /\ UNCHANGED <<staged, settled, up, parkedAt, tries, gap, locked, needed, seen, latest, elapsed, answer, used, usedAt>>

\* With the lock taken, a change staged since sends the observation back to
\* wait for it.
Rewait == /\ phase = "locking" /\ ~locked /\ Owed /\ Bug # "earlierOnly"
          /\ phase' = "waiting"
          /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, needed, seen, latest, elapsed, answer, used, usedAt>>

\* The oldest change owed is at rest and will not settle: the wait gives up.
GiveUp == /\ phase = "waiting" /\ Owed /\ AtRest
          /\ answer' = "unsettled" /\ phase' = "done"
          /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, needed, seen, latest, elapsed, used, usedAt>>

\* Nothing is owed and the lock is free (with the bugs: the change it began
\* waiting for has settled, or regardless of what is owed): the provider is
\* asked, and the check under the same lock finds nothing staged since.
Ask == /\ \/ phase = "locking" /\ ~locked /\ ~Owed
          \/ Bug = "earlierOnly" /\ phase = "locking" /\ ~locked /\ settled >= needed
          \/ Bug = "nowait" /\ phase = "waiting"
       /\ seen' = settled /\ latest' = staged /\ answer' = "answered" /\ phase' = "done"
       /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, needed, elapsed, used, usedAt>>

\* The transaction reads the answer: its prepared input is keyed by the latest
\* change staged on the tuple (with the bug, by nothing of it), and nothing is
\* owed on the tuple in its view.
Reuse == /\ answer = "answered" /\ ~used /\ ~Owed
         /\ (staged = latest \/ Bug = "noHead")
         /\ used' = TRUE /\ usedAt' = staged
         /\ UNCHANGED <<staged, settled, up, parkedAt, tries, due, gap, pending, locked, phase, needed, seen, latest, elapsed, answer>>

\* Any of these, in any order.
Next == Tick \/ Notify \/ StageAgain \/ Settle \/ Release \/ Toggle \/ Begin \/ Expire
        \/ Wake \/ Rewait \/ GiveUp \/ Ask \/ Reuse

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* An answer comes from a provider state that holds the change the
\* observation found owed.
INV_NotOlderThanOwed == answer = "answered" => seen >= needed

\* An answer comes from a provider state that holds every change staged on
\* the tuple when it answered, later ones included.
INV_HoldsLatest == answer = "answered" => seen >= latest

\* Once the deadline has passed, the observation is no longer waiting: time
\* never runs past it while the observation waits.
INV_EndsInTime == Active => elapsed <= Patience

\* A transaction uses an answer only from a provider state that holds every
\* change staged on the tuple when it reads it.
INV_ReuseHoldsLatest == used => seen >= usedAt

\* Waiting spends none of a change's tries: within the deadline the
\* drainer's own back-off never refuses a change the provider fails.
INV_BackoffHeld == tries < MaxTries
=============================================================================
