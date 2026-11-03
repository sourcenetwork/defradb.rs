---- MODULE DefaultPool ----
\* The story, in plain words.
\*
\* Work submitted without naming a pool (a spawn or a timer made off every
\* worker, the work of a database that was given no pool) runs on the
\* process's default pool. Which pool is that? A node registers its own
\* data-path pool with make_default, and while that pool runs it is the
\* default. When no registered pool runs, the default is a pool started on
\* first use and kept for the life of the process.
\*
\* Two pieces of shared memory decide the answer:
\*   * the registration slot: one Atom naming at most one registered pool. It
\*     names the pool weakly, so it never keeps a pool alive;
\*   * the started cell: one OnceCell, written at most once, holding the pool
\*     started on first use.
\* default_pool() loads the slot and checks the pool it names: alive and not
\* closing means that pool is the answer. Otherwise it reads the started
\* cell. An empty cell means it waits for the cell's one turn, looks again,
\* starts the pool and publishes it. A worker thread cannot wait for a start,
\* so on a worker an empty cell is an error. make_default(p) loads the slot:
\* if the pool named there still runs, p is the default only if it is that
\* pool; otherwise p swaps itself in with one compare-and-swap against what it
\* loaded, and loads again if the slot moved in between.
\*
\* A tiny example. Node pool p1 registers: the slot names p1. Pool p2
\* registers while p1 runs: it hears "no", and the slot keeps p1. Every call
\* answers p1. Then p1 is stopped. Its stop shuts p1 down first and then asks
\* for a pool to time its own deadline on. The slot still names p1, but p1 is
\* closing, so the call falls through to the started cell: it starts s1 once
\* and answers s1. Had p2 registered after p1 began closing, p2 would have
\* won the slot and been the answer.
\*
\* What must never happen:
\*   1. default_pool() hands out a pool that is closing or gone
\*      - INV_NeverHandsOutClosing;
\*   2. the slot moves away from a registered pool that still runs, so two
\*      running pools take turns being the default - INV_FirstRunningWins;
\*   3. a call answers the started pool although a registered pool ran during
\*      the whole call, or answers a registered pool the slot no longer names
\*      - INV_RegisteredWins;
\*   4. two default pools are started - INV_OneStart;
\*   5. two calls get two different started pools - INV_OneDefault;
\*   6. a call fails although a started pool was there to answer it
\*      - INV_ErrorHonest;
\*   7. Pool::stop times its deadline on the very pool it stops
\*      - INV_StopDeadlineElsewhere;
\*   8. make_default says "yes" when its pool is not the default, or "no" when
\*      no other running pool holds the slot - INV_MakeDefaultTellsTruth;
\*   9. the registration keeps a pool alive after its owner let it go
\*      - INV_RegistrationKeepsNothingAlive;
\*  10. a worker thread waits for a start, stalling every task on its core
\*      - INV_WorkerNeverWaits;
\*  11. a default_pool() call or a make_default call never returns
\*      - LIVE_EveryCallReturns, LIVE_EveryRegisterReturns.
\*
\* What the code promises when the default closes: the next call answers a
\* successor, never the closing pool. The successor is a running pool that
\* registered since, else the started pool (started now if it was not yet),
\* else an error: on a worker thread before the start, or when the start
\* fails (the OS refuses threads; a browser starts none). No call hangs.
\*
\* What a caller must uphold (each has its own RED configuration):
\*   C1 Pool::stop shuts its pool down before it asks for the default to time
\*      its deadline on (StopShutsFirst);
\*   C2 nobody shuts down the pool default_pool() started: it is kept for the
\*      life of the process (StartedKept);
\*   C3 a pool is registered by its owner while it runs, before anyone else
\*      holds it, so nobody shuts it down during the call
\*      (RegisterOwnRunning).
\*
\* TLC runs whole steps one at a time, as if memory were sequentially
\* consistent. The Rust orderings give each step the order it has here: the
\* slot is loaded with Acquire and swapped with AcqRel (kovan 0.1.22
\* src/atom.rs:341-349, 488-535); closing is set with an AcqRel swap
\* (mod.rs:193) and read with Acquire (mod.rs:274), and a stopper's own
\* shutdown comes before its own load on the same thread; the cell's pointer
\* is loaded with Acquire and published with an AcqRel compare-and-swap
\* (regolith 0.2.1 src/sync/once_cell.rs:78, 166-171).
\*
\* Correspondence with the Rust code (crates/workers/src/pool/ unless named;
\* once_cell.rs is regolith 0.2.1 src/sync/once_cell.rs):
\*
\*   | Model             | Rust                                                        |
\*   |-------------------|-------------------------------------------------------------|
\*   | slot              | default.rs:24 DefaultPool.registered,                       |
\*   |                   | Atom<Option<Weak<Handle>>>; slot.node is the Atom's node:   |
\*   |                   | kovan compares the loaded node with the current one         |
\*   |                   | (atom.rs:497-505) and swaps with compare_exchange           |
\*   |                   | (atom.rs:510-516), and a replaced node is only retired      |
\*   |                   | (atom.rs:519-522), never reused while a loader pins it, so  |
\*   |                   | a fresh number per store stands for it; Empty is            |
\*   |                   | Atom::new(None) (default.rs:78)                             |
\*   | cell              | default.rs:25 DefaultPool.started, OnceCell<Pool>           |
\*   | turn              | once_cell.rs:51 init, the one-permit RawSemaphore           |
\*   | state             | mod.rs:95 PoolShared.closing ("closing"), and whether any   |
\*   |                   | Arc<Handle> is left ("gone": Weak::upgrade fails)           |
\*   | ownerHolds        | the owner's Pool handle (mod.rs:226-228)                    |
\*   | regPc, regSeen,   | default.rs:88-99 DefaultPool::register: where the loop is,  |
\*   | regResult         | the `current` it loaded, what it returned                   |
\*   | pc, seen, result  | default.rs:101-123 DefaultPool::get and started: where a    |
\*   |                   | call is, the slot it loaded (default.rs:85), its answer     |
\*   | onWorker          | worker/mod.rs:72-74 on_worker()                             |
\*   | stopping          | the pool a Pool::stop caller shut down (mod.rs:505-510)     |
\*   | regHonest,        | ghosts: no Rust value; each records, at the step it is set, |
\*   | handedRunning,    | the fact one invariant judges                               |
\*   | named, errHonest, |                                                             |
\*   | quiet, flapped    |                                                             |
\*   | RegBegins         | Pool::make_default (default.rs:70-72), called by its owner  |
\*   | RegLoad           | default.rs:90-93: load, running(&current), Arc::ptr_eq      |
\*   | RegCas            | default.rs:94-97: Arc::downgrade, compare_and_swap          |
\*   | Release           | the owner's last handle dropped: Handle::drop,              |
\*   |                   | mod.rs:238-243 (shutdown, abandon); the slot's Weak never   |
\*   |                   | holds it (default.rs:24, 94)                                |
\*   | MayShut, Stop     | Pool::stop: mod.rs:506-509 (on_worker, shutdown: the        |
\*   |                   | closing swap, mod.rs:192-196), then the ask at mod.rs:510   |
\*   |                   | -> deadline.rs:101 -> deadline.rs:114 -> deadline.rs:52,    |
\*   |                   | 59; a bare Pool::shutdown (mod.rs:485-487) is a Stop whose  |
\*   |                   | caller never asks                                           |
\*   | Ask               | default.rs:41-43 default_pool -> get (default.rs:105) ->    |
\*   |                   | registered (default.rs:85): the slot's load                 |
\*   | Check             | default.rs:136-140 running(): upgrade, then !is_closing;    |
\*   |                   | default.rs:105-107 answers it, else default.rs:108          |
\*   | Fast              | default.rs:118 started.get() and once_cell.rs:139 get(),    |
\*   |                   | then default.rs:121 block_on -> completion.rs:278-279       |
\*   |                   | refuses on a worker                                         |
\*   | Turn              | once_cell.rs:144 init.acquire(1), once_cell.rs:145-147 the  |
\*   |                   | look again                                                  |
\*   | StartOk           | once_cell.rs:148 init.await (default.rs:46-48 start_default |
\*   |                   | -> Pool::start, start.rs:51-167), once_cell.rs:152-157,     |
\*   |                   | 164-183 publish; the turn's permit returns when `_turn`     |
\*   |                   | drops at the end of get_or_try_init                         |
\*   | StartFail         | once_cell.rs:148 `?` returns the error and `_turn` drops    |
\*   |                   | (once_cell.rs:142-144); a browser's start always fails      |
\*   |                   | (default.rs:52-54)                                          |
\*   | Answer            | get's and started's returns (default.rs:106, 108, 119, 122) |
\*   | Observe           | ghost bookkeeping, no Rust counterpart                      |
\*   | CheckClosing      | default.rs:140 (!pool.is_closing()) (RED: only upgrade)     |
\*   | RegisterChecks    | default.rs:91-93 return while the slot's pool runs (RED:    |
\*   |                   | always swap)                                                |
\*   | RegisterCas       | default.rs:95 compare_and_swap against `current` (RED: a    |
\*   |                   | plain store)                                                |
\*   | RegisteredFirst   | default.rs:105-108 the slot before the cell (RED: the cell  |
\*   |                   | first)                                                      |
\*   | WeakSlot          | default.rs:24, 94 Weak<Handle> (RED: an Arc)                |
\*   | AnswerCompares    | default.rs:92 Arc::ptr_eq (RED: "yes" whenever a pool runs) |
\*   | RetryOnMoved      | default.rs:89 the loop around a failed swap (RED: one try,  |
\*   |                   | a failed swap answers "no")                                 |
\*   | WorkerRefused     | default.rs:121 block_on -> completion.rs:278-279 (RED: a    |
\*   |                   | worker waits for the turn like a plain thread; its own      |
\*   |                   | start is refused, start.rs:58-60)                           |
\*   | FastPathFirst     | default.rs:118 before completion.rs:278 (RED: a worker is   |
\*   |                   | refused before the cell is read)                            |
\*   | InitTurn          | once_cell.rs:144 one initializer at a time (RED: none)      |
\*   | Recheck           | once_cell.rs:145-147 (RED: no second look)                  |
\*   | FailReleases      | once_cell.rs:142-144 the permit returns on an error (RED:   |
\*   |                   | kept)                                                       |
\*   | PublishCas        | once_cell.rs:166-171, 155 the loser gets the winner (RED,   |
\*   |                   | with InitTurn: a check-then-store cell)                     |
\*   | StopShutsFirst    | mod.rs:509 before mod.rs:510 (RED: ask, then shut down)     |
\*   | StartedKept       | default.rs:5-8, 30-31 kept for the life of the process      |
\*   |                   | (RED: a caller shuts it down)                               |
\*   | RegisterOwnRunning| crates/storage/src/backends/regolith/worker_io/mod.rs:92-93 |
\*   |                   | start_pool registers the pool it just started (RED: a       |
\*   |                   | closing pool registers)                                     |
\*   | INV_NeverHands-   | default.rs:140 with mod.rs:274 for a registered answer; for |
\*   | OutClosing        | the started one, contract C2 (nothing in the code checks)   |
\*   | INV_FirstRunning- | default.rs:91-97: the swap happens only after the look      |
\*   | Wins              | found no running pool, against the node that look loaded    |
\*   | INV_Registered-   | default.rs:105-108, with the load (default.rs:85) before    |
\*   | Wins              | the check (default.rs:140)                                  |
\*   | INV_OneStart      | once_cell.rs:144-148 through default.rs:121                 |
\*   | INV_OneDefault    | once_cell.rs:144-148, 152-157, 166-171                      |
\*   | INV_ErrorHonest   | default.rs:118 before completion.rs:278-279                 |
\*   | INV_WorkerNever-  | default.rs:121 block_on -> completion.rs:278-279            |
\*   | Waits             |                                                             |
\*   | INV_StopDeadline- | mod.rs:509-510 with default.rs:140                          |
\*   | Elsewhere         |                                                             |
\*   | INV_MakeDefault-  | default.rs:66-72 ("Returns whether this pool is the default |
\*   | TellsTruth        | now"), default.rs:89-97 (the look, Arc::ptr_eq, the swap,   |
\*   |                   | the loop); contract C3                                      |
\*   | INV_Registration- | default.rs:68-69 ("The registration does not keep the pool  |
\*   | KeepsNothingAlive | alive"), default.rs:24, 94                                  |
\*   | LIVE_EveryCall-   | default.rs:118-122; once_cell.rs:142-148 (the permit always |
\*   | Returns           | returns); completion.rs:278-279 (a worker never waits)      |
\*   | LIVE_Every-       | default.rs:89-98: a swap fails only after another call's    |
\*   | RegisterReturns   | swap worked, and each pool registers a bounded number of    |
\*   |                   | times                                                       |
\*
\* Gaps, closed or explained:
\*   * PROCESS (default.rs:28) is a regolith Lazy: threads that touch it first
\*     at the same moment each build a DefaultPool and all of them use the one
\*     published (once_cell.rs:110-115, 152-157). DefaultPool::new has no
\*     effect, so the model starts from that one DefaultPool.
\*   * Weak::upgrade and the closing load are one step (Check): once upgraded,
\*     the pool cannot go away before the closing load, and closing never
\*     clears, so the one step sees what the second load sees.
\*   * make_default's load and its running check are one step (RegLoad): the
\*     check reads only the pool the load named, and a pool that stopped
\*     running never runs again, so a split only adds runs whose
\*     compare-and-swap then fails and loads again.
\*   * once_cell.rs:139 (get before the turn) is folded into Fast, and the
\*     acquire and the look after it are one step (Turn): only the turn
\*     holder ever writes the cell, so nothing can change it in between.
\*   * Pool::start is one step: the only effects this mechanism sees are the
\*     new pool and its publication. Its threads run no caller code before
\*     they report (the default has no hook: config/mod.rs:197 through
\*     Config::named, default.rs:58-63), so a start never waits on a thread
\*     that waits for the start.
\*   * After a publish, waiters get through two ways: the permits the publish
\*     opens (once_cell.rs:175; Turn is enabled once the cell holds a pool)
\*     and the holder's permit when `_turn` drops (StartOk gives the turn
\*     back). Either alone keeps every call returning, so neither has a
\*     switch of its own; only the failure path (FailReleases) has one way.
\*   * PublishCas alone has no RED: only the turn holder writes the cell, so
\*     the compare-and-swap keeps the callers agreeing only together with the
\*     turn. Red_RacyCell removes both, which is what a check-then-store cell
\*     would be.
\*   * In a browser started() never fills the cell (default.rs:127-132) and
\*     every start fails, which is a run where every StartOk is skipped.
\*   * The model's callers drop their copy of a pool when their call ends, so
\*     the only other holder Release has to consider is the registration.

