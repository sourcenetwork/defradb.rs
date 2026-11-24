---------------------------- MODULE EventOrder ----------------------------
\* The story. Several transactions of one store commit on different threads.
\* Each carries events (bus messages, in-memory updates, hand-offs to P2P)
\* that must go out after it commits, in the order the store made the
\* commits durable. The store (regolith) gives every commit that writes a
\* receipt, a number that grows with each such commit. A commit that writes
\* nothing gets its snapshot as its receipt, which can be older than
\* receipts that already went out.
\* Who acts: each transaction opens (takes a snapshot), is bound to the
\* stream just before it enters regolith, then commits or aborts. A writing
\* commit is made durable in regolith's ordered step, which hands out its
\* receipt; its on_commit callback runs later, on its own thread, so the
\* callbacks of two commits reach the drainer in either order. Its owner
\* hands its events over once it knows the outcome, may run hooks and later
\* say they finished, or is cancelled and hands over without hooks. One
\* drainer applies these messages and sends out the events no commit still
\* in flight could precede.
\* What must never happen: events go out of commit order or of position
\* order; a committed commit's events are lost; an event goes out while its
\* commit's hooks still run; an aborted commit's events go out.
\* Not modelled: the drainer's message queues (each message is applied when
\* it is queued, one schedule the FIFO queues the code drains allow; a
\* dispatch may also run at any point, which allows more than the code
\* does), a panicking event (caught and reported, the stream goes on), and
\* liveness.
\* Rust owners: crates/engine/storage/src/backends/regolith/event_order.rs
\* (EventOrder::bind, Binding::hand_over, HooksRunning::done,
\* Drainer::apply_life, Drainer::apply_work, Drainer::complete,
\* Drainer::floor, Drainer::dispatch).
EXTENDS Integers, FiniteSets, Sequences

\* Txns are the transactions; MaxReceipt bounds the receipts regolith may
\* hand out; Mode picks the guard a RED configuration breaks.
CONSTANTS Txns, MaxReceipt, Mode

\* "safe" is the design; every other mode breaks exactly one guard.
ASSUME Mode \in {"safe", "no_floor_guard", "receipt_regress", "no_floor_clamp",
                 "dispatch_before_attach", "dispatch_during_hooks",
                 "abort_publishes", "not_min_seq"}
\* Receipts start at 1, so 0 can be the snapshot of an empty store.
ASSUME MaxReceipt \in Nat \ {0}

\* None marks a number not known yet.
None == -1
\* The larger of two numbers.
Max(a, b) == IF a >= b THEN a ELSE b

\* phase: idle, open (snapshot taken), bound, durable (made durable, its
\* on_commit not yet applied), committed or aborted.
\* snap: the receipt the snapshot saw. floor: the drainer's highest position
\* when the Begin was read. receipt: what regolith reported. pos: the
\* position the drainer gives the commit. order: the how-many-th writing
\* commit it was (0 for a commit that writes nothing).
\* wantHooks: the owner has hooks to run. attached: the owner handed over.
\* hooks: hooks are running. held: the drainer holds the events. settled:
\* the commit holds the stream back no more.
\* ready: events waiting to go out. published: events that went out, in
\* order. completed: the highest position read. lastReceipt: regolith's
\* latest receipt. commits: writing commits so far. arrivals: the
\* tie-break counter for equal positions.
VARIABLES phase, snap, floor, receipt, pos, order, wantHooks, attached,
          hooks, held, settled, ready, published, completed, lastReceipt,
          commits, arrivals

\* Every variable, for stuttering steps.
vars == <<phase, snap, floor, receipt, pos, order, wantHooks, attached,
          hooks, held, settled, ready, published, completed, lastReceipt,
          commits, arrivals>>

\* The transactions whose events went out.
Published == {published[i].txn : i \in 1..Len(published)}
\* The transactions whose events wait to go out.
ReadyTxns == {r.txn : r \in ready}

\* Nothing has opened, nothing is held and nothing went out.
Init == /\ phase = [t \in Txns |-> "idle"]
        /\ snap = [t \in Txns |-> None]
        /\ floor = [t \in Txns |-> None]
        /\ receipt = [t \in Txns |-> None]
        /\ pos = [t \in Txns |-> None]
        /\ order = [t \in Txns |-> 0]
        /\ wantHooks = [t \in Txns |-> FALSE]
        /\ attached = [t \in Txns |-> FALSE]
        /\ hooks = [t \in Txns |-> FALSE]
        /\ held = [t \in Txns |-> FALSE]
        /\ settled = [t \in Txns |-> FALSE]
        /\ ready = {}
        /\ published = <<>>
        /\ completed = 0
        /\ lastReceipt = 0
        /\ commits = 0
        /\ arrivals = 0

