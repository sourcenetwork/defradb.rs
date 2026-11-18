---- MODULE SelectedUpdate ----
\* The story: a caller selects a document, then updates it. The selection
\* happens before the update's transaction, so another writer may change the
\* document in between. The caller names the fields it compared (say the title
\* its filter matched), and the update writes other fields (say the body).
\*
\* The update's attempt reads the document again and compares it with the
\* selection. If a field it depends on changed, the update lost a race: it
\* fails with a conflict and the caller runs it again from a fresh selection.
\* Otherwise it writes. The fields it depends on are the ones its caller
\* compared and the ones it writes. A change to any other field is not its
\* business, so it must not make the update lose.
\*
\* A collection with `@chunk` fields runs the same update through a batch of
\* one. Both paths take the fields from one helper, so they agree.
\*
\* What must never happen:
\*   - an update writes over a change to a field it writes, made after its
\*     selection, without losing the race (INV_NoSilentOverwrite);
\*   - an update loses the race when only a field it doesn't depend on
\*     changed (INV_LosesOnlyToItsFields).
\*
\* Code it models:
\*   crates/db/src/write/autocommit/selected.rs dependencies, unchanged
\*   crates/db/src/write/autocommit/update.rs   run_update_attempt (the plain path)
\*   crates/db/src/write/autocommit/one_op.rs   UpdateWork::run (the batch of one, for `@chunk`)
\* Commit validation of what the attempt read is regolith's; the model takes it as given.
EXTENDS Naturals

CONSTANTS
  Compare,  \* which fields the attempt compares: "Dependencies" (the guard), "ExpectedOnly" or "Whole"
  MaxRuns   \* how many times the caller may run the update

\* The compare mode is one of the three.
ASSUME Compare \in {"Dependencies", "ExpectedOnly", "Whole"}
\* The caller runs it at least once.
ASSUME MaxRuns \in Nat \ {0}

\* The document's fields.
Fields == {"title", "body", "other"}

\* The fields the caller compared, and the field the update writes.
Expected == {"title"}
Written == {"body"}

\* What the update depends on.
Dependencies == Expected \cup Written

\* The fields an attempt compares, by mode.
Compared == CASE Compare = "Dependencies" -> Dependencies   \* the guard
              [] Compare = "ExpectedOnly" -> Expected       \* the bug: the written field is skipped
              [] Compare = "Whole"        -> Fields         \* too much: every field

\* The values a field can hold: the start, another writer's, the update's.
Values == {"v1", "v2", "new"}

VARIABLES
  doc,        \* the stored document
  sel,        \* the caller's selection of it
  phase,      \* the caller: "selected", "done" (wrote) or "lost" (lost the race)
  runs,       \* how many times the caller has selected
  changed,    \* the fields the other writer has changed
  overwrote,  \* the update ever wrote over a change it didn't see
  wrongLoss   \* the update ever lost when none of its fields had changed

\* All the variables, for stuttering steps.
vars == <<doc, sel, phase, runs, changed, overwrote, wrongLoss>>

\* The type of every variable.
TypeOK ==
  /\ doc \in [Fields -> Values]                   \* a document
  /\ sel \in [Fields -> Values]                   \* a selection
  /\ phase \in {"selected", "done", "lost"}       \* the caller's phase
  /\ runs \in 1..MaxRuns                          \* runs so far
  /\ changed \subseteq Fields                     \* changed fields
  /\ overwrote \in BOOLEAN /\ wrongLoss \in BOOLEAN  \* two flags

\* The document starts at v1, and the caller has selected it once.
Init ==
  /\ doc = [f \in Fields |-> "v1"]                \* every field at v1
  /\ sel = doc                                    \* selected as it is
  /\ phase = "selected"                           \* about to update
  /\ runs = 1                                     \* the first run
  /\ changed = {}                                 \* no other write yet
  /\ overwrote = FALSE                            \* nothing overwritten
  /\ wrongLoss = FALSE                            \* nothing lost

\* The other writer changes field f once, at any moment.
Change(f) ==
  /\ f \notin changed                             \* not changed yet
  /\ doc' = [doc EXCEPT ![f] = "v2"]              \* its new value
  /\ changed' = changed \cup {f}                  \* remembered
  /\ UNCHANGED <<sel, phase, runs, overwrote, wrongLoss>>

\* Whether the stored document still matches the selection on the compared fields.
Unchanged == \A f \in Compared : doc[f] = sel[f]

\* The update's attempt finds its compared fields unchanged and writes the body.
Write ==
  /\ phase = "selected"                           \* the caller is updating
  /\ Unchanged                                    \* nothing compared changed
  /\ overwrote' = (overwrote \/ \E f \in Written : doc[f] # sel[f])  \* bad if a written field had changed
  /\ doc' = [f \in Fields |-> IF f \in Written THEN "new" ELSE doc[f]]  \* the body is written
  /\ phase' = "done"                              \* the update is done
  /\ UNCHANGED <<sel, runs, changed, wrongLoss>>

\* The attempt finds a compared field changed: the update lost the race.
Lose ==
  /\ phase = "selected"                           \* the caller is updating
  /\ ~Unchanged                                   \* a compared field changed
  /\ wrongLoss' = (wrongLoss \/ \A f \in Dependencies : doc[f] = sel[f])  \* bad if none of its fields did
  /\ phase' = "lost"                              \* it reports a conflict
  /\ UNCHANGED <<doc, sel, runs, changed, overwrote>>

\* The caller runs the update again from a fresh selection.
Rerun ==
  /\ phase = "lost"                               \* it lost last time
  /\ runs < MaxRuns                               \* runs left
  /\ sel' = doc                                   \* a fresh selection
  /\ phase' = "selected"                          \* updating again
  /\ runs' = runs + 1                             \* one more run
  /\ UNCHANGED <<doc, changed, overwrote, wrongLoss>>

\* Every step anyone can take.
Next ==
  \/ \E f \in Fields : Change(f)  \* the other writer writes,
  \/ Write                        \* the update writes,
  \/ Lose                         \* the update loses,
  \/ Rerun                        \* or the caller runs it again

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* An update never writes over a change to a field it writes that it didn't see.
INV_NoSilentOverwrite == ~overwrote

\* An update loses the race only when a field it depends on changed.
INV_LosesOnlyToItsFields == ~wrongLoss
====
