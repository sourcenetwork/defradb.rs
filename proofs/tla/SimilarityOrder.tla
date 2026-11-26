---- MODULE SimilarityOrder ----
\* The story: a reader runs a query that scores documents by SIMILARITY to a
\* query vector, at the root or inside a nested select. Each document passes
\* through four steps, one after the other: the row filter, the access check,
\* the scoring, and the filter on the score's alias. Several documents move
\* through the steps at once, in any interleaving.
\*
\* Scoring reads the document's vector. That read is observable: a score can
\* be returned, ordered by or filtered on, and a vector of the wrong length
\* fails the whole query. So a document the reader may not read must never
\* be scored. The alias filter reads the score, so it must run after scoring,
\* or it drops every row for want of a score.
\*
\* What must never happen:
\*   - a document the reader may not read is scored (INV_HiddenNeverScored);
\*   - a finished query returns other rows than the readable, matching
\*     documents whose score passes the alias filter (INV_AnswerIsExact).
\*
\* Code it models:
\*   crates/engine/query/src/planner/builder/mod.rs Planner::plan_with_index_info (maybe_wrap_with_acp_filter,
\*                                          then add_similarity_nodes and add_chunk_similarity_nodes,
\*                                          then apply_similarity_alias_filter)
\*   crates/engine/query/src/planner/joins/sub_joins.rs Planner::apply_relation_sub_joins (split_for_scores, the row
\*                                          filter, maybe_wrap_with_acp_filter, the scoring nodes, then
\*                                          the alias filter)
\*   crates/engine/query/src/mapper/filter/split.rs Filter::split_for_scores (the row part and the alias part)
EXTENDS Naturals, Sequences, FiniteSets  \* numbers, the step order, and sets

CONSTANTS
  ScoreAfterAccess, \* TRUE: scoring runs after the access check (the guard)
  AliasAfterScore   \* TRUE: the alias filter runs after scoring (the guard)

\* Both switches are yes-or-no.
ASSUME ScoreAfterAccess \in BOOLEAN /\ AliasAfterScore \in BOOLEAN

\* Two documents are enough: one may be hidden while the other shows.
Docs == {"d1", "d2"}

\* The order of the steps the plan builds. Each RED swaps the two steps its guard keeps apart.
Order ==
  IF ~ScoreAfterAccess THEN <<"filter", "score", "access", "alias">>      \* scores before the check
  ELSE IF ~AliasAfterScore THEN <<"filter", "access", "alias", "score">>  \* filters on a missing score
  ELSE <<"filter", "access", "score", "alias">>                           \* the plan's order

\* How many steps there are.
Steps == Len(Order)

VARIABLES
  hidden,  \* for each document, whether the reader may not read it
  matches, \* for each document, whether it passes the row filter
  near,    \* for each document, whether its score passes the alias filter
  pc,      \* for each document, the next step it takes, Steps + 1 once past the last
  alive,   \* for each document, whether every step so far kept it
  score,   \* for each document, its score: "none" until scored, then "near" or "far"
  scored,  \* the documents whose vectors were read
  out      \* the rows the query returned

\* All the variables, for stuttering steps.
vars == <<hidden, matches, near, pc, alive, score, scored, out>>

\* The type of every variable.
TypeOK ==
  /\ hidden \in [Docs -> BOOLEAN]                 \* a flag per document
  /\ matches \in [Docs -> BOOLEAN]                \* a flag per document
  /\ near \in [Docs -> BOOLEAN]                   \* a flag per document
  /\ pc \in [Docs -> 1..(Steps + 1)]              \* a step per document
  /\ alive \in [Docs -> BOOLEAN]                  \* a flag per document
  /\ score \in [Docs -> {"none", "near", "far"}]  \* a score per document
  /\ scored \subseteq Docs                        \* the scored documents
  /\ out \subseteq Docs                           \* the returned rows

\* Any mix of documents: hidden or not, matching or not, near or not.
Init ==
  /\ hidden \in [Docs -> BOOLEAN]       \* who may read what
  /\ matches \in [Docs -> BOOLEAN]      \* what the row filter keeps
  /\ near \in [Docs -> BOOLEAN]         \* what the alias filter would keep once scored
  /\ pc = [d \in Docs |-> 1]            \* every document starts at the first step
  /\ alive = [d \in Docs |-> TRUE]      \* nothing dropped yet
  /\ score = [d \in Docs |-> "none"]    \* nothing scored yet
  /\ scored = {}                        \* no vector read yet
  /\ out = {}                           \* no row returned yet

\* Document d takes its next step, if no earlier step dropped it.
Step(d) ==
  /\ alive[d] /\ pc[d] <= Steps                         \* still in the plan, steps left
  /\ LET s == Order[pc[d]] IN                           \* the step it takes
       /\ alive' = [alive EXCEPT ![d] =                 \* whether it stays
            CASE s = "filter" -> matches[d]             \* the row filter keeps a match
              [] s = "access" -> ~hidden[d]             \* the check keeps a readable one
              [] s = "score"  -> TRUE                   \* scoring drops nothing
              [] s = "alias"  -> score[d] = "near"]     \* a missing score fails the alias filter
       /\ score' = IF s = "score"                       \* scoring sets the score
                   THEN [score EXCEPT ![d] = IF near[d] THEN "near" ELSE "far"]
                   ELSE score                           \* any other step leaves it
       /\ scored' = IF s = "score" THEN scored \cup {d} ELSE scored  \* and reads the vector
  /\ pc' = [pc EXCEPT ![d] = pc[d] + 1]                 \* on to the next step
  /\ UNCHANGED <<hidden, matches, near, out>>

\* Document d passed every step and is returned.
Emit(d) ==
  /\ alive[d] /\ pc[d] = Steps + 1                      \* kept by every step
  /\ d \notin out                                       \* not returned yet
  /\ out' = out \cup {d}                                \* returned now
  /\ UNCHANGED <<hidden, matches, near, pc, alive, score, scored>>

\* Every step anyone can take.
Next == \E d \in Docs : Step(d) \/ Emit(d)

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Whether the query is over: every document was dropped or returned.
Finished == \A d \in Docs : ~alive[d] \/ d \in out

\* A document the reader may not read is never scored, so nothing about its vector shows.
INV_HiddenNeverScored == \A d \in scored : ~hidden[d]

\* A finished query returns exactly the readable, matching documents whose score passes the alias filter.
INV_AnswerIsExact ==
  Finished => out = {d \in Docs : matches[d] /\ ~hidden[d] /\ near[d]}
====
