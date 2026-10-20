---- MODULE PendingCache ----
\* The story, in plain words.
\*
\* A DiskANN index does not link a new vector into its graph right away. The
\* vector waits in a pending list until a background merge links it in, and
\* until then every search reads the whole pending list as well as walking
\* the graph, so a search finds every vector its snapshot holds. Reading that
\* list from storage on every search is slow, so each node keeps a copy of
\* it in memory: the pending cache.
\*
\* A copy is only safe if no search ever trusts it to hold less than the
\* search's own snapshot holds. A vector a search's snapshot has pending but
\* the cache lacks is a vector that search can never return: a hidden vector.
\* Extra vectors in the cache cost nothing, because a search ranks every
\* candidate by the full vector it reads in its own snapshot, and drops a
\* candidate that snapshot does not have.
\*
\* How the code keeps the copy honest. Every change to the pending list also
\* appends one entry to a commit-ordered log, in the same transaction: an
\* insert appends the vector's id and its code, a delete appends the id
\* alone. The log's head is the newest position, and positions follow commit
\* order with no holes. A merge removes the vectors it links, bumps the
\* graph's generation, and truncates the log through its head, all in one
\* transaction. The cache is tagged with the generation it belongs to, the
\* head it was fully loaded at (loadedAt), and the head it has caught up to
\* (pos). A search reads the generation and the head in its snapshot, then:
\*   1. its transaction wrote to the pending list itself: it scans;
\*   2. the cache is of its generation and its head is at least loadedAt:
\*      it reads the log entries after pos up to its head, adds the inserted
\*      ones to the cache, and uses the cache;
\*   3. the cache is of an older generation, or there is none: it scans the
\*      pending list and loads the cache from that scan;
\*   4. otherwise (the cache is newer than the search): it scans.
\* Catch-ups only ever add to the cache, so a cache that is ahead of a
\* search is still a superset of what the search needs.
\*
\* A tiny example. Vector a is inserted (log position 1) and a search loads
\* the cache at head 1: {a}. Vector b is inserted (position 2) and deleted
\* (position 3). A search at head 3 catches up through 3: b's insert entry
\* carries b, so the cache becomes {a, b}, although b is gone at head 3. A
\* slower search that took its snapshot at head 2, where b is pending, uses
\* that cache and finds b. Had the catch-up kept only what is still pending
\* at head 3, the slower search would have hidden b.
\*
\* What must never happen:
\*   1. a search ends without a vector its snapshot has pending
\*      - INV_NoHiddenPending;
\*   2. the cache lacks a vector that some snapshot it is valid for has
\*      pending - INV_CacheCovers.
\*
\* Each guard has a RED configuration that removes it and fails:
\*   MC_PendingCache_Red_NoGeneration: the cache is used whatever its
\*     generation, so a search older than a merge uses a cache loaded after
\*     it and misses what that merge linked;
\*   MC_PendingCache_Red_NoLoadedAt: the cache is used by a search older than
\*     its load, so a vector deleted between the two is missing;
\*   MC_PendingCache_Red_SilentDeletes: deletes append nothing, so two
\*     snapshots with one head can hold different pending lists;
\*   MC_PendingCache_Red_StillPending: a catch-up adds only what is still
\*     pending at its own snapshot (an id-only log), as the example shows;
\*   MC_PendingCache_Red_OwnWrites: a transaction that wrote to the pending
\*     list uses the cache, which cannot hold its uncommitted writes.
\*
\* The log is regolith's commit-ordered append, and the truncate is
\* sourcenetwork/regolith#299. A snapshot sees the log as it was at its
\* commit: entries a later merge truncates are still there for it (MVCC).
\*
\* Correspondence with the Rust code (crates/db/src/index/vector/engine/
\* diskann/, the functions this change adds or touches):
\*   db           the committed pending list (codec::PENDING), graph state
\*                generation (codec GraphState::generation) and the pending
\*                log, one record per commit
\*   cache        pending_cache.rs PendingCache, its published Copy and Tag
\*   Insert       mod.rs insert: put_aux(PENDING) and the log append
\*   Delete       mod.rs delete: delete_aux(PENDING) and the log append
\*   Merge        merge.rs merge: delete_aux(PENDING) per linked vector,
\*                generation += 1, then truncate_pending_log through the
\*                head it read
\*   Begin        graph.rs search_graph: the snapshot, then the cache load
\*   OwnWrite     a pending put or delete made by the search's transaction,
\*                seen as crates/db/src/index/vector/store/kv.rs
\*                pending_log's write count differing from the log head
\*   Search       graph.rs pending_candidates: the four rules above, and
\*                PendingCache::publish
EXTENDS Naturals, Integers, FiniteSets, Sequences, TLC

