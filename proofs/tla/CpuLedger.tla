---- MODULE CpuLedger ----
\* The story, in plain words.
\*
\* A process can run several worker pools. A placed pool pins its threads to
\* CPUs, and two placed pools must not put threads on one CPU: they would
\* fight for it. The process keeps one ledger, a counter per CPU that says how
\* many live pools hold that CPU. A starting pool plans on the CPUs whose
\* counter reads zero, then claims them in one of two ways:
\*   * alone (Ledger::hold): it walks its CPUs from the lowest id up and moves
\*     each counter from 0 to 1 with one compare-and-swap. If it finds a CPU
\*     already held, it gives back every CPU it took so far and fails, holding
\*     nothing. It then plans again on what is free now. After 16 failed
\*     claims, or when it no longer fits on the free CPUs, it shares instead;
\*   * shared (Ledger::share): it adds one to each counter, whatever the
\*     counter was. It never fails, its report says it is oversubscribed, and
\*     its CPUs read held, so no later pool takes them alone.
\* The claim is a Hold. The pool keeps it in its placer until the pool, its
\* blocking threads and every env placing threads through it are gone; then
\* the Hold is dropped and each counter it raised goes down by one.
\*
\* A tiny example, with CPUs 0 and 1. Pool A holds CPU 1 alone (counter 1 is
\* 1). Pool B asks for both: it moves CPU 0 from 0 to 1, finds CPU 1 at 1,
\* puts CPU 0 back to 0 and fails. B plans again on CPU 0 alone and wins it.
\* Pool C finds nothing free and shares CPU 0 (counter 0 is 2). When B goes,
\* counter 0 is 1: CPU 0 still reads held, because C runs there.
\*
\* What must never happen:
\*   1. two pools that claimed alone have one CPU taken at the same time
\*      - INV_AloneTakesDisjoint;
\*   2. two pools that claimed alone run threads on one CPU
\*      - INV_AloneRunsAlone;
\*   3. a claim alone wins a CPU that another pool holds or runs on, a sharing
\*      pool included - INV_TakenAlone;
\*   4. a failed claim leaves a CPU taken, or a won claim holds less than it
\*      asked for - INV_AllOrNothing;
\*   5. a CPU some pool holds or runs on reads free - INV_HeldNeverFree;
\*   6. a counter differs from the number of pools whose claim lists that CPU
\*      (a leak, or a count stolen from another pool) - INV_CountExact;
\*   7. every pool is gone and some counter is not back at zero
\*      - INV_DoneLeavesNothing;
\*   8. a pool's start never ends - LIVE_StartEnds.
\*
\* What a caller of the ledger must uphold (the Hold's contract), each with
\* its own RED configuration:
\*   * keep the Hold while any thread placed through it runs (KeepWhileRunning);
\*   * give it back exactly once (ReleaseOnce);
\*   * run threads only on CPUs the Hold covers (RunOnHeld);
\*   * give it back at all once the pool is gone (ReleaseAtEnd).
\*
\* TLC runs whole steps one at a time, as if memory were sequentially
\* consistent. Every ledger step here is one read-modify-write on one CPU's
\* counter (compare_exchange, fetch_add, fetch_sub), and read-modify-writes on
\* one atomic are totally ordered, each reading the value the one before it
\* wrote, whatever the memory ordering. Every promise is about one CPU's
\* counter at a time, or about one pool's own steps in program order. A pool's
\* threads start after its claim returned (the threads are spawned after
\* reserve) and end before its Hold is dropped (the Hold lives in the placer,
\* behind the Arc every thread keeps). So the interleavings here cover the
\* Rust; the AcqRel and Acquire orderings order the pool's other memory
\* around a take and a give-back, and no promise here needs more.
\*
\* Correspondence with the Rust code (crates/workers/src/ unless named):
\*
\*   | Model             | Rust                                                        |
\*   |-------------------|-------------------------------------------------------------|
\*   | count             | ledger.rs:21 PROCESS, one AtomicU32 per CPU id, reached     |
\*   |                   | through Ledger::holds (ledger.rs:25-27)                     |
\*   | want              | the `cpus` argument of hold and share: reserve.rs:63        |
\*   |                   | cores_of(&fitted, &rest), reserve.rs:137                    |
\*   |                   | cores_of(&planned, machine)                                 |
\*   | todo              | the part of `cpus.iter()` the loop has not reached yet      |
\*   |                   | (ledger.rs:65, ledger.rs:88); CpuSet::iter goes up from the |
\*   |                   | lowest id, which is Min here                                |
\*   | taken             | the local `taken` of hold and share (ledger.rs:64, 87),     |
\*   |                   | which becomes Hold::cpus (ledger.rs:32)                     |
\*   | runs              | the CPUs the pool's threads are pinned to: the plan's seats |
\*   |                   | (pool/placer.rs:93-101 place_worker) and background CPUs    |
\*   |                   | (pool/placer.rs:106-127 place), all inside cores_of         |
\*   |                   | (reserve.rs:154-165)                                        |
\*   | mode              | which ledger call reserve made: Ledger::hold "alone"        |
\*   |                   | (reserve.rs:64) or Ledger::share "share" (reserve.rs:80,137)|
\*   | tries             | reserve.rs:47 and :70, `attempt`                            |
\*   | life              | the pools a process starts one after another: each          |
\*   |                   | Pool::start (pool/start.rs:51-53) is a new life of a slot   |
\*   | crowded           | history only, to state INV_TakenAlone; nothing in the Rust  |
\*   | Plan              | reserve.rs:46-63: free, fit, cores_of; a plan that does not |
\*   |                   | fit goes to shared() (reserve.rs:55-61, 112-142)            |
\*   | Take (win), Win   | ledger.rs:66-69 compare_exchange(0, 1, AcqRel, Acquire)     |
\*   |                   | succeeds, then taken.insert (ledger.rs:77); after the last  |
\*   |                   | CPU, Some(Hold) (ledger.rs:79-82), Reserved (reserve.rs:    |
\*   |                   | 64-68) and the threads start (pool/start.rs:64-135)         |
\*   | Take (lose), Lose | the same compare_exchange fails: `!won` (ledger.rs:70)      |
\*   | Store             | RED only: the take after a separate look (no Rust)          |
\*   | Rollback          | ledger.rs:71-74 drop(Hold { cpus: taken }) -> Hold::drop    |
\*   |                   | (ledger.rs:109-117), fetch_sub per CPU (ledger.rs:113),     |
\*   |                   | then `return None` (ledger.rs:75)                           |
\*   | Retry             | reserve.rs:70-82, back to the loop at reserve.rs:49         |
\*   | Share             | ledger.rs:86-98: fetch_add(1, AcqRel) (ledger.rs:90), then  |
\*   |                   | taken.insert (ledger.rs:91)                                 |
\*   | Stop              | the pool, its blocking threads and every env placing        |
\*   |                   | threads through it are gone: the last Arc<Placer> drops     |
\*   |                   | (pool/placer.rs:51-54, pool/start.rs:72-93)                 |
\*   | Release           | Placer::_hold dropped -> Hold::drop (ledger.rs:109-117)     |
\*   | Restart           | a later Pool::start in the same process                     |
\*   | AtomicTake        | the one compare_exchange (ledger.rs:67) (RED: a load, then  |
\*   |                   | a separate add)                                             |
\*   | RollBack          | the drop of the partial Hold before `return None`           |
\*   |                   | (ledger.rs:71-75) (RED: return None without it)             |
\*   | InsertAfterWin    | taken.insert (ledger.rs:77) after the `!won` check          |
\*   |                   | (ledger.rs:70) (RED: insert before the check)               |
\*   | ShareCounts       | share's fetch_add (ledger.rs:90) and shared() returning a   |
\*   |                   | Hold (reserve.rs:137-141) (RED: the share left uncounted)   |
\*   | AttemptCap        | ATTEMPTS (reserve.rs:28) and its check (reserve.rs:71)      |
\*   |                   | (RED: claims again forever)                                 |
\*   | KeepWhileRunning  | Placer::_hold (pool/placer.rs:54), Placer::holding          |
\*   |                   | (pool/placer.rs:85-90), `.holding(hold)` (pool/start.rs:79) |
\*   |                   | (RED: the Hold dropped while threads still run)             |
\*   | ReleaseOnce       | Hold is not Clone, and Drop runs once (ledger.rs:109)       |
\*   |                   | (RED: the Hold given back twice)                            |
\*   | RunOnHeld         | cores_of covers every CPU the seats and background use      |
\*   |                   | (reserve.rs:154-165) (RED: a thread on a CPU outside it)    |
\*   | ReleaseAtEnd      | the Hold dropped with the placer (ledger.rs:109)            |
\*   |                   | (RED: never given back, a forgotten or leaked placer)       |
\*
\* Gaps, explained:
\*   * Ledger::free (ledger.rs:50-59) is the planner's look at the counters.
\*     The model lets a pool plan on any non-empty set of CPUs, a superset of
\*     what free allows, so free's answer never decides a promise here: the
\*     compare_exchange does.
\*   * `holds.get(cpu)` returning None (ledger.rs:55, 66, 89, 112) cannot
\*     happen for the process ledger: CpuSet::insert drops ids at or above
\*     MAX_CPUS (topology/cpuset.rs:55-58), the length of PROCESS. The Lean
\*     law CpuLedger.hold_outside_fails covers that branch anyway.
\*   * A pool that holds nothing (Placement::Unpinned, reserve.rs:40-45; Auto
\*     with no CPU free, reserve.rs:119-130; a driven pool, pool/driven.rs:
\*     63-77) is not placed and never touches the ledger: here, a slot that
\*     stays idle.
\*   * A pin the OS refuses falls back to the process's allowed CPUs
\*     (pool/placer.rs:43-44, 129-131) and the start-up report says so; the
\*     model takes every pin as granted.
\*   * KeepWhileRunning holds for the threads a pool starts: each worker keeps
\*     the pool's shared state, which owns the placer (pool/start.rs:93,
\*     111-122), each blocking thread keeps the placer itself
\*     (pool/blocking.rs:55-59), and so does each thread an env starts
\*     (pool/env.rs:49-53). Pool::place_current_thread (pool/mod.rs:473)
\*     pins a thread the pool did not start and keeps no Arc for it: if that
\*     thread outlives the pool, its CPUs go back to the ledger while it still
\*     runs there, which is the ReleaseEarly RED. No caller outside the
\*     workers tests uses it today (pool/tests.rs:672).

