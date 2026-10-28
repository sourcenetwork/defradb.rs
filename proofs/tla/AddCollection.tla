---- MODULE AddCollection ----
\* The story: a client sends SDL, text that describes one or more collections,
\* and asks the database to add them. The database does it once, in one place,
\* for every way in (HTTP, CLI, FFI, embedded, WASM).
\*
\* It checks four things in order, and stops at the first that fails:
\*   1. nac       the caller may change the node's collections;
\*   2. parse     the SDL is well-formed;
\*   3. validate  the collections it describes make a valid schema;
\*   4. policy    every @policy names a real policy and resource (skipped on a
\*                node with no policy lookup; the model has one installed).
\* Only when all four pass does it create the collections, and it creates them
\* in one transaction: every collection of the SDL exists afterwards, or none.
\*
\* Who acts: the caller's request moving through the checks, then the create.
\* Whether each check would pass, and whether the create itself would fail, is
\* decided up front and kept in the "ok" variables, so the model explores every
\* combination.
\*
\* What must never happen: a collection created although a check should have
\* refused the request; a request that ends with only some of its collections.
\*
\* Code it models (crates/db/src/collection/ops/add.rs):
\*   Client::add_collection for DB   the whole sequence
\*   check_collection_patch          nac
\*   prepare_new_collections         parse, validate, policy (check_policies)
\*   create_collections_atomic_*     the all-or-nothing create
\*   crates/db/src/txn/registry/schemas.rs add_collection_in_txn_as  the same checks inside a transaction
EXTENDS Naturals, FiniteSets

CONSTANTS
  Colls,        \* the collections the SDL describes
  Enforced,     \* which checks the database really runs (a subset of Checks)
  CreateMode    \* "Atomic" (one transaction) | "OneByOne" (one create per collection)

\* The four checks, in the order they run.
Checks == <<"nac", "parse", "validate", "policy">>

\* Two collections are enough to show a half-created SDL.
ASSUME Cardinality(Colls) >= 2
ASSUME Enforced \subseteq {"nac", "parse", "validate", "policy"}
ASSUME CreateMode \in {"Atomic", "OneByOne"}

VARIABLES
  ok,       \* ok[c] says whether check c would pass for this request, and ok["create"] whether the create itself would succeed
  step,     \* how far the request got: 1..4 is the next check, 5 is the create, 6 is finished
  created,  \* the collections that exist because of this request
  refused   \* the request ended with an error

vars == <<ok, step, created, refused>>

\* Every name an "ok" answer is kept under.
Keys == {"nac", "parse", "validate", "policy", "create"}

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ ok \in [Keys -> BOOLEAN]
  /\ step \in 1..6
  /\ created \subseteq Colls
  /\ refused \in BOOLEAN

\* Any mix of answers can arrive; nothing exists and nothing has run yet.
Init ==
  /\ ok \in [Keys -> BOOLEAN]
  /\ step = 1
  /\ created = {}
  /\ refused = FALSE

\* Check number `step`. A failing check that the database enforces ends the
\* request with an error. Otherwise the request moves on, even if the check
\* would have failed (that is the bug a RED configuration plants).
RunCheck ==
  /\ step \in 1..4
  /\ LET c == Checks[step] IN
     IF c \in Enforced /\ ~ok[c]
       THEN /\ refused' = TRUE /\ step' = 6
       ELSE /\ refused' = refused /\ step' = step + 1
  /\ UNCHANGED <<ok, created>>

\* Atomic create: one transaction makes every collection or none of them.
CreateAtomic ==
  /\ step = 5 /\ CreateMode = "Atomic"
  /\ step' = 6
  /\ IF ok["create"]
       THEN /\ created' = Colls /\ refused' = refused
       ELSE /\ created' = {} /\ refused' = TRUE
  /\ UNCHANGED ok

\* One-by-one create: each collection is its own commit, so one that lands
\* stays even if a later one fails.
CreateOne ==
  /\ step = 5 /\ CreateMode = "OneByOne"
  /\ created # Colls
  /\ \E c \in Colls \ created : created' = created \cup {c}
  /\ UNCHANGED <<ok, step, refused>>

\* One-by-one create, a later collection fails: the request ends with an error
\* and the ones already created stay.
CreateOneFails ==
  /\ step = 5 /\ CreateMode = "OneByOne"
  /\ ~ok["create"] /\ created # {} /\ created # Colls
  /\ refused' = TRUE /\ step' = 6
  /\ UNCHANGED <<ok, created>>

\* One-by-one create, every collection landed: the request is done.
CreateOneDone ==
  /\ step = 5 /\ CreateMode = "OneByOne"
  /\ created = Colls
  /\ step' = 6
  /\ UNCHANGED <<ok, created, refused>>

Next == RunCheck \/ CreateAtomic \/ CreateOne \/ CreateOneFails \/ CreateOneDone

\* A finished request stays finished.
Done == step = 6 /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Done]_vars

\* No collection exists unless every check would have passed.
INV_NoCollectionUnlessAllChecksPass ==
  created # {} => (ok["nac"] /\ ok["parse"] /\ ok["validate"] /\ ok["policy"])

\* A finished request leaves all of its collections or none of them.
INV_AllOrNone == step = 6 => (created = {} \/ created = Colls)
====