EXTENDS Naturals, FiniteSets
\* Naturals gives numbers and ..; FiniteSets gives Cardinality, the size of a set.

CONSTANTS NP, NC, CheckClosing, RegisterChecks, RegisterCas, AnswerCompares,
          RetryOnMoved, RegisteredFirst, WeakSlot, WorkerRefused, FastPathFirst,
          InitTurn, Recheck, FailReleases, PublishCas, StopShutsFirst,
          StartedKept, RegisterOwnRunning
\* NP is how many pools can register (0 to 2) and NC how many callers ask for
\* the default (1 or 2). The sixteen switches are all TRUE in the code as it
\* is; each RED configuration turns one guard or one caller contract off.

ASSUME /\ NP \in 0..2
       \* zero, one or two pools that can register;
       /\ NC \in 1..2
       \* one or two callers;
       /\ CheckClosing \in BOOLEAN
       \* each switch is on or off, nothing else:
       /\ RegisterChecks \in BOOLEAN
       \* make_default's look at the pool already registered;
       /\ RegisterCas \in BOOLEAN
       \* make_default's compare-and-swap;
       /\ AnswerCompares \in BOOLEAN
       \* make_default's answer compares the running pool with its own;
       /\ RetryOnMoved \in BOOLEAN
       \* make_default loads again after a failed swap;
       /\ RegisteredFirst \in BOOLEAN
       \* the slot is read before the cell;
       /\ WeakSlot \in BOOLEAN
       \* the slot holds the pool weakly;
       /\ WorkerRefused \in BOOLEAN
       \* a worker is refused instead of waiting for a start;
       /\ FastPathFirst \in BOOLEAN
       \* the cell is read before a worker is refused;
       /\ InitTurn \in BOOLEAN
       \* one start at a time;
       /\ Recheck \in BOOLEAN
       \* a look at the cell after the turn;
       /\ FailReleases \in BOOLEAN
       \* a failed start gives the turn back;
       /\ PublishCas \in BOOLEAN
       \* the cell is published with one compare-and-swap;
       /\ StopShutsFirst \in BOOLEAN
       \* contract C1;
       /\ StartedKept \in BOOLEAN
       \* contract C2;
       /\ RegisterOwnRunning \in BOOLEAN
       \* contract C3.

