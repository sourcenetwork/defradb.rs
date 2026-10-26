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
\* An update of a pending vector appends again: the vector now has an entry
\* per version, and only its newest one at or before a search's head is the
\* code that search may score it by. An older code would rank the vector
\* where it used to be, and a crowd of such vectors could fill every slot a
\* search re-ranks, pushing out the vector that is nearest now.
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
\* buffer, without positions, oldest first. Catch-ups only ever add, and
\* within one generation the log only grows, so a cache that is ahead of a
\* search still holds everything that search needs. It also holds entries
\* from that search's future, so the cache keeps each entry's position and
\* the search skips every entry past its own head. Last, the search keeps
\* one entry per vector, the newest: its own appends are newer than any log
\* entry, and the last of them is the newest.
\*
\* Keeping one entry per vector must not cost a map on every search, so the
\* cache marks each entry with the position of its vector's next entry: its
\* superseded marker. A search skips an entry only when that next entry is
\* at or before its own head; a marker from its future changes nothing for
\* it. Markers sit on entries copies share, and say only what the log says,
\* so any search may set them, even one whose copy is never published. A
\* copy stores no positions: entries run gap-free from the first, so a read
\* with an entry missing must keep no copy, or every later position in it
\* would be wrong.
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
\*      find it - INV_NoLostVector;
\*   4. a search scores a vector by any entry but the newest one its
\*      snapshot, or its own transaction, holds - INV_ScoresLatest.
\*
\* Each guard has a RED configuration that removes it and fails:
\*   MC_PendingCache_Red_NoGeneration: the cache is used whatever its
\*     generation, so a search newer than a merge scores a vector by an
\*     entry that merge truncated, and a search older than a merge uses a
\*     cache loaded after it and misses what that merge linked, as the
\*     example shows;
\*   MC_PendingCache_Red_OwnWrites: a search leaves out its own transaction's
\*     appends, which no log read can show it;
\*   MC_PendingCache_Red_TruncateThroughHead: a merge truncates through the
\*     head rather than through what it linked, and drops vectors it never
\*     linked;
\*   MC_PendingCache_Red_EveryVersion: a search scores every entry of an
\*     updated vector, so an obsolete code competes with the current one;
\*   MC_PendingCache_Red_FutureVersion: a search uses every entry of a cache
\*     that is ahead of it, so a code from its future is the newest it sees;
\*   MC_PendingCache_Red_HeadBlindMarks: a search skips an entry for any
\*     marker, even one set by an update from its future, and so loses the
\*     entry that is newest for it;
\*   MC_PendingCache_Red_NoGapCheck: a copy loaded from a read with an entry
\*     missing numbers the rest one short, and searches trust it.
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
\*   cache        pending_cache.rs PendingCache, its published Copy and Tag;
\*                entries are Segment's positions and ids, read through a
\*                head by Copy::for_each_latest_through
\*   Latest       graph.rs pending_candidates: on a scan, the map from each
\*                vector to the distance of its last entry visited
\*   marks        pending_cache.rs Segment::next (set by Segment::push within
\*                a segment and Segment::supersede across segments, from
\*                Copy::caught_up), read by Copy::for_each_latest_through
\*   fullLoad     scan_pending into a Segment; Segment::push refusing a gap
\*                (broken) is GapCheck
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
\*   Search       graph.rs pending_candidates and snapshot_pending: the three
\*                rules above, the own appends, and PendingCache::publish. Rule 1's full load
\*                is Copy::can_hold refusing the gap, or read_pending_log
\*                finding an entry gone. A copy past the
\*                64 MiB cap is published as an oversized marker that is
\*                never read as cover, so it acts as no cache: the log read
\*                of the last rule
EXTENDS Naturals, Integers, FiniteSets, Sequences, TLC

