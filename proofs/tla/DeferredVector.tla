---- MODULE DeferredVector ----
\* The story: a vector index declared `maintenance: DEFERRED` does not touch
\* its graph when a document is written. The write puts one pending entry for
\* the document's node instead: "set its vector to v" or "remove it". Later a
\* drain applies pending entries to the graph and deletes them. A search reads
\* the graph and the pending entries on one snapshot, and for a node with a
\* pending entry it trusts the entry, not the graph.
\*
\* Who acts: writers, each committing one document write at a time; drains,
\* each of which plans a batch on a snapshot, reads the planned entries in its
\* own transaction, applies them and commits (the drain task, and the rebuild
\* sweep, which applies the pending entries the same way before it clears the
\* graph it rebuilds); searches; one crash.
\*
\* A write chooses how it lands. Usually it defers (a blind put of the
\* pending entry, which reads nothing and so conflicts with nobody). When the
\* node counts too many pending entries, or no drain stands behind the index,
\* it writes the graph itself, inline, and deletes any older pending entry of
\* the node, which the drain would otherwise apply over it later. Which way a
\* write goes is left free here, so every limit and every count is covered.
\*
\* The drain reads each entry it is about to delete in its transaction. regolith
\* validates that read at commit: an entry rewritten (or deleted) since makes
\* the commit fail, and the attempt runs again and reads the newer entry.
\*
\* A crash loses the drain's open attempt and its in-memory request; the
\* pending entries are stored, and the node drains them when it opens.
\*
\* What must never happen: a search shows a document's vector other than its
\* latest committed one, or shows a deleted document (INV_SearchSeesLatest);
\* a pending entry that is not the node's latest write (INV_PendingIsLatest);
\* a node with nothing pending whose graph disagrees with its document
\* (INV_GraphSettled). What must always happen at last: once writes stop,
\* nothing stays pending and the graph holds every latest vector
\* (PROP_Converges).
\*
\* Code it models:
\*   crates/database/db/src/index/vector/index.rs  VectorIndex::save, delete (defers_now: a pending
\*                                            put; otherwise supersede_pending then the graph),
\*                                            search_where (pending_entries, merge_pending),
\*                                            rebuild_if_due (apply_all_pending before the clear)
\*   crates/database/db/src/index/vector/pending/search.rs  merge_pending (stale graph hits dropped,
\*                                            pending upserts scored)
\*   crates/database/db/src/index/vector/pending/drain.rs  drain_vector_turn (plan on a snapshot),
\*                                            apply_attempt
\*   crates/database/db/src/index/vector/index/deferred.rs  VectorIndex::apply_planned (get_pending, a
\*                                            validated read, then apply_pending and delete_pending
\*                                            in one transaction), apply_all_pending
\*   crates/database/db/src/index/manager/deferred.rs  IndexManager::ask_vector_drains (the commit
\*                                            callback that asks for the drain)
\*   crates/database/db/src/index/deferred/task.rs  the settle at open, a sweep of every deferred index
EXTENDS Naturals, FiniteSets

CONSTANTS
  Nodes,          \* the documents, each one node of the graph
  Drains,         \* the transactions that apply pending entries: the drain task, the rebuild sweep
  MaxWrites,      \* how many document writes happen in all
  MaxCrashes,     \* how many crashes happen in all
  ValidateRead,   \* TRUE: the drain's read of an entry is validated at commit | FALSE: it is not
  Supersede,      \* TRUE: an inline write deletes the node's pending entry | FALSE: it leaves it
  MergePending,   \* TRUE: a search reads the pending entries | FALSE: it reads the graph alone
  DropStale,      \* TRUE: a search drops graph hits of nodes with an entry | FALSE: it keeps them
  AskOnCommit,    \* TRUE: a deferred write asks for a drain when it commits | FALSE: it does not
  SettleAtOpen    \* TRUE: a node that opens drains what is pending | FALSE: it waits for a request

\* Each switch is a yes or a no.
ASSUME ValidateRead \in BOOLEAN /\ Supersede \in BOOLEAN /\ MergePending \in BOOLEAN
ASSUME DropStale \in BOOLEAN /\ AskOnCommit \in BOOLEAN /\ SettleAtOpen \in BOOLEAN
\* The bounds are plain numbers.
ASSUME MaxWrites \in Nat /\ MaxCrashes \in Nat

\* A write is named by its number, 1 to MaxWrites; 0 means "no vector".
Writes == 1..MaxWrites

\* No pending entry for a node.
NoEntry == [op |-> "None", w |-> 0]

\* The pending entries there can be: set the vector to write w's, or remove the node.
Entries == [op : {"Up"}, w : Writes] \cup [op : {"Del"}, w : Writes]

VARIABLES
  doc,       \* per node: the write whose vector the document holds now, 0 for none
  graph,     \* per node: the write whose vector the graph holds, 0 when it holds none
  pending,   \* per node: its pending entry, or NoEntry
  written,   \* how many writes have committed
  asked,     \* whether a drain has been asked for and not yet taken
  pc,        \* per drain: "Idle", "Planned" (a batch chosen), or "Read" (entries read in its transaction)
  plan,      \* per drain: the nodes its batch names
  read,      \* per drain and planned node: the entry its transaction read
  crashes    \* how many crashes happened