PoolNames == <<"p1", "p2">>
\* The names the registrable pools can have.

Pools == {PoolNames[i] : i \in 1..NP}
\* The pools that can register: the first NP names.

CallerNames == <<"c1", "c2">>
\* The names the callers can have.

Callers == {CallerNames[i] : i \in 1..NC}
\* The callers of default_pool(): the first NC names.

Mine(c) == IF c = "c1" THEN "s1" ELSE "s2"
\* The pool caller c starts if it is the one that runs the start.

Started == {Mine(c) : c \in Callers}
\* Every pool a start could create: one per caller.

AllPools == Pools \cup Started
\* Every pool there is: registrable ones and started ones.

NoPool == "none"
\* A marker for "no pool": an empty slot or cell, no answer yet, nothing stopped.

NoCaller == "nobody"
\* A marker for "nobody holds the turn".

Empty == [node |-> 0, pool |-> NoPool]
\* The slot before any registration: Atom::new(None), node number 0.

SlotValues == [node : 0..NP, pool : Pools \cup {NoPool}]
\* Every value the slot can hold: a node number and the pool it names.

Asking == {"check", "fast", "turn", "start"}
\* Where a default_pool() call can be once it has begun and before it returns.

RegInFlight == {"load", "cas"}
\* Where a make_default call can be once it has begun and before it returns.