EXTENDS Integers, FiniteSets
\* Integers gives numbers that can go below zero (so a RED run can show it);
\* FiniteSets gives Cardinality, the size of a set.

CONSTANTS NCpu, NPool, Lives, Attempts,
          AtomicTake, RollBack, InsertAfterWin, ShareCounts, AttemptCap,
          KeepWhileRunning, ReleaseOnce, RunOnHeld, ReleaseAtEnd
\* NCpu is how many CPUs the machine has; NPool how many pools can be alive at
\* once; Lives how many pools each of those slots runs, one after another;
\* Attempts is reserve's ATTEMPTS (16 in the Rust), how many claims alone a
\* pool makes before it shares. The nine switches are all TRUE in the real
\* design; each RED configuration turns one off.

ASSUME /\ NCpu \in 1..3
       \* one to three CPUs;
       /\ NPool \in 1..3
       \* one to three pools at once;
       /\ Lives \in 1..3
       \* one to three pools per slot over the run;
       /\ Attempts \in 1..3
       \* one to three claims alone before sharing;
       /\ AtomicTake \in BOOLEAN
       \* and every switch is either on or off.
       /\ RollBack \in BOOLEAN
       \* (the rollback of a failed claim)
       /\ InsertAfterWin \in BOOLEAN
       \* (the record of a CPU only once it was won)
       /\ ShareCounts \in BOOLEAN
       \* (a share counted in the ledger)
       /\ AttemptCap \in BOOLEAN
       \* (the cap on claims alone)
       /\ KeepWhileRunning \in BOOLEAN
       \* (the Hold kept while threads run)
       /\ ReleaseOnce \in BOOLEAN
       \* (the Hold given back once)
       /\ RunOnHeld \in BOOLEAN
       \* (threads only on held CPUs)
       /\ ReleaseAtEnd \in BOOLEAN
       \* (the Hold given back at all)

