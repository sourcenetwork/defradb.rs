---- MODULE RelationLinkConflict ----
\* Two explicit transactions make a one-to-one link decision before either
\* commits. That decision is a validated link read: the unique-index conflict
\* lookup for an indexed create, or the one-to-one relation-join scan for a
\* linked query. It does not leave later scans validated. In validated mode,
\* a later commit of the first link makes the second conflict, and the first
\* link refuses after the second took the value. In unvalidated mode, both
\* Rust owners: crates/database/db/src/index/manager/unique.rs
\* (save_local_unique_validated, save_healing_stale_unique),
\* crates/database/db/src/index/manager/mod.rs (on_document_create,
\* on_document_update, on_document_update_modified), and
\* crates/engine/query/src/runner/query/nested.rs
\* (has_one_to_one_relation_select), and
\* crates/engine/datastore/src/multistore.rs (SharedTxn::set_scans_validated).
EXTENDS Naturals

CONSTANT Mode
ASSUME Mode \in {"validated", "unvalidated"}

VARIABLES firstCommitted, secondRead, secondSawFirst, secondCommitted
vars == <<firstCommitted, secondRead, secondSawFirst, secondCommitted>>

TypeOK == /\ firstCommitted \in BOOLEAN
          /\ secondRead \in BOOLEAN
          /\ secondSawFirst \in BOOLEAN
          /\ secondCommitted \in BOOLEAN

Init == /\ firstCommitted = FALSE
        /\ secondRead = FALSE
        /\ secondSawFirst = FALSE
        /\ secondCommitted = FALSE

ReadLinkDecision ==
  /\ ~secondRead
  /\ secondRead' = TRUE
  /\ secondSawFirst' = firstCommitted
  /\ UNCHANGED <<firstCommitted, secondCommitted>>

CommitFirstLink ==
  /\ ~firstCommitted
  /\ ~(Mode = "validated" /\ secondCommitted)
  /\ firstCommitted' = TRUE
  /\ UNCHANGED <<secondRead, secondSawFirst, secondCommitted>>

CommitSecondLink ==
  /\ secondRead
  /\ ~secondCommitted
  /\ (Mode = "unvalidated" \/ (~secondSawFirst /\ ~firstCommitted))
  /\ secondCommitted' = TRUE
  /\ UNCHANGED <<firstCommitted, secondRead, secondSawFirst>>

Next == ReadLinkDecision \/ CommitFirstLink \/ CommitSecondLink
Spec == Init /\ [][Next]_vars

FairSpec ==
  /\ Spec
  /\ WF_vars(ReadLinkDecision)
  /\ WF_vars(CommitSecondLink)

\* A second link cannot commit after the first took the linked value.
INV_SecondLinkIsRefused == secondCommitted => ~firstCommitted

\* Validated mode still lets a second link that read an empty index commit.
LIVE_EmptyReadCommits == (secondRead /\ ~secondSawFirst) => <>(secondCommitted)

====