VARIABLES state, ownerHolds, slot, regPc, regSeen, regResult, regHonest, cell,
          turn, pc, onWorker, seen, stopping, result, handedRunning, named,
          errHonest, quiet, flapped
\* state         - each pool's life: "none" (a start that has not run),
\*                 "running", "closing" (PoolShared.closing set) or "gone" (no
\*                 Arc<Handle> left, so Weak::upgrade fails);
\* ownerHolds    - whether each registrable pool's owner still holds it;
\* slot          - DefaultPool.registered: the Atom's node and the pool it names;
\* regPc         - where each pool's make_default call is: "idle" (not
\*                 called yet), "load", "cas" or "done";
\* regSeen       - the slot value make_default loaded (`current`), for its swap;
\* regResult     - what make_default returned: "none" yet, "yes" or "no";
\* regHonest     - whether that answer was true when it was given;
\* cell          - DefaultPool.started: the started pool, or NoPool;
\* turn          - who holds the cell's one initializer turn, or NoCaller;
\* pc            - where each default_pool() call is: "idle", "stopped" (it
\*                 shut a pool down and has not asked yet), the Asking states,
\*                 or "done";
\* onWorker      - whether each caller runs on a worker thread (on_worker());
\* seen          - the slot value the call loaded (default.rs:85);
\* stopping      - the pool the caller shut down before asking, or NoPool;
\* result        - what the call returned: "none" yet, a pool, or "error";
\* handedRunning - whether the pool handed out was running when handed out;
\* named         - whether a registered answer was named by the slot then;
\* errHonest     - whether an error was given only with no pool to give;
\* quiet         - whether, at some moment of the call, no registered pool
\*                 ran (kept only for a call that answered a started pool);
\* flapped       - whether the slot ever moved away from a running pool.

vars == <<state, ownerHolds, slot, regPc, regSeen, regResult, regHonest, cell,
          turn, pc, onWorker, seen, stopping, result, handedRunning, named,
          errHonest, quiet, flapped>>
\* Every variable, for "nothing else changes" and for fairness.

RegRunsIn(st, sl) == sl.pool \in Pools /\ st[sl.pool] = "running"
\* The truth, in pool states st and slot sl: the slot names a pool that runs.

CodeRuns(x) ==
  \* What running() (default.rs:136-140) answers for the pool x a load named.
  /\ x \in Pools
  \* an empty slot names nothing that could run;
  /\ IF CheckClosing
     \* with the closing check (the code):
     THEN state[x] = "running"
     \* the pool is upgraded and not closing;
     ELSE state[x] # "gone"
     \* RED: only the upgrade, so a closing pool still counts.

ShutTargets == Pools \cup (IF StartedKept THEN {} ELSE Started)
\* The pools a caller may shut down: registrable ones, and (RED C2 only) the
\* started one.

MayShut(x) ==
  \* Whether pool x may be shut down now.
  /\ state[x] = "running"
  \* only a running pool can begin to close;
  /\ (x \in Pools /\ RegisterOwnRunning) => regPc[x] \in {"idle", "done"}
  \* contract C3: nobody else holds a pool while its owner registers it.

Init ==
  \* At the start every registrable pool runs and nothing has happened.
  /\ state = [x \in AllPools |-> IF x \in Pools THEN "running" ELSE "none"]
  \* registrable pools run; no start has run;
  /\ ownerHolds = [p \in Pools |-> TRUE]
  \* every owner holds its pool;
  /\ slot = Empty
  \* nothing is registered;
  /\ regPc = [p \in Pools |-> "idle"]
  \* no make_default call has begun;
  /\ regSeen = [p \in Pools |-> Empty]
  \* nothing loaded yet;
  /\ regResult = [p \in Pools |-> "none"]
  \* no make_default answer yet;
  /\ regHonest = [p \in Pools |-> TRUE]
  \* and no false one;
  /\ cell = NoPool
  \* the started cell is empty;
  /\ turn = NoCaller
  \* nobody holds the turn;
  /\ pc = [c \in Callers |-> "idle"]
  \* no call has begun;
  /\ onWorker \in {w \in [Callers -> BOOLEAN] : ~w["c1"]}
  \* c1 is a plain thread; c2 may be a worker thread or a plain one;
  /\ seen = [c \in Callers |-> Empty]
  \* nothing loaded yet;
  /\ stopping = [c \in Callers |-> NoPool]
  \* nobody is stopping a pool;
  /\ result = [c \in Callers |-> "none"]
  \* no answers yet;
  /\ handedRunning = [c \in Callers |-> TRUE]
  \* no closing pool handed out;
  /\ named = [c \in Callers |-> TRUE]
  \* no stale registered answer;
  /\ errHonest = [c \in Callers |-> TRUE]
  \* no needless error;
  /\ quiet = [c \in Callers |-> FALSE]
  \* no call has begun, so no moment of one was seen;
  /\ flapped = FALSE
  \* and the slot never moved.

