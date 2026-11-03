---- MODULE EngineTickers ----
\* The story, in plain words.
\*
\* Every open store has an engine that counts what it does: its commits, its
\* conflicts, its merges. The node reports those counts to its metrics backend
\* as one running total per kind of count (a ticker), summed over every store
\* the process has opened. A store can close at any moment, and what it
\* counted must stay in the total after it is gone; a total a dashboard has
\* seen must never go back down.
\*
\* How the code does it. Each open store is one export: an entry in a shared
\* map of live exports, plus one number per ticker, its mark: how much of the
\* store's count is in the total already. Whoever wants to add a store's new
\* counts (a read of the totals, or the store's own close) does the same small
\* dance, called publish:
\*   1. read the store's count;
\*   2. read the mark;
\*   3. if the count is not past the mark, stop: those counts are in already;
\*   4. otherwise swap the mark from what it read to the count, with one
\*      compare-and-swap; if another publisher moved the mark first, the swap
\*      fails, so take the mark the swap found and go back to 3;
\*   5. the swap won: add the gap (the count minus the old mark) to the total.
\* Only the winner of a swap adds, and it adds exactly the stretch of counts
\* its swap claimed, so each count is added once, whoever adds it. A read
\* walks the map and publishes every export it finds, then reads the total. A
\* store that closes publishes its last counts first and leaves the map after.
\*
\* A tiny example. A store has made 2 commits and its mark is 0. A read and the
\* store's close publish at the same time. The read's count was a moment old:
\* it saw 1. The close saw 2. The read swaps 0 -> 1, wins and adds 1. The
\* close's swap 0 -> 2 fails, because the mark is 1 now; it goes round from 1,
\* swaps 1 -> 2, wins and adds 1. The total grew by 2: once per commit.
\*
\* What must never happen:
\*   1. the total goes back down - PROP_TotalNeverBack;
\*   2. an export's mark goes back down - PROP_MarkNeverBack;
\*   3. a claimed stretch of counts is added twice, or never: the total plus
\*      what winners still have to add must equal the sum of the marks
\*      - INV_Accounted;
\*   4. the total holds more than the engines counted - INV_NeverOvercount;
\*   5. a close finishes with some of its counts unclaimed
\*      - INV_ClosedComplete;
\*   6. an export leaves the map with some of its counts unclaimed
\*      - INV_LeftClaimed;
\*   7. once every read and every close is over, the total differs from what
\*      the engines counted - INV_QuiescentExact;
\*   8. a read or a close never finishes - LIVE_EveryCallEnds.
\*
\* A promise the code makes and does not keep (a finding, INV_LeftInTotals):
\* the comment on the close says a read that no longer finds an export finds
\* its counts in the totals. Claiming (the swap) and adding (the add) are two
\* steps, so a read can claim an export's counts and pause before it adds
\* them; the close then finds nothing left to claim and leaves the map, and a
\* read that starts now misses the export and loads a total without those
\* counts, until the paused read adds them. The total never goes back and
\* every count arrives exactly once; it can arrive late.
\* MC_EngineTickers_Finding_ClaimGap.cfg holds the trace.
\*
\* A reading of a count is the sum of per-thread shards, each read at its own
\* moment, so two readings that overlap in time can come back out of order.
\* Each still lies between the count when its read began and the count when it
\* ended. Here a count grows one at a time and a reading is the count at one
\* instant, so every reading a real read can return is a value the model's
\* count holds at some instant inside that read: the model covers them all.
\*
\* TLC runs whole steps one at a time, as if memory were sequentially
\* consistent. Every promise here but the close's order is about one shared
\* word at a time (a mark, the total) or about a value a thread read itself,
\* where an atomic read-modify-write already sees the latest value, so the
\* Rust orderings (AcqRel on the swap and the add, Acquire on the loads) are
\* not what carries them. The close's order (add, then leave the map) reaches
\* a later read through the map's own release on removal and acquire on the
\* walk.
\*
\* Each ticker has its own mark and its own total, and no step touches two
\* tickers, so one ticker stands for all of them.
\*
\* Correspondence with the Rust code (crates/telemetry/src/engine_tickers.rs,
\* by line, unless named):
\*
\*   | Model          | Rust                                                        |
\*   |----------------|-------------------------------------------------------------|
\*   | count          | regolith::Statistics::get_ticker: the sum of the shards the |
\*   |                | engine's record_tick adds to (regolith src/statistics.rs)   |
\*   | made           | bookkeeping only: every count the engine made               |
\*   | mark           | Source::published[at] (:71), zero at export (:119)          |
\*   | totals         | TOTALS[at] (:43)                                            |
\*   | live           | the export's entry in LIVE (:46): LIVE.insert (:121),       |
\*   |                | LIVE.remove (:134)                                          |
\*   | held           | the Arc<Statistics> the store and its engine hold (storage  |
\*   |                | backends/shared.rs TransactionMetrics::engine, the engine's |
\*   |                | Options); Source::statistics is only Weak (:70)             |
\*   | pc, at         | where a read is in engine_ticker_totals (:140-154) and a    |
\*   |                | close in EngineTickerExport::drop (:130-135); `at` is the   |
\*   |                | export a read's walk of LIVE.iter (:141) has reached        |
\*   | now            | Source::publish: let now = statistics.get_ticker (:83)      |
\*   | seen           | let mut seen = published[at].load (:84);                    |
\*   |                | Err(current) => seen = current (:96)                        |
\*   | owed           | now - seen, between a won compare_exchange_weak (:86-92)    |
\*   |                | and its TOTALS[at].fetch_add (:93)                          |
\*   | Count          | the engine counting into its Statistics (regolith           |
\*   |                | Statistics::record_tick)                                    |
\*   | Reset          | regolith::Statistics::reset (regolith src/statistics.rs)    |
\*   | LetGo          | the store and its engine dropping their Arc<Statistics>     |
\*   | Visit          | engine_ticker_totals: LIVE.iter (:141), upgrade (:142);     |
\*   |                | past the last export, the TOTALS loads (:146-153)           |
\*   | Drop           | EngineTickerExport::drop (:130) reaching publish (:133)     |
\*   | Remove         | LIVE.remove(&self.id) (:134)                                |
\*   | ReadStat       | Source::publish :83                                         |
\*   | LoadMark       | Source::publish :84                                         |
\*   | Claim          | while now > seen (:85), compare_exchange_weak(seen, now)    |
\*   |                | (:86-91), Err(current) => seen = current (:96)              |
\*   | Add            | Ok(_) => TOTALS[at].fetch_add(now - seen) (:92-94)          |
\*   | Forward        | while now > seen (:85) (RED: while now != seen)             |
\*   | ClaimByCas     | compare_exchange_weak (:86) (RED: a plain store)            |
\*   | Retry          | the loop goes round after Err (:85, :96) (RED: break)       |
\*   | Refresh        | Err(current) => seen = current (:96) (RED: Err(_) => {})    |
\*   | PublishFirst   | publish (:133) before LIVE.remove (:134) (RED: after it)    |
\*   | HoldStats      | the guard's own Arc<Statistics> (:111, :125) the close      |
\*   |                | publishes with (:133) (RED: upgrade the Weak of :70)        |
\*   | OnlyGrow       | caller contract: no Statistics::reset on exported           |
\*   |                | statistics (Source::publish doc, :77-79)                    |
\*   | OwnStats       | caller contract: one export per Statistics, ever (storage   |
\*   |                | regolith/store.rs:82-87 makes one per open)                 |
\*   | StopFirst      | caller contract: the engine stops counting before its guard |
\*   |                | drops (#[must_use], :105; storage shared.rs:76 keeps the    |
\*   |                | guard in the metrics the engine's listener holds)           |
\*   | INV_*, PROP_*  | checked on real threads by tests/engine_tickers_race.rs     |
\*   |                | (:34 never back, :35 never over, :48 exact after the close) |
\*   |                | and the unit test at :215-241                               |
\*
\* Gaps, each closed or explained:
\*   * several tickers: one stands for all (each has its own two words);
\*   * the export's creation (:116-121) is not a step: the store exports
\*     before its engine opens (storage regolith/store.rs:87, then :94), so
\*     no count comes before the entry; the model starts with it in the map;
\*   * NEXT_SOURCE.fetch_add (:116) gives every export its own key; the model
\*     gives every export its own slot;
\*   * HopscotchMap::iter may or may not show an entry that comes or goes
\*     during the walk; the model looks at each export at the moment the walk
\*     reaches it, which covers both;
\*   * a read's lookup and its upgrade are one step: while the entry is in
\*     the map the guard's own Arc keeps the statistics alive, so the upgrade
\*     fails only once the close is over, when a lookup would miss it anyway;
\*     a read that found the export and publishes after the close is a read
\*     whose later steps run after the close, which the model has;
\*   * compare_exchange_weak may fail with the very mark it expected: the loop
\*     goes round in the same state, a stutter TLA+ always allows;
\*   * an owed amount below zero (RED Backward only) is Rust's `now - seen`
\*     with now < seen: a debug build panics, a release build wraps, and adding
\*     the wrapped number takes the total back by the same amount;
\*   * u64 overflow of a count or a total is not modeled.

EXTENDS Integers
\* Integers gives numbers below zero, so a RED run can take the total back.

CONSTANTS NS, NR, MaxCount,
          Forward, ClaimByCas, Retry, Refresh, PublishFirst, HoldStats,
          OnlyGrow, OwnStats, StopFirst
\* NS is how many exports (open stores) the model runs, NR how many reads,
\* MaxCount how far each engine counts. The six guards (Forward to HoldStats)
\* are all TRUE in the real code; the three caller contracts (OnlyGrow,
\* OwnStats, StopFirst) are all TRUE for a correct caller. Each RED
\* configuration turns exactly one of them off.

ASSUME /\ NS \in 1..2
       \* one or two exports;
       /\ NR \in 1..2
       \* one or two reads;
       /\ MaxCount \in 1..3
       \* a few counts per engine;
       /\ Forward \in BOOLEAN
       \* the loop runs only while the reading is past the mark it saw;
       /\ ClaimByCas \in BOOLEAN
       \* the mark moves by compare-and-swap;
       /\ Retry \in BOOLEAN
       \* a lost swap goes round the loop;
       /\ Refresh \in BOOLEAN
       \* going round, the loop takes the mark the lost swap found;
       /\ PublishFirst \in BOOLEAN
       \* the close publishes before it leaves the map;
       /\ HoldStats \in BOOLEAN
       \* the close holds the statistics itself;
       /\ OnlyGrow \in BOOLEAN
       \* contract: nobody resets exported statistics;
       /\ OwnStats \in BOOLEAN
       \* contract: every export has statistics of its own;
       /\ StopFirst \in BOOLEAN
       \* contract: an engine stops counting before its guard drops.

Sources == 1..NS
\* The exports, named 1, 2: one per open store. Each number also names the
\* statistics that store's engine counts into.

Readers == 1..NR
\* The reads of the totals, named 1, 2: each is one call of
\* engine_ticker_totals, say one metrics collection.

Pubs == ({"read"} \X Readers) \cup ({"close"} \X Sources)
\* Everyone who does the publish dance: a read, or the close of one export.

IsCloser(p) == p[1] = "close"
\* Whether a publisher is an export's close rather than a read.

Closer(s) == <<"close", s>>
\* The close of export s.

StatOf(s) == IF OwnStats THEN s ELSE 1
\* The statistics export s reads: its own, or, in the RED that breaks the
\* contract, the first export's, shared by both.

PcStates == {"visit", "open", "read", "load", "cas", "add", "remove", "done"}
\* Where a publisher can be:
\*   visit  - a read walking the map, about to look at its next export;
\*   open   - a close that has not begun: its store is open;
\*   read   - about to read the export's count;
\*   load   - about to read the export's mark;
\*   cas    - about to test its reading against what it saw, and maybe swap;
\*   add    - its swap won; it adds what it claimed to the total next;
\*   remove - a close about to take its export out of the map;
\*   done   - a read that returned, or a close that is over.

Span == (NR + 1) * NS * MaxCount
\* How far the total can move either way: each publisher adds at most once
\* per export, never more than MaxCount at a time.

VARIABLES count, made, mark, totals, live, held, pc, at, now, seen, owed
\* count  - each statistics' count as a read finds it (Statistics::get_ticker);
\* made   - each statistics' counts its engine made, never taken back
\*          (bookkeeping for the promises; equal to count unless reset);
\* mark   - each export's mark (Source::published);
\* totals - the exported total (TOTALS);
\* live   - whether each export is in the map of live exports (LIVE);
\* held   - whether the store and its engine still hold each statistics;
\* pc     - where each publisher is (PcStates);
\* at     - the export each publisher works on (a read moves through them);
\* now    - each publisher's reading of the count (publish's `now`);
\* seen   - each publisher's idea of the mark (publish's `seen`);
\* owed   - what each publisher's won swap claimed and it has not added yet.

