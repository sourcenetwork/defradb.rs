---------------------------- MODULE IndexFormatGuard ----------------------------
\* The story: a store carries a stamp saying which index key layout wrote it.
\* The current layout tags index keys with `/i/`; older layouts did not. At open
\* the node compares the stamp with the layout it knows. A stamp newer than this
\* build is refused. Any older stamp, or a missing stamp from a store that
\* predates stamps, means the indexes are rebuilt and the current stamp is
\* written. A new empty store is stamped so a later open sees it as current.
\*
\* Who acts: the open check, the fresh-store stamp, and the rebuild-and-stamp
\* step.
\*
\* What must never happen: the node opens an index layout newer than this build
\* without rebuilding first, or leaves a rebuilt store without the current stamp.
\*
\* Code it models:
\*   crates/database/db/src/definition/migration/format.rs
\*     CURRENT_INDEX_FORMAT, DB::stored_index_format, DB::migrate_index_format
\*   crates/database/db/src/database/mod.rs DB::finish_open

EXTENDS Integers, FiniteSets

CONSTANTS
  Collections,         \* the collections the store may hold
  RefusesOld,          \* "On": a non-current stamp is refused | "Off": the guard accepts it
  RebuildsLegacy,      \* "On": an older or missing stamp rebuilds and stamps | "Off": accepted without a rebuild
  StampsFresh,         \* "On": a new empty store gets the current stamp | "Off": left unstamped
  MutatesOnRefusal     \* "Off": a refusal writes nothing | "On": the refusal mutates the store

ASSUME RefusesOld \in {"On", "Off"}
ASSUME RebuildsLegacy \in {"On", "Off"}
ASSUME StampsFresh \in {"On", "Off"}
ASSUME MutatesOnRefusal \in {"On", "Off"}

VARIABLES
  stamp,             \* "Current", "Old" (any earlier layout), or "None"
  storedCollections, \* the active collections on the store
  phase,             \* "Check", "Open", "Refused", or "Rebuilt"
  mutated            \* whether the open path has written the store

vars == <<stamp, storedCollections, phase, mutated>>

\* Every variable stays in its range.
TypeOK ==
  /\ stamp \in {"Current", "Old", "None"}
  /\ storedCollections \subseteq Collections
  /\ phase \in {"Check", "Open", "Refused", "Rebuilt"}
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

\* The open-time guard. A current store opens; a newer one is refused. The
\* rebuild path accepts every older or missing layout.
Compatible ==
  \/ stamp = "Current"
  \/ stamp = "None" /\ RebuildsLegacy = "Off"
  \/ stamp = "Old" /\ RefusesOld = "Off"

Rebuild ==
  /\ phase = "Check"
  /\ RebuildsLegacy = "On"
  /\ stamp \in {"None", "Old"}
  /\ phase' = "Rebuilt"
  /\ stamp' = "Current"
  /\ UNCHANGED <<storedCollections, mutated>>

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
Next == FreshStamp \/ Rebuild \/ Evaluate

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

\* A rebuilt store carries the layout this build writes.
INV_RebuiltIsCurrent == phase = "Rebuilt" => stamp = "Current"

\* An unstamped store with no collections is new, not legacy: it opens.
INV_FreshStoreOpens ==
  (phase = "Refused" /\ stamp = "None") => storedCollections /= {}

\* Every opened or rebuilt store reports the layout this build wrote.
INV_StampedAfterOpen ==
  phase \in {"Open", "Rebuilt"} => stamp = "Current"

====