\* --------------------------------------------- the owners of the pools

RegBegins(p) ==
  \* Whether p's owner may call make_default(p) now, at a moment it chooses.
  /\ regPc[p] = "idle"
  \* the call has not been made yet;
  /\ ownerHolds[p]
  \* the owner still holds the pool (the call borrows it);
  /\ RegisterOwnRunning => state[p] = "running"
  \* contract C3: and the pool runs (RED: a closing pool may register).

RegLoad(p) ==
  \* make_default(p) loads the slot and looks at the pool it names
  \* (default.rs:90-93): a fresh call by the owner, or the loop's next round
  \* after a failed swap. If that pool still runs, the call returns whether
  \* it is p; otherwise it goes on to swap p in.
  /\ RegBegins(p) \/ regPc[p] = "load"
  \* a fresh call, or the next round of the loop;
  /\ IF RegisterChecks /\ CodeRuns(slot.pool)
     \* the pool the slot names still runs (the code looks; RED never does):
     THEN /\ LET yes == slot.pool = p \/ ~AnswerCompares
                 \* "yes" if that pool is p (Arc::ptr_eq; RED: always "yes"),
                 fits == IF yes THEN slot.pool = p ELSE slot.pool # p
                 \* and whether the slot names p ("yes") or another pool ("no");
             IN /\ regResult' = [regResult EXCEPT ![p] = IF yes THEN "yes" ELSE "no"]
                \* the call returns that answer,
                /\ regHonest' = [regHonest EXCEPT ![p] = RegRunsIn(state, slot) /\ fits]
                \* which is true when the slot names a running pool that fits it;
          /\ regPc' = [regPc EXCEPT ![p] = "done"]
          \* the call is over,
          /\ UNCHANGED regSeen
          \* with nothing to remember for a swap;
     ELSE /\ regPc' = [regPc EXCEPT ![p] = "cas"]
          \* no running pool there: the swap comes next,
          /\ regSeen' = [regSeen EXCEPT ![p] = slot]
          \* against the value just loaded;
          /\ UNCHANGED <<regResult, regHonest>>
          \* no answer yet.
  /\ UNCHANGED <<state, ownerHolds, slot, cell, turn, pc, onWorker, seen,
                 stopping, result, handedRunning, named, errHonest>>
  \* loading changes nothing else.

RegCas(p) ==
  \* make_default(p) swaps p into the slot (default.rs:94-97), with the
  \* compare-and-swap against the node it loaded: it works only if the slot
  \* has not moved since. A failed swap goes round the loop again.
  /\ regPc[p] = "cas"
  \* only a call that is about to swap;
  /\ IF ~RegisterCas \/ slot.node = regSeen[p].node
     \* the slot still holds what was loaded (RED: a plain store always writes):
     THEN /\ slot' = [node |-> slot.node + 1, pool |-> p]
          \* a new node naming p replaces it,
          /\ regPc' = [regPc EXCEPT ![p] = "done"]
          \* the call returns,
          /\ regResult' = [regResult EXCEPT ![p] = "yes"]
          \* "yes, the default now",
          /\ regHonest' = [regHonest EXCEPT ![p] = (state[p] = "running")]
          \* which is true only if p runs;
     ELSE /\ UNCHANGED slot
          \* someone else swapped first: the slot is untouched,
          /\ IF RetryOnMoved
             \* and the loop goes round (RED: there is no loop):
             THEN /\ regPc' = [regPc EXCEPT ![p] = "load"]
                  \* load again,
                  /\ UNCHANGED <<regResult, regHonest>>
                  \* with no answer yet;
             ELSE /\ regPc' = [regPc EXCEPT ![p] = "done"]
                  \* RED: the call returns at once,
                  /\ regResult' = [regResult EXCEPT ![p] = "no"]
                  \* answering "no",
                  /\ regHonest' = [regHonest EXCEPT ![p] = RegRunsIn(state, slot) /\ slot.pool # p]
                  \* which is true only if another running pool holds the slot;
  /\ regSeen' = [regSeen EXCEPT ![p] = Empty]
  \* the loaded value is forgotten either way;
  /\ UNCHANGED <<state, ownerHolds, cell, turn, pc, onWorker, seen, stopping,
                 result, handedRunning, named, errHonest>>
  \* swapping changes nothing else.

Release(p) ==
  \* The owner drops its last handle to pool p, at any moment it chooses
  \* outside its make_default call (which borrows the pool). Handle::drop
  \* shuts the pool down and it is gone, unless something else still holds
  \* it: the slot holds it only weakly (RED: strongly).
  /\ ownerHolds[p]
  \* only an owner that still holds the pool;
  /\ regPc[p] \in {"idle", "done"}
  \* and not during its own make_default call;
  /\ ownerHolds' = [ownerHolds EXCEPT ![p] = FALSE]
  \* the owner lets go,
  /\ state' = IF WeakSlot \/ slot.pool # p THEN [state EXCEPT ![p] = "gone"] ELSE state
  \* and the pool is gone, unless a strong slot keeps it as it was;
  /\ UNCHANGED <<slot, regPc, regSeen, regResult, regHonest, cell, turn, pc,
                 onWorker, seen, stopping, result, handedRunning, named,
                 errHonest>>
  \* nothing else changes.

\* ------------------------------------------------------- the callers

