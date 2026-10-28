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

TypeOK ==
  /\ state \in [Txns -> {"none", "open", "committed", "discarded"}]
  /\ owner \in [Txns -> Callers]
  /\ readOnly \in [Txns -> BOOLEAN]
  /\ writes \in [Txns -> SUBSET Keys]
  /\ base \in [Txns -> SUBSET Keys]
  /\ committed \subseteq Keys
  /\ final \in [Txns -> SUBSET Keys]
  /\ ranBy \in [Txns -> SUBSET Callers]
  /\ lateRan \in BOOLEAN
  /\ lostUpdate \in BOOLEAN

Init ==
  /\ state = [t \in Txns |-> "none"]
  /\ owner = [t \in Txns |-> Anyone]
  /\ readOnly = [t \in Txns |-> FALSE]
  /\ writes = [t \in Txns |-> {}]
  /\ base = [t \in Txns |-> {}]
  /\ committed = {}
  /\ final = [t \in Txns |-> {}]
  /\ ranBy = [t \in Txns |-> {}]
  /\ lateRan = FALSE
  /\ lostUpdate = FALSE

\* Caller c opens transaction t (Node::txn_begin). The store as it is now is
\* the transaction's starting picture.
Begin(c, t, ro) ==
  /\ state[t] = "none"
  /\ state' = [state EXCEPT ![t] = "open"]
  /\ owner' = [owner EXCEPT ![t] = c]
  /\ readOnly' = [readOnly EXCEPT ![t] = ro]
  /\ base' = [base EXCEPT ![t] = committed]
  /\ UNCHANGED <<writes, committed, final, ranBy, lateRan, lostUpdate>>

\* May caller c's request, carrying t in its context, run in t?
\* Real code: only while t is open, and only for the caller that opened it.
Admits(c, t) ==
  CASE Bug = "NoOwnerCheck" -> state[t] = "open"
    [] Bug = "RunAfterEnd"  -> state[t] # "none"
    [] OTHER                -> state[t] = "open" /\ owner[t] = c

\* Caller c runs a write request on key k with t in its context (exec_request).
\* If t is not admitted the request gets "transaction not found" and nothing
\* changes. A read-only transaction refuses the write: also no change.
Request(c, t, k) ==
  /\ state[t] # "none"
  /\ IF Admits(c, t)
       THEN /\ ranBy' = [ranBy EXCEPT ![t] = @ \cup {c}]
            /\ lateRan' = (lateRan \/ state[t] # "open")
            /\ writes' = IF readOnly[t] /\ Bug # "ReadOnlyWrites"
                           THEN writes
                           ELSE [writes EXCEPT ![t] = @ \cup {k}]
       ELSE UNCHANGED <<ranBy, lateRan, writes>>
  /\ UNCHANGED <<state, owner, readOnly, base, committed, final, lostUpdate>>

\* Caller c commits t (Node::txn_commit). Only the opener, only while open.
\* The commit fails with a conflict when it writes a key another transaction
\* committed after t opened; a failed commit ends t and keeps nothing.
Commit(c, t) ==
  /\ state[t] = "open" /\ owner[t] = c
  /\ LET clash == writes[t] \cap (committed \ base[t]) IN
       IF clash # {} /\ Bug # "NoConflictCheck"
         THEN /\ state' = [state EXCEPT ![t] = "discarded"]
              /\ UNCHANGED <<committed, final, lostUpdate>>
         ELSE /\ state' = [state EXCEPT ![t] = "committed"]
              /\ committed' = committed \cup writes[t]
              /\ final' = [final EXCEPT ![t] = writes[t]]
              /\ lostUpdate' = (lostUpdate \/ clash # {})
  /\ writes' = [writes EXCEPT ![t] = {}]
  /\ UNCHANGED <<owner, readOnly, base, ranBy, lateRan>>

\* Caller c discards t (Node::txn_discard). The writes are dropped.
Discard(c, t) ==
  /\ state[t] = "open" /\ owner[t] = c
  /\ state' = [state EXCEPT ![t] = "discarded"]
  /\ committed' = IF Bug = "DiscardLeaks" THEN committed \cup writes[t] ELSE committed
  /\ writes' = [writes EXCEPT ![t] = {}]
  /\ UNCHANGED <<owner, readOnly, base, final, ranBy, lateRan, lostUpdate>>

Next ==
  \E c \in Callers, t \in Txns :
    \/ \E ro \in BOOLEAN : Begin(c, t, ro)
    \/ \E k \in Keys : Request(c, t, k)
    \/ Commit(c, t)
    \/ Discard(c, t)

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
