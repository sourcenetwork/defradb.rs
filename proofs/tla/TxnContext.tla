---- MODULE TxnContext ----
\* The story, in plain words.
\*
\* A caller opens a transaction. The database hands back an ID. From then on
\* the caller puts that ID in the context of each request it makes, and the
\* request runs inside that transaction. Its writes stay private to the
\* transaction until the caller commits. If the caller discards instead, the
\* writes vanish. Every edge (HTTP, FFI, WASM, the node) does exactly
\* this through the same core, so one model covers them all.
\*
\* The core enforces five rules. The model checks each one, and for each rule
\* it has a RED configuration that plants the bug the rule prevents.
\*
\* What must never happen:
\*   1. A caller runs a request in a transaction another caller opened.
\*      (INV_OnlyOwnerRuns) Such a request gets "transaction not found".
\*   2. A read-only transaction ends up holding a write. (INV_ReadOnlyNeverWrites)
\*   3. A request runs in a transaction that was already committed or
\*      discarded. (INV_FinishedNeverRuns)
\*   4. A discarded transaction leaves something in the shared store.
\*      (INV_DiscardLeavesNothing)
\*   5. Two transactions write the same key, both commit, and the second
\*      silently overwrites the first. The second commit must fail with a
\*      conflict instead. (INV_NoLostUpdate)
\*
\* The Rust code this models:
\*   - RequestContext::txn / with_txn            crates/defra-core/src/request_context.rs
\*   - Node::txn_begin / txn_commit / txn_discard
\*                                                crates/defra-node/src/ops/request.rs
\*   - DbTransactionRegistry begin_as / run_as / commit_as / discard_as,
\*     and the owner check Owners::run / finish   crates/db/src/txn/registry/lifecycle.rs,
\*                                                crates/db/src/txn/owner/mod.rs
\*   - the read-only rejection                    crates/query/src/runner/turn.rs
\*   - commit-time conflict (regolith validates)  crates/db/src/txn/registry/lifecycle.rs commit_error
EXTENDS Naturals, FiniteSets

CONSTANTS
  Callers,   \* the identities that can open transactions
  Txns,      \* the transaction IDs the database can hand out
  Keys,      \* the keys a request can write
  Bug        \* "None" is the real code; each other value plants one bug

ASSUME Bug \in {"None", "NoOwnerCheck", "ReadOnlyWrites", "RunAfterEnd",
                "DiscardLeaks", "NoConflictCheck"}

VARIABLES
  state,       \* [Txns -> {"none","open","committed","discarded"}] where each ID is in its life
  owner,       \* [Txns -> Callers] who opened it (only meaningful once opened)
  readOnly,    \* [Txns -> BOOLEAN] opened read-only?
  writes,      \* [Txns -> SUBSET Keys] the transaction's private, uncommitted writes
  base,        \* [Txns -> SUBSET Keys] what the shared store held when it opened
  committed,   \* SUBSET Keys: the shared store everyone reads
  final,       \* [Txns -> SUBSET Keys] what each transaction put in the store when it committed
  ranBy,       \* [Txns -> SUBSET Callers] who has had a request run in it
  lateRan,     \* BOOLEAN: a request once ran in a finished transaction (oracle latch)
  lostUpdate   \* BOOLEAN: a commit once overwrote a key another commit made after it opened (oracle latch)

vars == <<state, owner, readOnly, writes, base, committed, final, ranBy, lateRan, lostUpdate>>

\* The first caller, only to give `owner` a value before any transaction opens.
Anyone == CHOOSE c \in Callers : TRUE

\* Every variable holds a value of the shape it was declared with.
TypeOK ==
  /\ state \in [Txns -> {"none", "open", "committed", "discarded"}]  \* each ID is unused, open, committed or discarded
  /\ owner \in [Txns -> Callers]                                      \* each ID names one caller as its opener
  /\ readOnly \in [Txns -> BOOLEAN]                                   \* each ID is read-only or not
  /\ writes \in [Txns -> SUBSET Keys]                                 \* each ID holds some private writes
  /\ base \in [Txns -> SUBSET Keys]                                   \* each ID remembers the store it started from
  /\ committed \subseteq Keys                                         \* the shared store is a set of keys
  /\ final \in [Txns -> SUBSET Keys]                                  \* each ID remembers what its commit added
  /\ ranBy \in [Txns -> SUBSET Callers]                               \* each ID remembers who ran requests in it
  /\ lateRan \in BOOLEAN                                              \* the late-run latch is up or down
  /\ lostUpdate \in BOOLEAN                                           \* the lost-update latch is up or down

\* At the start nothing is open, nothing is written and nothing went wrong.
Init ==
  /\ state = [t \in Txns |-> "none"]       \* no ID has been handed out yet
  /\ owner = [t \in Txns |-> Anyone]       \* a placeholder opener until an ID opens
  /\ readOnly = [t \in Txns |-> FALSE]     \* nothing is read-only yet
  /\ writes = [t \in Txns |-> {}]          \* no transaction holds a write
  /\ base = [t \in Txns |-> {}]            \* no transaction has a starting picture
  /\ committed = {}                        \* the shared store is empty
  /\ final = [t \in Txns |-> {}]           \* no commit has added anything
  /\ ranBy = [t \in Txns |-> {}]           \* no request has run anywhere
  /\ lateRan = FALSE                       \* no request has run in a finished transaction
  /\ lostUpdate = FALSE                    \* no commit has overwritten another