vars == <<count, made, mark, totals, live, held, pc, at, now, seen, owed>>
\* Every variable, for "nothing else changes" and for fairness.

Init == /\ count = [t \in Sources |-> 0]
        \* no statistics has counted anything;
        /\ made = [t \in Sources |-> 0]
        \* no engine has made a count;
        /\ mark = [s \in Sources |-> 0]
        \* every mark starts at zero (export_engine_tickers);
        /\ totals = 0
        \* the total starts at zero;
        /\ live = [s \in Sources |-> TRUE]
        \* every export is in the map: it joins before its engine counts;
        /\ held = [s \in Sources |-> TRUE]
        \* every store and its engine hold their statistics;
        /\ pc = [p \in Pubs |-> IF IsCloser(p) THEN "open" ELSE "visit"]
        \* every store is open, and every read is about to walk the map;
        /\ at = [p \in Pubs |-> IF IsCloser(p) THEN p[2] ELSE 1]
        \* a close works on its own export, a read starts at the first;
        /\ now = [p \in Pubs |-> 0]
        \* nobody has a reading yet;
        /\ seen = [p \in Pubs |-> 0]
        \* nobody has seen a mark yet;
        /\ owed = [p \in Pubs |-> 0]
        \* and nobody owes the total anything.

\* -------------------------------------------------------------- engines