CPUs == 0..(NCpu - 1)
\* The CPU ids, from 0 up.

Pools == 1..NPool
\* The pool slots, named 1, 2, ...

Claims == (SUBSET CPUs) \ {{}}
\* Every set of CPUs a pool can plan on: any non-empty set.

Modes == {"alone", "share"}
\* The two ways to claim: Ledger::hold (alone) or Ledger::share.

PcStates == {"idle", "taking", "looked", "rollback", "failed",
             "sharing", "held", "releasing", "done"}
\* Where a pool can be:
\*   idle      - it has not started yet;
\*   taking    - inside Ledger::hold, at the next CPU of its claim;
\*   looked    - (RED only) it saw that CPU free and takes it next;
\*   rollback  - its claim lost a CPU; it gives back what it took;
\*   failed    - Ledger::hold returned None; reserve decides what is next;
\*   sharing   - inside Ledger::share, at the next CPU;
\*   held      - reserve returned and the pool's threads run;
\*   releasing - the pool is gone and its Hold gives its CPUs back;
\*   done      - the pool and its Hold are gone.

Starting == {"taking", "looked", "rollback", "failed", "sharing"}
\* The places a pool passes through while it starts (inside reserve).

GiveBack == IF ReleaseOnce THEN 1 ELSE 2
\* How much one give-back of one CPU lowers its counter: one; the RED switch
\* gives it back twice in one step (the window only shrinks, the bug stays).

