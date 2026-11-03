# Peer retry markers: generations, the bounded sweep, and discharge

## What the model covers

A sender keeps one durable retry marker per document scope it owes a
replicator peer. The model checks three things about those markers:

- an old acknowledgement or an old clear never erases a newer registration;
- one sweep tick reads a bounded number of marker keys, whatever the backlog;
- the marker of a document that left this node is eventually discharged.

## Code it names

| Model | Code |
|---|---|
| `Register` | `crates/storage/src/stores/peerstore.rs` `RegisterScopeMarkerAttempt`: every registration stores a fresh generation |
| `Observe`, `Ack` | `complete_retry_scope` compare-deletes only the generation the worker saw |
| `Forget`, `Readd` | `delete_replicator`, `create_replicator`: the incarnation changes |
| `BeginClear`, `FinishClear` | `clear_retry_peer_for_incarnation` reads the replicator row and incarnation in the transaction that clears retry state |
| `Vanish` | `purge_by_doc_ids`, `truncate_collection`: the document's DocID mapping is deleted |
| `Tick` | `crates/p2p-adapter/src/retry.rs` `run_retry_pass`: a one-marker probe (`Peerstore::get_retry_page(peer, 1)`) answers "any markers?" and "due?"; only a due peer loads `get_retry_page(peer, MAX_MARKERS_PER_PEER_PASS + 1)` |
| `NewPage`, `Rotation` | `Peerstore::load_scope_markers`: the scopes after `RetryInfo::dispatch_after`, then wrapping round, in key order |
| `Step` | `retry_doc` in `crates/db/src/merge/push_docs.rs`; a document with no DocID mapping goes to `complete_document_retry_if_absent` before its collection is resolved |
| end of a failed pass | `reschedule_retry_peer_for_incarnation` / `restart_retry_peer_for_incarnation` with the last marker tried, which becomes `dispatch_after` |
| `Elapse` | `RetryInfo::is_due` once the ladder deadline passes |

## Configurations

`Green` uses two scopes and a one-marker page, every guard on, and the fair
specification. It checks the two fences, `INV_SweepReadsBounded` and
`LIVE_VanishedMarkerDischarged`.

Each RED turns one guard off:

- `Red_StaleAck`: the acknowledgement deletes without comparing generations.
  A registration made after the worker observed is erased
  (`INV_NewRegistrationSurvivesOldAck`).
- `Red_Readd`: a clear finishes without checking the incarnation. A clear
  begun before forget/re-add erases the re-added peer's state
  (`INV_NewRegistrationSurvivesOldClear`).
- `Red_UnboundedReads`: the tick reads every marker before asking whether the
  peer is due. This was `run_retry_pass` before the bounded page: every two
  seconds it loaded and sorted each scheduled peer's whole marker set, then
  loaded it again after the pass (`INV_SweepReadsBounded`).
- `Red_KeepVanished`: a replay of a purged document errors and keeps the
  marker. This was `retry_doc` before the fix: the sweep loads document markers
  with an empty collection id, so `retry_doc` resolved the collection first and
  failed with "collection for document not found" before it could reach the
  absent-document completion. The marker stayed forever and failed every pass
  (`LIVE_VanishedMarkerDischarged`).
- `Red_NoRotation`: a failed pass leaves the cursor where it was. A head marker
  the peer keeps refusing is replayed first on every pass, and with one-page
  passes the vanished marker behind it is never reached
  (`LIVE_VanishedMarkerDischarged`).

## Fairness

`FairSpec` is weakly fair only to the scheduler: the tick, the running pass's
next step, and the clock elapsing. No scope, acknowledgement or peer outcome
is fair; the peer may refuse a live document forever. Discharge follows from
the cursor and the page alone.

## What it leaves out

- A marker for a live document that the peer keeps refusing, or for a peer that
  never comes back, is an obligation and is kept. Deleting the replicator is
  its terminal state.
- A collection deleted without truncating its documents leaves document
  mappings behind; their markers still fail on the missing collection.
- The clear is abstracted as erasing the peer's retry state. The code also
  requires the peer to hold no markers in the same transaction, which only
  narrows when it acts.
- The discharge check and the compare-delete are one step. In the code they are
  two transactions; a document created again between them registers a new
  generation, which the compare-delete preserves.
