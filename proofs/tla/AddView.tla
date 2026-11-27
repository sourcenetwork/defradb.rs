---- MODULE AddView ----
\* The story: a client asks a node to add views. It sends a source query (the
\* one collection the views read from), an SDL (the view types to create) and
\* maybe the CID of a lens the rows go through. The same request can arrive
\* two ways: through the embedded node (FFI, the node's own HTTP server, the
\* CLI against it) or through the HTTP server `defradb start` runs.
\*
\* The node runs these checks in order and stops at the first that fails:
\*   1. nac        the caller may add views;
\*   2. sdl        the SDL parses against the collections the node knows;
\*   3. one        the source query parses and selects exactly one source;
\*   4. cid        a lens CID, when one is given, is not blank;
\*   5. chunk      no view field is a chunk;
\*   6. downsample every downsample view's definition is valid;
\*   7. policy     every @policy names a policy and resource the node holds;
\*   8. identity   the acting identity, when there is one, is a DID.
\* Only when all pass does it create the view collections, in one
\* transaction, so every view of the SDL exists afterwards or none does.
\* After that commit, and only then, it builds the materialized ones.
\*
\* Who acts: the request moving through each way in. Whether each check
\* would pass, and whether the create itself would fail, is chosen up front
\* in "ok", so the model explores every combination.
\*
\* What must never happen: a view created although a check should have
\* refused it; a request that leaves only some of its views; a view built
\* that was never created; and the two ways in answering the same request
\* differently (one refuses, the other creates).
\*
\* Code it models:
\*   crates/database/db/src/view/client/mod.rs DbTransactionRegistry::add_view
\*     the whole sequence, and view_source (checks one and cid)
\*   crates/database/db/src/view/client/manage.rs DbTransactionRegistry::refresh_views_as,
\*     gc_downsample_views_as  the NAC check before the refresh and the GC
\*   crates/database/db/src/collection/ops/create.rs DB::create_view_collections_as
\*     the all-or-nothing create; crates/database/db/src/txn/registry/views.rs
\*     DbTransactionRegistry::add_view_in_txn the same inside a transaction
\*   crates/entry-points/defra-node/src/db_impls.rs DbSchemaOps::add_view and
\*     crates/entry-points/cli/src/view_adapter.rs ViewAdapter::add_view, the
\*     two ways in, which only forward to the core
EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS
  Views,        \* the view types the SDL describes
  CoreChecks,   \* which checks the core really runs (a subset of the eight)
  ServerChecks, \* which checks the `defradb start` server runs: the core's when it forwards
  BuildFirst    \* TRUE when materialized views are built before the create is known to have landed

\* The eight checks, in the order they run.
Checks == <<"nac", "sdl", "one", "cid", "chunk", "downsample", "policy", "identity">>

\* The same checks as a set.
AllChecks == {Checks[i] : i \in 1..Len(Checks)}

\* The two ways a request reaches a node.
Edges == {"node", "server"}

\* Two views are enough to show a half-created SDL.
ASSUME Cardinality(Views) >= 2
ASSUME CoreChecks \subseteq AllChecks
ASSUME ServerChecks \subseteq AllChecks
ASSUME BuildFirst \in BOOLEAN

\* The checks the way in `e` runs: the embedded node always calls the core.
Enforced(e) == IF e = "node" THEN CoreChecks ELSE ServerChecks

\* Where a request is: 1..8 is the next check, 9 the create, 10 the build, 11 finished.
CreateStep == Len(Checks) + 1
BuildStep == Len(Checks) + 2
DoneStep == Len(Checks) + 3

VARIABLES
  ok,       \* ok[c] says whether check c would pass, ok["create"] whether the create would succeed
  step,     \* step[e]: how far the request through way in e got
  created,  \* created[e]: the views that exist because of the request through e
  built,    \* built[e]: the views whose materialized rows were built
  refused   \* refused[e]: the request through e ended with an error

vars == <<ok, step, created, built, refused>>

\* Every name an "ok" answer is kept under.
Keys == AllChecks \cup {"create"}

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ ok \in [Keys -> BOOLEAN]
  /\ step \in [Edges -> 1..DoneStep]
  /\ created \in [Edges -> SUBSET Views]
  /\ built \in [Edges -> SUBSET Views]
  /\ refused \in [Edges -> BOOLEAN]

\* Any mix of answers can arrive; nothing exists and nothing has run yet.
Init ==
  /\ ok \in [Keys -> BOOLEAN]
  /\ step = [e \in Edges |-> 1]
  /\ created = [e \in Edges |-> {}]
  /\ built = [e \in Edges |-> {}]
  /\ refused = [e \in Edges |-> FALSE]

\* Way in `e` runs check number step[e]. A failing check it enforces ends the
\* request with an error; otherwise the request moves on, even when the check
\* would have failed (the bug a RED configuration plants).
RunCheck(e) ==
  /\ step[e] \in 1..Len(Checks)
  /\ LET c == Checks[step[e]] IN
     IF c \in Enforced(e) /\ ~ok[c]
       THEN /\ refused' = [refused EXCEPT ![e] = TRUE]
            /\ step' = [step EXCEPT ![e] = DoneStep]
       ELSE /\ step' = [step EXCEPT ![e] = step[e] + 1]
            /\ UNCHANGED refused
  /\ UNCHANGED <<ok, created, built>>

\* The create: one transaction makes every view or none. With BuildFirst the
\* views are built here, whether or not the create lands.
Create(e) ==
  /\ step[e] = CreateStep
  /\ IF ok["create"]
       THEN /\ created' = [created EXCEPT ![e] = Views]
            /\ step' = [step EXCEPT ![e] = BuildStep]
            /\ UNCHANGED refused
       ELSE /\ created' = [created EXCEPT ![e] = {}]
            /\ refused' = [refused EXCEPT ![e] = TRUE]
            /\ step' = [step EXCEPT ![e] = DoneStep]
  /\ built' = IF BuildFirst THEN [built EXCEPT ![e] = Views] ELSE built
  /\ UNCHANGED ok

\* After the commit: build the materialized views the create made.
Build(e) ==
  /\ step[e] = BuildStep
  /\ built' = [built EXCEPT ![e] = created[e]]
  /\ step' = [step EXCEPT ![e] = DoneStep]
  /\ UNCHANGED <<ok, created, refused>>

\* Either way in takes its next step.
Next == \E e \in Edges : RunCheck(e) \/ Create(e) \/ Build(e)

\* Both requests finished stay finished.
Done == (\A e \in Edges : step[e] = DoneStep) /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Done]_vars

\* No view exists unless every check would have passed.
INV_NoViewUnlessAllChecksPass ==
  \A e \in Edges : created[e] # {} => \A c \in AllChecks : ok[c]

\* A finished request leaves all of its views or none of them.
INV_AllOrNone ==
  \A e \in Edges : step[e] = DoneStep => created[e] \in {{}, Views}

\* Only a view that was created is ever built.
INV_BuiltOnlyCreated ==
  \A e \in Edges : built[e] \subseteq created[e]

\* Once both are finished, the two ways in gave the same answer and left the
\* same views.
INV_SameAnswer ==
  (\A e \in Edges : step[e] = DoneStep) =>
    /\ refused["node"] = refused["server"]
    /\ created["node"] = created["server"]
====
