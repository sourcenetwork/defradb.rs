---- MODULE RelationLinkConflict ----
\* Two explicit transactions link through the same one-to-one unique index.
\* The second transaction reads the empty index when it creates its link. The
\* first transaction commits first. A validated unique-link read makes the
\* second commit conflict; an unvalidated read lets both links commit.
\* Rust owners: crates/database/db/src/index/manager/unique.rs
\* (save_local_unique_validated, save_healing_stale_unique),
\* crates/database/db/src/index/manager/mod.rs (on_document_create,
\* on_document_update, on_document_update_modified), and
\* crates/engine/datastore/src/multistore.rs (SharedTxn::set_scans_validated).
EXTENDS Naturals

CONSTANT Mode
ASSUME Mode \in {"validated", "unvalidated"}

VARIABLES firstCommitted, secondRead, secondCommitted
vars == <<firstCommitted, secondRead, secondCommitted>>

TypeOK == /\ firstCommitted \in BOOLEAN
          /\ secondRead \in BOOLEAN
          /\ secondCommitted \in BOOLEAN

Init == /\ firstCommitted = FALSE
        /\ secondRead = FALSE
        /\ secondCommitted = FALSE

ReadUniqueIndex ==
  /\ ~secondRead
  /\ secondRead' = TRUE
  /\ UNCHANGED <<firstCommitted, secondCommitted>>

CommitFirst ==
  /\ ~firstCommitted
  /\ firstCommitted' = TRUE
  /\ UNCHANGED <<secondRead, secondCommitted>>

CommitSecond ==
  /\ secondRead
  /\ ~secondCommitted
  /\ Mode = "unvalidated"
  /\ secondCommitted' = TRUE
  /\ UNCHANGED <<firstCommitted, secondRead>>

Next == ReadUniqueIndex \/ CommitFirst \/ CommitSecond
Spec == Init /\ [][Next]_vars

\* A second link cannot commit after the first took the unique value.
INV_SecondLinkIsRefused == secondCommitted => ~firstCommitted

====