Bound == NPool * Lives * (Attempts + 1)
\* The furthest any counter can move from zero, even in a RED run.

Min(S) == CHOOSE x \in S : \A y \in S : x <= y
\* The lowest CPU of a non-empty set: the next one CpuSet::iter yields.

VARIABLES count, pc, mode, want, todo, taken, runs, tries, crowded, life
\* count   - each CPU's counter (PROCESS in ledger.rs);
\* pc      - where each pool is (PcStates);
\* mode    - how each pool claims: "alone", "share", or "none" before it plans;
\* want    - the CPUs of each pool's current claim (`cpus` in reserve);
\* todo    - the CPUs of that claim the loop has not reached yet;
\* taken   - the CPUs each pool's claim has raised and not given back: the
\*           local `taken` of hold and share, then Hold::cpus;
\* runs    - the CPUs each pool's threads are pinned to;
\* tries   - how many claims alone each pool has lost (`attempt`);
\* crowded - whether a claim alone ever won a CPU another pool held or ran on;
\* life    - which pool, of the ones a slot runs in turn, the slot is on.

vars == <<count, pc, mode, want, todo, taken, runs, tries, crowded, life>>
\* Every variable, for "nothing else changes" and for fairness.

Init == /\ count = [c \in CPUs |-> 0]
        \* every counter starts at zero (PROCESS is zero pages);
        /\ pc = [p \in Pools |-> "idle"]
        \* no pool has started;
        /\ mode = [p \in Pools |-> "none"]
        \* no pool has chosen how to claim;
        /\ want = [p \in Pools |-> {}]
        \* no pool has planned;
        /\ todo = [p \in Pools |-> {}]
        \* no loop is running;
        /\ taken = [p \in Pools |-> {}]
        \* no pool has raised a counter;
        /\ runs = [p \in Pools |-> {}]
        \* no thread runs anywhere;
        /\ tries = [p \in Pools |-> 0]
        \* no claim has failed;
        /\ crowded = FALSE
        \* no claim has won a CPU someone else had;
        /\ life = [p \in Pools |-> 1]
        \* and each slot is on its first pool.

Others(p, c) == \E q \in Pools \ {p} : c \in taken[q] \cup runs[q]
\* Some pool other than p holds CPU c in the ledger or runs a thread on it.

\* ----------------------------------------------------------- helpers

Begin(p, w, m) == /\ want' = [want EXCEPT ![p] = w]
                  \* the pool's claim is now the CPUs w;
                  /\ todo' = [todo EXCEPT ![p] = w]
                  \* the loop will go through all of them;
                  /\ mode' = [mode EXCEPT ![p] = m]
                  \* it claims alone or shares, as m says;
                  /\ pc' = [pc EXCEPT ![p] = IF m = "alone" THEN "taking" ELSE "sharing"]
                  \* and it enters Ledger::hold or Ledger::share.
\* A claim begins: reserve calls hold or share with the CPUs it planned.

RunOn(p) == IF RunOnHeld
              \* the real design: the threads run on the claimed CPUs only;
              THEN runs' = [runs EXCEPT ![p] = want[p]]
              \* every seat and background CPU is inside the claim;
              ELSE \E extra \in CPUs : runs' = [runs EXCEPT ![p] = want[p] \cup {extra}]
              \* RED: one thread may also run on a CPU outside it.
\* The pool's threads start and pin themselves (reserve has returned).

\* ----------------------------------------------------------- a claim alone