\* Drainer::complete's early return: it completes a commit only when its
\* position p is known, it has not settled (st), its owner has handed over
\* (att) and no hook runs (hk). Two REDs drop one hold each.
CanComplete(p, att, hk, st) ==
    \* The position is known and the commit still holds the stream.
    /\ p # None
    /\ ~st
    \* The `!attached` hold, dropped by Red_DispatchBeforeAttach.
    /\ (att \/ Mode = "dispatch_before_attach")
    \* The `hooks` hold, dropped by Red_DispatchDuringHooks.
    /\ (~hk \/ Mode = "dispatch_during_hooks")

\* Drainer::complete itself: t settles at position p and, when it holds
\* events (h), they join the ready set with the next tie-break number.
Complete(t, p, h) ==
    \* The commit settles and gives its events up.
    /\ settled' = [settled EXCEPT ![t] = TRUE]
    /\ held' = [held EXCEPT ![t] = FALSE]
    \* Held events wait to go out at p.
    /\ ready' = IF h THEN ready \cup {[pos |-> p, arrival |-> arrivals, txn |-> t]}
                     ELSE ready
    \* The tie-break counter moves only when events were added.
    /\ arrivals' = IF h THEN arrivals + 1 ELSE arrivals

\* Not completing: the completion variables stay as they are.
Stay == UNCHANGED <<settled, held, ready, arrivals>>

\* The transaction begins and takes a snapshot of the receipts so far.
Open(t) ==
    \* Each transaction runs once.
    /\ phase[t] = "idle"
    /\ phase' = [phase EXCEPT ![t] = "open"]
    \* Its snapshot is the latest receipt.
    /\ snap' = [snap EXCEPT ![t] = lastReceipt]
    /\ UNCHANGED <<floor, receipt, pos, order, wantHooks, attached, hooks,
                   held, settled, ready, published, completed, lastReceipt,
                   commits, arrivals>>

\* EventOrder::bind and Drainer::apply_life(Begin): just before it enters
\* regolith, the commit is bound with or without hooks (h); its floor is
\* the highest position the drainer has read.
Bind(t, h) ==
    \* Only an open transaction is bound.
    /\ phase[t] = "open"
    /\ phase' = [phase EXCEPT ![t] = "bound"]
    \* Life::Begin sets the floor to `completed`.
    /\ floor' = [floor EXCEPT ![t] = completed]
    \* The owner says now whether it runs hooks.
    /\ wantHooks' = [wantHooks EXCEPT ![t] = h]
    /\ UNCHANGED <<snap, receipt, pos, order, attached, hooks, held, settled,
                   ready, published, completed, lastReceipt, commits, arrivals>>

\* A writing commit made durable: regolith's ordered step hands it receipt
\* s. Nothing reaches the drainer yet; its on_commit runs later (OnCommit).
NativeCommit(t, s) ==
    \* A bound transaction commits once.
    /\ phase[t] = "bound"
    /\ s \in 1..MaxReceipt
    \* Receipts grow in durability order; Red_ReceiptRegress lets one go back.
    /\ (s > lastReceipt \/ Mode = "receipt_regress")
    /\ phase' = [phase EXCEPT ![t] = "durable"]
    \* The receipt is recorded.
    /\ receipt' = [receipt EXCEPT ![t] = s]
    \* It is the next writing commit to be made durable.
    /\ order' = [order EXCEPT ![t] = commits + 1]
    /\ commits' = commits + 1
    \* Regolith's latest receipt.
    /\ lastReceipt' = Max(lastReceipt, s)
    /\ UNCHANGED <<snap, floor, pos, wantHooks, attached, hooks, held, settled,
                   ready, published, completed, arrivals>>

