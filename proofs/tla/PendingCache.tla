---- MODULE PendingCache ----
\* The story, in plain words.
\*
\* A DiskANN index does not link a new vector into its graph right away. The
\* vector waits as pending until a background merge links it in, and until
\* then every search reads every pending vector as well as walking the graph,
\* so a search finds every vector its snapshot holds.
\*
\* The pending vectors are a commit-ordered log, and nothing else. An insert
\* appends the vector's id and its code, and that one append is its only
\* pending write. The log's head is the newest position, positions follow
\* commit order with no holes, and the oldest entries a merge has dealt with
\* are truncated. What is pending at a snapshot is every vector its live
\* entries insert.
\*
\* A delete appends nothing. A deleted vector's entry stays a candidate until
\* a merge truncates it, which costs a search nothing: a search ranks every
\* candidate by the full vector it reads in its own snapshot, and drops one
\* that snapshot does not have or has deleted. A merge skips it the same way.
\*
\* A merge links the live vectors a prefix of the log inserts, up to a
\* position it picks (at most a plan's worth), bumps the graph's generation,
\* and truncates the log through that position, all in one transaction.
\* Entries past it, inserts that landed while the merge ran among them, stay
\* for the next merge.
\*
\* Reading the log on every search is slow, so each node keeps a copy of
\* what is pending in memory: the pending cache. A copy is only safe if no
\* search ever trusts it to hold less than the search's own snapshot holds. A
\* vector a search's snapshot has pending but the cache lacks is a vector that
\* search can never return: a hidden vector. Extra vectors cost nothing, for
\* the reason a deleted one costs nothing.
\*
\* The cache is tagged with the generation it belongs to and the head it has
\* caught up to (pos). A search reads the generation and the head in its
\* snapshot, then:
\*   1. the cache is of its generation: it reads the log entries after pos up
\*      to its head, adds what they insert to the cache, and uses the cache;
\*      or, when those entries could not fit under the cache's cap or one
\*      of them is gone, it reads the log and loads the cache from that read;
\*   2. the cache is of an older generation, or there is none: it reads the
\*      log and loads the cache from that read;
\*   3. otherwise (the cache is newer than the search): it reads the log.
\* Whichever it used, it adds the vectors its own transaction appended and
\* has not committed: regolith hands those back from the transaction's
\* buffer, without positions. Catch-ups only ever add, and within one
\* generation the log only grows, so a cache that is ahead of a search still
\* holds everything that search needs.
\*
\* A tiny example. Vector a is inserted (log position 1) and a search loads
\* the cache at head 1: {a}. A merge links a and truncates position 1: the
\* generation is now 1. A search whose snapshot is still at generation 0 has
\* a pending and a not yet in its graph; were it to use a cache loaded at
\* generation 1, which no longer holds a, it would hide a. The generation
\* check sends it to the log instead.
\*
\* What must never happen:
\*   1. a search ends without a vector its snapshot, or its own transaction,
\*      has pending - INV_NoHiddenPending;
\*   2. the cache lacks a vector that some snapshot it is valid for has
\*      pending - INV_CacheCovers;
\*   3. a live vector is neither linked nor pending, so no search can ever
\*      find it - INV_NoLostVector.
\*
\* Each guard has a RED configuration that removes it and fails:
\*   MC_PendingCache_Red_NoGeneration: the cache is used whatever its
\*     generation, so a search older than a merge uses a cache loaded after
\*     it and misses what that merge linked, as the example shows;
\*   MC_PendingCache_Red_OwnWrites: a search leaves out its own transaction's
\*     appends, which no log read can show it;
\*   MC_PendingCache_Red_TruncateThroughHead: a merge truncates through the
\*     head rather than through what it linked, and drops vectors it never
\*     linked.
\*
\* The log is regolith's commit-ordered append, the truncate is
\* sourcenetwork/regolith#300, and the own appends are
\* Transaction::appended. A snapshot sees the log as it was at its commit:
\* entries a later merge truncates are still there for it (MVCC).
\*
\* Correspondence with the Rust code (crates/db/src/index/vector/engine/
\* diskann/, the functions this change adds or touches):
\*   db           the committed state, one record per commit: the pending
\*                log (store/kv.rs pending_log_head, iterate_pending_log,
\*                pending_log_entry), graph state generation (codec
\*                GraphState::generation), the live vectors (the staging
\*                store) and the linked ones (codec::GRAPH)
\*   Pending      graph.rs scan_pending: every vector the live entries insert
\*   cache        pending_cache.rs PendingCache, its published Copy and Tag
\*   Insert       mod.rs insert: the staging write and the log append
\*   Delete       mod.rs delete: the staging delete and the codec::DELETED
\*                tombstone; nothing goes to the log
\*   Merge        merge.rs merge_plan (the log read up to MAX_PLAN_PENDING
\*                vectors, and the position it stopped at) and merge (links
\*                the live ones, generation += 1, truncate_pending_log
\*                through that position)
\*   Begin        graph.rs search_graph: the snapshot, then the cache load
\*   OwnWrite     an insert made by the search's own transaction, read back
\*                by store/kv.rs own_pending_log (regolith
\*                Transaction::appended)
\*   Search       graph.rs pending_candidates: the three rules above, the
\*                own appends, and PendingCache::publish. Rule 1's full load
\*                is Copy::can_hold refusing the gap, or read_pending_log
\*                finding an entry gone. A copy past the
\*                64 MiB cap is published as an oversized marker that is
\*                never read as cover, so it acts as no cache: the log read
\*                of the last rule
EXTENDS Naturals, Integers, FiniteSets, Sequences, TLC

\* Nodes: the vector ids. Readers: the searches. MaxCommits: how many
\* commits TLC explores. The rest switch each guard on (TRUE) or off.
CONSTANTS Nodes, Readers, MaxCommits,
          GenerationCheck, OwnAppendsRead, TruncateThroughLinked

\* db: one record per commit, the newest last; a snapshot is an index into it.
\* cache: the published PendingCache; gen = -1 means nothing is loaded.
\* rs: each search's progress, snapshot, its own appends, the copy of the
\*     cache it loaded, and whether its candidates held everything it needed.
VARIABLES db, cache, rs
\* Every variable, for the steps that leave some unchanged.
vars == <<db, cache, rs>>

\* The cache before anything was loaded.
NoCache == [gen |-> -1, pos |-> 0, ids |-> {}]

\* The state before any commit: an empty log, generation 0, nothing live or
\* linked. entry is the log, the vector each position inserts; trunc is the
\* newest truncated position, so the live entries are trunc+1 .. head.
Genesis == [head |-> 0, entry |-> <<>>, trunc |-> 0, gen |-> 0,
            live |-> {}, graph |-> {}]

\* The newest committed state.
Last == db[Len(db)]

\* The vectors snapshot s's log inserts at positions from+1 .. to.
Inserted(s, from, to) == {s.entry[p] : p \in (from + 1)..to}

\* What is pending at snapshot s: every vector its live entries insert.
Pending(s) == Inserted(s, s.trunc, s.head)

\* A search that has not started.
Idle == [phase |-> "idle", snap |-> 0, own |-> {}, seen |-> NoCache, ok |-> TRUE]

\* At the start: no commit yet, no cache, every search idle.
Init == /\ db = <<Genesis>>
        /\ cache = NoCache
        /\ rs = [r \in Readers |-> Idle]

\* Room for one more commit, so TLC's search stays finite.
CanCommit == Len(db) <= MaxCommits

\* An insert (mod.rs insert): the vector is live, and its id and code go to
\* the log. Inserting a live vector again is an update.
Insert(n) == /\ CanCommit
             /\ db' = Append(db, [Last EXCEPT !.head = @ + 1,
                                              !.entry = Append(@, n),
                                              !.live = @ \cup {n}])
             /\ UNCHANGED <<cache, rs>>

