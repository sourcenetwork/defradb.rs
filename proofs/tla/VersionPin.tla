---- MODULE VersionPin ----
\* The story, in plain words.
\*
\* A collection has several versions; one is active. A caller may ask for a
\* document operation through a particular version (CollectionOptions'
\* version), inactive ones included, as Go's C bindings allow. Every lookup
\* of a collection by name goes through the transaction's catalog, so the
\* request pins the selected version there for its turn: every name lookup in
\* that turn sees the pinned version, and the pin is cleared when the turn
\* ends, so a later request on the same transaction sees the active version
\* again.
\*
\* A write through the pinned version is recorded at that version, as Go
\* records it. A read migrates the document to the version it reads through;
\* reading through the active version may store the migrated document back
\* (a cache), but reading through any other version must not, or a read
\* would quietly rewrite the document at an old version.
\*
\* Who acts: requests on one transaction, one turn at a time (the transaction
\* runs one request at a time), and DDL between turns that makes another
\* version active.
\*
\* What must never happen:
\*   1. a pin is still set when no turn runs - INV_PinScoped;
\*   2. a lookup in a turn that pinned nothing sees a version other than the
\*      active one - INV_UnpinnedSeesActive;
\*   3. a read through a non-active version changes the stored document -
\*      INV_ReadNoSideEffect;
\*   4. a write lands at a version other than the one its turn selected -
\*      INV_WriteAtSelected.
\*
\* Each guard has a RED configuration that removes it:
\*   * ClearPin: a turn's end clears its pin (Red_PinLeaks);
\*   * NoWriteBackOnPin: a read through a non-active version stores nothing
\*     back (Red_WriteBackOnPin).
\*
\* Rust: crates/db/src/txn/catalog.rs Catalog::pin, Catalog::get;
\* crates/db/src/txn/registry/owned.rs OwnedTxn's TxnTurn::pin_collection;
\* crates/query/src/runner/turn.rs run_turn_with_delegation (pins, then
\* clears); crates/query/src/runner/pinned.rs PinnedProvider (the query-side
\* lookup); crates/db/src/txn/mod.rs Txn::validate_collection_for_write;
\* crates/db/src/read/lensed/fetcher/migration.rs (no write-back through a
\* pin); crates/ffi/src/document/run.rs DocumentCall (sets the version).
EXTENDS Naturals

CONSTANTS
  Versions,          \* the collection's versions
  None,              \* stands for "no version": no pin, no document yet
  MaxSteps,          \* how many steps a run takes (bounds TLC)
  ClearPin,          \* TRUE: a turn's end clears its pin
  NoWriteBackOnPin   \* TRUE: a read through a non-active version stores nothing

\* A version, or none.
VersionOrNone == Versions \cup {None}

VARIABLES
  active,      \* the active version
  pin,         \* the version pinned in the transaction's catalog, or None
  inTurn,      \* TRUE while a request's turn runs
  selected,    \* the version the running turn asked for (pin, else active)
  stored,      \* the version the document is stored at, or None before any write
  sideEffect,  \* TRUE once a read through a non-active version changed `stored`
  misplaced,   \* TRUE once a write landed at a version its turn did not select
  strayLookup, \* TRUE once an unpinned lookup saw a non-active version
  steps        \* how many steps have run

vars == <<active, pin, inTurn, selected, stored, sideEffect, misplaced, strayLookup, steps>>

\* Every variable has the kind of value it should.
TypeOK ==
  /\ active \in Versions
  /\ pin \in VersionOrNone
  /\ inTurn \in BOOLEAN
  /\ selected \in VersionOrNone
  /\ stored \in VersionOrNone
  /\ sideEffect \in BOOLEAN
  /\ misplaced \in BOOLEAN
  /\ strayLookup \in BOOLEAN
  /\ steps \in 0..MaxSteps

\* Some version is active, nothing is pinned, no turn runs, no document yet.
Init ==
  /\ active \in Versions
  /\ pin = None
  /\ inTurn = FALSE
  /\ selected = None
  /\ stored = None
  /\ sideEffect = FALSE
  /\ misplaced = FALSE
  /\ strayLookup = FALSE
  /\ steps = 0

\* One more step may run while the run has steps left.
Step == steps < MaxSteps /\ steps' = steps + 1

\* What a name lookup answers: the pinned version, else the active one.
Lookup == IF pin # None THEN pin ELSE active

\* A request's turn begins, pinning version `v`, or nothing when `v` is None.
BeginTurn(v) ==
  /\ Step
  /\ ~inTurn
  /\ inTurn' = TRUE
  /\ pin' = v
  /\ selected' = IF v # None THEN v ELSE active
  \* A turn that pins nothing must see the active version at once.
  /\ strayLookup' = (strayLookup \/ (v = None /\ pin # None))
  /\ UNCHANGED <<active, stored, sideEffect, misplaced>>

\* The turn writes the document through the version its lookup answers.
Write ==
  /\ Step
  /\ inTurn
  /\ stored' = Lookup
  /\ misplaced' = (misplaced \/ Lookup # selected)
  /\ UNCHANGED <<active, pin, inTurn, selected, sideEffect, strayLookup>>

\* The turn reads the document through the version its lookup answers. Read
\* through the active version, the migrated document may be stored back;
\* through any other version, only with NoWriteBackOnPin off.
Read ==
  /\ Step
  /\ inTurn
  /\ stored # None
  /\ LET target == Lookup
         writesBack == stored # target /\ (target = active \/ ~NoWriteBackOnPin)
     IN /\ stored' = IF writesBack THEN target ELSE stored
        /\ sideEffect' = (sideEffect \/ (writesBack /\ target # active))
  /\ UNCHANGED <<active, pin, inTurn, selected, misplaced, strayLookup>>

\* The turn ends; its pin is cleared (unless ClearPin is off).
EndTurn ==
  /\ Step
  /\ inTurn
  /\ inTurn' = FALSE
  /\ pin' = IF ClearPin THEN None ELSE pin
  /\ selected' = None
  /\ UNCHANGED <<active, stored, sideEffect, misplaced, strayLookup>>

\* Between turns, DDL makes version `v` active.
SetActive(v) ==
  /\ Step
  /\ ~inTurn
  /\ active' = v
  /\ UNCHANGED <<pin, inTurn, selected, stored, sideEffect, misplaced, strayLookup>>

Next ==
  \/ \E v \in VersionOrNone : BeginTurn(v)
  \/ Write
  \/ Read
  \/ EndTurn
  \/ \E v \in Versions : SetActive(v)

Spec == Init /\ [][Next]_vars

\* ---- what must hold in every state ----

\* No pin is set while no turn runs.
INV_PinScoped == ~inTurn => pin = None

\* A turn that pinned nothing only ever sees the active version.
INV_UnpinnedSeesActive == ~strayLookup

\* A read through a non-active version never changed the stored document.
INV_ReadNoSideEffect == ~sideEffect

\* Every write landed at the version its turn selected.
INV_WriteAtSelected == ~misplaced
====