Count(s) ==
  \* The engine of export s counts one more thing (a commit, say). It
  \* happens whenever the engine works.
  /\ held[s]
  \* only a running engine counts, one that still holds its statistics;
  /\ StopFirst => pc[Closer(s)] = "open"
  \* the contract: it stops counting before its export's close begins (the
  \* RED lets it count on after);
  /\ made[StatOf(s)] < MaxCount
  \* the model stops at MaxCount counts;
  /\ count' = [count EXCEPT ![StatOf(s)] = @ + 1]
  \* its statistics' count grows by one,
  /\ made' = [made EXCEPT ![StatOf(s)] = @ + 1]
  \* and so does the number of counts made;
  /\ UNCHANGED <<mark, totals, live, held, pc, at, now, seen, owed>>
  \* nothing else changes.

Reset(t) ==
  \* RED only: a caller calls Statistics::reset on exported statistics t,
  \* and its count reads zero again although the counts were made.
  /\ ~OnlyGrow
  \* only when the contract "counts only grow" is broken;
  /\ count[t] > 0
  \* resetting a zero count would change nothing;
  /\ count' = [count EXCEPT ![t] = 0]
  \* the count reads zero;
  /\ UNCHANGED <<made, mark, totals, live, held, pc, at, now, seen, owed>>
  \* the counts made stay made, and nothing else changes.