\* The drain task: the first drain in the model's order.
First == CHOOSE d \in Drains : TRUE

\* Every variable, for stuttering and fairness.
vars == <<doc, graph, pending, written, asked, pc, plan, read, crashes>>

\* The nodes with a pending entry.
PendingNodes == {n \in Nodes : pending[n] /= NoEntry}

\* What a search returns for node n, as pairs of node and write.
\* The graph's hit for n, if it holds a vector for it.
GraphHit(n) == IF graph[n] /= 0 THEN {<<n, graph[n]>>} ELSE {}
\* The pending entry's hit for n: its vector if it sets one.
EntryHit(n) == IF pending[n].op = "Up" THEN {<<n, pending[n].w>>} ELSE {}
\* With pending entries read: a node with an entry shows the entry, plus the
\* graph's stale hit when stale hits are kept; any other node shows the graph.
Seen(n) ==
  IF MergePending /\ pending[n] /= NoEntry
  THEN EntryHit(n) \cup (IF DropStale THEN {} ELSE GraphHit(n))
  ELSE GraphHit(n)

\* A search, over every node, on the committed state.
Search == UNION {Seen(n) : n \in Nodes}

\* What a search should return: every document's latest vector.
Latest == {<<n, doc[n]>> : n \in {m \in Nodes : doc[m] /= 0}}

\* Every variable stays in its range.
TypeOK ==
  /\ doc \in [Nodes -> 0..MaxWrites]               \* a write or none
  /\ graph \in [Nodes -> 0..MaxWrites]             \* a write or none
  /\ pending \in [Nodes -> Entries \cup {NoEntry}] \* an entry or none
  /\ written \in 0..MaxWrites                      \* writes so far
  /\ asked \in BOOLEAN                             \* a request waiting
  /\ pc \in [Drains -> {"Idle", "Planned", "Read"}] \* each drain's step
  /\ plan \in [Drains -> SUBSET Nodes]             \* each batch
  /\ read \in [Drains -> [Nodes -> Entries \cup {NoEntry}]]   \* what each drain read
  /\ crashes \in 0..MaxCrashes                     \* crashes so far

\* An empty index: no documents, no graph, nothing pending, the drain asleep.
Init ==
  /\ doc = [n \in Nodes |-> 0]           \* no document has a vector
  /\ graph = [n \in Nodes |-> 0]         \* the graph is empty
  /\ pending = [n \in Nodes |-> NoEntry] \* nothing is pending
  /\ written = 0                         \* no write yet
  /\ asked = FALSE                       \* nobody asked
  /\ pc = [d \in Drains |-> "Idle"]      \* the drains sleep
  /\ plan = [d \in Drains |-> {}]        \* with no batch
  /\ read = [d \in Drains |-> [n \in Nodes |-> NoEntry]]   \* and nothing read
  /\ crashes = 0                         \* no crash yet

\* A write that defers: one blind put of the node's entry, and a request for
\* the drain once it commits. `del` says whether it removes the vector.
\* Removing a vector the document does not hold writes nothing, as the code skips it.
WriteDeferred(n, del) ==
  /\ written < MaxWrites                 \* writes are bounded
  /\ ~del \/ doc[n] /= 0                 \* a remove needs a vector to remove
  /\ LET w == written + 1 IN             \* this write's number
     /\ written' = w                     \* it commits
     /\ doc' = [doc EXCEPT ![n] = IF del THEN 0 ELSE w]   \* the document changes
     /\ pending' = [pending EXCEPT ![n] = [op |-> IF del THEN "Del" ELSE "Up", w |-> w]]   \* the entry, replacing any older one
  /\ asked' = (asked \/ AskOnCommit)     \* the commit callback asks for a drain
  /\ UNCHANGED <<graph, pc, plan, read, crashes>>

\* A write that updates the graph itself, as when the node counts too many
\* pending entries; it deletes the node's older entry when Supersede holds.
WriteInline(n, del) ==
  /\ written < MaxWrites                 \* writes are bounded
  /\ ~del \/ doc[n] /= 0                 \* a remove needs a vector to remove
  /\ LET w == written + 1 IN             \* this write's number
     /\ written' = w                     \* it commits
     /\ doc' = [doc EXCEPT ![n] = IF del THEN 0 ELSE w]     \* the document changes
     /\ graph' = [graph EXCEPT ![n] = IF del THEN 0 ELSE w] \* and so does the graph
  /\ pending' = IF Supersede THEN [pending EXCEPT ![n] = NoEntry] ELSE pending   \* the older entry goes
  /\ UNCHANGED <<asked, pc, plan, read, crashes>>