\* The durable commit's on_commit reaches the drainer, in any order against
\* other commits' callbacks: Drainer::apply_life(Committed) sets position =
\* max(receipt, floor).
OnCommit(t) ==
    \* A durable commit's callback is applied once.
    /\ phase[t] = "durable"
    \* The position is the receipt raised to the floor; Red_NoFloorClamp
    \* takes the receipt alone.
    /\ LET p == IF Mode = "no_floor_clamp" THEN receipt[t] ELSE Max(receipt[t], floor[t])
       IN /\ phase' = [phase EXCEPT ![t] = "committed"]
          \* The position is recorded.
          /\ pos' = [pos EXCEPT ![t] = p]
          \* The drainer's highest position.
          /\ completed' = Max(completed, p)
          \* Committed calls complete, which fires only when the holds allow.
          /\ IF CanComplete(p, attached[t], hooks[t], settled[t])
                THEN Complete(t, p, held[t])
                ELSE Stay
    /\ UNCHANGED <<snap, floor, receipt, order, wantHooks, attached, hooks,
                   published, lastReceipt, commits>>

\* A commit that writes nothing: its receipt is its snapshot, and the clamp
\* to its floor keeps it from going out before what already went out.
EmptyCommit(t) ==
    \* A bound transaction commits once.
    /\ phase[t] = "bound"
    \* Its position is the snapshot raised to the floor; Red_NoFloorClamp
    \* takes the snapshot alone.
    /\ LET p == IF Mode = "no_floor_clamp" THEN snap[t] ELSE Max(snap[t], floor[t])
       IN /\ phase' = [phase EXCEPT ![t] = "committed"]
          \* Regolith reports the snapshot as its receipt.
          /\ receipt' = [receipt EXCEPT ![t] = snap[t]]
          /\ pos' = [pos EXCEPT ![t] = p]
          /\ completed' = Max(completed, p)
          \* Committed calls complete, which fires only when the holds allow.
          /\ IF CanComplete(p, attached[t], hooks[t], settled[t])
                THEN Complete(t, p, held[t])
                ELSE Stay
    /\ UNCHANGED <<snap, floor, order, wantHooks, attached, hooks, published,
                   lastReceipt, commits>>

\* Regolith's on_abort and Drainer::apply_life(Aborted): the commit settles
\* and its events are dropped. Red_AbortPublishes handles the abort as a
\* commit that writes nothing, at its floor, so held events go out.
Abort(t) ==
    \* A bound transaction aborts once (a conflict, an error or a drop).
    /\ phase[t] = "bound"
    /\ phase' = [phase EXCEPT ![t] = "aborted"]
    /\ IF Mode = "abort_publishes"
          \* The planted bug: the abort is given a position and completes.
          THEN /\ pos' = [pos EXCEPT ![t] = floor[t]]
               /\ IF CanComplete(floor[t], attached[t], hooks[t], settled[t])
                     THEN Complete(t, floor[t], held[t])
                     ELSE Stay
          \* The design: settle, drop the events, add nothing to ready.
          ELSE /\ pos' = pos
               /\ settled' = [settled EXCEPT ![t] = TRUE]
               /\ held' = [held EXCEPT ![t] = FALSE]
               /\ UNCHANGED <<ready, arrivals>>
    /\ UNCHANGED <<snap, floor, receipt, order, wantHooks, attached, hooks,
                   published, completed, lastReceipt, commits>>