LetGo(s) ==
  \* The store and its engine let go of export s's statistics: the store
  \* closes and its engine goes away. This can come before or after the
  \* export's close begins.
  /\ held[s]
  \* only what is held can be let go;
  /\ held' = [held EXCEPT ![s] = FALSE]
  \* now only the export (and a read in flight) can hold it;
  /\ UNCHANGED <<count, made, mark, totals, live, pc, at, now, seen, owed>>
  \* nothing else changes.

Readable(s) == HoldStats \/ held[s]
\* Whether a publisher can still read export s's statistics: while the
\* export's guard lives, always, because the guard holds them itself (the
\* real code); in the RED without that, only while the store still does.

\* ---------------------------------------------------------------- reads

Visit(r) ==
  \* Read r looks at the next export of its walk of the map (LIVE.iter) and
  \* upgrades its weak handle to the statistics. Past the last export it
  \* loads the total and returns.
  LET p == <<"read", r>> IN
  \* p is this read, as a publisher;
  /\ pc[p] = "visit"
  \* only a read that is walking the map;
  /\ IF at[p] > NS
     \* it has looked at every export:
     THEN /\ pc' = [pc EXCEPT ![p] = "done"]
          \* it loads the total and returns it (the load changes nothing);
          /\ at' = at
          \* and stays where it is;
     ELSE IF live[at[p]] /\ Readable(at[p])
          \* the export is in the map and its statistics are still there:
          THEN /\ pc' = [pc EXCEPT ![p] = "read"]
               \* it publishes that export next,
               /\ at' = at
               \* still pointing at it;
          ELSE /\ pc' = pc
               \* the export left the map, or its statistics are gone:
               /\ at' = [at EXCEPT ![p] = @ + 1]
               \* it skips to the next one;
  /\ UNCHANGED <<count, made, mark, totals, live, held, now, seen, owed>>
  \* looking changes nothing else.

