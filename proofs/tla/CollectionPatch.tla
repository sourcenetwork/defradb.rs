---- MODULE CollectionPatch ----
\* The story: a client sends one JSON patch that may change several
\* collections at once (add fields to /Users and /Books, add both sides of a
\* new relation, remove /Users and /Books together). Go applies the patch to
\* every collection at once, so the database must too.
\*
\* It works in three steps:
\*   1. prepare   each collection's share of the patch is applied to that
\*                collection and checked on its own;
\*   2. validate  the whole result is checked together: every collection
\*                sees the other collections as the patch leaves them, and a
\*                removed collection may not be referenced by a field that
\*                stays;
\*   3. commit    every change is written in one transaction.
\*
\* Who acts: the patch request moving through the steps. Whether each share
\* is valid, whether the combined result is valid, whether a relation spans
\* two collections, whether a removed collection is still referenced, and
\* whether the commit itself succeeds are all decided up front, so the model
\* explores every combination.
\*
\* What must never happen: a patch that ends with only some collections
\* changed; a change applied although the result is invalid or leaves a
\* field pointing at a removed collection; a valid patch refused because one
\* collection was checked without seeing the others.
\*
\* Code it models (crates/db/src/definition/patch/):
\*   mod.rs        DB::patch_collections_as   splits the patch by collection
\*   targets.rs    group_by_target, fold_collection_copies
\*   many.rs       DB::patch_many_as           prepare, validate, one commit
\*   prepare.rs    DB::prepare_patch           step 1
\*   store.rs      DB::store_new_version       step 2 against `siblings`
\*   in_place.rs   DB::write_in_place          step 2 against `siblings`
\*   crates/db/src/collection/ops/version_delete.rs
\*                 DB::check_versions_removable   the removal checks
\*                 DB::write_version_deletion     removals in the same commit
EXTENDS Naturals, FiniteSets

CONSTANTS
  Colls,            \* the collections the patch changes
  CommitMode,       \* "Atomic" (one transaction) | "OneByOne" (a commit per collection)
  SiblingsVisible,  \* each collection is validated seeing the others as patched
  CheckRefs         \* removals are refused while a staying field references them

\* Two collections are enough to show a half-applied patch.
ASSUME Cardinality(Colls) >= 2
\* The commit mode is one of the two the code could use.
ASSUME CommitMode \in {"Atomic", "OneByOne"}
\* The two guards are each on or off.
ASSUME SiblingsVisible \in BOOLEAN /\ CheckRefs \in BOOLEAN

VARIABLES
  ok,         \* ok[c]: collection c's own share of the patch is valid
  joint,      \* the combined result passes Go's whole-set validators
  spans,      \* a relation the patch adds needs both sides to be present
  dangling,   \* a removed collection is still referenced by a field that stays
  commitOk,   \* the commit itself would succeed (no conflict)
  step,       \* "prepare", "validate", "commit" or "done"
  applied,    \* the collections whose change is committed
  refused     \* the patch ended with an error

vars == <<ok, joint, spans, dangling, commitOk, step, applied, refused>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  \* each collection's share is valid or not
  /\ ok \in [Colls -> BOOLEAN]
  \* the four up-front answers are true or false
  /\ joint \in BOOLEAN /\ spans \in BOOLEAN /\ dangling \in BOOLEAN /\ commitOk \in BOOLEAN
  \* the request is at one of its steps
  /\ step \in {"prepare", "validate", "commit", "done"}
  \* only collections the patch names can be changed
  /\ applied \subseteq Colls
  \* the request was refused or not
  /\ refused \in BOOLEAN

\* The patch is really valid: every share, and the whole, with nothing left
\* pointing at a removed collection.
Valid == (\A c \in Colls : ok[c]) /\ joint /\ ~dangling

\* Any mix of answers can arrive; nothing is applied yet.
Init ==
  \* any share may be valid or not
  /\ ok \in [Colls -> BOOLEAN]
  \* any combination of the whole-patch answers
  /\ joint \in BOOLEAN /\ spans \in BOOLEAN /\ dangling \in BOOLEAN /\ commitOk \in BOOLEAN
  \* a new request starts at prepare, with nothing applied or refused
  /\ step = "prepare" /\ applied = {} /\ refused = FALSE