Win(p, c) == /\ count' = [count EXCEPT ![c] = @ + 1]
             \* the counter of c goes up by one (0 to 1 for the real swap);
             /\ taken' = [taken EXCEPT ![p] = @ \cup {c}]
             \* the claim records c as taken;
             /\ todo' = [todo EXCEPT ![p] = @ \ {c}]
             \* and moves past it;
             /\ crowded' = (crowded \/ Others(p, c))
             \* note it if another pool held or ran on c at this moment;
             /\ IF todo[p] = {c}
                   \* c was the last CPU of the claim:
                   THEN /\ pc' = [pc EXCEPT ![p] = "held"]
                        \* hold returns Some, reserve returns, the pool runs,
                        /\ RunOn(p)
                        \* and its threads start;
                   ELSE /\ pc' = [pc EXCEPT ![p] = "taking"]
                        \* otherwise the loop goes on to the next CPU,
                        /\ UNCHANGED runs
                        \* and no thread runs yet;
             /\ UNCHANGED <<mode, want, tries, life>>
             \* nothing else changes.
\* The claim alone takes CPU c.

Lose(p, c) == /\ pc' = [pc EXCEPT ![p] = IF RollBack THEN "rollback" ELSE "failed"]
              \* it gives back what it took (RED: returns None at once);
              /\ taken' = IF InsertAfterWin
                             \* the real design: c was never won, so it is not recorded;
                             THEN taken
                             \* the claim's record stays as it was;
                             ELSE [taken EXCEPT ![p] = @ \cup {c}]
                             \* RED: c is recorded before the check, so it is given back too;
              /\ UNCHANGED <<count, mode, want, todo, runs, tries, crowded, life>>
              \* a lost swap writes nothing.
\* The claim alone finds CPU c held: its compare-and-swap fails.

Take(p) == /\ pc[p] = "taking"
           \* the pool is inside Ledger::hold,
           /\ todo[p] # {}
           \* with a CPU still to go;
           /\ LET c == Min(todo[p]) IN
                \* the next CPU is the lowest one left;
                IF count[c] # 0
                   \* someone holds it:
                   THEN Lose(p, c)
                   \* the claim loses;
                   ELSE IF AtomicTake
                           \* nobody holds it, and the look and the take are one swap:
                           THEN Win(p, c)
                           \* the claim wins it in the same step;
                           ELSE /\ pc' = [pc EXCEPT ![p] = "looked"]
                                \* RED: it only looked; the take is a later step,
                                /\ UNCHANGED <<count, mode, want, todo, taken, runs, tries, crowded, life>>
                                \* and nothing else changes yet.
\* One turn of Ledger::hold's loop: compare_exchange(0, 1) on the next CPU.

Store(p) == /\ pc[p] = "looked"
            \* RED only: the pool saw its next CPU free a step ago,
            /\ Win(p, Min(todo[p]))
            \* and now adds one to it, whatever happened in between.
\* The second half of a look-then-add take, which the real swap does not have.

Rollback(p) == /\ pc[p] = "rollback"
               \* the claim lost a CPU and drops its partial Hold;
               /\ IF taken[p] = {}
                     \* everything is given back:
                     THEN /\ pc' = [pc EXCEPT ![p] = "failed"]
                          \* hold returns None;
                          /\ UNCHANGED <<count, taken>>
                          \* and no counter moves;
                     ELSE /\ count' = [count EXCEPT ![Min(taken[p])] = @ - 1]
                          \* otherwise the lowest CPU taken goes down by one,
                          /\ taken' = [taken EXCEPT ![p] = @ \ {Min(taken[p])}]
                          \* leaves the record,
                          /\ UNCHANGED pc
                          \* and the rollback goes on;
               /\ UNCHANGED <<mode, want, todo, runs, tries, crowded, life>>
               \* nothing else changes.
\* One give-back of the failed claim: Hold::drop's fetch_sub on one CPU.

Retry(p) == /\ pc[p] = "failed"
            \* hold returned None to reserve;
            /\ IF AttemptCap /\ tries[p] + 1 = Attempts
                  \* that was the last claim alone reserve allows:
                  THEN /\ Begin(p, want[p], "share")
                       \* it shares the same CPUs, marked oversubscribed,
                       /\ UNCHANGED tries
                       \* and counts no more;
                  ELSE /\ \E w \in Claims, m \in Modes : Begin(p, w, m)
                       \* otherwise it plans again: alone on what looks free,
                       \* or shared when it no longer fits;
                       /\ tries' = IF AttemptCap THEN [tries EXCEPT ![p] = @ + 1] ELSE tries
                       \* and counts the lost claim (RED: never counts);
            /\ taken' = [taken EXCEPT ![p] = {}]
            \* a new claim starts with nothing recorded;
            /\ UNCHANGED <<count, runs, crowded, life>>
            \* nothing else changes.