\* --------------------------------------------------------------- closes

Drop(s) ==
  \* The guard of export s drops (EngineTickerExport::drop begins): the
  \* store is closing. Nothing forces it.
  LET p == Closer(s) IN
  \* p is this close, as a publisher;
  /\ pc[p] = "open"
  \* only an export that is still open;
  /\ pc' = [pc EXCEPT ![p] =
              IF ~PublishFirst THEN "remove"
              ELSE IF Readable(s) THEN "read" ELSE "remove"]
  \* the real order publishes first, with the guard's own handle to the
  \* statistics; the RED order leaves the map first; the RED without that
  \* handle finds the statistics gone, publishes nothing and leaves the map;
  /\ UNCHANGED <<count, made, mark, totals, live, held, at, now, seen, owed>>
  \* beginning the close changes nothing else.

Remove(s) ==
  \* The close takes export s out of the map (LIVE.remove): a read that
  \* looks from now on does not find it.
  LET p == Closer(s) IN
  \* p is this close;
  /\ pc[p] = "remove"
  \* only a close at this step;
  /\ live' = [live EXCEPT ![s] = FALSE]
  \* the export leaves the map;
  /\ pc' = [pc EXCEPT ![p] =
              IF PublishFirst THEN "done"
              ELSE IF Readable(s) THEN "read" ELSE "done"]
  \* in the real order the close is over; in the RED order it publishes now;
  /\ UNCHANGED <<count, made, mark, totals, held, at, now, seen, owed>>
  \* nothing else changes.

\* -------------------------------------------------------------- publish

After(p) == IF IsCloser(p)
               THEN IF PublishFirst THEN "remove" ELSE "done"
               ELSE "visit"
\* Where a publisher goes once its publish is over: a close goes on to leave
\* the map (or is done, in the RED order); a read goes back to its walk.

Leave(p) == /\ pc' = [pc EXCEPT ![p] = After(p)]
            \* the publisher moves on;
            /\ at' = IF IsCloser(p) THEN at ELSE [at EXCEPT ![p] = @ + 1]
            \* a read moves to the next export, a close stays on its own;
            /\ now' = [now EXCEPT ![p] = 0]
            \* its reading
            /\ seen' = [seen EXCEPT ![p] = 0]
            \* and its seen mark are forgotten: they were locals of the loop.
\* The end of one publish: the loop exits.

ReadStat(p) ==
  \* Step 1 of publish: read the export's count (statistics.get_ticker).
  /\ pc[p] = "read"
  \* only a publisher at this step;
  /\ now' = [now EXCEPT ![p] = count[StatOf(at[p])]]
  \* its reading is the count right now;
  /\ pc' = [pc EXCEPT ![p] = "load"]
  \* it reads the mark next;
  /\ UNCHANGED <<count, made, mark, totals, live, held, at, seen, owed>>
  \* nothing else changes.

LoadMark(p) ==
  \* Step 2 of publish: read the export's mark (published[at].load).
  /\ pc[p] = "load"
  \* only a publisher at this step;
  /\ seen' = [seen EXCEPT ![p] = mark[at[p]]]
  \* it notes the mark as it is right now;
  /\ pc' = [pc EXCEPT ![p] = "cas"]
  \* it tests and swaps next;
  /\ UNCHANGED <<count, made, mark, totals, live, held, at, now, owed>>
  \* nothing else changes.

