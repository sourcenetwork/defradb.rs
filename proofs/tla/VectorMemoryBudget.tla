---- MODULE VectorMemoryBudget ----
\* The story, in plain words.
\*
\* A node keeps some of its vector indexes' data in memory, so searches do not
\* read it from storage every time: a DiskANN index's codes, loaded all at
\* once, and single codes a search read on a miss. The operator gives the node
\* a budget: how many bytes all of that may take together. Every worker shares
\* the one budget, a counter of the bytes held. Here every code costs one unit.
\*
\* Who acts:
\*   * merges, which make a new generation of the graph, so the codes in
\*     memory are out of date;
\*   * searches, each reading one generation. A search that finds the codes
\*     out of date reloads them: it drops the old codes, then takes the codes
\*     one by one, each only while the budget has room, and finally publishes
\*     them. A search that finds the codes current may keep a code it read on
\*     a miss, if the budget has room. A reload can also stop half way (an
\*     error, or the search was cancelled).
\*
\* What must never happen:
\*   1. the caches hold more than the budget - INV_BudgetHolds;
\*   2. the counter disagrees with what the caches hold, so room leaks away or
\*      appears from nowhere - INV_Accounted;
\*   3. a reload publishes fewer codes than fit, because other memory of the
\*      same index took the room it needed - INV_FullLoad. The one thing that
\*      may squeeze it is a reload that was taken over and has not noticed
\*      yet: it lets go at its next step;
\*   4. a running reload loses its claim to anything but a reload of a newer
\*      generation, which throws its work away for nothing -
\*      INV_ClaimLostOnlyToNewer.
\*
\* Each guard has a RED configuration that removes it and fails:
\*   MC_VectorMemoryBudget_Red_SplitCheck: the room check and the take are
\*     two steps, so two takers both fit into the same room (INV_BudgetHolds);
\*   MC_VectorMemoryBudget_Red_Leak: a reload that stops half way keeps its
\*     room, which nothing ever gives back (INV_Accounted);
\*   MC_VectorMemoryBudget_Red_KeepOld: a reload keeps the old codes until it
\*     publishes, so they hold the room the new ones need (INV_FullLoad);
\*   MC_VectorMemoryBudget_Red_MissesWhileLoading: searches keep missed codes
\*     while a reload runs, taking its room (INV_FullLoad);
\*   MC_VectorMemoryBudget_Red_NoSingleFlight: every search that finds the
\*     codes out of date reloads them, taking over a running reload of the
\*     same generation (INV_ClaimLostOnlyToNewer);
\*   MC_VectorMemoryBudget_Red_StaleFinish: a reload that was taken over still
\*     publishes its older codes, wiping the newer reload's claim
\*     (INV_ClaimLostOnlyToNewer);
\*   MC_VectorMemoryBudget_Red_StepAfterLoss: a reload that was taken over
\*     keeps taking codes instead of stopping (INV_NoTakeAfterLoss);
\*   MC_VectorMemoryBudget_Red_StaleAbandon: an older search's cancellation
\*     clears a newer reload's claim (INV_NoClaimClearedByStaleAbandon);
\*   MC_VectorMemoryBudget_Red_PendingWhileLoading: a pending-list copy is
\*     published while a reload runs and takes its room (INV_FullLoad).
\*
\* Correspondence with the Rust code (crates/database/db/src/index/vector/):
\*   used, Limit     budget.rs MemoryBudget (`used`, `limit`), MemoryBudget::take
\*   Take            budget.rs Charge::grow, Charge::cover (one compare-and-swap)
\*   cache           engine/diskann/cache.rs Loaded: `generation`, `loading`,
\*                   `codes` with `codes_charge`, `misses` with `misses_charge`
\*   Begin           engine/diskann/reload.rs Reload::begin, called from
\*                   engine/diskann/graph.rs DiskAnn::refresh_codes
\*   Owns, by        engine/diskann/reload.rs Reload::owned and the ticket
\*                   CodeCache::begin_reload draws for each reload
\*   Step            engine/diskann/reload.rs Reload::insert, Reload::stop
\*   Finish          engine/diskann/reload.rs Reload::finish,
\*                   engine/diskann/cache.rs CodeCache::replace_charged
\*   Abandon         engine/diskann/reload.rs Drop for Reload, budget.rs Drop
\*                   for Charge
\*   Fill            engine/diskann/cache.rs CodeCache::insert
\* The pending-list copy (engine/diskann/pending_cache.rs PendingCache::publish_if,
\* called by engine/diskann/cache.rs CodeCache::publish_pending) is the
\* one-code `pending` holder. It is cleared when a reload begins, and the
\* publish and its no-reload check share one exclusive update, so it cannot
\* take the room codes need.
\*
\* Abstractions: one index, one unit per code. A miss filled at the very moment
\* a reload swaps the codes out is charged to the old copy and held until that
\* copy is reclaimed; that is at most one code per concurrent search, and it is
\* left out here.

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
    \* The searches that run, each one at a time.
    Searches,
    \* The budget, in codes.
    Limit,
    \* How many codes the index has.
    Codes,
    \* The newest generation a merge can make.
    MaxGen,
    \* Guard: checking for room and taking it are one step.
    AtomicTake,
    \* Guard: a reload that stops gives its room back.
    ReleaseOnDrop,
    \* Guard: a reload drops the old codes before it takes new ones.
    DropOldFirst,
    \* Guard: no missed code is kept while a reload runs.
    NoMissesWhileLoading,
    \* Guard: only one search reloads a generation.
    SingleFlight,
    \* Guard: a reload publishes only while it still owns its generation.
    OwnedFinish,
    \* Guard: a reload that was taken over stops taking codes.
    OwnedStep,
    \* Guard: a search that does not own the claim cannot clear it on abandon.
    OwnedAbandon,
    \* Guard: a pending-list copy is not published while a reload runs.
    NoPendingWhileLoading

