---------------------------- MODULE IndexFormatGuard ----------------------------
\* The story: a store carries a stamp saying which index key layout wrote it.
\* The current layout tags index keys with `/i/`; older layouts did not. At open
\* the node compares the stamp with the layout it knows. A stamp from another
\* layout is refused. A stamp that is missing is ambiguous: a brand-new store has
\* no stamp and no collections, while a store written before stamps existed may
\* already hold collections. The latter must be refused, not guessed at. A new
\* empty store is stamped so a later open sees it as current.
\*
\* Who acts: the open check, the fresh-store stamp, and the store's collection
\* set as the guard sees it.
\*
\* What must never happen: the node opens an index layout it cannot read, or
\* changes a store it has just refused.
\*
\* Code it models:
\*   crates/database/db/src/definition/migration/format.rs
\*     CURRENT_INDEX_FORMAT, DB::stored_index_format, DB::migrate_index_format
\*   crates/database/db/src/database/mod.rs DB::finish_open

EXTENDS Integers, FiniteSets

CONSTANTS
  Collections,         \* the collections the store may hold
  RefusesOld,          \* "On": a non-current stamp is refused | "Off": the guard accepts it
  RefusesUnstamped,    \* "On": a missing stamp with collections is refused | "Off": accepted
  StampsFresh,         \* "On": a new empty store gets the current stamp | "Off": left unstamped
  MutatesOnRefusal     \* "Off": a refusal writes nothing | "On": the refusal mutates the store

ASSUME RefusesOld \in {"On", "Off"}
ASSUME RefusesUnstamped \in {"On", "Off"}
ASSUME StampsFresh \in {"On", "Off"}
ASSUME MutatesOnRefusal \in {"On", "Off"}

VARIABLES
  stamp,             \* "Current", "Old" (any earlier layout), or "None"
  storedCollections, \* the active collections on the store
  phase,             \* "Check", "Open", or "Refused"
  mutated            \* whether the open path has written the store

vars == <<stamp, storedCollections, phase, mutated>>

\* Every variable stays in its range.
TypeOK ==
  /\ stamp \in {"Current", "Old", "None"}
  /\ storedCollections \subseteq Collections
  /\ phase \in {"Check", "Open", "Refused"}
  /\ mutated \in BOOLEAN

\* A store starts with any stamp and any collection set, ready for the guard.
Init ==
  /\ stamp \in {"Current", "Old", "None"}
  /\ storedCollections \in SUBSET Collections
  /\ phase = "Check"
  /\ mutated = FALSE

\* A brand-new empty store gets the current stamp before the guard runs.
FreshStamp ==
  /\ phase = "Check"
  /\ stamp = "None"
  /\ storedCollections = {}
  /\ StampsFresh = "On"
  /\ stamp' = "Current"
  /\ UNCHANGED <<storedCollections, phase, mutated>>

\* The open-time guard. `Compatible` is the only state it may open.
Compatible ==
  \/ stamp = "Current"
  \/ stamp = "None" /\ RefusesUnstamped = "Off"
  \/ stamp = "Old" /\ RefusesOld = "Off"

Evaluate ==
  /\ phase = "Check"
  /\ \/ StampsFresh = "Off"
     \/ stamp /= "None"
     \/ storedCollections /= {}
  /\ IF Compatible
     THEN /\ phase' = "Open"
          /\ mutated' = FALSE
     ELSE /\ phase' = "Refused"
          /\ mutated' = (MutatesOnRefusal = "On")
  /\ UNCHANGED <<stamp, storedCollections>>

\* Any one step happens next.
Next == FreshStamp \/ Evaluate

\* Start in Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK == TypeOK

\* An open store uses the current layout, or is a new empty store not yet stamped.
INV_OpenCompatible ==
  phase = "Open" =>
    \/ stamp = "Current"
    \/ stamp = "None" /\ storedCollections = {}

\* A refusal leaves the store untouched.
INV_RefusalReadOnly == phase = "Refused" => ~mutated

\* An unstamped store with no collections is new, not legacy: it opens.
INV_FreshStoreOpens ==
  (phase = "Refused" /\ stamp = "None") => storedCollections /= {}

====