Ahead(p) == IF Forward THEN now[p] > seen[p] ELSE now[p] # seen[p]
\* Step 3, the loop's test: the real code goes on only while its reading is
\* past the mark it saw; the RED goes on whenever the two differ, even when
\* the reading is behind.

Claim(p) ==
  \* Steps 3 and 4 of publish: the loop's test, then one compare-and-swap of
  \* the mark from what it saw to its reading. The test reads only the
  \* publisher's own numbers, so it and the swap can be one step.
  LET s == at[p] IN
  \* s is the export it publishes;
  /\ pc[p] = "cas"
  \* only a publisher at this step;
  /\ IF ~Ahead(p)
     \* the reading is not past the mark it saw:
     THEN /\ Leave(p)
          \* the loop ends: those counts were claimed already,
          /\ UNCHANGED <<mark, owed>>
          \* and it claims nothing;
     ELSE IF ~ClaimByCas \/ mark[s] = seen[p]
          \* the mark still holds what it saw (the RED does not check):
          THEN /\ mark' = [mark EXCEPT ![s] = now[p]]
               \* the swap wins: the mark becomes its reading,
               /\ owed' = [owed EXCEPT ![p] = now[p] - seen[p]]
               \* and it owes the total the stretch it claimed,
               /\ pc' = [pc EXCEPT ![p] = "add"]
               \* which it adds next,
               /\ UNCHANGED <<at, now, seen>>
               \* keeping its numbers;
          ELSE IF Retry
               \* someone moved the mark first, and the loop goes round:
               THEN /\ seen' = [seen EXCEPT ![p] = IF Refresh THEN mark[s] ELSE @]
                    \* it takes the mark the lost swap found (the RED keeps
                    \* its old one, so every later swap loses the same way),
                    /\ UNCHANGED <<mark, owed, pc, at, now>>
                    \* and tries again;
               ELSE /\ Leave(p)
                    \* the RED gives up after one lost swap,
                    /\ UNCHANGED <<mark, owed>>
                    \* claiming nothing;
  /\ UNCHANGED <<count, made, totals, live, held>>
  \* the swap never touches the counts, the total or the map.

Add(p) ==
  \* Step 5 of publish: the winner adds what it claimed to the total
  \* (TOTALS[at].fetch_add(now - seen)), and its publish is over.
  /\ pc[p] = "add"
  \* only a publisher whose swap won;
  /\ totals' = totals + owed[p]
  \* the total grows by the stretch it claimed;
  /\ owed' = [owed EXCEPT ![p] = 0]
  \* it owes nothing now;
  /\ Leave(p)
  \* and the loop ends;
  /\ UNCHANGED <<count, made, mark, live, held>>
  \* nothing else changes.

\* ----------------------------------------------------------------- spec

PubStep(p) == ReadStat(p) \/ LoadMark(p) \/ Claim(p) \/ Add(p)
\* Every step of one publish.

Next == \/ \E s \in Sources : Count(s) \/ Reset(s) \/ LetGo(s) \/ Drop(s) \/ Remove(s)
        \* an engine counts, a caller resets, a store lets go, a close moves;
        \/ \E r \in Readers : Visit(r)
        \* a read walks the map;
        \/ \E p \in Pubs : PubStep(p)
        \* or a publisher takes a step of its dance.
\* Anyone takes any step it can, in any order.

Fairness == /\ \A r \in Readers : WF_vars(Visit(r))
            \* a read keeps walking;
            /\ \A s \in Sources : WF_vars(Remove(s))
            \* a close that reached the map step takes it;
            /\ \A p \in Pubs : WF_vars(PubStep(p))
            \* a publish keeps stepping while it can.
\* Counting, resetting, letting go and beginning a close are choices:
\* nothing forces them.

Spec == Init /\ [][Next]_vars /\ Fairness
\* The model: start at Init, take steps of Next, with the fairness above.

\* ----------------------------------------------------------- properties