VARIABLES
    \* The bytes (codes) the budget counter says are held.
    used,
    \* The bytes the one pending-list copy holds.
    pending,
    \* The newest generation merged.
    gen,
    \* The codes in memory: the generation they belong to, the generation a
    \* reload is loading (0 for none) and the search running it (NoOne for
    \* none), how many loaded codes and missed codes.
    cache,
    \* What each search is doing.
    load,
    \* The last reload that published: its generation, how many codes it
    \* published, and whether an older reload took room while it ran.
    published,
    \* A stale search cleared another reload's claim when it was abandoned.
    staleAbandon

\* Every variable, for stuttering steps.
vars == <<used, pending, gen, cache, load, published, staleAbandon>>

\* Stands for no search: the reload claim nobody holds.
NoOne == "none"

\* A search doing nothing: no generation, nothing held, nothing scanned, no
\* refusal excused, no claim lost (`lost` is the generation of whatever took
\* its claim, 0 while it has lost none), and no code taken after that loss.
Idle == [phase |-> "idle", g |-> 0, have |-> 0, scanned |-> 0,
         full |-> FALSE, excused |-> FALSE, lost |-> 0, late |-> FALSE]

\* The smaller of two numbers.
Min(a, b) == IF a < b THEN a ELSE b

\* Adds up what a set of searches holds, one search at a time.
RECURSIVE HeldBy(_)
HeldBy(S) == IF S = {} THEN 0
             \* Take any search out, add what it holds to what the rest hold.
             ELSE LET s == CHOOSE x \in S : TRUE
                  IN load[s].have + HeldBy(S \ {s})

\* The search's reload is running: scanning codes, or between a check and a take.
Loading(s) == load[s].phase \in {"loading", "adding"}

\* Search s still holds the claim on its reload. Each reload has a claim of
\* its own (Rust draws a ticket per reload): a generation alone would let a
\* reload that was taken over mistake a later reload of its generation for
\* its own.
Owns(s) == cache.loading = load[s].g /\ cache.by = s

\* A reload that was taken over holds room right now, which excuses a
\* refusal: it gives the room back at its next step.
StaleHolds(s) == \E t \in Searches :
                    \* t is another search, reloading,
                    /\ t # s
                    /\ Loading(t)
                    \* that no longer holds its claim,
                    /\ ~Owns(t)
                    \* and still holds some room.
                    /\ load[t].have > 0

\* The searches after the claim's holder loses it to a reload of generation g:
\* the holder remembers what took it.
LoseClaim(l, g) == IF cache.by \in Searches /\ Loading(cache.by)
                   THEN [l EXCEPT ![cache.by].lost = g]
                   ELSE l

\* Nothing held, generation 1 merged, no codes in memory, every search idle.
Init ==
    \* The budget counter starts empty.
    /\ used = 0
    \* No pending-list copy is held.
    /\ pending = 0
    \* The first graph exists.
    /\ gen = 1
    \* No codes are loaded yet: generation 0 is older than any search's.
    /\ cache = [generation |-> 0, loading |-> 0, by |-> NoOne, codes |-> 0,
                 misses |-> 0]
    \* Every search starts idle.
    /\ load = [s \in Searches |-> Idle]
    \* No reload has published; a full one stands in so the invariant holds.
    /\ published = [g |-> 0, have |-> Min(Codes, Limit), excused |-> FALSE]
    \* No stale search has cleared another reload's claim.
    /\ staleAbandon = FALSE

\* A merge makes a newer generation, so the codes in memory are out of date.
Merge ==
    \* There is a newer generation left to make;
    /\ gen < MaxGen
    \* the graph moves on to it.
    /\ gen' = gen + 1
    \* Nothing else changes.
    /\ UNCHANGED <<used, pending, cache, load, published, staleAbandon>>

