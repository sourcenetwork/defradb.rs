---- MODULE RelationRead ----
\* The story: a public note points at a secret. Only the secret's owner, and
\* readers the owner grants, may read the secret. The owner grants and revokes
\* a reader, changes the pin, and deletes the secret, at any moment.
\*
\* The reader queries notes. Each query reaches the secret through one join
\* shape: it selects the secret, filters notes on the pin, negates that filter,
\* counts notes whose pin matches, orders notes by the pin, or reads the
\* secret's ID from the note. Some shapes drive the join from an index on the
\* secret, but they still read the same secret.
\*
\* The reader sees the secret only when it is live and the reader may read it
\* at the query's snapshot. Otherwise the note's secret is null.
\*
\* What must never happen:
\*   - a query's answer depends on a secret the reader may not see: it must be
\*     the answer the query gives when that secret is null (INV_AnswerSeesOnlyVisible).
\*
\* Code it models:
\*   crates/query/src/planner/builder/gate.rs  Planner::gate_child, GatedChild (every
\*                                             join child passes the read gate)
\*   crates/query/src/planner/builder/mod.rs   maybe_wrap_with_acp_filter (the gate itself)
\*   crates/query/src/plan/permission_filter.rs PermissionFilterNode::permission_check
\*   crates/query/src/plan/type_join/type_join_one.rs TypeJoinOne::new (takes a GatedChild)
\*   crates/query/src/plan/type_join/type_join_many/node.rs TypeJoinMany::new,
\*                                             with_filter_child_plan (take a GatedChild)
\*   crates/query/src/planner/joins/filter_only.rs apply_filter_only_joins (filter shapes)
\*   crates/query/src/planner/joins/child_plan.rs build_relation_child_plan (indexed filter child)
\*   crates/query/src/planner/joins/secondary_id.rs apply_secondary_id_joins (the ID shape)
\*   crates/query/src/planner/joins/sub_joins.rs apply_relation_sub_joins (the select shape)
EXTENDS Naturals

CONSTANTS
  MaxSteps,    \* how many changes the owner makes, at most
  GateFilters  \* TRUE: joins built only for a filter or an ID also pass the gate (the guard)

\* The owner changes things at least once.
ASSUME MaxSteps \in Nat \ {0}
\* The guard is yes-or-no.
ASSUME GateFilters \in BOOLEAN

\* The pins the owner may set. "4721" is the one the reader probes for.
Pins == {"4721", "1111"}

\* The pin the reader's filters ask about.
Probe == "4721"

\* Null: what the reader gets for a secret it may not see.
Null == "null"

\* The ways a query reaches the secret.
Shapes == {"select", "filter", "not", "count", "order", "id", "indexFilter"}

\* The shapes the planner built through the selection join, which was gated.
SelectShapes == {"select"}

\* What a query sees: the secret, frozen at one moment.
Views == [pin : Pins, deleted : BOOLEAN, granted : BOOLEAN]

\* The answers a query may give: a pin or null, a match flag, a count.
Answers == Pins \cup {Null, "yes", "no", "0", "1", "first", "last", "some", "none"}

VARIABLES
  pin,      \* the secret's current pin
  deleted,  \* whether the secret is deleted
  granted,  \* whether the reader may read the secret
  steps,    \* how many changes the owner has made
  phase,    \* the reader: "idle", "reading" (snapshot taken) or "done"
  shape,    \* the join shape of the current query
  snap,     \* the snapshot the current query reads
  result    \* the answer the last query gave

\* All the variables, for stuttering steps.
vars == <<pin, deleted, granted, steps, phase, shape, snap, result>>

\* The store's state right now, as a query would see it.
Now == [pin |-> pin, deleted |-> deleted, granted |-> granted]

\* Whether the reader may see the secret in view v: live and readable.
Visible(v) == ~v.deleted /\ v.granted

\* Whether the query's join for shape s passes the gate. The selection join
\* always did. The others do only with the guard.
Gated(s) == s \in SelectShapes \/ GateFilters

\* The secret as the join hands it to the note. A deleted secret is never
\* scanned. A gated join drops a secret the reader may not read.
Joined(s, v) ==
  IF v.deleted THEN Null                         \* the scan skips deleted secrets
  ELSE IF Gated(s) /\ ~v.granted THEN Null       \* the gate drops an unreadable secret
  ELSE v.pin                                     \* the join hands over the pin