\* A delete (mod.rs delete): the vector is no longer live. Nothing goes to
\* the log, and a linked vector stays in the graph as a tombstone.
Delete(n) == /\ CanCommit
             /\ n \in Last.live
             /\ db' = Append(db, [Last EXCEPT !.live = @ \ {n}])
             /\ UNCHANGED <<cache, rs>>

\* A merge (merge.rs merge_plan, then merge): reads the log up to a position
\* c it picks, links the live vectors those entries insert, bumps the
\* generation, and truncates the log through c. Picking c = trunc links
\* nothing, as a consolidation-only merge does. The TruncateThroughHead bug
\* (TruncateThroughLinked = FALSE) truncates through the head instead.
Merge == /\ CanCommit
         /\ \E c \in Last.trunc..Last.head:
              LET linked == Inserted(Last, Last.trunc, c) \cap Last.live
              IN db' = Append(db, [Last EXCEPT !.graph = @ \cup linked,
                                               !.gen = @ + 1,
                                               !.trunc = IF TruncateThroughLinked
                                                         THEN c ELSE Last.head])
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

\* The search's own transaction inserts a vector before it searches. Nothing
\* is committed, so it has no log position; only this search can see it. One
\* such write is enough to break the rule it tests, and keeps TLC small.
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
      needed == Pending(st) \cup rs[r].own    \* what it must find
      \* Its own appends, read back from its transaction; the OwnWrites bug
      \* leaves them out.
      own == IF OwnAppendsRead THEN rs[r].own ELSE {}
      \* Ends the search with candidates `got` plus its own appends.
      done(got) == rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "done",
                                                     !.ok = needed \subseteq (got \cup own)]]
      \* The cache belongs to this search's generation; the NoGeneration bug
      \* accepts any loaded cache.
      sameGen == IF GenerationCheck THEN seen.gen = st.gen ELSE seen.gen >= 0
      \* What the log entries after pos, up to the search's head, insert:
      \* the live ones only, since a truncated entry is gone from the log.
      added == Inserted(st, IF seen.pos > st.trunc THEN seen.pos ELSE st.trunc, st.head)
  IN /\ rs[r].phase = "loaded"
     \* The rules are tried in order, as the code's if-else chain tries them.
     /\ IF sameGen /\ seen.pos >= st.head
        \* Rule 1, nothing to catch up on.
        THEN done(seen.ids) /\ UNCHANGED cache
        ELSE IF sameGen
        \* Rule 1, caught up through the search's head, then published; or,
        \* when the gap is past the cap or an entry is gone, a full load from
        \* the log, then published. TLC tries both, since either may happen.
        THEN \/ LET next == [seen EXCEPT !.pos = st.head, !.ids = @ \cup added]
                IN done(next.ids) /\ Publish(next)
             \/ done(Pending(st)) /\ Publish([gen |-> st.gen, pos |-> st.head,
                                              ids |-> Pending(st)])
        ELSE IF seen.gen < st.gen
        \* Rule 2: a full load from the log, then published.
        THEN done(Pending(st)) /\ Publish([gen |-> st.gen, pos |-> st.head,
                                           ids |-> Pending(st)])
        \* Rule 3: the cache is newer than the search.
        ELSE done(Pending(st)) /\ UNCHANGED cache
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
\* it is valid for: its generation, and a head it has caught up to.
INV_CacheCovers ==
  cache.gen >= 0 =>
    \A s \in 1..Len(db):
      (db[s].gen = cache.gen /\ db[s].head <= cache.pos)
        => Pending(db[s]) \subseteq cache.ids

\* Promise 3: at every snapshot, every live vector is linked or pending.
INV_NoLostVector == \A s \in 1..Len(db): db[s].live \subseteq db[s].graph \cup Pending(db[s])
====
