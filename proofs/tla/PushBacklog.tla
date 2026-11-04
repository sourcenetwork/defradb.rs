---- MODULE PushBacklog ----
\* Outbound PushLog backlog on the pusher (#1099, audit F24). The code it models:
\*   crates/p2p/src/sync/push_backlog.rs: BacklogOwner::enqueue (admission),
\*     BacklogOwner::pop_eligible (which job a free worker takes next, and which job
\*     is a probe), BacklogOwner::park_peer, BacklogOwner::park_peer_unreachable,
\*     BacklogOwner::peer_answered and BacklogOwner::retire_idle_cooldowns (the
\*     unreachable state and its spells), BacklogOwner::take_queued_for_peer,
\*     BacklogOwner::job_done;
\*   crates/p2p/src/sync/coordinator/push_worker.rs: spawn_push_workers (the fixed
\*     pool) and run_push_job (send, park on a timeout, demote the peer's queue).
\*
\* THE STORY. A node pushes every write to each replicator peer; one (write, peer)
\* push is a job. Jobs wait in one bounded queue, and a fixed pool of Workers worker
\* tasks takes them one at a time and sends them. A healthy peer replies. A dead peer
\* (a sleeping laptop, a node down for an upgrade) never replies: the send ends when
\* its send timeout fires, and the job goes to the durable retry ledger.
\*
\* WHAT MUST NEVER HAPPEN: the queue grows without bound; a worker slot is lost; one
\* peer holds every worker; jobs for peers already seen timing out hold every worker;
\* a job vanishes; a peer's queued jobs move to the ledger before one of its probes
\* timed out. WHAT MUST HAPPEN: every job for a healthy peer leaves the queue.
\*
\* THE RULES (GREEN). The queue has an item cap and a byte cap, and a full queue
\* rejects the job into the retry ledger. One peer holds at most PerPeerCap < Workers
\* workers. A send that times out marks its peer unreachable and parks it, and until
\* the peer answers again it shares one PerPeerCap with every other unreachable peer,
\* so all of them together hold at most the workers of one slow peer. A job started
\* while its peer is unreachable is a probe. Only when a probe times out too do the
\* peer's queued jobs move to the retry ledger, so a peer that answers right after one
\* failed round keeps its queue live.
\*
\* Five knobs; each RED value breaks one rule:
\*   AdmissionMode = "BoundedQueue"     - admit against the item and byte caps. [GREEN]
\*                 = "SpawnPerItem"     - every arrival becomes resident spawned work
\*                                        (the old broadcast spawn per (write, peer)). [RED]
\*   ReleaseMode   = "Release"          - a finished job returns its worker slot. [GREEN]
\*                 = "Leak"             - it does not; capacity decays. [RED]
\*   HandleMode    = "FixedWorkers"     - only the pool's handles are kept. [GREEN]
\*                 = "RetainAll"        - every finished job's handle is kept. [RED]
\*   FairnessMode  = "PerPeerCap"       - at most PerPeerCap running jobs per peer. [GREEN]
\*                 = "Unfair"           - one peer may take every worker. [RED]
\*   ParkMode      = "ParkUnreachable"  - a timeout parks the peer as described. [GREEN]
\*                 = "RetryUnreachable" - a timeout frees the worker and nothing else,
\*                                        so dead peers take it straight back (F24). [RED]
\*                 = "DemoteEveryTimeout" - every timeout also moves the peer's queued
\*                                        jobs to the ledger, probe or not, so one
\*                                        transient failure costs a peer that answers
\*                                        again its live queue. [RED]
\*
\* Every variable is state the code holds; the properties are stated over that state,
\* as invariants or as properties of each step, and nothing is recorded only to state
\* them. Variables and the code state each one is (crates/p2p/src/sync):
\*   arrivals     the push_backlog.rs Counters enqueued_total + coalesced_total +
\*                rejected_items_total + rejected_bytes_total
\*   backlog[p]   Inner::queues[p].len()
\*   active[p]    Inner::active[p]
\*   freeWorkers  the pool's workers waiting in next_job (BacklogOwner::waiters):
\*                worker_count - Inner::active_jobs
\*   sent         Counters::completed_total
\*   coalesced    Counters::coalesced_total
\*   rejected     Counters::rejected_items_total + rejected_bytes_total
\*   handles      the JoinHandles SyncShutdownHandle keeps (coordinator/mod.rs)
\*   failed       the jobs run_push_job hands to the retry recorder (report_push_event
\*                with create_retry): its own failed job and the queued jobs that
\*                park_peer_unreachable returns
\*   unreachable  the peers whose Inner::peer_cooldowns entry has an `unreachable` spell
\*   probes[p]    p's running jobs whose PushJobSpec::probe_of is that spell
\*
\* Abstractions: `backlog` is admitted-but-not-running work per peer (the bounded queue,
\* or the spawned waiting futures under SpawnPerItem). Payload size is a per-peer
\* constant Weight. The park's cooldown is a timer the model lets elapse at any moment,
\* the worst case for the bounds. A healthy peer's send gets a reply or times out, so a
\* peer that answers again is covered. The code retires an idle peer's parking state
\* only past its bound (retire_idle_cooldowns); the model lets that happen whenever the
\* peer is idle. Rejected, timed-out and demoted jobs are terminal ledger entries here;
\* the re-push loop they feed is Replicator.tla's subject. Round-robin order is not
\* modeled: the scheduler may hand a free worker to any job that may start.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,         \* target peers of the fan-out
  SlowPeers,     \* dead peers: their sends never get a reply and always time out
  Weight,        \* [Peers -> Nat \ {0}] payload bytes of one job for that peer
  MaxArrivals,   \* total (write, peer) arrivals; more than QueueCap to stress the caps
  QueueCap,      \* queue item cap
  ByteCap,       \* queue byte cap
  Workers,       \* fixed worker-pool size
  PerPeerCap,    \* per-peer running cap, < Workers; also the unreachable peers' shared cap
  AdmissionMode, \* "BoundedQueue" | "SpawnPerItem"
  ReleaseMode,   \* "Release" | "Leak"
  HandleMode,    \* "FixedWorkers" | "RetainAll"
  FairnessMode,  \* "PerPeerCap" | "Unfair"
  ParkMode       \* "ParkUnreachable" | "RetryUnreachable" | "DemoteEveryTimeout"

\* At least one peer is healthy, so the liveness promise is about someone.
ASSUME Peers # {} /\ SlowPeers \subseteq Peers /\ SlowPeers # Peers
\* Every job costs at least one byte.
ASSUME Weight \in [Peers -> Nat] /\ \A p \in Peers : Weight[p] >= 1
\* The caps are positive.
ASSUME QueueCap \in Nat /\ QueueCap >= 1
ASSUME ByteCap \in Nat /\ ByteCap >= 1
\* More arrivals than the queue holds, so overflow is reachable.
ASSUME MaxArrivals \in Nat /\ MaxArrivals > QueueCap
\* The pool has a worker, and one peer's cap always leaves at least one free.
ASSUME Workers \in Nat /\ Workers >= 1
ASSUME PerPeerCap \in Nat /\ PerPeerCap >= 1 /\ PerPeerCap < Workers
ASSUME AdmissionMode \in {"BoundedQueue", "SpawnPerItem"}
ASSUME ReleaseMode \in {"Release", "Leak"}
ASSUME HandleMode \in {"FixedWorkers", "RetainAll"}
ASSUME FairnessMode \in {"PerPeerCap", "Unfair"}
ASSUME ParkMode \in {"ParkUnreachable", "RetryUnreachable", "DemoteEveryTimeout"}

VARIABLES
  arrivals,    \* arrivals so far
  backlog,     \* [Peers -> Nat] admitted jobs not yet on a worker
  active,      \* [Peers -> Nat] jobs holding a worker (their send is running)
  freeWorkers, \* unheld worker slots
  sent,        \* jobs whose send got a reply
  coalesced,   \* arrivals folded into an already-queued job for the same (peer, cid)
  rejected,    \* arrivals rejected at a full queue, recorded in the retry ledger
  handles,     \* retained JoinHandles
  failed,      \* jobs handed to the retry ledger by a timeout or by a park's demotion
  unreachable, \* peers whose last send timed out and that have not answered since
  probes       \* [Peers -> Nat] running jobs started during the peer's current unreachable
               \* spell (a reply ends the spell, and its leftover jobs become ordinary)

vars == <<arrivals, backlog, active, freeWorkers, sent, coalesced, rejected, handles,
          failed, unreachable, probes>>

\* The sum of f over the peers in S.
RECURSIVE SumOver(_, _)
SumOver(f, S) ==
  IF S = {} THEN 0
  ELSE LET p == CHOOSE x \in S : TRUE
       IN f[p] + SumOver(f, S \ {p})

\* Jobs waiting in the queue, and the bytes they hold.
BacklogItems == SumOver(backlog, Peers)
BacklogBytes == SumOver([p \in Peers |-> backlog[p] * Weight[p]], Peers)
\* Jobs holding a worker.
ActiveJobs   == SumOver(active, Peers)
\* Workers held by peers that are currently unreachable (probes and older jobs alike):
\* what pop_eligible counts against the shared cap.
UnreachableActive == SumOver([p \in Peers |-> IF p \in unreachable THEN active[p] ELSE 0], Peers)
\* Running jobs started for peers already known to be unreachable (probes).
ProbeJobs == SumOver(probes, Peers)

\* The queue has room for one more job for p.
HasRoom(p) ==
  /\ BacklogItems < QueueCap
  /\ BacklogBytes + Weight[p] <= ByteCap

TypeOK ==
  /\ arrivals \in 0..MaxArrivals
  /\ backlog \in [Peers -> 0..MaxArrivals]
  /\ active \in [Peers -> 0..Workers]
  /\ freeWorkers \in 0..Workers
  /\ sent \in 0..MaxArrivals
  /\ coalesced \in 0..MaxArrivals
  /\ rejected \in 0..MaxArrivals
  /\ handles \in Workers..(Workers + MaxArrivals)
  /\ failed \in 0..MaxArrivals
  /\ unreachable \subseteq Peers
  /\ probes \in [Peers -> 0..Workers]

\* Nothing has arrived; every worker is free; every peer is presumed reachable.
Init ==
  /\ arrivals = 0
  /\ backlog = [p \in Peers |-> 0]
  /\ active = [p \in Peers |-> 0]
  /\ freeWorkers = Workers
  /\ sent = 0
  /\ coalesced = 0
  /\ rejected = 0
  /\ handles = Workers
  /\ failed = 0
  /\ unreachable = {}
  /\ probes = [p \in Peers |-> 0]

\* The variables only a send's end or a park changes.
parkVars == <<failed, unreachable, probes>>

\* A (write, peer) arrival is admitted as resident work: under BoundedQueue only within
\* the item and byte caps, under SpawnPerItem always (the spawned future keeps its
\* payload while it waits). [RED path when unbounded]
Enqueue(p) ==
  /\ arrivals < MaxArrivals
  /\ (AdmissionMode = "SpawnPerItem" \/ HasRoom(p))
  /\ arrivals' = arrivals + 1
  /\ backlog' = [backlog EXCEPT ![p] = @ + 1]
  /\ UNCHANGED <<active, freeWorkers, sent, coalesced, rejected, handles>>
  /\ UNCHANGED parkVars

\* An arrival for a (peer, cid) already queued folds into that job instead of taking a
\* slot. A running job cannot absorb it: its send has begun.
Coalesce(p) ==
  /\ AdmissionMode = "BoundedQueue"
  /\ arrivals < MaxArrivals
  /\ backlog[p] > 0
  /\ arrivals' = arrivals + 1
  /\ coalesced' = coalesced + 1
  /\ UNCHANGED <<backlog, active, freeWorkers, sent, rejected, handles>>
  /\ UNCHANGED parkVars

\* Queue full (either cap): the arrival is rejected explicitly into the durable retry
\* ledger, never dropped silently.
Reject(p) ==
  /\ AdmissionMode = "BoundedQueue"
  /\ arrivals < MaxArrivals
  /\ ~HasRoom(p)
  /\ arrivals' = arrivals + 1
  /\ rejected' = rejected + 1
  /\ UNCHANGED <<backlog, active, freeWorkers, sent, coalesced, handles>>
  /\ UNCHANGED parkVars

\* pop_eligible's shared cap: an unreachable peer may start a job only while the
\* unreachable peers together hold fewer than PerPeerCap workers. RetryUnreachable (the
\* old code) has no such rule.
MayStart(p) ==
  \/ ParkMode = "RetryUnreachable"
  \/ p \notin unreachable
  \/ UnreachableActive < PerPeerCap

\* A free worker takes one of p's queued jobs. Under PerPeerCap a peer holds at most
\* PerPeerCap workers; under Unfair nothing stops one peer taking them all. A job taken
\* while p is unreachable is a probe.
StartJob(p) ==
  /\ backlog[p] > 0
  /\ freeWorkers > 0
  /\ (FairnessMode = "PerPeerCap" => active[p] < PerPeerCap)
  /\ MayStart(p)
  /\ backlog' = [backlog EXCEPT ![p] = @ - 1]
  /\ active' = [active EXCEPT ![p] = @ + 1]
  /\ freeWorkers' = freeWorkers - 1
  /\ probes' = IF p \in unreachable THEN [probes EXCEPT ![p] = @ + 1] ELSE probes
  /\ UNCHANGED <<arrivals, sent, coalesced, rejected, handles, failed, unreachable>>

\* One of p's running jobs ends (job_done). Leak: its worker slot is not returned.
\* RetainAll: its JoinHandle is kept for good.
JobEnds(p) ==
  /\ active[p] > 0
  /\ active' = [active EXCEPT ![p] = @ - 1]
  /\ freeWorkers' = IF ReleaseMode = "Release" THEN freeWorkers + 1 ELSE freeWorkers
  /\ handles' = IF HandleMode = "RetainAll" THEN handles + 1 ELSE handles

\* The job that ended is a probe (k = 1) or a job started before p's current
\* unreachable spell (k = 0); the code knows which (PushJobSpec::probe_of).
ProbeEnds(p, k) ==
  /\ k <= probes[p]
  /\ (k = 0 => probes[p] < active[p])
  /\ probes' = [probes EXCEPT ![p] = @ - k]

\* A healthy peer replies to a send. A reply means the peer is reachable again
\* (peer_answered): it leaves `unreachable`, its spell ends, and its other running
\* jobs are ordinary jobs from now on.
Complete(p) ==
  /\ p \notin SlowPeers
  /\ JobEnds(p)
  /\ sent' = sent + 1
  /\ unreachable' = unreachable \ {p}
  /\ probes' = [probes EXCEPT ![p] = 0]
  /\ UNCHANGED <<arrivals, backlog, coalesced, rejected, failed>>

\* Whether the timeout of a job (a probe when k = 1) moves p's queued jobs to the
\* ledger (park_peer_unreachable returns take_queued_for_peer).
Demotes(k) ==
  \/ ParkMode = "DemoteEveryTimeout"
  \/ ParkMode = "ParkUnreachable" /\ k = 1

\* A send times out: always for a dead peer, and for a healthy one whenever the
\* environment drops it. The job goes to the retry ledger and its peer becomes
\* unreachable. ParkUnreachable also parks the peer, and when the job was a probe moves
\* its queued jobs to the ledger.
Timeout(p) ==
  /\ JobEnds(p)
  /\ \E k \in {0, 1} :
       /\ ProbeEnds(p, k)
       /\ IF Demotes(k)
            THEN /\ failed' = failed + 1 + backlog[p]
                 /\ backlog' = [backlog EXCEPT ![p] = 0]
            ELSE /\ failed' = failed + 1
                 /\ UNCHANGED backlog
  /\ unreachable' = unreachable \cup {p}
  /\ UNCHANGED <<arrivals, sent, coalesced, rejected>>

\* An unreachable peer with nothing queued or running loses its parking state
\* (retire_idle_cooldowns): its next job is an ordinary job again.
Retire(p) ==
  /\ p \in unreachable
  /\ backlog[p] = 0
  /\ active[p] = 0
  /\ unreachable' = unreachable \ {p}
  /\ UNCHANGED <<arrivals, backlog, active, freeWorkers, sent, coalesced, rejected, handles,
                 failed, probes>>

Next ==
  \E p \in Peers :
    \/ Enqueue(p)
    \/ Coalesce(p)
    \/ Reject(p)
    \/ StartJob(p)
    \/ Complete(p)
    \/ Timeout(p)
    \/ Retire(p)

\* Arrivals exhausted and every healthy peer's work drained: stutter so TLC does not
\* report a finished schedule as a deadlock.
Done ==
  /\ arrivals = MaxArrivals
  /\ \A p \in Peers \ SlowPeers : backlog[p] = 0 /\ active[p] = 0
Terminating == Done /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Terminating]_vars

\* Scheduler-only fairness. The dispatcher hands a job to a free worker whenever some
\* job may start: weak fairness on "some StartJob", never on any one peer. A running
\* send always ends, by a reply or by its send timeout: that is a timer on each send,
\* not dispatch fairness for any peer. Arrivals need no fairness.
FairSpec ==
  /\ Spec
  /\ WF_vars(\E p \in Peers : StartJob(p))
  /\ \A p \in Peers : WF_vars(Complete(p) \/ Timeout(p))

INV_TypeOK == TypeOK

\* The #1099 residency bound: work waiting for a worker stays within the queue caps
\* however many writes arrive. SpawnPerItem breaks it once arrivals outrun the workers.
INV_QueueBounded ==
  /\ BacklogItems <= QueueCap
  /\ BacklogBytes <= ByteCap

\* Worker slots are conserved: free + held = pool size. Leak breaks it at the first
\* finished job.
INV_PermitConservation == freeWorkers + ActiveJobs = Workers

\* Only the fixed pool's handles are kept. RetainAll breaks it at the first finished job.
INV_HandlesBounded == handles <= Workers

\* No arrival is lost silently: each is queued, running, sent, coalesced into a queued
\* job, rejected into the ledger, or handed to the ledger after a timeout or a park.
INV_NoSilentLoss ==
  arrivals = BacklogItems + ActiveJobs + sent + coalesced + rejected + failed

\* One peer never holds every worker. Unfair breaks it: a dead peer's sends take the
\* whole pool for a send timeout, again and again.
INV_PeerLeavesAWorker == \A p \in Peers : active[p] < Workers

\* Audit F24: jobs started for peers already seen timing out never hold every worker,
\* however many such peers there are. RetryUnreachable breaks it: two dead peers whose
\* sends timed out take the freed workers straight back.
INV_UnreachableLeaveAWorker == ProbeJobs < Workers

\* p's queued jobs leave the queue in this step without a worker taking one: they move
\* to the retry ledger.
QueueDemoted(p) == backlog'[p] < backlog[p] /\ active'[p] <= active[p]

\* A step that moves a peer's queued jobs to the ledger ends one of its probes: the
\* failed round that marked it unreachable is not enough. DemoteEveryTimeout breaks it:
\* a peer's first timeout hands its queue to the ledger, before any probe ran.
ACT_DemoteAfterFailedProbe ==
  [][\A p \in Peers : QueueDemoted(p) => probes'[p] < probes[p]]_vars

\* Liveness (under FairSpec): every job admitted for a healthy peer leaves the queue, by
\* its reply or, after a transient timeout, into the retry ledger.
LIVE_HealthyProgress ==
  \A p \in Peers \ SlowPeers :
    (backlog[p] + active[p] > 0) ~> (backlog[p] + active[p] = 0)
====