RECURSIVE Sum(_, _)
\* Sum(f, S) adds up the values of f over the set S.
Sum(f, S) == IF S = {} THEN 0
             \* nothing left to add: zero;
             ELSE LET x == CHOOSE y \in S : TRUE
                  \* otherwise pick any element,
                  IN f[x] + Sum(f, S \ {x})
                  \* and add its value to the sum over the rest.

TypeOK == /\ count \in [Sources -> 0..MaxCount]
          \* every count is between zero and MaxCount;
          /\ made \in [Sources -> 0..MaxCount]
          \* so is every number of counts made;
          /\ mark \in [Sources -> 0..MaxCount]
          \* a mark is always some reading, so it is too;
          /\ totals \in -Span..Span
          \* the total stays within the bound (below zero only in a RED);
          /\ live \in [Sources -> BOOLEAN]
          \* each export is in the map or not;
          /\ held \in [Sources -> BOOLEAN]
          \* each statistics is held by its store or not;
          /\ pc \in [Pubs -> PcStates]
          \* each publisher is at a named step;
          /\ at \in [Pubs -> 1..(NS + 1)]
          \* each publisher works on an export, or a read is past the last;
          /\ now \in [Pubs -> 0..MaxCount]
          \* every reading is a count;
          /\ seen \in [Pubs -> 0..MaxCount]
          \* every seen mark is a mark;
          /\ owed \in [Pubs -> -MaxCount..MaxCount]
          \* and what a publisher owes is one reading minus one mark.
\* Every variable holds a value of the expected kind.

INV_Accounted ==
  \* Promise: every stretch of counts a swap claimed reaches the total
  \* exactly once.
  totals + Sum(owed, Pubs) = Sum(mark, Sources)
  \* the total, plus what winners still have to add, is the sum of the marks.

INV_NeverOvercount ==
  \* Promise: the total never holds more than the engines counted, so no
  \* count is added twice.
  totals <= Sum(made, Sources)
  \* the total is at most every count every engine made.

INV_ClosedComplete ==
  \* Promise: once an export's close is over, every count its engine made is
  \* claimed in its mark.
  \A s \in Sources : pc[Closer(s)] = "done" => mark[s] >= made[StatOf(s)]
  \* for every export: closed means its mark covers all its counts.

INV_LeftClaimed ==
  \* Promise: an export that left the map has every count its engine made
  \* claimed, so a read that no longer finds it loses nothing that is not
  \* already on its way into the total.
  \A s \in Sources : ~live[s] => mark[s] >= made[StatOf(s)]
  \* for every export: out of the map means its mark covers all its counts.

Quiet == \A p \in Pubs : pc[p] = "done"
\* Every read has returned and every close is over.

INV_QuiescentExact ==
  \* Promise: once every read and every close is over, the total is exactly
  \* what the engines counted: no count lost, none added twice.
  Quiet => totals = Sum(made, Sources)
  \* when all is quiet, the total equals every count every engine made.

OwersOf(s) == {p \in Pubs : at[p] = s}
\* The publishers working on export s (only one whose swap won owes anything).

INV_LeftInTotals ==
  \* The promise the close's comment makes (:131-132), which the code does
  \* not keep: an export that left the map has all its counts in the total
  \* itself, not only claimed. Checked only by the finding configuration.
  \A s \in Sources : ~live[s] => mark[s] - Sum(owed, OwersOf(s)) >= made[StatOf(s)]
  \* for every export out of the map: its mark, less what winners still owe
  \* for it, covers all its counts.

PROP_TotalNeverBack ==
  \* Promise: the exported total never goes back.
  [][totals' >= totals]_totals
  \* every step leaves the total where it was or higher.

PROP_MarkNeverBack ==
  \* Promise: an export's mark never goes back.
  [][\A s \in Sources : mark'[s] >= mark[s]]_mark
  \* every step leaves every mark where it was or higher.

Calling(p) == pc[p] \notin {"open", "done"}
\* A read that has not returned yet, or a close that began and is not over.

LIVE_EveryCallEnds ==
  \* Promise: every read returns, and every close that began finishes.
  \A p \in Pubs : (Calling(p) ~> (pc[p] = "done"))
  \* for every publisher: once it is in a call, it eventually finishes.

====
