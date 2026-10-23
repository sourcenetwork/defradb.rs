-------------------------- MODULE OwnerObservation --------------------------
\* A row and its owner must belong to one authorized transaction snapshot.
\* The Rust owner is QueryRunner::observe_owners.
EXTENDS Naturals
\* A RED configuration skips one gate, or reads an owner outside the snapshot.
CONSTANT Fault
\* The request chooses one relevant authorization/membership condition.
Scenarios == {"allowed", "actor", "node", "app", "document", "member", "registered", "bound"}
\* The live owner may change after the reader's transaction begins.
VARIABLES scenario, phase, liveOwner, snapshotOwner, returned, observedOwner
\* One tuple names every variable for stuttering.
vars == <<scenario, phase, liveOwner, snapshotOwner, returned, observedOwner>>
\* Begin from a captured owner and no released result.
Init == /\ scenario \in Scenarios
        /\ phase = "ready"
        /\ liveOwner = 0
        /\ snapshotOwner = 0
        /\ returned = FALSE
        /\ observedOwner = 2
\* Another transaction changes ownership without changing our snapshot.
ChangeOwner == /\ liveOwner = 0
               /\ liveOwner' = 1
               /\ UNCHANGED <<scenario, phase, snapshotOwner, returned, observedOwner>>
\* Only a wholly readable, registered and bound observation is eligible.
Eligible == scenario = "allowed" \/ scenario = Fault
\* The GREEN read uses the captured view; the snapshot fault uses live ACP.
Read == /\ phase = "ready"
        /\ phase' = "done"
        /\ returned' = Eligible
        /\ observedOwner' = IF Eligible THEN
                (IF Fault = "snapshot" THEN liveOwner ELSE snapshotOwner) ELSE 2
        /\ UNCHANGED <<scenario, liveOwner, snapshotOwner>>
\* Explore the outside commit before or after the metadata read.
Next == ChangeOwner \/ Read
\* The operation may finish or stutter; it creates no durable writes.
Spec == Init /\ [][Next]_vars
\* An inaccessible or unbound document never releases an owner.
NoUnauthorizedOwner == returned => scenario = "allowed"
\* Every released owner belongs to the same view as the document row.
SameSnapshot == returned => observedOwner = snapshotOwner
=============================================================================
