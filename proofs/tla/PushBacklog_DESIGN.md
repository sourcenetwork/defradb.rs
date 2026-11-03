# PushBacklog — bounded outbound push backlog (TLA+ design)

Models the pusher-side admission and dispatch of outbound PushLog work (issue **#1099**,
audit **F24**):

- `crates/p2p/src/sync/push_backlog.rs`: `BacklogOwner::enqueue` (admission against the
  item and byte caps, coalescing, explicit rejection), `BacklogOwner::pop_eligible`
  (which job a free worker takes next: the per-peer cap, the parked peers, the
  unreachable peers' shared cap, and which job is a probe), `BacklogOwner::park_peer`,
  `BacklogOwner::park_peer_unreachable`, `BacklogOwner::peer_answered`,
  `BacklogOwner::take_queued_for_peer`, `BacklogOwner::job_done`.
- `crates/p2p/src/sync/coordinator/push_worker.rs`: `spawn_push_workers` (the fixed
  pool) and `run_push_job` (send with `send_timeout`, park the peer on a capacity nack,
  a `retry_after` or a transport failure, demote its queued jobs to the retry ledger,
  record a reply).

Companion to `PushLogAdmission.tla`, which models the **hub's** reply decision at its
bounded pending map; this slice isolates the **pusher's** resource discipline.

## Mechanism

Work is admitted through a **bounded queue** (item cap and byte cap) as compact jobs
before anything is spawned; a **fixed pool** of `Workers` tasks drains it; one peer
holds at most `PerPeerCap < Workers` workers; queue-full is an **explicit outcome** (the
arrival is rejected into the durable retry ledger, and an identical `(peer, cid)`
coalesces into the queued job); only the pool's handles are retained.

A send to a dead peer (a sleeping laptop or phone, a node down for an upgrade) gets no
reply and ends at its send timeout (30 s). The per-peer cap isolates *one* such peer,
but with the default pool (8 workers, 4 per peer) two dead replicators hold every
worker, and the old code let them take each freed worker straight back after every
timeout: healthy peers got about one pool's worth of jobs per timeout (F24). Now a
transport failure **parks** the peer with an escalating cooldown, like a capacity nack,
and marks it **unreachable** until it answers again. Unreachable peers **share one
`PerPeerCap`**: together they hold the workers of one slow peer at most, however many
there are.

A job started while its peer is unreachable (so after the park's cooldown ran out) is a
**probe**. The peer's queued jobs move to the retry ledger only when a probe fails on
transport too. A peer that answers right after one failed round (a timeout, a lost
connection) keeps its queue live; a dead peer's queue moves to the ledger one cooldown
plus one send timeout after its first failure.

Five knobs; each RED value breaks one rule:

| Knob | GREEN | RED |
|------|-------|-----|
| `AdmissionMode` | `"BoundedQueue"` — admit/coalesce/reject against the caps | `"SpawnPerItem"` — every arrival becomes resident spawned work (the old broadcast spawn) |
| `ReleaseMode` | `"Release"` — a finished job returns the worker slot | `"Leak"` — it does not |
| `HandleMode` | `"FixedWorkers"` — only pool handles retained | `"RetainAll"` — every finished handle retained, never pruned |
| `FairnessMode` | `"PerPeerCap"` — ≤ `PerPeerCap` running per peer | `"Unfair"` — one peer may hold every worker |
| `ParkMode` | `"ParkUnreachable"` — a timeout parks the peer; unreachable peers share one cap; a failed probe demotes the queue | `"RetryUnreachable"` — a timeout frees the worker and nothing else (the code before F24); `"DemoteEveryTimeout"` — every timeout demotes the queue, probe or not |

## Properties

- `INV_QueueBounded` — waiting work stays within the item and byte caps however many
  writes arrive. Violated by `SpawnPerItem`.
- `INV_PermitConservation` — `freeWorkers + activeJobs = Workers`. Violated by `Leak`.
- `INV_HandlesBounded` — retained handles ≤ `Workers`. Violated by `RetainAll`.
- `INV_NoSilentLoss` — every arrival is queued, running, sent, coalesced, rejected into
  the ledger, or handed to the ledger by a timeout or a park's demotion.
- `INV_PeerLeavesAWorker` — one peer never holds every worker. Violated by `Unfair`.
- `INV_UnreachableLeaveAWorker` — jobs started for peers already known to be
  unreachable (probes) never hold every worker. Violated by `RetryUnreachable`: two dead
  peers each take a freed worker back after their timeouts.
- `INV_DemoteAfterFailedProbe` — a peer's queued jobs move to the ledger only once one of
  its probes has timed out in its current spell. With `MaxGlitches = 1` a healthy peer is
  never demoted. Violated by `DemoteEveryTimeout`: a healthy peer's one timeout hands its
  queue to the ledger although it answers its next send.
- `LIVE_HealthyProgress` (under `FairSpec`) — every job admitted for a healthy peer
  leaves the queue (its reply, or the ledger after a transient timeout).

`FairSpec` gives fairness to the scheduler only: weak fairness on *some* `StartJob`,
never on one peer's. The other fairness is per send and is a timer, not dispatch
fairness: every running send ends, by a reply or by its send timeout. With bounded
arrivals the old rule still passes `LIVE_HealthyProgress` (dead peers' jobs run out),
which is why F24, a latency defect of one send timeout per round, is caught by the
invariant. Dropping the scheduler's fairness makes `LIVE_HealthyProgress` fail, so the
GREEN liveness result is not vacuous.

## Abstractions

- **`backlog` is admitted-but-not-running work per peer** — the compact queue under
  `BoundedQueue`, the spawned waiting futures under `SpawnPerItem`.
- **Payload size is a per-peer constant** (`Weight`) so the byte cap is exercised
  independently of the item cap.
- **Dead peers (`SlowPeers`) never reply; their sends always time out.** A send to a
  healthy peer may also time out, at most `MaxGlitches` times in all, so a peer that
  answers again (and leaves the shared cap) is covered.
- **The park's cooldown is a timer the model lets elapse at any moment**, the worst
  case for the bounds. The real cooldown escalates from 2 s up to 64 s (plus jitter)
  while sends keep failing; it only spaces the probes out.
- **`probes` is a ghost**: running jobs started during the peer's current unreachable
  spell. A reply ends the spell, and the peer's leftover jobs become ordinary jobs. The
  code knows which job is a probe: `pop_eligible` stamps each job with the spell it was
  started in (`PushJobSpec::probe_of`), and `park_peer_unreachable` demotes only for a
  probe of the current spell.
- **`probeTimedOut` and `demoted` are ghosts**: the peers one of whose probes timed out,
  and the peers whose queued jobs moved to the ledger, in the current spell.
- **Rejected, timed-out and demoted jobs are terminal ledger entries.** The re-push
  loop they feed is `Replicator.tla`'s subject.
- **Round-robin order is not modeled** — the scheduler may give a free worker to any
  job that may start.
- **The first timeout is not avoided.** Before a dead peer's first send times out
  nothing marks it, so its jobs may hold workers for one send timeout; the invariant
  is about peers already known to be unreachable.

## Configs

| Config | Knob | Verdict | Meaning |
|--------|------|---------|---------|
| `MC_PushBacklog_Green.cfg` | all GREEN knobs | GREEN | one slow peer, two healthy: all bounds hold, healthy peers progress |
| `MC_PushBacklog_Green_TwoDead.cfg` | all GREEN knobs | GREEN | F24 fix: two dead peers, 2 workers, cap 1 — a worker always remains for the healthy peer |
| `MC_PushBacklog_Green_SharedCap.cfg` | all GREEN knobs | GREEN | F24 fix with 3 workers, cap 2 (the real pool's shape): several probes per peer, a reply mid-probe |
| `MC_PushBacklog_Red_TwoDead.cfg` | `ParkMode="RetryUnreachable"` | RED | the code before F24: dead peers take freed workers back → `INV_UnreachableLeaveAWorker` violated |
| `MC_PushBacklog_Red_DemoteEveryTimeout.cfg` | `ParkMode="DemoteEveryTimeout"` | RED | two healthy peers, one timeout: its queue goes to the ledger → `INV_DemoteAfterFailedProbe` violated |
| `MC_PushBacklog_Red_SpawnPerItem.cfg` | `AdmissionMode="SpawnPerItem"` | RED | spawn before the semaphore → `INV_QueueBounded` violated |
| `MC_PushBacklog_Red_PermitLeak.cfg` | `ReleaseMode="Leak"` | RED | a finished job keeps its worker slot → `INV_PermitConservation` violated |
| `MC_PushBacklog_Red_RetainHandles.cfg` | `HandleMode="RetainAll"` | RED | `SyncShutdownHandle` retention → `INV_HandlesBounded` violated |
| `MC_PushBacklog_Red_NoPeerCap.cfg` | `FairnessMode="Unfair"` | RED | the slow peer holds every worker → `INV_PeerLeavesAWorker` violated |

Each RED config checks only `INV_TypeOK` plus the one property it is meant to violate,
so the failure is attributable.

## Conformance fence

The Rust-side fence is the `crates/p2p/src/sync/push_backlog.rs` unit tests (queue
caps, coalescing, explicit rejection, parks, `unreachable_peers_share_one_peer_cap`,
`only_a_failed_probe_demotes_an_unreachable_peer`), the worker tests in
`crates/p2p/src/sync/coordinator/push_worker_tests.rs`
(`slow_peer_does_not_starve_healthy_peers`,
`two_stalled_replicators_do_not_starve_healthy_peer` and
`transient_transport_failure_keeps_queued_jobs_live` on the default pool and send
timeout under paused time), and the fan-out integration test
`tools/integration-test/tests/p2p_backlog.rs`
(`outbound_backlog_bounded_under_fanout_with_dead_peer`), which asserts bounded
residency and healthy-peer progress while one peer is SIGSTOPped.
