---- MODULE DiskAnnClusterKeys ----
\* The story, in plain words.
\*
\* A DiskANN index with resident codes keeps one graph record per node: the
\* ids of its neighbours. To put neighbours next to each other on disk, a
\* record's key starts with a cluster tag, so the records of one cluster sort
\* together and one page read serves several hops of a walk. A walk only
\* knows a neighbour's id, so it needs the cluster to build the key. The
\* cluster sits beside the node's code (the CODE key), and every search finds
\* it in the shared in-memory copy of the codes: the code cache.
\*
\* Who acts:
\*   * merges, one transaction each, which link nodes into the graph and
\*     consolidate deleted ones out of it, each bumping the generation;
\*   * searches, each reading one snapshot, that is one generation;
\*   * the code cache, shared by every search, loaded by some search from its
\*     own snapshot: possibly older or newer than another search's.
\*
\* What can happen: a node is linked (first time, or again after an update),
\* a node is consolidated away, and later linked again into another cluster.
\* The cache can be loaded from any generation at any time. An index written
\* before clusters existed starts with some nodes in the old layout: a code
\* with no cluster and a record keyed by the id alone. Such a node keeps that
\* key when linked again, and takes a cluster only once it is consolidated
\* away and linked fresh.
\*
\* Merges find a node's record the way searches do: through the cache first
\* (record.rs `DiskAnn::locate`, which `link` and `consolidate` call). So a
\* stale cache can mislead a merge too, not just a search.
\*
\* What must never happen:
\*   * a node has two records, or a record at a key its code does not name
\*     (INV_OneRecord): a walk would read a stale copy of its edges;
\*   * a search misses a record its snapshot holds (INV_FindsRecord): the
\*     walk would skip that node and lose recall.
\*
\* The two guards the Rust code keeps:
\*   * KeepCluster: a node linked again keeps the place its record already
\*     has, so its record is rewritten at the same key
\*     (graph.rs `DiskAnn::link`, which locates the node's record first);
\*   * Fallback: a lookup that does not find a record at the key the cache
\*     names reads the cluster from its own snapshot and tries that key
\*     (record.rs `DiskAnn::locate`). Without it a search skips a node, and a
\*     merge takes a linked node for a new one and gives it a second record.
\*
\* Rust anchors: `crates/database/db/src/index/vector/engine/diskann/graph.rs`
\* `link`; `record.rs` `locate`, `record`, `choose_cluster`; `merge.rs`
\* `consolidate`; `codec.rs` `graph_key`, `encode_code`, `decode_code`.

EXTENDS Naturals, Sequences, FiniteSets

\* Ids: the nodes. Clusters: the cluster tags a node can be given.
\* MaxGen: how many merges the model runs. The two guards are switches so a
\* RED configuration can turn one off.
CONSTANTS Ids, Clusters, MaxGen, KeepCluster, Fallback

\* None: a node with no code and no record (or, in the cache, a node the
\* cache does not hold).
None == "none"

\* Unclustered: the old layout. The node's code carries no cluster and its
\* record is keyed by the id alone (Rust: `graph_key(id, None)`, 8 bytes).
Unclustered == "unclustered"

\* Every place a record can sit: under a cluster, or at the bare id key.
Places == Clusters \cup {Unclustered}

\* snaps: every committed state, one per generation, oldest first. Each holds
\*   recs, the set of <<id, place>> keys that have a record, and
\*   code, the place each node's stored code names (None without a code).
\* cache: the place the shared code cache holds for each node (None for a
\*   node it does not hold: a lookup then reads its own snapshot).
VARIABLES snaps, cache

\* Every variable, for stuttering steps.
vars == <<snaps, cache>>

\* The newest committed state: what the next merge starts from.
Now == snaps[Len(snaps)]

\* A starting state: the nodes in old hold the old layout, a code with no
\* cluster and a record at their bare id key; every other node is absent.
Start(old) ==
    \* old nodes have a record at the id key, no one else has a record
    [recs |-> {<<i, Unclustered>> : i \in old},
     \* old nodes' codes name no cluster, the rest have no code
     code |-> [i \in Ids |-> IF i \in old THEN Unclustered ELSE None]]

\* At the start one state is committed: any mix of old-layout nodes (none of
\* them is an empty graph), and the cache holds nothing.
Init ==
    \* some set of nodes was written before clusters existed
    /\ \E old \in SUBSET Ids : snaps = <<Start(old)>>
    \* the cache knows no node yet
    /\ cache = [i \in Ids |-> None]

\* Where a lookup in state s finds node i's record, or None: the place the
\* cache names (or, on a cache miss, the place s's code names); then, with
\* Fallback, the place s's own code names. A node with no code anywhere the
\* lookup reads has no record. (Rust: `DiskAnn::locate`.)
Locate(s, i) ==
    \* the first place tried: the cache's, else the snapshot's own
    LET first == IF cache[i] # None THEN cache[i] ELSE s.code[i]
    \* nothing to try when neither names a place
    IN IF first = None THEN None
       \* the first place holds the record
       ELSE IF <<i, first>> \in s.recs THEN first
       \* the fallback: the snapshot's own code names a place that holds it
       ELSE IF Fallback /\ s.code[i] # None /\ <<i, s.code[i]>> \in s.recs
            THEN s.code[i]
       \* not found
       ELSE None

\* A merge commits a new state on top of the newest one.
Commit(state) ==
    \* the new state becomes the newest generation
    /\ snaps' = Append(snaps, state)
    \* a merge does not touch the cache; a search reloads it later
    /\ UNCHANGED cache

\* Link node i. The merge first looks the node up. A node it finds is linked
\* again: with KeepCluster its record is rewritten where it was found (an
\* old-layout node stays at its id key); without it the merge picks any
\* cluster, as a merge that ignored the stored place would. A node it does
\* not find is linked fresh into any cluster.
Link(i) ==
    \* the model stops after MaxGen merges
    /\ Len(snaps) <= MaxGen
    \* where the merge finds i's record, if anywhere
    /\ LET found == Locate(Now, i)
       \* the cluster a fresh link (or a link that ignores the guard) picks
       IN \E c \in Clusters :
            \* a found node keeps its place when the guard is on
            LET p == IF KeepCluster /\ found # None THEN found ELSE c
            \* the record goes at <<i, p>> and the code names p
            IN Commit([recs |-> Now.recs \cup {<<i, p>>},
                       code |-> [Now.code EXCEPT ![i] = p]])

\* Consolidate node i away: the merge looks its record up, and deletes the
\* record it finds together with the code. A record it cannot find stays,
\* and so does the code (Rust: `consolidate` deletes only what `locate`
\* returns).
Consolidate(i) ==
    \* the model stops after MaxGen merges
    /\ Len(snaps) <= MaxGen
    \* only a node in the graph can be consolidated
    /\ Now.code[i] # None
    \* where the merge finds i's record, if anywhere
    /\ LET found == Locate(Now, i)
       \* found: drop that record and the code
       IN IF found # None
          THEN Commit([recs |-> Now.recs \ {<<i, found>>},
                       code |-> [Now.code EXCEPT ![i] = None]])
          \* not found: the merge commits without touching i
          ELSE Commit(Now)

\* Some search loads the shared cache from its own snapshot, generation g:
\* any committed generation, newer or older than other searches.
LoadCache ==
    \* the generation the loading search reads
    \E g \in 1..Len(snaps) :
        \* the cache now holds that snapshot's places
        /\ cache' = snaps[g].code
        \* loading changes nothing in the store
        /\ UNCHANGED snaps

\* What a step can be: a link, a consolidation, or a cache load.
Next ==
    \/ \E i \in Ids : Link(i)
    \/ \E i \in Ids : Consolidate(i)
    \/ LoadCache

\* The whole behaviour: start with any old-layout nodes, then take any steps.
Spec == Init /\ [][Next]_vars

\* Promise: in every committed state a node has at most one record, and that
\* record sits at the key its code names, in either layout.
INV_OneRecord ==
    \A g \in 1..Len(snaps), i \in Ids :
        \A p \in Places :
            <<i, p>> \in snaps[g].recs => p = snaps[g].code[i]

\* Promise: a search at any generation finds every record its snapshot holds,
\* in either layout, whatever generation the shared cache was loaded from.
INV_FindsRecord ==
    \A g \in 1..Len(snaps), i \in Ids :
        (\E p \in Places : <<i, p>> \in snaps[g].recs) => Locate(snaps[g], i) # None
====