Stop(c, x) ==
  \* Caller c stops pool x (Pool::stop, mod.rs:505-510): it shuts x down,
  \* then asks for the default to time its deadline on (Ask). In the RED
  \* order it asks first; the shutdown that comes after its answer is left
  \* out, since the promise already broke when the answer named x.
  /\ pc[c] = "idle"
  \* a caller that has not begun;
  /\ ~onWorker[c]
  \* stop on a worker returns OnWorker at once (mod.rs:506-508);
  /\ MayShut(x)
  \* x runs and may be shut down now;
  /\ state' = IF StopShutsFirst THEN [state EXCEPT ![x] = "closing"] ELSE state
  \* contract C1: x is closing before the ask (RED: not yet);
  /\ stopping' = [stopping EXCEPT ![c] = x]
  \* remember which pool this caller stops;
  /\ pc' = [pc EXCEPT ![c] = "stopped"]
  \* the ask comes next, whenever the caller makes it;
  /\ UNCHANGED <<ownerHolds, slot, regPc, regSeen, regResult, regHonest, cell,
                 turn, onWorker, seen, result, handedRunning, named, errHonest>>
  \* nothing else changes.

Answer(c, x, alive) ==
  \* Caller c's call returns pool x; alive says whether x ran at that moment.
  /\ result' = [result EXCEPT ![c] = x]
  \* the answer is x,
  /\ handedRunning' = [handedRunning EXCEPT ![c] = alive]
  \* note whether x ran when handed out,
  /\ pc' = [pc EXCEPT ![c] = "done"]
  \* and the call is over.

Ask(c) ==
  \* Caller c calls default_pool() (default.rs:41-43 -> get): the call loads
  \* the slot (default.rs:85). (RED RegisteredFirst: it reads the started cell
  \* first and answers a started pool at once.)
  /\ pc[c] \in {"idle", "stopped"}
  \* a caller that has not asked yet, plain or stopping a pool;
  /\ IF ~RegisteredFirst /\ cell # NoPool
     \* RED: the cell is read first and holds a pool:
     THEN /\ Answer(c, cell, state[cell] = "running")
          \* that pool is the answer,
          /\ UNCHANGED seen
          \* and the slot is never read;
     ELSE /\ seen' = [seen EXCEPT ![c] = slot]
          \* the code: load the slot,
          /\ pc' = [pc EXCEPT ![c] = "check"]
          \* and check the pool it names next;
          /\ UNCHANGED <<result, handedRunning>>
          \* no answer yet.
  /\ UNCHANGED <<state, ownerHolds, slot, regPc, regSeen, regResult, regHonest,
                 cell, turn, onWorker, stopping, named, errHonest>>
  \* asking changes nothing else.

Check(c) ==
  \* The call checks the pool its load named (running(), default.rs:136-140):
  \* upgraded and not closing means it is the answer (default.rs:105-107);
  \* otherwise the call goes to the started cell (default.rs:108).
  /\ pc[c] = "check"
  \* only a call that has loaded the slot;
  /\ IF CodeRuns(seen[c].pool)
     \* the loaded pool counts as running:
     THEN /\ Answer(c, seen[c].pool, state[seen[c].pool] = "running")
          \* it is the answer,
          /\ named' = [named EXCEPT ![c] = (slot.pool = seen[c].pool)]
          \* and note whether the slot still names it now;
     ELSE /\ pc' = [pc EXCEPT ![c] = "fast"]
          \* no registered pool runs: on to the started cell,
          /\ UNCHANGED <<result, handedRunning, named>>
          \* no answer yet;
  /\ seen' = [seen EXCEPT ![c] = Empty]
  \* the loaded value is forgotten either way;
  /\ UNCHANGED <<state, ownerHolds, slot, regPc, regSeen, regResult, regHonest,
                 cell, turn, onWorker, stopping, errHonest>>
  \* checking changes nothing else.

Fast(c) ==
  \* The call reads the started cell (default.rs:118, once_cell.rs:139). A
  \* pool there is the answer. An empty cell means a start: a worker cannot
  \* wait for one, so block_on refuses it (completion.rs:278-279); a plain
  \* thread goes to wait for the turn. (RED FastPathFirst: a worker is
  \* refused before the cell is read. RED WorkerRefused: a worker is never
  \* refused and goes to wait like a plain thread.)
  /\ pc[c] = "fast"
  \* only a call that found no running registered pool;
  /\ LET found == cell # NoPool
         \* whether the cell holds a pool,
         refuse == onWorker[c] /\ WorkerRefused /\ (~FastPathFirst \/ ~found)
         \* and whether the call is refused: on a worker, when the cell is
         \* empty (RED FastPathFirst: whenever it is on a worker; RED
         \* WorkerRefused: never);
     IN IF refuse
        \* refused:
        THEN /\ result' = [result EXCEPT ![c] = "error"]
             \* the call returns an error (Error::OnWorker),
             /\ errHonest' = [errHonest EXCEPT ![c] = ~found]
             \* which is honest only if no started pool was there,
             /\ pc' = [pc EXCEPT ![c] = "done"]
             \* and the call is over;
             /\ UNCHANGED handedRunning
             \* no pool handed out;
        ELSE IF found
             \* the cell holds a pool:
             THEN /\ Answer(c, cell, state[cell] = "running")
                  \* it is the answer;
                  /\ UNCHANGED errHonest
                  \* no error;
             ELSE /\ pc' = [pc EXCEPT ![c] = "turn"]
                  \* empty on a plain thread: wait for the turn,
                  /\ UNCHANGED <<result, handedRunning, errHonest>>
                  \* no answer yet.
  /\ UNCHANGED <<state, ownerHolds, slot, regPc, regSeen, regResult, regHonest,
                 cell, turn, onWorker, seen, stopping, named>>
  \* reading the cell changes nothing else.

