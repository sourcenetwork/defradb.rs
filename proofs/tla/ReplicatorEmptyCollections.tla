--------------------- MODULE ReplicatorEmptyCollections ---------------------
\* Story. An operator adds replicators, each with a list of collections that
\* may be empty, removes collections from them, and restarts the node. The
\* node keeps a registry of replicators in memory and a copy on disk, which a
\* restart loads back. A local write to a collection is pushed to the
\* replicators the push selection picks for that collection.
\* Never: a replicator the operator added is missing from the list, right
\* after the add or after a restart (#2702); or a write is pushed to a
\* replicator that does not replicate its collection.
\* Removing a replicator's last collection deletes it, as Go's
\* deleteReplicator does, so that is not a loss.
\* Rust: crates/database/p2p/src/bitswap/registry.rs store_info (the insert),
\* ReplicatorRegistry::load_from_infos (the restore), remove_peer_collections;
\* crates/database/p2p/src/sync/coordinator/broadcast.rs
\* replicator_in_collection (the push selection).
EXTENDS FiniteSets
\* Peers, Cols: the replicators and collections.
\* KeepEmptyOnStore: store_info keeps a record with no collections (the fix);
\*   FALSE is main before #2702, which removed it.
\* KeepEmptyOnLoad: load_from_infos keeps a stored row with no collections.
\* EmptyMeansAll: the push selection treats an empty list as every collection
\*   (main's replicator_in_collection).
\* Absent: a model value standing for "no entry", unequal to every set.
\* KeepFiltered keeps a peer whose remaining filters outlive its collections.
CONSTANTS Peers, Cols, KeepEmptyOnStore, KeepEmptyOnLoad, EmptyMeansAll, KeepFiltered, Absent
\* reg[p]: the in-memory registry entry, a set of collections or Absent.
\* disk[p]: the persisted record, a set of collections or Absent.
\* regFilters and diskFilters hold collection IDs with filters; filter details
\* are abstracted because this model checks retention, not evaluation.
\* wrongPush: a write was pushed to a replicator not in its collection.
VARIABLES reg, disk, regFilters, diskFilters, wrongPush
\* Every variable, for stuttering steps.
vars == <<reg, disk, regFilters, diskFilters, wrongPush>>

\* What store_info leaves for a record with collections s.
Stored(s) == IF s = {} /\ ~KeepEmptyOnStore THEN Absent ELSE s

\* No replicators, nothing on disk, nothing pushed.
Init == /\ reg = [p \in Peers |-> Absent]
        /\ disk = [p \in Peers |-> Absent]
        /\ regFilters = [p \in Peers |-> {}]
        /\ diskFilters = [p \in Peers |-> {}]
        /\ wrongPush = FALSE

\* AddReplicator with any collection list, the empty one included. The
\* record is persisted as given and stored in the registry.
Add(p, s, f) == /\ disk' = [disk EXCEPT ![p] = s]
                /\ reg' = [reg EXCEPT ![p] = Stored(s)]
                /\ diskFilters' = [diskFilters EXCEPT ![p] = f]
                /\ regFilters' = [regFilters EXCEPT ![p] = IF Stored(s) = Absent THEN {} ELSE f]
                /\ UNCHANGED wrongPush

\* DeleteReplicator for some collections: removing the last one deletes the
\* peer only when no collection and no filter remains, on disk and in memory.
Remove(p, r) == /\ disk[p] # Absent
                /\ LET left == disk[p] \ r
                       filtersLeft == diskFilters[p] \ r
                       entry == IF left = {} /\ (filtersLeft = {} \/ ~KeepFiltered) THEN Absent ELSE left
                   IN /\ disk' = [disk EXCEPT ![p] = entry]
                      /\ reg' = [reg EXCEPT ![p] = entry]
                      /\ diskFilters' = [diskFilters EXCEPT ![p] = filtersLeft]
                      /\ regFilters' = [regFilters EXCEPT ![p] = filtersLeft]
                /\ UNCHANGED wrongPush

\* A restart loads the registry back from disk.
Restart == /\ reg' = [p \in Peers |->
                       IF disk[p] = {} /\ ~KeepEmptyOnLoad THEN Absent ELSE disk[p]]
           /\ regFilters' = [p \in Peers |-> IF disk[p] = {} /\ ~KeepEmptyOnLoad THEN {} ELSE diskFilters[p]]
           /\ UNCHANGED <<disk, diskFilters, wrongPush>>

\* Whether the push selection picks peer p for collection c.
Selected(p, c) == /\ reg[p] # Absent
                  /\ (c \in reg[p] \/ (EmptyMeansAll /\ reg[p] = {}))

\* A local write to collection c is pushed to every selected peer; a push to
\* a peer that does not replicate c is recorded.
Write(c) == /\ wrongPush' = (wrongPush \/ \E p \in Peers : Selected(p, c) /\ c \notin disk[p])
            /\ UNCHANGED <<reg, disk, regFilters, diskFilters>>

\* Any operator step, a restart, or a write, in any order.
Next == \/ \E p \in Peers, s \in SUBSET Cols, f \in SUBSET Cols : Add(p, s, f)
        \/ \E p \in Peers, r \in SUBSET Cols : Remove(p, r)
        \/ Restart
        \/ \E c \in Cols : Write(c)

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Every replicator on disk is listed, with the collections it was given.
INV_ListedAsAdded == \A p \in Peers : /\ reg[p] = disk[p]
                                       /\ regFilters[p] = diskFilters[p]
\* A remaining filter keeps its peer entry even when the collection list is empty.
INV_FilterRetained == \A p \in Peers : diskFilters[p] # {} => disk[p] # Absent
\* A write is pushed only to replicators of its collection.
INV_NoPushOutsideCollections == ~wrongPush
=============================================================================