\* Nodes: the vector ids. Readers: the searches. MaxCommits: how many
\* commits TLC explores. MaxOwn: how many appends a search's own
\* transaction may make. The rest switch each guard on (TRUE) or off.
CONSTANTS Nodes, Readers, MaxCommits, MaxOwn,
          GenerationCheck, OwnAppendsRead, TruncateThroughLinked,
          LatestOnly, HeadCut, HeadAwareMarks, GapCheck

\* db: one record per commit, the newest last; a snapshot is an index into it.
\* cache: the published PendingCache; gen = -1 means nothing is loaded.
\* rs: each search's progress, snapshot, its own appends, the copy of the
\*     cache it loaded, whether its candidates held everything it needed,
\*     and whether it scored each vector by its newest entry.
\* marks: every superseded marker any search has set, as <<p, q>>: the
\*     entry at position p is followed by its vector's next entry at q. A
\*     marker sits on an entry that copies share, so it is one set for all
\*     of them, including what a search whose copy lost the publish set.
VARIABLES db, cache, rs, marks
\* Every variable, for the steps that leave some unchanged.
vars == <<db, cache, rs, marks>>

\* The cache before anything was loaded. Its entries are <<position, id>>
\* pairs, so two versions of one vector are two entries.
NoCache == [gen |-> -1, pos |-> 0, entries |-> {}]

\* The state before any commit: an empty log, generation 0, nothing live or
\* linked. entry is the log, the vector each position inserts; trunc is the
\* newest truncated position, so the live entries are trunc+1 .. head.
Genesis == [head |-> 0, entry |-> <<>>, trunc |-> 0, gen |-> 0,
            live |-> {}, graph |-> {}]

\* The newest committed state.
Last == db[Len(db)]

\* Snapshot s's log entries at positions from+1 .. to, as <<position, id>>.
Entries(s, from, to) == {<<p, s.entry[p]>> : p \in (from + 1)..to}

\* The vectors a set of entries inserts.
Ids(es) == {e[2] : e \in es}

\* The vectors snapshot s's log inserts at positions from+1 .. to.
Inserted(s, from, to) == Ids(Entries(s, from, to))

\* Snapshot s's live entries.
PendingEntries(s) == Entries(s, s.trunc, s.head)

\* What is pending at snapshot s: every vector its live entries insert.
Pending(s) == Ids(PendingEntries(s))

\* The markers a run of entries carries: each entry, paired with the
\* position of its vector's next entry in the run, when there is one.
NextMarks(es) ==
  {<<e[1], f[1]>> : e \in es, f \in es}
    \cap {<<p, q>> \in Nat \X Nat : \E e, f \in es :
             \* e and f are the same vector's entries at p and q, p first
             /\ e[1] = p /\ f[1] = q /\ e[2] = f[2] /\ p < q
             \* and no entry of that vector sits between them.
             /\ ~\E g \in es : g[2] = e[2] /\ p < g[1] /\ g[1] < q}

\* Of a set of entries, each vector's newest: no entry of the same vector has
\* a later position.
Latest(es) == {e \in es : \A f \in es : f[2] = e[2] => f[1] <= e[1]}

\* The entries of a search's own appends. They have no log position; each
\* is newer than every position (at most MaxCommits) and than the own
\* appends before it.
OwnEntries(own) == {<<MaxCommits + i, own[i]>> : i \in 1..Len(own)}

\* A search that has not started.
Idle == [phase |-> "idle", snap |-> 0, own |-> <<>>, seen |-> NoCache,
         ok |-> TRUE, latest |-> TRUE]

\* At the start: no commit yet, no cache, every search idle.
Init == /\ marks = {}
        /\ db = <<Genesis>>
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
             /\ UNCHANGED <<cache, rs, marks>>

\* A delete (mod.rs delete): the vector is no longer live. Nothing goes to
\* the log, and a linked vector stays in the graph as a tombstone.
Delete(n) == /\ CanCommit
             /\ n \in Last.live
             /\ db' = Append(db, [Last EXCEPT !.live = @ \ {n}])
             /\ UNCHANGED <<cache, rs, marks>>

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
         /\ UNCHANGED <<cache, rs, marks>>