\* The answer of shape s when the note's secret is x (a pin or null).
Eval(s, x) ==
  CASE s = "select" -> x                                          \* the pin is shown
    [] s = "filter" -> IF x = Probe THEN "yes" ELSE "no"          \* the note matches
    [] s = "indexFilter" -> IF x = Probe THEN "yes" ELSE "no"     \* same, index-driven
    [] s = "not" -> IF x = Probe THEN "no" ELSE "yes"             \* the note matches the negation
    [] s = "count" -> IF x = Probe THEN "1" ELSE "0"              \* how many notes match
    [] s = "order" -> IF x = Probe THEN "last" ELSE "first"       \* where the note sorts
    [] s = "id" -> IF x = Null THEN "none" ELSE "some"            \* whether an ID is shown

\* The answer the reader should get: the one for the secret it may see.
Expected(s, v) == Eval(s, IF Visible(v) THEN v.pin ELSE Null)

\* The type of every variable.
TypeOK ==
  /\ pin \in Pins                                   \* a pin
  /\ deleted \in BOOLEAN /\ granted \in BOOLEAN     \* two flags
  /\ steps \in 0..MaxSteps                          \* changes made
  /\ phase \in {"idle", "reading", "done"}          \* reader phase
  /\ shape \in Shapes                               \* a join shape
  /\ snap \in Views                                 \* a snapshot
  /\ result \in Answers                             \* an answer

\* At the start the secret holds the probed pin and the reader may read it.
Init ==
  /\ pin = Probe                                    \* the pin the reader hunts
  /\ deleted = FALSE                                \* live
  /\ granted = TRUE                                 \* readable
  /\ steps = 0                                      \* no changes yet
  /\ phase = "idle"                                 \* no query running
  /\ shape = "select"                               \* any shape before a query
  /\ snap = Now                                     \* any view before a query
  /\ result = Null                                  \* no answer yet

\* The owner sets the pin to p.
SetPin(p) ==
  /\ steps < MaxSteps /\ ~deleted                   \* changes left, still live
  /\ pin' = p                                       \* the new pin
  /\ steps' = steps + 1                             \* one more change
  /\ UNCHANGED <<deleted, granted, phase, shape, snap, result>>

\* The owner grants or revokes the reader.
SetGrant(g) ==
  /\ steps < MaxSteps /\ granted # g                \* changes left, a real change
  /\ granted' = g                                   \* granted or revoked
  /\ steps' = steps + 1                             \* one more change
  /\ UNCHANGED <<pin, deleted, phase, shape, snap, result>>

\* The owner deletes the secret.
Delete ==
  /\ steps < MaxSteps /\ ~deleted                   \* changes left, still live
  /\ deleted' = TRUE                                \* now deleted
  /\ steps' = steps + 1                             \* one more change
  /\ UNCHANGED <<pin, granted, phase, shape, snap, result>>

\* A query of shape s starts and takes its snapshot.
Start(s) ==
  /\ phase \in {"idle", "done"}                     \* no query running
  /\ phase' = "reading"                             \* one is now
  /\ shape' = s                                     \* through this join shape
  /\ snap' = Now                                    \* reading this moment
  /\ UNCHANGED <<pin, deleted, granted, steps, result>>

\* The query answers from what its join handed the note.
Answer ==
  /\ phase = "reading"                              \* a query is running
  /\ result' = Eval(shape, Joined(shape, snap))     \* computed from the joined secret
  /\ phase' = "done"                                \* and finished
  /\ UNCHANGED <<pin, deleted, granted, steps, shape, snap>>

\* Every step the system can take.
Next ==
  \/ \E p \in Pins : SetPin(p)                      \* a pin change
  \/ \E g \in BOOLEAN : SetGrant(g)                 \* a grant or a revoke
  \/ Delete                                         \* a delete
  \/ \E s \in Shapes : Start(s)                     \* a query starts
  \/ Answer                                         \* a query answers

\* The whole behaviour: start, then take steps.
Spec == Init /\ [][Next]_vars

\* A finished query's answer is the one it gives when every secret the reader
\* may not see is null, so no answer tells the reader anything about it.
INV_AnswerSeesOnlyVisible ==
  phase = "done" => result = Expected(shape, snap)

====