\* Drain d plans a batch on a snapshot: some nodes that have an entry (a batch
\* is capped, so any non-empty part of them). The first drain is the drain
\* task, which takes the request; any other is the rebuild sweep, which runs
\* on its own timer, so it needs no request and leaves it in place.
Plan(d) ==
  /\ pc[d] = "Idle"                      \* the drain is free
  /\ d = First => asked                  \* the drain task waits to be asked
  /\ PendingNodes /= {}                  \* and something is pending
  /\ \E batch \in SUBSET PendingNodes :  \* it picks a batch
     /\ batch /= {}                      \* of at least one node
     /\ plan' = [plan EXCEPT ![d] = batch]
  /\ asked' = IF d = First THEN FALSE ELSE asked   \* the drain task takes the request
  /\ pc' = [pc EXCEPT ![d] = "Planned"]
  /\ UNCHANGED <<doc, graph, pending, written, read, crashes>>

\* A request with nothing pending is simply taken.
Idle ==
  /\ pc[First] = "Idle" /\ asked /\ PendingNodes = {}   \* asked, but the set is empty
  /\ asked' = FALSE                      \* nothing to do
  /\ UNCHANGED <<doc, graph, pending, written, pc, plan, read, crashes>>

\* In its transaction drain d reads each planned node's entry (writes may
\* have landed since the plan, and may land again before the commit).
Read(d) ==
  /\ pc[d] = "Planned"                   \* a batch is chosen
  /\ read' = [read EXCEPT ![d] = [n \in Nodes |-> IF n \in plan[d] THEN pending[n] ELSE NoEntry]]   \* what it reads now
  /\ pc' = [pc EXCEPT ![d] = "Read"]
  /\ UNCHANGED <<doc, graph, pending, written, asked, plan, crashes>>

\* The drain's commit. A validated read that changed since makes it fail, and
\* the attempt runs again from the read. Otherwise each entry it read is
\* applied to the graph and deleted (blindly: the delete names the node).
\* Anything still pending afterwards asks for another turn.
Commit(d) ==
  /\ pc[d] = "Read"                      \* the entries are read
  /\ IF ValidateRead /\ \E n \in plan[d] : pending[n] /= read[d][n]   \* one changed since
     THEN /\ pc' = [pc EXCEPT ![d] = "Planned"]   \* the commit fails; the retry reads again
          /\ UNCHANGED <<graph, pending, asked>>
     ELSE /\ graph' = [n \in Nodes |->   \* the read entries reach the graph
                         IF n \in plan[d] /\ read[d][n] /= NoEntry
                         THEN IF read[d][n].op = "Up" THEN read[d][n].w ELSE 0
                         ELSE graph[n]]
          /\ pending' = [n \in Nodes |-> \* and are deleted
                           IF n \in plan[d] /\ read[d][n] /= NoEntry THEN NoEntry ELSE pending[n]]
          /\ asked' = (asked \/ {n \in Nodes : pending'[n] /= NoEntry} /= {})   \* more left: ask again
          /\ pc' = [pc EXCEPT ![d] = "Idle"]
  /\ UNCHANGED <<doc, written, plan, read, crashes>>

\* A crash: the drain's open attempt and the request are lost; the entries
\* are stored. The node that opens drains what is pending when SettleAtOpen holds.
Crash ==
  /\ crashes < MaxCrashes                \* crashes are bounded
  /\ crashes' = crashes + 1
  /\ pc' = [d \in Drains |-> "Idle"]     \* the open attempts die uncommitted
  /\ plan' = [d \in Drains |-> {}]
  /\ read' = [d \in Drains |-> [n \in Nodes |-> NoEntry]]
  /\ asked' = (SettleAtOpen /\ PendingNodes /= {})   \* the settle at open, or nothing
  /\ UNCHANGED <<doc, graph, pending, written>>

\* Any one step happens next.
Next ==
  \/ \E n \in Nodes, del \in BOOLEAN : WriteDeferred(n, del) \/ WriteInline(n, del)
  \/ Idle \/ Crash
  \/ \E d \in Drains : Plan(d) \/ Read(d) \/ Commit(d)

\* The drain task keeps running whenever it can, and every drain that started
\* a batch finishes it; writes, crashes and the rebuild sweep's starts may stop.
Fairness ==
  /\ WF_vars(Plan(First)) /\ WF_vars(Idle)
  /\ \A d \in Drains : WF_vars(Read(d)) /\ WF_vars(Commit(d))

\* Start in Init, take any steps, under the fairness above.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* Promise: a search on any committed state returns exactly each document's
\* latest vector, and no deleted document.
INV_SearchSeesLatest == Search = Latest

\* Promise: a node's pending entry is always its latest write.
INV_PendingIsLatest ==
  \A n \in Nodes : pending[n] /= NoEntry =>
    IF pending[n].op = "Up" THEN doc[n] = pending[n].w ELSE doc[n] = 0

\* Promise: a node with nothing pending has the graph agree with its document.
INV_GraphSettled == \A n \in Nodes : pending[n] = NoEntry => graph[n] = doc[n]

\* Promise: once the writes stop, nothing stays pending and the graph holds
\* every document's latest vector.
PROP_Converges == <>[](PendingNodes = {} /\ \A n \in Nodes : graph[n] = doc[n])

====