\* A search's transaction begins: it takes the newest snapshot. It may run
\* other statements for a while before it searches.
Begin(r) == /\ rs[r].phase = "idle"
            /\ rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "snap", !.snap = Len(db)]]
            /\ UNCHANGED <<db, cache, marks>>

\* The search starts (graph.rs search_graph) and loads the cache's current
\* value, a copy no one else changes. The cache may be newer than the
\* snapshot by now.
Load(r) == /\ rs[r].phase = "snap"
           /\ rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "loaded", !.seen = cache]]
           /\ UNCHANGED <<db, cache, marks>>

\* The search's own transaction inserts or updates a vector before it
\* searches. Nothing is committed, so it has no log position; only this
\* search can see it. Up to MaxOwn such writes, so one can update another.
OwnWrite(r, n) == /\ rs[r].phase = "snap"
                  /\ Len(rs[r].own) < MaxOwn
                  /\ rs' = [rs EXCEPT ![r].own = Append(@, n)]
                  /\ UNCHANGED <<db, cache, marks>>

\* Whether `next` is newer than the published cache, so a publish of it may
\* replace it: a newer generation, or the same one caught up further.
Newer(next) == \/ next.gen > cache.gen
               \/ next.gen = cache.gen /\ next.pos > cache.pos

\* Publishes `next` if it is newer, as PendingCache::publish's rcu does.
Publish(next) == cache' = IF Newer(next) THEN next ELSE cache