\* Nodes: the vector ids. Readers: the searches. MaxCommits: how many
\* commits TLC explores. The rest switch each guard on (TRUE) or off.
CONSTANTS Nodes, Readers, MaxCommits,
          GenerationCheck, LoadedAtCheck, DeletesAppend, EntriesCarryCode,
          OwnWritesScan

\* db: one record per commit, the newest last; a snapshot is an index into it.
\* cache: the published PendingCache; gen = -1 means nothing is loaded.
\* rs: each search's progress, snapshot, its own write, the copy of the
\*     cache it loaded, and whether its candidates held everything it needed.
VARIABLES db, cache, rs
\* Every variable, for the steps that leave some unchanged.
vars == <<db, cache, rs>>

\* The cache before anything was loaded.
NoCache == [gen |-> -1, loadedAt |-> 0, pos |-> 0, ids |-> {}]

\* The state before any commit: nothing pending, an empty log, generation 0.
\* entry is the log, position by position; trunc is the newest truncated
\* position, so the live entries are trunc+1 .. head.
Genesis == [pending |-> {}, head |-> 0, entry |-> <<>>, trunc |-> 0, gen |-> 0]

\* The newest committed state.
Last == db[Len(db)]

\* A search that has not started.
Idle == [phase |-> "idle", snap |-> 0, own |-> {}, seen |-> NoCache, ok |-> TRUE]

\* At the start: no commit yet, no cache, every search idle.
Init == /\ db = <<Genesis>>
        /\ cache = NoCache
        /\ rs = [r \in Readers |-> Idle]

\* Room for one more commit, so TLC's search stays finite.
CanCommit == Len(db) <= MaxCommits

\* Commits one more state: the newest, with `pending` and the log changed.
\* `logged` is the entry the commit appends, or <<>> for none.
CommitWith(pending, logged) ==
  LET s == Last IN
  db' = Append(db, [s EXCEPT !.pending = pending,
                             !.head = IF logged = <<>> THEN @ ELSE @ + 1,
                             !.entry = IF logged = <<>> THEN @ ELSE Append(@, logged[1])])

\* An insert (mod.rs insert): the vector becomes pending, and its id and
\* code go to the log. Inserting a pending vector again is an update.
Insert(n) == /\ CanCommit
             /\ CommitWith(Last.pending \cup {n}, <<[node |-> n, kind |-> "ins"]>>)
             /\ UNCHANGED <<cache, rs>>

\* A delete (mod.rs delete): the vector leaves the pending list, and its id
\* goes to the log, unless the SilentDeletes bug skips the append.
Delete(n) == /\ CanCommit
             /\ n \in Last.pending
             /\ CommitWith(Last.pending \ {n},
                           IF DeletesAppend THEN <<[node |-> n, kind |-> "del"]>> ELSE <<>>)
             /\ UNCHANGED <<cache, rs>>

\* A merge (merge.rs merge): links some pending vectors (`linked`, possibly
\* none, as a consolidation-only merge does), bumps the generation, and
\* truncates the log through its head. The head stays where it is.
Merge == /\ CanCommit
         /\ \E linked \in SUBSET Last.pending:
              db' = Append(db, [Last EXCEPT !.pending = @ \ linked,
                                            !.gen = @ + 1,
                                            !.trunc = Last.head])
         /\ UNCHANGED <<cache, rs>>

\* A search's transaction begins: it takes the newest snapshot. It may run
\* other statements for a while before it searches.
Begin(r) == /\ rs[r].phase = "idle"
            /\ rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "snap", !.snap = Len(db)]]
            /\ UNCHANGED <<db, cache>>

\* The search starts (graph.rs search_graph) and loads the cache's current
\* value, a copy no one else changes. The cache may be newer than the
\* snapshot by now.
Load(r) == /\ rs[r].phase = "snap"
           /\ rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "loaded", !.seen = cache]]
           /\ UNCHANGED <<db, cache>>

\* The search's own transaction puts a vector on the pending list before it
\* searches. Nothing is committed; only this search can see it. One such
\* write is enough to break the rule it tests, and keeps TLC's search small.
OwnWrite(r, n) == /\ rs[r].phase = "snap"
                  /\ rs[r].own = {}
                  /\ rs' = [rs EXCEPT ![r].own = {n}]
                  /\ UNCHANGED <<db, cache>>