Turn(c) ==
  \* The call gets the initializer's turn (once_cell.rs:144) and looks at the
  \* cell again (once_cell.rs:145-147). The turn is free, or a publish opened
  \* it to everyone. A pool there now is the answer; an empty cell means this
  \* call runs the start.
  /\ pc[c] = "turn"
  \* only a call waiting for the turn;
  /\ ~InitTurn \/ turn = NoCaller \/ cell # NoPool
  \* the wait ends: the turn is free, or the cell holds a pool (RED: no turn
  \* at all, so nobody waits);
  /\ IF Recheck /\ cell # NoPool
     \* the second look finds a pool (RED: there is no second look):
     THEN /\ Answer(c, cell, state[cell] = "running")
          \* it is the answer,
          /\ UNCHANGED turn
          \* and the turn was never taken;
     ELSE /\ pc' = [pc EXCEPT ![c] = "start"]
          \* this call runs the start,
          /\ turn' = IF InitTurn /\ cell = NoPool THEN c ELSE turn
          \* holding the turn while it does (with the cell open, or with no
          \* turn at all, it holds nothing);
          /\ UNCHANGED <<result, handedRunning>>
          \* no answer yet.
  /\ UNCHANGED <<state, ownerHolds, slot, regPc, regSeen, regResult, regHonest,
                 cell, onWorker, seen, stopping, named, errHonest>>
  \* nothing else changes.

StartOk(c) ==
  \* The start works (start_default -> Pool::start, default.rs:46-48) and the
  \* new pool is published with one compare-and-swap from empty
  \* (once_cell.rs:152-157, 164-183). If another pool was published first,
  \* the new one is dropped and the winner is the answer. The turn is given
  \* back as the call returns. (RED PublishCas: a plain store overwrites.)
  /\ pc[c] = "start"
  \* only a call running the start;
  /\ ~onWorker[c]
  \* Pool::start refuses on a worker (start.rs:58-60): only RED WorkerRefused
  \* lets a worker get here, and its start can only fail;
  /\ LET wins == ~PublishCas \/ cell = NoPool
         \* whether this pool is the one the cell keeps,
         winner == IF wins THEN Mine(c) ELSE cell
         \* and the pool the cell holds after the publish;
     IN /\ state' = [state EXCEPT ![Mine(c)] = IF wins THEN "running" ELSE "gone"]
        \* the new pool runs, or is dropped (gone) when it lost,
        /\ cell' = winner
        \* the cell holds the winner,
        /\ Answer(c, winner, IF wins THEN TRUE ELSE state[cell] = "running")
        \* and the winner is the answer;
  /\ turn' = IF turn = c THEN NoCaller ELSE turn
  \* the turn is given back if this call held it;
  /\ UNCHANGED <<ownerHolds, slot, regPc, regSeen, regResult, regHonest,
                 onWorker, seen, stopping, named, errHonest>>
  \* nothing else changes.

StartFail(c) ==
  \* The start fails (a thread the OS will not start; a browser starts none):
  \* the error is the answer and the turn is given back (once_cell.rs:142-148)
  \* so the next caller tries its own start. (RED FailReleases: the turn is
  \* kept.)
  /\ pc[c] = "start"
  \* only a call running the start;
  /\ result' = [result EXCEPT ![c] = "error"]
  \* the call returns the start's error,
  /\ errHonest' = [errHonest EXCEPT ![c] = TRUE]
  \* an honest one: no pool could start;
  /\ pc' = [pc EXCEPT ![c] = "done"]
  \* the call is over;
  /\ turn' = IF turn = c /\ FailReleases THEN NoCaller ELSE turn
  \* and the turn is given back (RED: kept forever);
  /\ UNCHANGED <<state, ownerHolds, slot, regPc, regSeen, regResult, regHonest,
                 cell, onWorker, seen, stopping, handedRunning, named>>
  \* nothing else changes.

\* ------------------------------------------------------------- ghosts

Observe ==
  \* Bookkeeping after every step, read only by the invariants. It records
  \* whether a call saw a moment with no running registered pool, and
  \* whether the slot ever moved off a running pool.
  /\ quiet' = [c \in Callers |->
                 IF pc'[c] = "done" /\ result'[c] \notin Started
                 \* a call that ended without a started pool:
                 THEN FALSE
                 \* nothing to keep;
                 ELSE IF pc[c] \in {"idle", "stopped"} /\ pc'[c] \in {"check", "done"}
                 \* a call that began with this step:
                 THEN ~RegRunsIn(state', slot')
                 \* is quiet if no registered pool runs right after it;
                 ELSE IF pc[c] \in Asking
                 \* a call under way:
                 THEN quiet[c] \/ ~RegRunsIn(state', slot')
                 \* stays quiet, or becomes quiet now;
                 ELSE quiet[c]]
                 \* anything else keeps what it had;
  /\ flapped' = (flapped \/ (slot' # slot /\ RegRunsIn(state, slot)))
  \* the slot moved while it named a running pool.

\* ---------------------------------------------------------------- spec

Step ==
  \* One step of one actor.
  \/ \E p \in Pools : RegLoad(p) \/ RegCas(p) \/ Release(p)
  \* an owner registers its pool or lets it go,
  \/ \E c \in Callers :
     \* or some caller c does one of these:
       \/ Ask(c) \/ Check(c) \/ Fast(c) \/ Turn(c) \/ StartOk(c) \/ StartFail(c)
       \* a caller asks or moves its call along,
       \/ \E x \in ShutTargets : Stop(c, x)
       \* or a caller stops a pool.

Next == Step /\ Observe
\* Every step, with the bookkeeping after it.

CallerStep(c) == Check(c) \/ Fast(c) \/ Turn(c) \/ StartOk(c) \/ StartFail(c)
\* What a begun call does until it returns.

RegStep(p) == (regPc[p] = "load" /\ RegLoad(p)) \/ RegCas(p)
\* What a begun make_default call does until it returns.

