---- MODULE DuplicateIndex ----
\* The story, in plain words. Two callers each ask a collection for a new
\* index. Each request runs in a transaction: it reads the collection's
\* definition (the list of its indexes), checks that no listed index has the
\* same definition as the one it wants, and writes the definition back with
\* its index added. regolith validates the definition read at commit: when
\* another create committed first, this attempt conflicts and runs again,
\* reading the new definition.
\*
\* What must never happen: the collection lists the same definition twice;
\* an index a caller was told it created is missing from the definition; a
\* caller is refused although no listed index matches its request.
\*
\* Rust: crates/database/db/src/index/duplicate.rs refuse_duplicate_index,
\* called from crates/database/db/src/index/create.rs DefineIndexAttempt::run and
\* crates/database/db/src/index/in_txn.rs DB::create_index_in (index creation
\* requests only; ensure_ordinary_index does not run it);
\* the validated definition read is
\* crates/database/db/src/txn/mod.rs DbTxn::validate_collection_for_write.
EXTENDS Naturals, Sequences, FiniteSets

\* Check: the create transaction refuses a copy of a listed definition.
\* Validate: the commit conflicts when the definition changed since the read.
CONSTANTS Check, Validate

\* The two callers, and the definitions they may ask for. Two callers asking
\* for "a" is the race the guard must win; "a" and "b" must both succeed.
Callers == {"c1", "c2"}
Defs == {"a", "b"}

VARIABLES
  defs,    \* the committed definition: the sequence of listed index definitions
  version, \* how many times the definition was rewritten, which a commit validates
  want,    \* the definition each caller asks for
  pc,      \* each caller's step: "idle", "read", "created" or "refused"
  snap,    \* the definition each caller's transaction read
  seen     \* the version each caller's transaction read

vars == <<defs, version, want, pc, snap, seen>>

\* The set of definitions listed in a sequence.
Listed(s) == {s[i] : i \in 1..Len(s)}

\* Start with no index; each caller picks the definition it wants.
Init ==
  /\ defs = <<>>
  /\ version = 0
  /\ want \in [Callers -> Defs]
  /\ pc = [c \in Callers |-> "idle"]
  /\ snap = [c \in Callers |-> <<>>]
  /\ seen = [c \in Callers |-> 0]

\* A caller's transaction reads the definition and its version.
Read(c) ==
  /\ pc[c] = "idle"
  /\ pc' = [pc EXCEPT ![c] = "read"]
  /\ snap' = [snap EXCEPT ![c] = defs]
  /\ seen' = [seen EXCEPT ![c] = version]
  /\ UNCHANGED <<defs, version, want>>

\* With the guard on, a request that repeats a definition it read is refused
\* and writes nothing.
Refuse(c) ==
  /\ pc[c] = "read"
  /\ Check
  /\ want[c] \in Listed(snap[c])
  /\ pc' = [pc EXCEPT ![c] = "refused"]
  /\ UNCHANGED <<defs, version, want, snap, seen>>

\* The commit finds the definition rewritten since the read: the attempt
\* conflicts and the caller runs again from a fresh read.
Conflict(c) ==
  /\ pc[c] = "read"
  /\ Validate
  /\ seen[c] # version
  /\ pc' = [pc EXCEPT ![c] = "idle"]
  /\ UNCHANGED <<defs, version, want, snap, seen>>

\* The check passed (or is off) and the commit is allowed: the definition it
\* read, with the new index added, replaces the committed one.
Commit(c) ==
  /\ pc[c] = "read"
  /\ ~(Check /\ want[c] \in Listed(snap[c]))
  /\ ~(Validate /\ seen[c] # version)
  /\ defs' = Append(snap[c], want[c])
  /\ version' = version + 1
  /\ pc' = [pc EXCEPT ![c] = "created"]
  /\ UNCHANGED <<want, snap, seen>>

\* Any caller takes its next step.
Next == \E c \in Callers : Read(c) \/ Refuse(c) \/ Conflict(c) \/ Commit(c)

\* The whole behaviour: start, then take steps.
Spec == Init /\ [][Next]_vars

\* No definition is listed twice: every position holds a different one.
INV_NoDuplicate ==
  \A i, j \in 1..Len(defs) : i # j => defs[i] # defs[j]

\* A caller told its index was created finds it in the definition.
INV_NoLostIndex ==
  \A c \in Callers : pc[c] = "created" => want[c] \in Listed(defs)

\* A caller is refused only when the definition lists what it asked for.
INV_RefusedOnlyCopies ==
  \A c \in Callers : pc[c] = "refused" => want[c] \in Listed(defs)
====
