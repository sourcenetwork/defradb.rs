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
\* The cache can be loaded from any generation at any time.
\*
\* What must never happen:
\*   * a node has two records, or a record at a key its code does not name
\*     (INV_OneRecord): a walk would read a stale copy of its edges;
\*   * a search misses a record its snapshot holds (INV_FindsRecord): the
\*     walk would skip that node and lose recall.
\*
\* The two guards the Rust code keeps:
\*   * KeepCluster: a node linked again keeps the cluster its stored code
\*     names, so its record is rewritten at the same key
\*     (graph.rs `DiskAnn::link`, which locates the node's record first);
\*   * Fallback: a search that does not find a record at the key the cache
\*     names reads the cluster from its own snapshot and tries that key
\*     (record.rs `DiskAnn::locate`).
\*
\* Rust anchors: `crates/database/db/src/index/vector/engine/diskann/graph.rs`
\* `link`; `record.rs` `locate`, `record`, `choose_cluster`; `merge.rs`
\* `consolidate`; `codec.rs` `graph_key`, `encode_code`.

EXTENDS Naturals, Sequences, FiniteSets

\* Ids: the nodes. Clusters: the cluster tags a node can be given.
\* MaxGen: how many merges the model runs. The two guards are switches so a
\* RED configuration can turn one off.
CONSTANTS Ids, Clusters, MaxGen, KeepCluster, Fallback

\* None: a node with no code and no record.
None == "none"

\* snaps: every committed state, one per generation, oldest first. Each holds
\*   recs, the set of <<id, cluster>> keys that have a record, and
\*   code, the cluster each node's stored code names (None without a code).
\* cache: the cluster the shared code cache holds for each node (None for a
\*   node it does not hold: a search then reads its own snapshot).
VARIABLES snaps, cache

\* Every variable, for stuttering steps.
vars == <<snaps, cache>>

\* The newest committed state: what the next merge starts from.
Now == snaps[Len(snaps)]

\* The empty graph: no records and no codes.
Empty == [recs |-> {}, code |-> [i \in Ids |-> None]]

\* At the start only the empty graph is committed, and the cache holds nothing.
Init ==
    \* one committed state, the empty one
    /\ snaps = <<Empty>>
    \* the cache knows no cluster yet
    /\ cache = [i \in Ids |-> None]

\* A merge commits a new state on top of the newest one.
Commit(state) ==
    \* the new state becomes the newest generation
    /\ snaps' = Append(snaps, state)
    \* a merge does not touch the cache; a search reloads it later
    /\ UNCHANGED cache

\* Link node i. A node that has a code is linked again: with KeepCluster its
\* record is rewritten at the key its code names; without it the merge picks
\* any cluster and writes there, as a merge that ignored the stored cluster
\* would. A node without a code is linked fresh into any cluster.
Link(i) ==
    \* the model stops after MaxGen merges
    /\ Len(snaps) <= MaxGen
    \* the cluster this link uses
    /\ \E c \in Clusters :
        \* a node linked again keeps its cluster when the guard is on
        /\ (KeepCluster /\ Now.code[i] # None) => c = Now.code[i]
        \* the record goes at <<i, c>> and the code names c
        /\ Commit([recs |-> Now.recs \cup {<<i, c>>},
                   code |-> [Now.code EXCEPT ![i] = c]])

\* Consolidate node i away: its record (at the key its code names) and its
\* code are deleted together.
Consolidate(i) ==
    \* the model stops after MaxGen merges
    /\ Len(snaps) <= MaxGen
    \* only a node in the graph can be consolidated
    /\ Now.code[i] # None
    \* drop its record and its code
    /\ Commit([recs |-> Now.recs \ {<<i, Now.code[i]>>},
               code |-> [Now.code EXCEPT ![i] = None]])

\* Some search loads the shared cache from its own snapshot, generation g:
\* any committed generation, newer or older than other searches.
LoadCache ==
    \* the generation the loading search reads
    \E g \in 1..Len(snaps) :
        \* the cache now holds that snapshot's clusters
        /\ cache' = snaps[g].code
        \* loading changes nothing in the store
        /\ UNCHANGED snaps

\* What a step can be: a link, a consolidation, or a cache load.
Next ==
    \/ \E i \in Ids : Link(i)
    \/ \E i \in Ids : Consolidate(i)
    \/ LoadCache

\* The whole behaviour: start empty, then take any steps.
Spec == Init /\ [][Next]_vars

\* The cluster a search at generation g tries first: the cache's, or, on a
\* cache miss, the one its own snapshot's code names.
FirstTry(g, i) == IF cache[i] # None THEN cache[i] ELSE snaps[g].code[i]

\* Whether a search at generation g finds node i's record: at the first key
\* it tries, or, with Fallback, at the key its own snapshot's code names.
Found(g, i) ==
    \* the first key holds the record
    \/ <<i, FirstTry(g, i)>> \in snaps[g].recs
    \* or the fallback read of the snapshot's code finds it
    \/ (Fallback /\ <<i, snaps[g].code[i]>> \in snaps[g].recs)

\* Promise: in every committed state a node has at most one record, and that
\* record sits at the key its code names.
INV_OneRecord ==
    \A g \in 1..Len(snaps), i \in Ids :
        \A c \in Clusters :
            <<i, c>> \in snaps[g].recs => c = snaps[g].code[i]

\* Promise: a search at any generation finds every record its snapshot holds,
\* whatever generation the shared cache was loaded from.
INV_FindsRecord ==
    \A g \in 1..Len(snaps), i \in Ids :
        (\E c \in Clusters : <<i, c>> \in snaps[g].recs) => Found(g, i)
====