\* reserve after a failed claim: attempt += 1, then share or plan again.

\* ----------------------------------------------------------- a shared claim

Share(p) == /\ pc[p] = "sharing"
            \* the pool is inside Ledger::share,
            /\ todo[p] # {}
            \* with a CPU still to go;
            /\ LET c == Min(todo[p]) IN
                 \* the next CPU is the lowest one left;
                 /\ count' = IF ShareCounts THEN [count EXCEPT ![c] = @ + 1] ELSE count
                 \* its counter goes up by one whatever it was (RED: untouched);
                 /\ taken' = IF ShareCounts THEN [taken EXCEPT ![p] = @ \cup {c}] ELSE taken
                 \* and the Hold records it (RED: the pool holds nothing);
                 /\ todo' = [todo EXCEPT ![p] = @ \ {c}]
                 \* the loop moves past it;
                 /\ IF todo[p] = {c}
                       \* that was the last CPU:
                       THEN /\ pc' = [pc EXCEPT ![p] = "held"]
                            \* share returns, reserve returns, the pool runs,
                            /\ RunOn(p)
                            \* and its threads start;
                       ELSE UNCHANGED <<pc, runs>>
                       \* otherwise the loop goes on, and no thread runs yet;
            /\ UNCHANGED <<mode, want, tries, crowded, life>>
            \* nothing else changes.
\* One turn of Ledger::share's loop: fetch_add(1) on the next CPU. It never fails.

\* ----------------------------------------------------------- the end of a pool

Stop(p) == /\ pc[p] = "held"
           \* a running pool,
           /\ pc' = [pc EXCEPT ![p] = IF ReleaseAtEnd THEN "releasing" ELSE "done"]
           \* is shut down and its placer dropped (RED: the Hold is never given back);
           /\ runs' = IF KeepWhileRunning THEN [runs EXCEPT ![p] = {}] ELSE runs
           \* its threads are gone first (RED: they still run while the Hold goes);
           /\ UNCHANGED <<count, mode, want, todo, taken, tries, crowded, life>>
           \* nothing else changes.
\* The pool, its blocking threads and every env placing threads through it are gone.

Release(p) == /\ pc[p] = "releasing"
              \* the Hold is being dropped;
              /\ IF taken[p] = {}
                    \* everything is given back:
                    THEN /\ pc' = [pc EXCEPT ![p] = "done"]
                         \* the pool and its Hold are gone,
                         /\ runs' = [runs EXCEPT ![p] = {}]
                         \* no thread of it is left,
                         /\ UNCHANGED <<count, taken>>
                         \* and no counter moves;
                    ELSE /\ count' = [count EXCEPT ![Min(taken[p])] = @ - GiveBack]
                         \* otherwise the lowest CPU held goes down by one (RED: two),
                         /\ taken' = [taken EXCEPT ![p] = @ \ {Min(taken[p])}]
                         \* and leaves the record,
                         /\ UNCHANGED <<pc, runs>>
                         \* and the drop goes on;
              /\ UNCHANGED <<mode, want, todo, tries, crowded, life>>
              \* nothing else changes.
\* One give-back of Hold::drop: fetch_sub(1) on one CPU.

Restart(p) == /\ pc[p] = "done"
              \* the slot's pool is gone,
              /\ life[p] < Lives
              \* and the slot has another pool to run;
              /\ pc' = [pc EXCEPT ![p] = "idle"]
              \* a new pool starts in it,
              /\ mode' = [mode EXCEPT ![p] = "none"]
              \* with no way of claiming chosen,
              /\ want' = [want EXCEPT ![p] = {}]
              \* no plan,
              /\ todo' = [todo EXCEPT ![p] = {}]
              \* no loop,
              /\ taken' = [taken EXCEPT ![p] = {}]
              \* nothing recorded (what the old one left in the ledger stays there),
              /\ tries' = [tries EXCEPT ![p] = 0]
              \* no lost claim,
              /\ life' = [life EXCEPT ![p] = @ + 1]
              \* and the slot moves to its next pool;
              /\ UNCHANGED <<count, runs, crowded>>
              \* nothing else changes.
