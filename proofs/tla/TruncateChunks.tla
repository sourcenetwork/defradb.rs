---- MODULE TruncateChunks ----
\* The story: an operator truncates a collection whose text field is
\* `@chunk`. Truncate empties the chunk collection and the parent collection,
\* each in its own steps, while a writer may still create an article. A create
\* writes the article and its chunks in one transaction.
\*
\* What must never happen: once truncate is done, a live article whose chunks
\* are gone (INV_LiveParentHasChunks). While it runs, an article it is about to
\* remove may already have lost its chunks; truncate is not one transaction.
\* Chunks left behind with no live article are allowed: readers hide them,
\* because no live article lists them.
\*
\* A purge is a second way to remove articles. It removes the chosen articles
\* and their chunks in one transaction (it runs again if it conflicts, so it
\* commits whole or not at all). Once it commits, no chunk of a purged article
\* may remain (INV_PurgedHaveNoChunks).
\*
\* Code it models:
\*   crates/db/src/collection/ops/truncate/all.rs  truncate_all (the order of the steps)
\*   crates/db/src/write/autocommit/one_op.rs  a create on a chunked collection, one batch
\*   crates/db/src/collection/ops/purge.rs  purge_documents (one filtered page or purge,
\*     parents and their chunks, in one transaction that runs again on a conflict)
\* Plain numbers are all this model needs.
EXTENDS Naturals

CONSTANTS
  Articles,      \* the articles a writer may create, for example {a1, a2}
  ChunksFirst,   \* TRUE: truncate empties the chunks before the parents (the fix)
  PurgeCascades  \* TRUE: a purge removes the chunks of the articles it purges (the fix)

\* Each switch is yes-or-no.
ASSUME ChunksFirst \in BOOLEAN
ASSUME PurgeCascades \in BOOLEAN

VARIABLES
  parents,   \* the live articles
  chunks,    \* the articles whose chunks are stored
  phase,     \* where truncate is: "start", "mid" or "done"
  purged     \* the articles a purge removed and nobody has created again since

\* All the variables, for stuttering steps.
vars == <<parents, chunks, phase, purged>>

\* The type of every variable.
TypeOK ==
  /\ parents \subseteq Articles               \* live articles are articles
  /\ chunks \subseteq Articles                \* chunks belong to articles
  /\ phase \in {"start", "mid", "done"}     \* truncate's three points
  /\ purged \subseteq Articles               \* purged ones are articles

\* At the start one article exists with its chunks; the others may be created.
Init ==
  /\ parents = {CHOOSE a \in Articles : TRUE}  \* one article is live
  /\ chunks = parents                         \* with its chunks
  /\ phase = "start"                          \* truncate has not begun
  /\ purged = {}                              \* nothing was purged

\* A writer creates article a: the article and its chunks in one transaction.
Create(a) ==
  /\ a \notin parents             \* a is not live yet
  /\ parents' = parents \cup {a}  \* the article is written
  /\ chunks' = chunks \cup {a}    \* with its chunks, in the same transaction
  /\ purged' = purged \ {a}       \* a created again is no longer a purged one
  /\ UNCHANGED phase              \* truncate does not move

\* A purge removes the live articles in `S` (at least one) in one transaction.
Purge(S) ==
  /\ S # {}                                   \* it purges something
  /\ S \subseteq parents                      \* and only live articles
  /\ parents' = parents \ S                  \* the articles are gone
  /\ IF PurgeCascades                         \* with the fix,
     THEN chunks' = chunks \ S                \* their chunks go in the same transaction;
     ELSE UNCHANGED chunks                    \* with the bug, the chunks stay behind
  /\ purged' = purged \cup S                  \* remember what was purged
  /\ UNCHANGED phase                          \* truncate does not move

\* Truncate's first step empties one of the two collections.
First ==
  /\ phase = "start"                          \* truncate is just starting
  /\ IF ChunksFirst                           \* with the fix,
     THEN /\ chunks' = {} /\ UNCHANGED parents \* the chunks go first;
     ELSE /\ parents' = {} /\ UNCHANGED chunks \* with the bug, the parents do
  /\ phase' = "mid"                           \* half done
  /\ UNCHANGED purged                         \* a purge is not involved

\* Truncate's second step empties the other one.
Second ==
  /\ phase = "mid"                            \* truncate is half done
  /\ IF ChunksFirst                           \* with the fix,
     THEN /\ parents' = {} /\ UNCHANGED chunks \* the parents go second;
     ELSE /\ chunks' = {} /\ UNCHANGED parents \* with the bug, the chunks do
  /\ phase' = "done"                          \* truncate is finished
  /\ UNCHANGED purged                         \* a purge is not involved

\* Every step: a create, a purge, or truncate moving on. Creates go on after truncate.
Next == (\E a \in Articles : Create(a)) \/ (\E S \in SUBSET Articles : Purge(S)) \/ First \/ Second

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* After truncate, every live article still has its chunks.
INV_LiveParentHasChunks == phase = "done" => parents \subseteq chunks

\* A purged article leaves no chunks behind.
INV_PurgedHaveNoChunks == purged \cap chunks = {}
====