\* Search s, reading generation g, finds the codes out of date and reloads.
Begin(s, g) ==
    \* The search is idle,
    /\ load[s].phase = "idle"
    \* the codes in memory are older than its generation,
    /\ cache.generation < g
    \* and, with single flight, no reload of g or newer is already running.
    /\ SingleFlight => cache.loading < g
    \* With the old codes dropped first, their room goes back now and the
    \* cache holds nothing while the reload runs; without, they stay.
    /\ IF DropOldFirst
       THEN /\ used' = used - cache.codes - cache.misses - pending
            /\ pending' = 0
            /\ cache' = [generation |-> cache.generation, loading |-> g,
                         by |-> s, codes |-> 0, misses |-> 0]
       ELSE /\ used' = used
            /\ pending' = pending
            /\ cache' = [cache EXCEPT !.loading = g, !.by = s]
    \* The search starts loading g with nothing held and nothing scanned.
    \* and whoever held the claim loses it to this reload.
    /\ load' = [LoseClaim(load, g) EXCEPT
                   ![s] = [Idle EXCEPT !.phase = "loading", !.g = g]]
    \* The generation and the last publish are unchanged.
    /\ UNCHANGED <<gen, published, staleAbandon>>

\* The reload gives up its room (when it releases on drop) and becomes idle.
StopReload(s) ==
    \* The room it held goes back, unless the release is missing.
    /\ used' = IF ReleaseOnDrop THEN used - load[s].have ELSE used
    \* The search is idle again.
    /\ load' = [load EXCEPT ![s] = Idle]

\* Search s takes its next code, if the budget has room.
Step(s) ==
    \* The search is scanning codes and has some left.
    /\ load[s].phase = "loading"
    /\ load[s].scanned < Codes
    /\ IF load[s].full
       \* Once refused, every later code is skipped.
       THEN /\ load' = [load EXCEPT ![s].scanned = @ + 1]
            /\ UNCHANGED <<used, cache>>
       ELSE IF ~Owns(s)
       \* Another reload took the generation over. With the guard this one
       \* lets go now; without it, it keeps taking codes (the stale-step RED).
       THEN IF OwnedStep
            THEN /\ StopReload(s)
                 /\ UNCHANGED cache
            ELSE IF used + 1 <= Limit
                 THEN /\ used' = used + 1
                      /\ load' = [load EXCEPT ![s].have = @ + 1,
                                           ![s].scanned = @ + 1,
                                           ![s].late = TRUE]
                      /\ UNCHANGED cache
                 ELSE /\ load' = [load EXCEPT ![s].full = TRUE,
                                           ![s].scanned = @ + 1,
                                           ![s].late = TRUE]
                      /\ UNCHANGED <<used, cache>>
       ELSE IF used + 1 <= Limit
       \* There is room: take it (in one step), or remember the check and
       \* take it in a later step (the split RED).
       THEN IF AtomicTake
            THEN /\ used' = used + 1
                 /\ load' = [load EXCEPT ![s].have = @ + 1, ![s].scanned = @ + 1]
                 /\ UNCHANGED cache
            ELSE /\ load' = [load EXCEPT ![s].phase = "adding"]
                 /\ UNCHANGED <<used, cache>>
       \* No room: this code and every later one are left out, and the
       \* refusal is excused only if an older reload holds room.
       ELSE /\ load' = [load EXCEPT ![s].full = TRUE, ![s].scanned = @ + 1,
                                    ![s].excused = @ \/ StaleHolds(s)]
            /\ UNCHANGED <<used, cache>>
    \* The generation and the last publish are unchanged.
    /\ UNCHANGED <<gen, pending, published, staleAbandon>>

\* The split RED's second half: take the room the earlier check saw.
Add(s) ==
    \* The search checked for room in an earlier step;
    /\ load[s].phase = "adding"
    \* now it takes it, whatever happened meanwhile.
    /\ used' = used + 1
    \* It holds one more code and moves on to the next.
    /\ load' = [load EXCEPT ![s].phase = "loading", ![s].have = @ + 1,
                            ![s].scanned = @ + 1]
    \* Nothing else changes.
    /\ UNCHANGED <<gen, pending, cache, published, staleAbandon>>

