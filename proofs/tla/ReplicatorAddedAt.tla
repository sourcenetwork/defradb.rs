------------------------ MODULE ReplicatorAddedAt ------------------------
\* An operator adds a peer, then may update its collection list. Both the
\* durable record and in-memory registry each preserve the previous status
\* timestamp on update. This models one copy, not their publication ordering.
\* A timestamp can be zero after status recovery.
\* Rust: replicator_install_store.rs Prepare::run and bitswap/registry.rs
\* ReplicatorRegistry::set_replicator_info.
EXTENDS Naturals
\* Overwrite plants the bug that replaces the timestamp during an update.
CONSTANT Overwrite
\* stored is the timestamp; exists says the record was created.
VARIABLES stored, exists
\* Every changing variable.
vars == <<stored, exists>>
\* There is no record before the add.
Init == /\ stored = 0 /\ exists = FALSE
\* Adding the peer records the current time, modeled as 1.
Add == /\ ~exists /\ exists' = TRUE /\ stored' = 1
\* Updating collections preserves that timestamp unless the bug is enabled.
Update == /\ exists /\ stored' = IF Overwrite THEN 2 ELSE stored
          /\ UNCHANGED exists
\* A status recovery can set both copies to zero.
Recover == /\ exists /\ stored' = 0 /\ UNCHANGED exists
\* The operator may add, update, or recover a record.
Next == Add \/ Update \/ Recover
\* The execution can also stutter.
Spec == Init /\ [][Next]_vars
\* Updating collections must not invent a new status-change time.
INV_NoOverwrite == stored # 2
=============================================================================
