---- MODULE NacOrder ----
\* The story, in plain words.
\*
\* Node access control (NAC) is on. Callers ask the node to write a
\* document (add, update, delete, truncate). Go reads the collection first,
\* and that read is itself checked: `get-collection`. Only then does Go check
\* the write's own permission. So a caller needs both, and a caller holding
\* neither is told it lacks `get-collection`, the first one Go checks.
\*
\* A caller may also name no identity. Go's in-process client then acts as no
\* one: the wildcard, which holds only what was granted to everyone. It never
\* acts as the node, which holds every permission.
\*
\* A request comes in process or over HTTP. Over HTTP, the server's edge
\* checks node permissions before the database does: for a route Go serves
\* through its `CollectionMiddleware`, `get-collection` first, then the
\* route's own permission. The edge only adds refusals, so its refusal has to
\* name Go's first missing permission too. The edge decides both permissions
\* in one call onto the database's workers, read at one snapshot of the
\* node's grants, so its check is one step here.
\*
\* While a request runs, the node owner may revoke a permission. defradb.rs
\* reads both permissions inside the write's transaction, and regolith
\* validates those reads at commit, so a revoke that lands first sends the
\* attempt round again, where it is decided anew.
\*
\* What must never happen:
\*   1. A write commits for a caller that lacks either permission at commit.
\*      (INV_NoEffectWithoutPermission)
\*   2. A refusal names any permission but the first one, in Go's order, the
\*      caller lacks. (INV_RefusalNamesGoFirst)
\*
\* The Rust code this models:
\*   - the order                         crates/acp/src/nac/permission.rs NodePermission::go_order
\*   - the checks, before and inside     crates/db/src/access/nac.rs DB::check_node_access_as,
\*     the transaction                     DB::check_node_access_in_txn
\*   - the write paths that call them    crates/db/src/write/doc.rs create_as, update_as, delete_as;
\*                                       crates/db/src/write/autocommit/batch.rs check_attempt_node_access;
\*                                       crates/db/src/collection/ops/truncate/mod.rs DB::truncate_target;
\*                                       crates/db/src/txn/registry/indexes.rs check_access_in
\*                                       (an index or encrypted index created, listed or
\*                                       dropped in a transaction)
\*   - a write in an explicit            crates/query/src/runner/turn.rs QueryRunner::lent,
\*     transaction, whose refusal          lent_node_permission: walks go_order once, stops at
\*     names the first missing one         the first missing permission and names it, never
\*                                         answers empty
\*   - the caller with no identity       crates/defra-node/src/ops/request.rs Node::caller
\*   - the HTTP edge's checks            crates/http/src/auth_middleware.rs auth_middleware;
\*                                       crates/http/src/route_permissions.rs reads_collection_first,
\*                                       route_permission; crates/http/src/nac_guard.rs require_permissions;
\*                                       crates/http/src/nac_adapter.rs NacAdapter::check_permissions;
\*                                       crates/db/src/nac/mod.rs NacManager::check_permissions
EXTENDS Naturals, Sequences, TLC

CONSTANTS
  a, b, c,   \* named callers: a holds both permissions, b only the write's, c none
  anon,      \* a caller that names no identity
  node,      \* the node's own identity, which holds every permission
  Bug        \* "None" for the real code; otherwise the guard a RED config breaks

\* The two permissions a document write needs.
Get == "get-collection"            \* reading the collection
Own == "own"                       \* the write's own permission (update-document, ...)
\* Go's order: the collection read first, then the write's own.
GoOrder == <<Get, Own>>
\* The order the code checks in. RED OwnFirst swaps it; RED SkipGet drops the read.
Order == CASE Bug = "OwnFirst" -> <<Own, Get>>
           [] Bug = "SkipGet"  -> <<Own>>
           [] OTHER            -> GoOrder

\* The order the HTTP edge checks in: `get-collection` (reads_collection_first),
\* then the route's own permission. RED EdgeOwnOnly checks only the route's
\* own permission, as the edge did before it read the collection first.
EdgeOrder == IF Bug = "EdgeOwnOnly" THEN <<Own>> ELSE GoOrder

\* Where a request comes from: in process (no edge) or over HTTP (the edge first).
Surfaces == {"in-process", "http"}

\* Everyone who can send a request.
Requesters == {a, b, c, anon}

