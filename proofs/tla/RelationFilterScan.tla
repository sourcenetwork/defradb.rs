---- MODULE RelationFilterScan ----
\* The story: a client deletes (or updates) the authors whose books match a
\* filter, such as "authors with a Dystopia book". The filter goes through a
\* relation, so the database answers it the way a query does: inside the
\* mutation's transaction it scans the books and picks the authors to delete.
\* The delete then writes only the author, never the books.
\*
\* Who acts: the deleter, which reads the books from its snapshot, decides,
\* and commits; and a writer, which changes the books (a genre edit, a new
\* book, a removed one) and commits, at any moment.
\*
\* What can happen: the writer commits after the deleter took its snapshot.
\* The deleter's decision then rests on books that are no longer true. An
\* unvalidated scan lets that commit go through; a validated scan of the
\* books' range makes regolith refuse it, and the client retries against the
\* new books.
\*
\* What must never happen: a committed delete whose decision no longer holds
\* for the books as they stand when it commits; and an abort with no
\* concurrent write to the scanned books.
\*
\* Code it models:
\*   crates/query/src/runner/mutation_inputs.rs  resolve_filter_to_doc_ids, which turns validated scans on while the planner resolves a relation filter
\*   crates/datastore/src/multistore.rs          SharedTxn::set_scans_validated and SharedTxn::iterator, which opens each scan validated while the flag is on
\*   crates/db/src/read/doc.rs                   DbDocFetcher::set_scans_validated (and the lensed fetcher's), which set the flag on the fetcher's transaction
EXTENDS FiniteSets   \* sets of books, and comparing them with the empty set

CONSTANTS
  Books,      \* the book slots: a book that exists, or one a write may add
  Validated   \* whether the deleter's scans are validated at commit

\* At least one book, so the filter can match something.
ASSUME Books # {}
\* The flag is a plain yes or no.
ASSUME Validated \in BOOLEAN

VARIABLES
  committed,     \* the books that match the filter in committed state
  snapshot,      \* the books the deleter saw match when it scanned
  dStep,         \* the deleter: "scan", then "commit", then "done"
  wStep,         \* the writer: "idle", then "done"
  rangeWritten,  \* the writer committed to the books after the deleter's scan
  outcome,       \* the deleter's end: "none", "committed" or "aborted"
  deleted,       \* the committed delete removed the author
  atCommit       \* the matching books in committed state when the deleter committed

\* Every variable, for saying a step leaves them all as they are.
vars == <<committed, snapshot, dStep, wStep, rangeWritten, outcome, deleted, atCommit>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ committed \subseteq Books                         \* matching books are book slots
  /\ snapshot \subseteq Books                          \* so are the ones the deleter saw
  /\ dStep \in {"scan", "commit", "done"}              \* the deleter's three stages
  /\ wStep \in {"idle", "done"}                        \* the writer writes once
  /\ rangeWritten \in BOOLEAN                          \* a yes or no
  /\ outcome \in {"none", "committed", "aborted"}      \* the deleter's three ends
  /\ deleted \in BOOLEAN                               \* a yes or no
  /\ atCommit \subseteq Books                          \* matching books at the commit

\* The start: any books may match, nothing has been read, written or decided.
Init ==
  /\ committed \in SUBSET Books   \* every starting set of matching books is explored
  /\ snapshot = {}                \* the deleter has not scanned yet
  /\ dStep = "scan"               \* its first move is the scan
  /\ wStep = "idle"               \* the writer has not written
  /\ rangeWritten = FALSE         \* so nothing was written after a scan
  /\ outcome = "none"             \* the deleter has not ended
  /\ deleted = FALSE              \* nothing is deleted
  /\ atCommit = {}                \* and no commit has happened

\* The deleter scans the books from its snapshot: it sees the committed matches.
Scan ==
  /\ dStep = "scan"                                     \* only once, first
  /\ snapshot' = committed                              \* its snapshot is what is committed now
  /\ dStep' = "commit"                                  \* next it commits
  /\ UNCHANGED <<committed, wStep, rangeWritten, outcome, deleted, atCommit>>   \* nothing else moves

\* The writer commits any change to the books: matches may appear, go, or stay.
Write ==
  /\ wStep = "idle"                                     \* the writer writes once
  /\ \E next \in SUBSET Books : committed' = next       \* any new set of matching books
  /\ rangeWritten' = (dStep # "scan")                   \* it lands in the scanned range only after the scan
  /\ wStep' = "done"                                    \* the writer is finished
  /\ UNCHANGED <<snapshot, dStep, outcome, deleted, atCommit>>   \* the deleter is untouched

\* The deleter commits. A validated scan with a write after it aborts; otherwise
\* the delete commits, removing the author if its snapshot saw a match.
Commit ==
  /\ dStep = "commit"                                   \* only after the scan
  /\ dStep' = "done"                                    \* the deleter ends either way
  /\ IF Validated /\ rangeWritten                       \* regolith finds a write in the validated range
       THEN /\ outcome' = "aborted"                     \* so the commit is refused
            /\ UNCHANGED <<deleted, atCommit>>          \* and nothing is deleted
       ELSE /\ outcome' = "committed"                   \* the commit goes through
            /\ deleted' = (snapshot # {})               \* deleting the author if its scan matched
            /\ atCommit' = committed                    \* while the books stand like this
  /\ UNCHANGED <<committed, snapshot, wStep, rangeWritten>>   \* the books and the writer stay

\* One step of either actor.
Next == Scan \/ Write \/ Commit

\* The whole behaviour: start, then any sequence of steps.
Spec == Init /\ [][Next]_vars

\* A committed delete is right for the books as they stand when it commits:
\* the author goes exactly when a book still matches.
INV_DecisionHoldsAtCommit ==
  outcome = "committed" => (deleted <=> atCommit # {})

\* The deleter aborts only because a write landed in the books it scanned.
INV_AbortOnlyOnRangeWrite ==
  outcome = "aborted" => rangeWritten
====