\* Caller c opens transaction t (Node::txn_begin). The store as it is now is
\* the transaction's starting picture.
Begin(c, t, ro) ==
  /\ state[t] = "none"                                    \* only an unused ID can be handed out
  /\ state' = [state EXCEPT ![t] = "open"]                \* t is now open
  /\ owner' = [owner EXCEPT ![t] = c]                     \* and belongs to c
  /\ readOnly' = [readOnly EXCEPT ![t] = ro]              \* read-only if c asked for it
  /\ base' = [base EXCEPT ![t] = committed]               \* t starts from the store as it is now
  /\ UNCHANGED <<writes, committed, final, ranBy, lateRan, lostUpdate>>  \* nothing else moves

\* May caller c's request, carrying t in its context, run in t?
\* Real code: only while t is open, and only for the caller that opened it.
Admits(c, t) ==
  CASE Bug = "NoOwnerCheck" -> state[t] = "open"          \* bug: any caller may use an open t
    [] Bug = "RunAfterEnd"  -> state[t] # "none"          \* bug: a committed or discarded t still runs requests
    [] OTHER                -> state[t] = "open" /\ owner[t] = c  \* real code: open, and c opened it

\* Caller c runs a write request on key k with t in its context (exec_request).
\* If t is not admitted the request gets "transaction not found" and nothing
\* changes. A read-only transaction refuses the write: also no change.
Request(c, t, k) ==
  /\ state[t] # "none"                                    \* the context names an ID that was handed out
  /\ IF Admits(c, t)                                      \* the core lets the request in
       THEN /\ ranBy' = [ranBy EXCEPT ![t] = @ \cup {c}]  \* remember that c ran a request in t
            /\ lateRan' = (lateRan \/ state[t] # "open")  \* raise the latch if t had already finished
            /\ writes' = IF readOnly[t] /\ Bug # "ReadOnlyWrites"  \* a read-only t refuses the write
                           THEN writes                    \* so its writes stay as they were
                           ELSE [writes EXCEPT ![t] = @ \cup {k}]  \* otherwise k joins t's private writes
       ELSE UNCHANGED <<ranBy, lateRan, writes>>          \* refused: "transaction not found", nothing changes
  /\ UNCHANGED <<state, owner, readOnly, base, committed, final, lostUpdate>>  \* a request never ends t or touches the store

\* Caller c commits t (Node::txn_commit). Only the opener, only while open.
\* The commit fails with a conflict when it writes a key another transaction
\* committed after t opened; a failed commit ends t and keeps nothing.
Commit(c, t) ==
  /\ state[t] = "open" /\ owner[t] = c                    \* only t's opener commits, only while t is open
  /\ LET clash == writes[t] \cap (committed \ base[t]) IN \* keys t wrote that someone committed since t opened
       IF clash # {} /\ Bug # "NoConflictCheck"           \* regolith sees the clash
         THEN /\ state' = [state EXCEPT ![t] = "discarded"]  \* the commit fails with a conflict and t ends
              /\ UNCHANGED <<committed, final, lostUpdate>>  \* the store keeps nothing of t
         ELSE /\ state' = [state EXCEPT ![t] = "committed"]  \* no clash (or the bug ignores it): t commits
              /\ committed' = committed \cup writes[t]    \* t's writes reach the shared store
              /\ final' = [final EXCEPT ![t] = writes[t]] \* remember what t added
              /\ lostUpdate' = (lostUpdate \/ clash # {}) \* raise the latch if t overwrote a newer commit
  /\ writes' = [writes EXCEPT ![t] = {}]                  \* t's private writes are gone either way
  /\ UNCHANGED <<owner, readOnly, base, ranBy, lateRan>>  \* nothing else moves

\* Caller c discards t (Node::txn_discard). The writes are dropped.
Discard(c, t) ==
  /\ state[t] = "open" /\ owner[t] = c                    \* only t's opener discards, only while t is open
  /\ state' = [state EXCEPT ![t] = "discarded"]           \* t ends
  /\ committed' = IF Bug = "DiscardLeaks" THEN committed \cup writes[t] ELSE committed  \* bug: the writes leak into the store
  /\ writes' = [writes EXCEPT ![t] = {}]                  \* t's private writes are dropped
  /\ UNCHANGED <<owner, readOnly, base, final, ranBy, lateRan, lostUpdate>>  \* nothing else moves

\* Each step, some caller does one thing with some transaction ID.
Next ==
  \E c \in Callers, t \in Txns :                          \* pick a caller and an ID
    \/ \E ro \in BOOLEAN : Begin(c, t, ro)                \* open it, read-only or not
    \/ \E k \in Keys : Request(c, t, k)                   \* run a write of some key in it
    \/ Commit(c, t)                                       \* commit it
    \/ Discard(c, t)                                      \* or discard it

\* Start from Init and take Next steps forever (or stand still).
Spec == Init /\ [][Next]_vars


\* ---- What must always hold ----

\* Rule 1: whoever has had a request run in t is the one who opened it.
INV_OnlyOwnerRuns ==
  \A t \in Txns : state[t] = "none" \/ ranBy[t] \subseteq {owner[t]}

\* Rule 2: a read-only transaction holds no writes.
INV_ReadOnlyNeverWrites ==
  \A t \in Txns : readOnly[t] => writes[t] = {}

\* Rule 3: no request ever ran in a finished transaction.
INV_FinishedNeverRuns == ~lateRan

\* Rule 4: everything in the shared store came from a transaction that committed.
INV_DiscardLeavesNothing ==
  committed \subseteq UNION {final[t] : t \in Txns}

\* Rule 5: no commit ever overwrote another commit's key unseen.
INV_NoLostUpdate == ~lostUpdate

====