\* Search s has scanned every code and publishes what it loaded.
Finish(s) ==
    \* The search has scanned every code.
    /\ load[s].phase = "loading"
    /\ load[s].scanned = Codes
    \* Whether it still owns its generation.
    /\ LET owned == Owns(s)
           \* It publishes if its codes are newer, and, with the ownership
           \* guard, only while it still owns the generation.
           install == /\ cache.generation < load[s].g
                      /\ OwnedFinish => owned
       IN IF install
          \* The cache now holds its codes; what the cache held goes back.
          THEN /\ used' = used - cache.codes - cache.misses
               /\ cache' = [generation |-> load[s].g, loading |-> 0,
                            by |-> NoOne, codes |-> load[s].have, misses |-> 0]
               \* Remember what this publish held, for INV_FullLoad.
               /\ published' = [g |-> load[s].g, have |-> load[s].have,
                                excused |-> load[s].excused]
               \* The search is idle; a reload whose claim this publish
               \* wiped loses it to this generation.
               /\ load' = [LoseClaim(load, load[s].g) EXCEPT ![s] = Idle]
          \* Not published: its codes are dropped like a stopped reload's.
          ELSE /\ StopReload(s)
               /\ UNCHANGED <<cache, published>>
    \* The generation is unchanged.
    /\ UNCHANGED <<pending, gen, staleAbandon>>

\* Search s's reload stops half way: an error, or the search was cancelled.
Abandon(s) ==
    \* The search is reloading.
    /\ Loading(s)
    \* It gives up its room (if released on drop) and is idle.
    /\ StopReload(s)
    \* If it still owns its generation, it hands it back, so the next search
    \* reloads it.
    /\ cache' = IF Owns(s) \/ ~OwnedAbandon
                THEN [cache EXCEPT !.loading = 0, !.by = NoOne]
                ELSE cache
    \* Without the guard, abandoning a stale search can clear a newer claim.
    /\ staleAbandon' = (staleAbandon \/
         (~OwnedAbandon /\ ~Owns(s) /\
          cache.by \in Searches /\ Loading(cache.by)))
    \* The generation and the last publish are unchanged.
    /\ UNCHANGED <<pending, gen, published>>

\* A search publishes one pending-list copy, if the budget has room and no
\* reload is running. It is a holder just like a missed code.
Publish(s) ==
    \* The search reads through the cache between reloads,
    /\ load[s].phase = "idle"
    \* it has no pending copy yet,
    /\ pending = 0
    \* with the guard, no reload is running,
    /\ NoPendingWhileLoading => cache.loading = 0
    \* and the budget has room, taken in the same step.
    /\ used + 1 <= Limit
    /\ used' = used + 1
    \* The cache keeps the pending copy.
    /\ pending' = 1
    \* Nothing else changes.
    /\ UNCHANGED <<gen, cache, load, published, staleAbandon>>

\* An idle search keeps a code it read on a miss, if the budget has room.
Fill(s) ==
    \* The search is idle (it reads through the cache);
    /\ load[s].phase = "idle"
    \* some code is neither loaded nor kept as a miss;
    /\ cache.codes + cache.misses < Codes
    \* with the guard, no reload is running;
    /\ NoMissesWhileLoading => cache.loading = 0
    \* and the budget has room, taken in the same step.
    /\ used + 1 <= Limit
    /\ used' = used + 1
    \* The cache keeps one more missed code.
    /\ cache' = [cache EXCEPT !.misses = @ + 1]
    \* Nothing else changes.
    /\ UNCHANGED <<pending, gen, load, published, staleAbandon>>

\* One thing happens: a merge, or a search acts.
Next ==
    \/ Merge
    \/ \E s \in Searches :
        \* A search may reload any generation merged so far.
        \/ \E g \in 1..gen : Begin(s, g)
        \/ Step(s)
        \/ Add(s)
        \/ Finish(s)
        \/ Abandon(s)
        \/ Fill(s)
        \/ Publish(s)

\* Start in Init and take steps of Next forever.
Spec == Init /\ [][Next]_vars

\* Promise: the caches never hold more than the budget.
INV_BudgetHolds == used <= Limit

\* Promise: the counter is exactly what the codes, the misses and every
\* running reload hold.
INV_Accounted == used = pending + cache.codes + cache.misses + HeldBy(Searches)

\* Promise: a published reload holds every code that fits in the budget,
\* unless a reload that was taken over still held room when it was refused.
INV_FullLoad == published.excused \/ published.have >= Min(Codes, Limit)

\* Promise: a running reload that lost its claim lost it to a reload of a
\* newer generation, never to one of its own or an older one.
INV_ClaimLostOnlyToNewer == \A s \in Searches :
    \* Either it lost no claim, or what took it is newer than it.
    Loading(s) => (load[s].lost = 0 \/ load[s].lost > load[s].g)

\* Promise: a reload that lost its claim takes no later code.
INV_NoTakeAfterLoss == \A s \in Searches :
    \* A running reload has never taken a code after losing its claim.
    Loading(s) => ~load[s].late

\* Promise: abandoning a search that no longer owns the claim leaves it alone.
INV_NoClaimClearedByStaleAbandon == staleAbandon = FALSE

\* Searches are interchangeable.
Symmetry == Permutations(Searches)

====