\* Binding::hand_over (or Binding's drop) and Drainer::apply_work(Attach):
\* the owner hands the events over. Hooks run only for a commit the owner
\* saw commit and did not cancel (c); an owner dropped before the outcome
\* hands over without hooks. The drainer keeps the events only when the
\* commit has not settled (an aborted one has); Red_AbortPublishes keeps
\* them always.
Attach(t, c) ==
    \* The owner hands over once, after binding.
    /\ phase[t] \in {"bound", "committed", "aborted"}
    /\ ~attached[t]
    \* hk: hooks run now; h: the drainer keeps the events.
    /\ LET hk == phase[t] = "committed" /\ wantHooks[t] /\ ~c
           h == ~settled[t] \/ Mode = "abort_publishes"
       IN /\ attached' = [attached EXCEPT ![t] = TRUE]
          /\ hooks' = [hooks EXCEPT ![t] = hk]
          \* Attach calls complete with the values it just set.
          /\ IF CanComplete(pos[t], TRUE, hk, settled[t])
                THEN Complete(t, pos[t], h)
                ELSE /\ held' = [held EXCEPT ![t] = h]
                     /\ UNCHANGED <<settled, ready, arrivals>>
    /\ UNCHANGED <<phase, snap, floor, receipt, pos, order, wantHooks,
                   published, completed, lastReceipt, commits>>

\* HooksRunning::done (or its drop) and Drainer::apply_work(HooksDone):
\* the hooks finished, or their owner was cancelled while they ran.
HooksDone(t) ==
    \* Only running hooks finish.
    /\ hooks[t]
    /\ hooks' = [hooks EXCEPT ![t] = FALSE]
    \* HooksDone calls complete with the hooks cleared.
    /\ IF CanComplete(pos[t], attached[t], FALSE, settled[t])
          THEN Complete(t, pos[t], held[t])
          ELSE Stay
    /\ UNCHANGED <<phase, snap, floor, receipt, pos, order, wantHooks,
                   attached, published, completed, lastReceipt, commits>>

\* Drainer::floor: no commit still in flight has a floor below p.
FloorAllows(p) ==
    \A t \in Txns : (floor[t] # None /\ ~settled[t]) => p <= floor[t]

\* The heap's top (Ready's Ord): r has the lowest position, then the
\* oldest arrival.
First(r) == \A o \in ready : \/ o = r
                             \/ r.pos < o.pos
                             \/ (r.pos = o.pos /\ r.arrival < o.arrival)

\* Drainer::dispatch: the first ready commit goes out once no floor holds it
\* back. Red_Arrival drops the floor guard; Red_NotMinSeq sends any one.
Dispatch(r) ==
    \* The min-seq choice, dropped by Red_NotMinSeq.
    /\ r \in ready
    /\ (First(r) \/ Mode = "not_min_seq")
    \* The floor guard, dropped by Red_Arrival.
    /\ (FloorAllows(r.pos) \/ Mode = "no_floor_guard")
    \* Its events leave the ready set and go out.
    /\ ready' = ready \ {r}
    /\ published' = Append(published, r)
    /\ UNCHANGED <<phase, snap, floor, receipt, pos, order, wantHooks,
                   attached, hooks, held, settled, completed, lastReceipt,
                   commits, arrivals>>

\* Each step is one transaction or owner action, or one dispatch.
Next == \/ \E t \in Txns : Open(t)
        \/ \E t \in Txns, h \in BOOLEAN : Bind(t, h)
        \/ \E t \in Txns, s \in 1..MaxReceipt : NativeCommit(t, s)
        \/ \E t \in Txns : OnCommit(t)
        \/ \E t \in Txns : EmptyCommit(t)
        \/ \E t \in Txns : Abort(t)
        \/ \E t \in Txns, c \in BOOLEAN : Attach(t, c)
        \/ \E t \in Txns : HooksDone(t)
        \/ \E r \in ready : Dispatch(r)

\* Behaviours start in Init and take Next steps.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
TypeOK ==
    /\ phase \in [Txns -> {"idle", "open", "bound", "durable", "committed", "aborted"}]
    /\ snap \in [Txns -> {None} \cup 0..MaxReceipt]
    /\ floor \in [Txns -> {None} \cup 0..MaxReceipt]
    /\ receipt \in [Txns -> {None} \cup 0..MaxReceipt]
    /\ pos \in [Txns -> {None} \cup 0..MaxReceipt]
    /\ order \in [Txns -> 0..Cardinality(Txns)]
    /\ wantHooks \in [Txns -> BOOLEAN]
    /\ attached \in [Txns -> BOOLEAN]
    /\ hooks \in [Txns -> BOOLEAN]
    /\ held \in [Txns -> BOOLEAN]
    /\ settled \in [Txns -> BOOLEAN]
    /\ completed \in 0..MaxReceipt
    /\ lastReceipt \in 0..MaxReceipt

\* Events go out in position order: a position never goes back.
Ordered == \A i, j \in 1..Len(published) :
               i < j => published[i].pos <= published[j].pos

\* Writing commits' events go out in the order regolith made them durable.
INV_CommitOrder ==
    \A i, j \in 1..Len(published) :
        \* a went out before b.
        LET a == published[i].txn
            b == published[j].txn
        IN (i < j /\ order[a] > 0 /\ order[b] > 0) => order[a] < order[b]

\* A committed commit that handed over and settled has its events out or
\* waiting: none are lost.
INV_CommittedEventsKept ==
    \A t \in Txns : (phase[t] = "committed" /\ attached[t] /\ settled[t])
                    => t \in Published \cup ReadyTxns

\* No event goes out while its commit's hooks still run.
INV_HooksBeforeEvents == \A t \in Published : ~hooks[t]

\* Only committed commits' events go out.
INV_OnlyCommitted == \A t \in Published : phase[t] = "committed"

=============================================================================
