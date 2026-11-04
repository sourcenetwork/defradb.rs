---- MODULE CoreDispatch ----
\* The story, in plain words.
\*
\* Every edge (HTTP, the CLI through HTTP, FFI, the node) asks the
\* core for the same schema-evolution operations: patch a collection, set the
\* active version, set a migration, add a lens, list lenses, list
\* collections. Each request carries a context: who is asking, and maybe the
\* ID of a transaction the caller opened. The core is one function per
\* operation, and it decides three things in the same way for every edge.
\*
\*   - It checks the caller may do the operation (NAC) before it does anything.
\*   - A request that carries a transaction runs inside that transaction, so
\*     its writes stay private until the caller commits.
\*   - A request with no transaction runs on its own (autocommit) and its
\*     writes reach the shared store at once.
\*   - A patch inside a transaction is refused for now and changes nothing.
\*
\* What must never happen:
\*   1. A caller without the permission changes anything. (INV_NoEffectWithoutPermission)
\*   2. A request that names a transaction writes straight to the shared
\*      store, outside its transaction: the bug the HTTP patch handler had
\*      when it ignored the transaction header. (INV_TxnNeverAutocommits)
\*
\* The Rust code this models:
\*   - Node's patch_collection_as, set_active_collection_version_as,
\*     set_migration_as, add_lens_as, list_lenses_as,
\*     get_collections_as                         crates/defra-node/src/ops/collection.rs
\*   - the one dispatch per operation              crates/db/src/txn/registry/lifecycle.rs
\*     get_collections_as, patch_collection_as, set_active_collection_version_as,
\*     set_migration_as, add_lens_as, list_lenses_as
\*   - the NAC checks                              crates/db/src/definition/patch/mod.rs,
\*     crates/db/src/collection/ops/version.rs, crates/db/src/collection/ops/select.rs,
\*     crates/db/src/definition/migration/{set,lenses}.rs, crates/db/src/txn/registry/lenses.rs
\*   - the edges that call it                      crates/defra-node/src/ops/collection.rs,
\*     crates/http/src/handlers/{collections,lens}.rs, crates/ffi/src/{lens,schema}.rs,
\*     crates/ffi/src/collection/{write,migration}.rs
EXTENDS Naturals, FiniteSets

CONSTANTS
  Callers,   \* who can send requests
  Allowed,   \* the callers NAC lets do these operations
  Txns,      \* transaction IDs a request may carry
  Bug        \* "None" for the real code; otherwise the guard a RED config removes

\* A request carries no transaction when its txn is NoTxn.
NoTxn == "none"
\* The operations: a patch, and every other write-shaped operation as one.
Ops == {"patch", "other"}

VARIABLES
  store,     \* the writes that reached the shared store: <<caller, op>> pairs
  private,   \* per transaction, the writes it holds until commit
  sent,      \* how many requests have been sent, to keep the model finite
  leaked     \* TRUE once a request that named a transaction wrote the store

vars == <<store, private, sent, leaked>>

TypeOK ==
  /\ store \subseteq (Callers \X Ops)                     \* the store holds caller-op pairs
  /\ private \in [Txns -> SUBSET (Callers \X Ops)]        \* each transaction holds caller-op pairs
  /\ sent \in 0..3                                         \* at most three requests
  /\ leaked \in BOOLEAN                                    \* the leak flag is a truth value

Init ==
  /\ store = {}                                            \* nothing written yet
  /\ private = [t \in Txns |-> {}]                         \* every transaction is empty
  /\ sent = 0                                              \* no request sent
  /\ leaked = FALSE                                        \* nothing leaked

\* Whether the core lets caller c through. RED NoNac skips the check.
Permitted(c) == Bug = "NoNac" \/ c \in Allowed

\* Caller c sends operation op, carrying transaction t (or NoTxn).
Request(c, op, t) ==
  /\ sent < 3                                              \* keep the run finite
  /\ sent' = sent + 1                                      \* count it
  /\ IF ~Permitted(c)
       THEN UNCHANGED <<store, private, leaked>>           \* refused: nothing changes
     ELSE IF t = NoTxn \/ Bug = "IgnoreTxn"
       \* No transaction (or, under RED IgnoreTxn, a transaction the edge
       \* dropped): the operation commits on its own.
       THEN /\ store' = store \cup {<<c, op>>}
            /\ leaked' = (leaked \/ t # NoTxn)                \* a named transaction was bypassed
            /\ UNCHANGED private
     ELSE IF op = "patch"
       THEN UNCHANGED <<store, private, leaked>>           \* a patch in a transaction is refused
     ELSE /\ private' = [private EXCEPT ![t] = @ \cup {<<c, op>>}]  \* held by its transaction
          /\ UNCHANGED <<store, leaked>>

Next == \E c \in Callers, op \in Ops, t \in Txns \cup {NoTxn} : Request(c, op, t)

Spec == Init /\ [][Next]_vars

\* INV_NoEffectWithoutPermission: nothing a refused caller sent is anywhere.
INV_NoEffectWithoutPermission ==
  /\ \A w \in store : w[1] \in Allowed                     \* the store holds only allowed callers' writes
  /\ \A t \in Txns : \A w \in private[t] : w[1] \in Allowed \* and so does every transaction

\* INV_TxnNeverAutocommits: a request that named a transaction never wrote
\* the shared store directly.
INV_TxnNeverAutocommits == ~leaked

\* A patch inside a transaction changes nothing while it is refused.
INV_PatchInTxnChangesNothing ==
  \A t \in Txns : \A w \in private[t] : w[2] # "patch"     \* no transaction holds a patch
====