\* Who a request acts as. With no identity it is the wildcard (anon's grants);
\* RED AnonAsNode makes it the node, as the old in-process default did.
Acting(r) == IF r = anon /\ Bug = "AnonAsNode" THEN node ELSE r

VARIABLES
  grants,     \* what each requester holds now (anon: what the wildcard holds)
  checked,    \* per requester, the decision its attempt read: "idle", "ok" or a missing permission
  revoked,    \* how many revokes have happened, to keep the model finite
  bad,        \* TRUE once a write committed for a caller lacking a permission
  wrong       \* TRUE once a refusal named a permission Go would not

vars == <<grants, checked, revoked, bad, wrong>>

\* Whether identity w holds permission p: the node holds everything.
Holds(w, p) == w = node \/ p \in grants[w]

\* The first permission of `order` that w lacks, or "none".
FirstMissing(w, order) ==
  IF \E i \in 1..Len(order) : ~Holds(w, order[i])                   \* something is missing
  THEN order[CHOOSE i \in 1..Len(order) :                            \* the earliest missing one
                 /\ ~Holds(w, order[i])
                 /\ \A j \in 1..(i - 1) : Holds(w, order[j])]
  ELSE "none"                                                        \* holds them all

TypeOK ==
  /\ grants \in [Requesters -> SUBSET {Get, Own}]                    \* grants are sets of permissions
  /\ checked \in [Requesters -> {"idle", "ok", Get, Own}]            \* each attempt's decision
  /\ revoked \in 0..1                                                \* at most one revoke
  /\ bad \in BOOLEAN                                                 \* the bad-commit flag
  /\ wrong \in BOOLEAN                                               \* the wrong-refusal flag

Init ==
  /\ grants = (a :> {Get, Own}) @@ (b :> {Own}) @@ (c :> {}) @@ (anon :> {})  \* a has both, b the write's, c and the wildcard none
  /\ checked = [r \in Requesters |-> "idle"]                         \* no attempt running
  /\ revoked = 0                                                     \* nothing revoked yet
  /\ bad = FALSE                                                     \* nothing committed wrongly
  /\ wrong = FALSE                                                   \* nothing refused wrongly

\* Requester r's attempt, arriving over surface s, reads its permissions: over
\* HTTP the edge's first (as r itself: the wildcard for no identity), then the
\* database's in the code's order. The first refusal is the one r hears.
Check(r, s) ==
  /\ checked[r] = "idle"                                             \* r has no attempt running
  /\ LET edge == IF s = "http"                                       \* only HTTP has an edge
                   THEN FirstMissing(r, EdgeOrder)                   \* what the edge finds missing
                   ELSE "none"                                       \* in process: nothing checked here
         missing == IF edge # "none"                                 \* the edge refused first
                      THEN edge                                      \* so that is the refusal
                      ELSE FirstMissing(Acting(r), Order)            \* else what the database finds missing
     IN
       /\ checked' = [checked EXCEPT ![r] = IF missing = "none" THEN "ok" ELSE missing]  \* remember the decision
       /\ wrong' = (wrong \/ (missing # "none" /\ missing # FirstMissing(r, GoOrder)))  \* a refusal must name Go's first
  /\ UNCHANGED <<grants, revoked, bad>>

\* Requester r's attempt commits. The reads were in the transaction, so a
\* revoke since the check aborts the commit and the attempt checks again
\* (back to "idle"). RED OutsideTxn read them outside: the old decision stands.
Commit(r) ==
  /\ checked[r] = "ok"                                               \* only an allowed attempt commits
  /\ IF Bug # "OutsideTxn" /\ FirstMissing(Acting(r), Order) # "none"
       THEN /\ checked' = [checked EXCEPT ![r] = "idle"]             \* validation fails: run again
            /\ UNCHANGED bad                                         \* nothing written
       ELSE /\ checked' = [checked EXCEPT ![r] = "idle"]             \* the write lands; r may ask again
            /\ bad' = (bad \/ FirstMissing(r, GoOrder) # "none")    \* landing without Go's permissions is the bug
  /\ UNCHANGED <<grants, revoked, wrong>>

\* A refused attempt ends; its caller may ask again.
Refused(r) ==
  /\ checked[r] \in {Get, Own}                                       \* r was refused
  /\ checked' = [checked EXCEPT ![r] = "idle"]                       \* the attempt is over
  /\ UNCHANGED <<grants, revoked, bad, wrong>>

\* The node owner revokes permission p from requester r, once.
Revoke(r, p) ==
  /\ revoked = 0                                                     \* one revoke keeps the model small
  /\ p \in grants[r]                                                 \* r holds p
  /\ grants' = [grants EXCEPT ![r] = @ \ {p}]                        \* p is gone
  /\ revoked' = 1                                                    \* count it
  /\ UNCHANGED <<checked, bad, wrong>>

Next ==
  \/ \E r \in Requesters, s \in Surfaces : Check(r, s)               \* a request arrives and is checked
  \/ \E r \in Requesters : Commit(r) \/ Refused(r)                   \* a request's later steps
  \/ \E r \in Requesters, p \in {Get, Own} : Revoke(r, p)            \* a concurrent revoke

Spec == Init /\ [][Next]_vars

\* Promise 1: no write commits for a caller that lacks a permission Go checks.
INV_NoEffectWithoutPermission == ~bad

\* Promise 2: every refusal names the first permission, in Go's order, the caller lacks.
INV_RefusalNamesGoFirst == ~wrong
====
