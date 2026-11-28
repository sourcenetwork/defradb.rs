---- MODULE RelationLinkConflict ----
\* The story, in plain words. Two explicit transactions each want to link a
\* one-to-one value. Each decides on a read first: the unique-index conflict
\* lookup of an indexed create. The first transaction just commits its link.
\* The second reads, then tries to commit. A query's one-to-one join is no
\* such read: it decides nothing and is never validated (RelationJoinRead).
\*
\* What can happen: the second reads before or after the first commits, and
\* the two commits land in either order.
\*
\* What must never happen: both links land. A second link that read an empty
\* range commits when nobody wrote into that range in between, and conflicts
\* when the first link landed there after its read. A first link is refused
\* once the second took the value. And the validation covers that one read
\* only: it never leaves later scans of the transaction validated.
\*
\* In unvalidated mode the read is a plain snapshot read, so nothing stops
\* the second commit and both links land. That is the planted bug.
\*
\* Rust owners: crates/database/db/src/index/manager/unique.rs
\* (save_local_unique_validated, save_healing_stale_unique),
\* crates/database/db/src/index/manager/mod.rs (on_document_create,
\* on_document_update, on_document_update_modified), and
\* crates/engine/datastore/src/multistore.rs (SharedTxn::set_scans_validated).
EXTENDS Naturals

\* Mode: whether the link read is validated at commit or a plain snapshot read.
CONSTANT Mode
\* Only these two modes exist.
ASSUME Mode \in {"validated", "unvalidated"}

\* firstCommitted: the first transaction's link has landed.
\* secondRead: the second transaction has made its link read.
\* secondSawFirst: that read already saw the first link.
\* secondCommitted: the second transaction's link has landed.
VARIABLES firstCommitted, secondRead, secondSawFirst, secondCommitted

\* Every variable, for the steps that leave them alone.
vars == <<firstCommitted, secondRead, secondSawFirst, secondCommitted>>

\* Each variable is a plain yes or no.
TypeOK == /\ firstCommitted \in BOOLEAN
          /\ secondRead \in BOOLEAN
          /\ secondSawFirst \in BOOLEAN
          /\ secondCommitted \in BOOLEAN

\* Nothing has been read or committed yet.
Init == /\ firstCommitted = FALSE
        /\ secondRead = FALSE
        /\ secondSawFirst = FALSE
        /\ secondCommitted = FALSE

\* The second transaction makes its link read, once.
ReadLinkDecision ==
  \* It has not read yet.
  /\ ~secondRead
  \* Now it has.
  /\ secondRead' = TRUE
  \* It remembers whether the first link was already there.
  /\ secondSawFirst' = firstCommitted
  \* Reading commits nothing.
  /\ UNCHANGED <<firstCommitted, secondCommitted>>

\* The first transaction commits its link.
CommitFirstLink ==
  \* It commits once.
  /\ ~firstCommitted
  \* A validated first link is refused after the second took the value.
  /\ ~(Mode = "validated" /\ secondCommitted)
  \* Its link lands.
  /\ firstCommitted' = TRUE
  \* The second transaction's state is untouched.
  /\ UNCHANGED <<secondRead, secondSawFirst, secondCommitted>>

\* The second transaction commits its link.
CommitSecondLink ==
  \* It decided on its read first.
  /\ secondRead
  \* It commits once.
  /\ ~secondCommitted
  \* Unvalidated, nothing checks the read. Validated, the commit lands only
  \* when the read found the range empty and the first link has not landed
  \* in it since.
  /\ (Mode = "unvalidated" \/ (~secondSawFirst /\ ~firstCommitted))
  \* Its link lands.
  /\ secondCommitted' = TRUE
  \* The first transaction's state and the read are untouched.
  /\ UNCHANGED <<firstCommitted, secondRead, secondSawFirst>>

\* Any one of the three steps.
Next == ReadLinkDecision \/ CommitFirstLink \/ CommitSecondLink

\* Start at Init, then take steps or stand still.
Spec == Init /\ [][Next]_vars

\* The same, with the second transaction never stalling forever on a read
\* or a commit it is allowed to make.
FairSpec ==
  /\ Spec
  /\ WF_vars(ReadLinkDecision)
  /\ WF_vars(CommitSecondLink)

\* Promise: the two links never both land.
INV_SecondLinkIsRefused == ~(firstCommitted /\ secondCommitted)

\* Promise: a second link that read an empty range is never stuck. Whenever
\* it has read and not seen the first link, its own commit lands unless the
\* first link lands first and takes the value.
LIVE_EmptyReadCommits ==
  (secondRead /\ ~secondSawFirst) ~> (secondCommitted \/ firstCommitted)

====