\* Whether `next` is newer than the published cache, so a publish of it may
\* replace it: a newer generation, or the same one caught up further.
Newer(next) == \/ next.gen > cache.gen
               \/ next.gen = cache.gen /\ next.pos > cache.pos

\* Publishes `next` if it is newer, as PendingCache::publish's rcu does.
Publish(next) == cache' = IF Newer(next) THEN next ELSE cache

\* The search itself (graph.rs pending_candidates), in one step: it decides
\* by the rules in the story, and records whether its candidates held every
\* vector it needed.
Search(r) ==
  LET st == db[rs[r].snap]                    \* the search's snapshot
      seen == rs[r].seen                      \* the cache copy it loaded
      visible == st.pending \cup rs[r].own    \* what its scan would find
      done(got) == rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "done", !.ok = visible \subseteq got]]
      \* The cache belongs to this search's generation; the NoGeneration bug
      \* accepts any loaded cache.
      sameGen == IF GenerationCheck THEN seen.gen = st.gen ELSE seen.gen >= 0
      \* The search is no older than the cache's load; the NoLoadedAt bug
      \* skips the check.
      fresh == ~LoadedAtCheck \/ st.head >= seen.loadedAt
      \* The log positions the cache has not seen, up to the search's head.
      need == {p \in (seen.pos + 1)..st.head : p > st.trunc}
      \* The vectors those entries insert. The StillPending bug keeps only
      \* those still pending at this snapshot, as an id-only log must.
      added == {st.entry[p].node : p \in {q \in need : st.entry[q].kind = "ins"}}
      kept == IF EntriesCarryCode THEN added ELSE added \cap st.pending
  IN /\ rs[r].phase = "loaded"
     \* The rules are tried in order, as the code's if-else chain tries them.
     /\ IF rs[r].own # {} /\ OwnWritesScan
        \* Rule 1: its own writes are only in its scan.
        THEN done(visible) /\ UNCHANGED cache
        ELSE IF sameGen /\ fresh /\ seen.pos >= st.head
        \* Rule 2, nothing to catch up on.
        THEN done(seen.ids) /\ UNCHANGED cache
        ELSE IF sameGen /\ fresh
        \* Rule 2, caught up through the search's head, then published.
        THEN LET next == [seen EXCEPT !.pos = st.head, !.ids = @ \cup kept]
             IN done(next.ids) /\ Publish(next)
        ELSE IF seen.gen < st.gen
        \* Rule 3: a full load from the scan, then published.
        THEN done(visible) /\ Publish([gen |-> st.gen, loadedAt |-> st.head,
                                       pos |-> st.head, ids |-> st.pending])
        \* Rule 4: the cache is newer than the search.
        ELSE done(visible) /\ UNCHANGED cache
     \* A search commits nothing.
     /\ UNCHANGED db

\* A finished search that held everything starts over, so TLC also runs
\* searches after later commits. One that missed something stays, for the
\* invariant to report.
Again(r) == /\ rs[r].phase = "done"
            /\ rs[r].ok
            /\ rs' = [rs EXCEPT ![r] = Idle]
            /\ UNCHANGED <<db, cache>>

\* A step is any commit, any search step, or a search's own write.
Next == \/ \E n \in Nodes: Insert(n) \/ Delete(n)
        \/ Merge
        \/ \E r \in Readers: Begin(r) \/ Load(r) \/ Search(r) \/ Again(r)
        \/ \E r \in Readers, n \in Nodes: OwnWrite(r, n)

\* Runs start at Init and take Next steps, or stay still.
Spec == Init /\ [][Next]_vars

\* Vector ids are interchangeable, and so are searches: TLC checks one of each
\* family of states that differ only by renaming them.
Symmetry == Permutations(Nodes) \cup Permutations(Readers)

\* Promise 1: every finished search holds every vector its snapshot, and its
\* own transaction, has pending (its `ok`, set when it ended).
INV_NoHiddenPending == \A r \in Readers: rs[r].phase = "done" => rs[r].ok

\* Promise 2: the published cache holds every vector pending at any snapshot
\* it is valid for: its generation, a head at or after its load, and a head
\* it has caught up to.
INV_CacheCovers ==
  cache.gen >= 0 =>
    \A s \in 1..Len(db):
      (db[s].gen = cache.gen /\ cache.loadedAt <= db[s].head /\ db[s].head <= cache.pos)
        => db[s].pending \subseteq cache.ids
====