Fairness ==
  \* A begun call keeps moving while it can; asking, stopping, registering and
  \* letting go are choices that nothing forces.
  /\ \A c \in Callers : WF_vars(CallerStep(c) /\ Observe)
  \* every begun default_pool() call takes its next step;
  /\ \A p \in Pools : WF_vars(RegStep(p) /\ Observe)
  \* every begun make_default call takes its next step.

Spec == Init /\ [][Next]_vars /\ Fairness
\* The model: start at Init, take steps of Next, with the fairness above.

\* ---------------------------------------------------------- properties

TypeOK ==
  \* Every variable holds a value of the expected kind.
  /\ state \in [AllPools -> {"none", "running", "closing", "gone"}]
  \* each pool is unstarted, running, closing or gone;
  /\ ownerHolds \in [Pools -> BOOLEAN]
  \* each owner holds its pool or not;
  /\ slot \in SlotValues
  \* the slot holds a node and a pool or nothing;
  /\ regPc \in [Pools -> {"idle", "load", "cas", "done"}]
  \* each make_default call is in one of its places;
  /\ regSeen \in [Pools -> SlotValues]
  \* each loaded value is a slot value;
  /\ regResult \in [Pools -> {"none", "yes", "no"}]
  \* each make_default answer is none yet, yes or no;
  /\ regHonest \in [Pools -> BOOLEAN]
  \* true or false;
  /\ cell \in Started \cup {NoPool}
  \* the cell holds a started pool or nothing;
  /\ turn \in Callers \cup {NoCaller}
  \* the turn is held by a caller or by nobody;
  /\ pc \in [Callers -> {"idle", "stopped", "check", "fast", "turn", "start", "done"}]
  \* each call is in one of its places;
  /\ onWorker \in [Callers -> BOOLEAN]
  \* each caller is on a worker or not;
  /\ seen \in [Callers -> SlotValues]
  \* each loaded value is a slot value;
  /\ stopping \in [Callers -> AllPools \cup {NoPool}]
  \* each caller stops a pool or none;
  /\ result \in [Callers -> AllPools \cup {"none", "error"}]
  \* each answer is none yet, a pool, or an error;
  /\ handedRunning \in [Callers -> BOOLEAN]
  \* each "the pool handed out ran" note is true or false;
  /\ named \in [Callers -> BOOLEAN]
  \* each "the slot still named it" note is true or false;
  /\ errHonest \in [Callers -> BOOLEAN]
  \* each "no pool could be given" note is true or false;
  /\ quiet \in [Callers -> BOOLEAN]
  \* each "a moment with no running registered pool" note is true or false;
  /\ flapped \in BOOLEAN
  \* and "the slot moved off a running pool" is true or false.

INV_NeverHandsOutClosing ==
  \* Promise: default_pool() never hands out a pool that is closing or gone.
  \A c \in Callers : result[c] \in AllPools => handedRunning[c]
  \* every pool handed out ran at the moment it was handed out.

INV_FirstRunningWins ==
  \* Promise: while the registered pool runs, the slot keeps it: the first
  \* registered pool that runs stays the default, and two running pools never
  \* take turns.
  ~flapped
  \* the slot never moved away from a running pool.

INV_RegisteredWins ==
  \* Promise: a call answers the running registered pool the slot names, and
  \* answers a started pool only if, at some moment during the call, no
  \* registered pool ran.
  \A c \in Callers :
    \* for every caller:
    /\ result[c] \in Pools => named[c]
    \* a registered answer was still named by the slot when given;
    /\ result[c] \in Started => quiet[c]
    \* a started answer had a moment with no running registered pool.

INV_OneStart ==
  \* Promise: at most one default pool is ever started.
  Cardinality({s \in Started : state[s] # "none"}) <= 1
  \* the started pools that ran number zero or one.

INV_OneDefault ==
  \* Promise: every call that got a started pool got the same one: the one
  \* the cell holds.
  \A c \in Callers : result[c] \in Started => result[c] = cell
  \* a started answer is the cell's pool, which never changes once set.

INV_ErrorHonest ==
  \* Promise: a call fails only when no pool could be given: on a worker
  \* before the start, or when its own start failed.
  \A c \in Callers : result[c] = "error" => errHonest[c]
  \* every error was given with no pool to give.

INV_StopDeadlineElsewhere ==
  \* Promise: Pool::stop never times its deadline on the pool it stops.
  \A c \in Callers : (stopping[c] # NoPool /\ result[c] \in AllPools) => result[c] # stopping[c]
  \* a stopper's answer is some other pool.

INV_MakeDefaultTellsTruth ==
  \* Promise: make_default answers "yes" only when its pool runs and is the
  \* default right then, and "no" only when another running pool holds the
  \* slot.
  \A p \in Pools : regResult[p] # "none" => regHonest[p]
  \* every answer given was true when given.

INV_WorkerNeverWaits ==
  \* Promise: a worker thread never waits for a start (it would stall every
  \* task on its core); it is answered at once, with a pool or an error.
  \A c \in Callers : onWorker[c] => pc[c] \notin {"turn", "start"}
  \* no worker ever waits for the turn or runs the start.

INV_RegistrationKeepsNothingAlive ==
  \* Promise: the registration never keeps a pool alive: once its owner lets
  \* go, the pool is gone.
  \A p \in Pools : ~ownerHolds[p] => state[p] = "gone"
  \* every pool its owner let go of is gone.

LIVE_EveryCallReturns ==
  \* Promise: every default_pool() call returns, with a pool or an error.
  \A c \in Callers : (pc[c] \in Asking) ~> (pc[c] = "done")
  \* a begun call always ends.

LIVE_EveryRegisterReturns ==
  \* Promise: every make_default call returns.
  \A p \in Pools : (regPc[p] \in RegInFlight) ~> (regPc[p] = "done")
  \* a begun registration always ends.

====