\* A later Pool::start in the same process.

Plan(p) == /\ pc[p] = "idle"
           \* a pool that has not started,
           /\ \E w \in Claims, m \in Modes : Begin(p, w, m)
           \* plans on some CPUs and claims them alone or shared;
           /\ UNCHANGED <<count, taken, runs, tries, crowded, life>>
           \* nothing else changes.
\* reserve begins: free, fit, cores_of, then hold, or shared() when it does not fit.

\* ------------------------------------------------------------------ spec

Step(p) == Take(p) \/ Store(p) \/ Rollback(p) \/ Retry(p) \/ Share(p) \/ Release(p)
\* Everything a pool does on its own once it began a claim or a give-back:
\* these run to the end.

Next == \E p \in Pools : Plan(p) \/ Step(p) \/ Stop(p) \/ Restart(p)
\* Any pool takes any step it can, in any order.

Fairness == \A p \in Pools : WF_vars(Step(p))
\* A pool inside reserve or inside a drop keeps going. Starting a pool,
\* stopping it and starting the next are choices: nothing forces them, so a
\* pool may run for ever.

Spec == Init /\ [][Next]_vars /\ Fairness
\* The model: start at Init, take steps of Next, with the fairness above.

\* ------------------------------------------------------------ properties

TypeOK == /\ count \in [CPUs -> -Bound..Bound]
          \* each counter is a number near zero (below zero only in a RED run);
          /\ pc \in [Pools -> PcStates]
          \* each pool is in one of the named places;
          /\ mode \in [Pools -> {"none", "alone", "share"}]
          \* each pool claims alone, shares, or has not chosen;
          /\ want \in [Pools -> SUBSET CPUs]
          \* each claim is a set of CPUs;
          /\ todo \in [Pools -> SUBSET CPUs]
          \* so is what is left of it;
          /\ taken \in [Pools -> SUBSET CPUs]
          \* so is what it recorded;
          /\ runs \in [Pools -> SUBSET CPUs]
          \* so is where its threads run;
          /\ tries \in [Pools -> 0..Attempts]
          \* the lost claims stay within the cap;
          /\ crowded \in BOOLEAN
          \* the history flag is a yes or a no;
          /\ life \in [Pools -> 1..Lives]
          \* and each slot is on one of its pools.
\* Every variable holds a value of the expected kind.

Holders(c) == {p \in Pools : c \in taken[p]}
\* The pools whose claim lists CPU c.

INV_AloneTakesDisjoint ==
  \A p, q \in Pools : (p # q /\ mode[p] = "alone" /\ mode[q] = "alone")
                         => taken[p] \cap taken[q] = {}
\* Promise: two pools that claimed alone never have one CPU taken at the same
\* time, whether their claims are under way, won, or being given back.

INV_AloneRunsAlone ==
  \A p, q \in Pools : (p # q /\ mode[p] = "alone" /\ mode[q] = "alone")
                         => runs[p] \cap runs[q] = {}
\* Promise: two pools that claimed alone never run threads on one CPU.

INV_TakenAlone == ~crowded
\* Promise: a claim alone wins a CPU only when no other pool holds it or runs
\* on it, sharing pools included.

INV_AllOrNothing ==
  \A p \in Pools : /\ pc[p] = "failed" => taken[p] = {}
                   \* a claim that failed holds nothing;
                   /\ (pc[p] = "held" /\ mode[p] = "alone") => taken[p] = want[p]
                   \* a claim alone that won holds every CPU it asked for.
\* Promise: a claim alone takes all of its CPUs or none of them.

INV_HeldNeverFree ==
  \A p \in Pools : \A c \in taken[p] \cup runs[p] : count[c] >= 1
\* Promise: a CPU that some pool holds or runs on never reads free, so no later
\* pool takes it alone.

INV_CountExact == \A c \in CPUs : count[c] = Cardinality(Holders(c))
\* Promise: each counter is exactly the number of pools whose claim lists that
\* CPU: every raise is given back once, by the pool that made it.

INV_DoneLeavesNothing ==
  (\A p \in Pools : pc[p] = "done") => (\A c \in CPUs : count[c] = 0)
\* Promise: once every pool is gone, every counter is back at zero: no leak.

LIVE_StartEnds == \A p \in Pools : (pc[p] \in Starting) ~> (pc[p] = "held")
\* Promise: a pool that starts always gets to run, alone or shared.

====