\* The search itself (graph.rs pending_candidates), in one step: it decides
\* by the rules in the story, and records whether its candidates held every
\* vector it needed and scored each by its newest entry.
Search(r) ==
  LET st == db[rs[r].snap]                    \* the search's snapshot
      seen == rs[r].seen                      \* the cache copy it loaded
      mine == OwnEntries(rs[r].own)           \* its own appends' entries
      \* What it must find, and the one entry per vector it must score.
      needed == Pending(st) \cup Ids(mine)
      wanted == Latest(PendingEntries(st) \cup mine)
      \* Its own appends, read back from its transaction; the OwnWrites bug
      \* leaves them out.
      own == IF OwnAppendsRead THEN mine ELSE {}
      \* A cache's entries up to the search's head; the FutureVersion bug
      \* keeps them all.
      upToHead(es) == IF HeadCut THEN {e \in es : e[1] <= st.head} ELSE es
      \* Ends the search with `got`, read from a scan of the log, plus its own
      \* appends, keeping each vector's newest (graph.rs pending_candidates'
      \* map); the EveryVersion bug keeps every entry.
      done(got) ==
        LET all == got \cup own
            scored == IF LatestOnly THEN Latest(all) ELSE all
        IN rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "done",
                                              !.ok = needed \subseteq Ids(all),
                                              !.latest = (scored = wanted)]]
      \* Whether markers `ms` say a newer entry of e's vector is in reach: one
      \* at or before the search's head. The HeadBlindMarks bug skips e for
      \* any marker, even one from the search's future; the EveryVersion bug
      \* reads no marker at all.
      superseded(e, ms) ==
        /\ LatestOnly
        /\ \E mk \in ms : mk[1] = e[1] /\ (~HeadAwareMarks \/ mk[2] <= st.head)
      \* Ends the search with a copy's entries `es`, read through its head by
      \* Copy::for_each_latest_through: no map, an entry is skipped when its
      \* marker is in reach. A vector its own transaction appended is ranked
      \* by its newest own append instead.
      doneCopy(es, ms) ==
        LET visited == {e \in upToHead(es) : ~superseded(e, ms)}
            mineIds == Ids(own)
            scored == {e \in visited : e[2] \notin mineIds}
                        \cup (IF LatestOnly THEN Latest(own) ELSE own)
        IN rs' = [rs EXCEPT ![r] = [@ EXCEPT !.phase = "done",
                                              !.ok = needed \subseteq Ids(visited \cup own),
                                              !.latest = (scored = wanted)]]
      \* A full load (Copy::loaded from scan_pending's segment): the search
      \* scores what the scan read, and the copy is published with the
      \* markers its entries carry. The scan may also come back with an
      \* entry missing; Segment::push sees the gap and keeps no copy. The
      \* NoGapCheck bug numbers the entries after the gap one short and
      \* publishes them.
      fullLoad ==
        LET es == PendingEntries(st)
            loaded == [gen |-> st.gen, pos |-> st.head, entries |-> es]
        IN /\ done(es)
           /\ \/ Publish(loaded) /\ marks' = marks \cup NextMarks(es)
              \/ \E gap \in (st.trunc + 1)..st.head :
                   LET shifted == {<<IF e[1] > gap THEN e[1] - 1 ELSE e[1], e[2]>> :
                                     e \in es \ {<<gap, st.entry[gap]>>}}
                   IN IF GapCheck
                      THEN UNCHANGED <<cache, marks>>
                      ELSE /\ Publish([loaded EXCEPT !.entries = shifted])
                           /\ marks' = marks \cup NextMarks(shifted)
      \* The cache belongs to this search's generation; the NoGeneration bug
      \* accepts any loaded cache.
      sameGen == IF GenerationCheck THEN seen.gen = st.gen ELSE seen.gen >= 0
      \* The log entries after pos, up to the search's head: the live ones
      \* only, since a truncated entry is gone from the log.
      added == Entries(st, IF seen.pos > st.trunc THEN seen.pos ELSE st.trunc, st.head)
  IN /\ rs[r].phase = "loaded"
     \* The rules are tried in order, as the code's if-else chain tries them.
     /\ IF sameGen /\ seen.pos >= st.head
        \* Rule 1, nothing to catch up on: the copy as it is, with every
        \* marker set so far.
        THEN doneCopy(seen.entries, marks) /\ UNCHANGED <<cache, marks>>
        ELSE IF sameGen
        \* Rule 1, caught up through the search's head (Copy::caught_up marks
        \* the entries `added` supersedes before anything reads them), then
        \* published; or, when the gap is past the cap or an entry is gone, a
        \* full load from the log. TLC tries both, since either may happen.
        THEN \/ LET next == [seen EXCEPT !.pos = st.head, !.entries = @ \cup added]
                    ms == marks \cup NextMarks(next.entries)
                IN doneCopy(next.entries, ms) /\ Publish(next) /\ marks' = ms
             \/ fullLoad
        ELSE IF seen.gen < st.gen
        \* Rule 2: a full load from the log.
        THEN fullLoad
        \* Rule 3: the cache is newer than the search.
        ELSE done(PendingEntries(st)) /\ UNCHANGED <<cache, marks>>
     \* A search commits nothing.
     /\ UNCHANGED db

\* A finished search that held everything and scored it right starts over,
\* so TLC also runs searches after later commits. One that went wrong stays,
\* for the invariant to report.
Again(r) == /\ rs[r].phase = "done"
            /\ rs[r].ok
            /\ rs[r].latest
            /\ rs' = [rs EXCEPT ![r] = Idle]
            /\ UNCHANGED <<db, cache, marks>>

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

\* Promise 2: the published cache holds every entry live at any snapshot it
\* is valid for: its generation, and a head it has caught up to.
INV_CacheCovers ==
  cache.gen >= 0 =>
    \A s \in 1..Len(db):
      (db[s].gen = cache.gen /\ db[s].head <= cache.pos)
        => PendingEntries(db[s]) \subseteq cache.entries

\* Promise 3: at every snapshot, every live vector is linked or pending.
INV_NoLostVector == \A s \in 1..Len(db): db[s].live \subseteq db[s].graph \cup Pending(db[s])

\* Promise 4: every finished search scored each pending vector by exactly
\* one entry, the newest its snapshot or its own transaction holds.
INV_ScoresLatest == \A r \in Readers: rs[r].phase = "done" => rs[r].latest
====