\* Step 1: an invalid share ends the request with an error.
Prepare ==
  \* only at the first step
  /\ step = "prepare"
  \* every share valid moves on; any invalid one refuses
  /\ IF \A c \in Colls : ok[c]
       THEN /\ step' = "validate" /\ refused' = FALSE
       ELSE /\ step' = "done" /\ refused' = TRUE
  \* the answers and the store do not change here
  /\ UNCHANGED <<ok, joint, spans, dangling, commitOk, applied>>

\* Step 2: the whole result is checked. Without siblings in view, a relation
\* that spans two collections looks one-sided and is refused although it is
\* valid. Without the reference check, a dangling field passes.
Validate ==
  \* only at the second step
  /\ step = "validate"
  \* what the code's checks accept
  /\ LET passes == /\ joint
                   \* a spanning relation passes only when the other side is visible
                   /\ (spans => SiblingsVisible)
                   \* a dangling reference passes only when the check is off
                   /\ (CheckRefs => ~dangling)
     IN IF passes
          THEN /\ step' = "commit" /\ refused' = FALSE
          ELSE /\ step' = "done" /\ refused' = TRUE
  \* the answers and the store do not change here
  /\ UNCHANGED <<ok, joint, spans, dangling, commitOk, applied>>

\* Step 3, atomic: one transaction writes every change, or none.
CommitAtomic ==
  \* only at the last step, in the atomic mode
  /\ step = "commit" /\ CommitMode = "Atomic"
  \* the request finishes either way
  /\ step' = "done"
  \* a successful commit applies every collection; a failed one applies none
  /\ IF commitOk
       THEN /\ applied' = Colls /\ refused' = FALSE
       ELSE /\ applied' = {} /\ refused' = TRUE
  \* the answers do not change
  /\ UNCHANGED <<ok, joint, spans, dangling, commitOk>>

\* Step 3, one by one: each collection commits on its own and stays.
CommitOne ==
  \* only at the last step, in the one-by-one mode, with collections left
  /\ step = "commit" /\ CommitMode = "OneByOne" /\ applied # Colls
  \* any collection not yet applied commits
  /\ \E c \in Colls \ applied : applied' = applied \cup {c}
  \* the request is still committing
  /\ UNCHANGED <<ok, joint, spans, dangling, commitOk, step, refused>>

\* One by one, a later commit fails: the earlier ones stay behind.
CommitOneFails ==
  \* only once some but not all collections have committed
  /\ step = "commit" /\ CommitMode = "OneByOne"
  /\ ~commitOk /\ applied # {} /\ applied # Colls
  \* the request ends with an error, keeping what landed
  /\ step' = "done" /\ refused' = TRUE
  /\ UNCHANGED <<ok, joint, spans, dangling, commitOk, applied>>

\* One by one, every collection landed: the request is done.
CommitOneDone ==
  \* only once every collection has committed
  /\ step = "commit" /\ CommitMode = "OneByOne" /\ applied = Colls
  \* the request finishes without an error
  /\ step' = "done"
  /\ UNCHANGED <<ok, joint, spans, dangling, commitOk, applied, refused>>

\* Any of the steps can happen next.
Next == Prepare \/ Validate \/ CommitAtomic \/ CommitOne \/ CommitOneFails \/ CommitOneDone

\* A finished request stays finished.
Done == step = "done" /\ UNCHANGED vars

\* The request runs its steps, then rests.
Spec == Init /\ [][Next \/ Done]_vars

\* A finished patch changed every collection it names, or none of them.
INV_AllOrNone == step = "done" => applied \in {{}, Colls}

\* Nothing is changed unless the patch is valid, so no staying field ever
\* points at a removed collection.
INV_NothingUnlessValid == applied # {} => Valid

\* A valid patch whose commit succeeds is applied in full, even when its
\* relation spans two collections.
INV_ValidPatchApplies == (step = "done" /\ Valid /\ commitOk) => applied = Colls
====
