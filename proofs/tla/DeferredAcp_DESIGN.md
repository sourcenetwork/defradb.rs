# DeferredAcp — Deferred-ACP overlay consistency (TLA+)

Backlog item #2 (`proofs/README.md` Coverage map / `survey/query-plan.md`): *Deferred-ACP
overlay consistency — a txn-local ACP projection gates reads exactly as the committed state
would; fail-closed across commit/rollback.*

This slice models the **deferred-ACP overlay**: the ACP registrations an explicit DefraDB
transaction has written but not committed. They are the transaction's own writes in its ACP
namespace. A local provider joins the transaction (`DocumentACP::bind_transaction`) and
registers through its view; an external provider cannot, so the create records the
registration it owes in the same view (`PendingRegistrations::record`) and
`AcpRegistrationProjection` in `crates/engine/query/src/txn/context.rs` reads it back. Access
checks within the txn consult the projection FIRST (`check_doc_access_with_projection`);
the writes reach committed ACP state only when the storage txn commits. What happens to an
owed registration after the commit (settling it with the provider) is `AcpOutbox.tla`.

It is **distinct from the Acp and Commits slices**, which both assume an *already-committed*
ACP state (Zanzibar soundness + revocation cache; dual-path commit gating). This slice is the
only one that models the **uncommitted projection → committed transition** itself: isolation
between concurrent txns, atomic commit-time hook application, no-op rollback, and the
fail-closed gate across the projected→committed boundary.

The model assumes public access to unregistered objects, matching the local
backend. Rust now obtains that rule from
`DocumentACP::unregistered_documents_are_public()`. Native Vera returns `false`;
its missing-registration behavior is covered by Rust and integration tests, not
by this model. Branchable collection inheritance is also outside this slice.

## Property

The model proves, over all interleavings of **two concurrent txns** doing register /
unregister / read / commit / rollback:

| Invariant | Statement |
|---|---|
| `INV_FailClosedActive` | **Headline.** Every read the overlay GRANTED is, at the instant of the grant, also granted by the ground-truth ACP rule over the committed state that *this txn's own commit would produce* (`committed (+) proj[t]`). An in-flight projection never grants access the committed state would deny. |
| `INV_NoCrossTxnLeak` | **Isolation.** A read granted when the txn had NO projection of its own for the doc must be granted by the BARE committed state — never justified by a *sibling* txn's uncommitted projection. |
| `INV_RollbackNoOp` | **Atomicity / abort.** A rolled-back txn changes committed ACP not at all (ghost bit `rbDirtied`, set exactly when a rollback path mutates `committed`, stays FALSE). |
| `INV_TypeOK` | type invariant. |

### Independent oracle (why GREEN is not vacuous)

Correctness is judged from `committed` — the actually-committed ACP registration state — via
`Grant(state, d, r)`, the modeled backend's ACP rule (`Unregistered ⇒ anyone`; `Registered{o} ⇒
only the authenticated identity o`). This is **not** the overlay's own decision
(`OverlayGrant`). The `Read` action records, for each granted read, the *oracle* verdict
`Grant(ProspectiveCommitted(t), d, r)` computed by the independent `Grant`. `ProspectiveCommitted(t)`
is `committed` overlaid with **t's own projection only** — exactly what t's commit hooks
produce — so a buggy overlay that grants more than ground truth cannot make the invariant
vacuously true. The `RollbackNoOp` ghost bit is set by the `Rollback` action itself
independently of who rolled back or what they projected.

**Vacuity self-check (run, observed):** three negated reachability probes against the GREEN
config are each reported **violated** by TLC — i.e. GREEN actually reaches the interesting
states, so the invariants hold over a non-trivial space:

- `NoProjectedReadGranted` violated → a read is actually granted through a projection (or by a
  committed txn), not only by the committed fall-through.
- `NoCommittedChange` violated → a commit takes real effect (`committed` changes from init).
- `NotBothFinished` violated → both txns reach terminal (committed/rolledback) states.

## Source anchors

### Rust (this repo) — the mechanism under test

| Symbol in model | Code | Anchor |
|---|---|---|
| `Reg(o)` (a projected registration and its owner) | `PendingRegistration { creator, .. }` for an external provider; the bound provider's own owner record for a local one | `crates/engine/acp/src/pending.rs` |
| `proj[t]` (the txn's own ACP writes) | the transaction's ACP namespace view, read by `PendingRegistrations::get` | `crates/engine/acp/src/pending.rs`; `DbTxn::acpstore` in `crates/database/db/src/txn/mod.rs` |
| one projection per txn (isolation by construction) | `DbTxn::acp_registration_projection` builds it over the txn's own view | `crates/database/db/src/txn/mod.rs` |
| `proj[t][d] := Reg(o)` (`Register` action) | `register_created_doc`: `bind_transaction` then `register_doc_object`, or `PendingRegistrations::record` | `crates/database/db/src/write/autocommit/helpers.rs` |
| `proj[t][d] := Unreg` (`Unregister` action) | `unregister_deleted_doc`: `unregister_doc_object` through the bound provider, or `PendingRegistrations::clear` | `crates/database/db/src/write/autocommit/helpers.rs` |
| `EffectiveProj` / `OverlayGrant` (the gate: projection-first, else committed) | `check_doc_access_with_projection`, `is_doc_registered_with_projection` | `crates/engine/query/src/txn/context.rs` |
| `OwnerCheck="Strict"` (Registered grants only to owner) | `is_creator` | `crates/engine/query/src/txn/context.rs` |
| projected `Unregistered ⇒ open` for the modeled backend | `acp.unregistered_documents_are_public()` | `crates/engine/query/src/txn/context.rs` |
| `Commit` applies the projection atomically (one action) | the storage transaction's commit publishes its ACP-namespace writes with its rows | `DbTxn::commit` in `crates/database/db/src/txn/mod.rs` |
| `Rollback` changes nothing | the writes are the transaction's own, dropped by `DbTxn::discard`; the projection holds only a weak view (`PendingRegistrations::active_view`) | `crates/database/db/src/txn/mod.rs`; `crates/engine/acp/src/pending.rs` |

### Go (origin/develop — the live upstream, not the stale checkout)

The Rust deferred overlay is a **Rust-specific** mechanism; Go has no txn-local projection that
gates reads before commit. In Go, document registration runs **synchronously** against the live
document-ACP, and access checks read the live ACP directly:

| Concept | Anchor |
|---|---|
| `registerDocWithACP` calls `RegisterDocObject` synchronously (no projection/overlay) | `internal/db/collection_acp.go:31-46` |
| `RegisterDocOnCollectionWithDocumentACP` → `documentACP.RegisterDocObject(...)` directly | `internal/db/acp/register.go:23-51` |
| `checkAccessOfDocWithACP` reads live ACP (`CheckAccessOfDocOnCollectionWithACP`) — no projection | `internal/db/collection_acp.go:48-72` |
| Go's txn-local staging analog: `txn.OnSuccess(...)` deferred side-effects (e.g. `doc.Clean()`) wait for commit success, mirroring the same "delay state side-effects until commit" discipline the Rust hooks follow | `internal/db/document.go:506-507` |

**Audit note (for `proofs/README.md` Boundaries):** the Rust overlay's *value-add* over Go is
that, within an explicit txn, a just-created doc's reads are gated by the projection *before*
the real ACP write lands at commit. The committed-state oracle in this slice is exactly what
Go computes synchronously, so the model's GREEN claim — "projection gates as committed would" —
is the Rust mechanism reproducing Go's semantics one phase early, fail-closed. The hazards the
RED variants expose (shared overlay across txns, hooks on rollback, owner-check bypass) are
Rust-implementation hazards with no Go counterpart, which is precisely why they are worth a
model.

## Scenarios (red/green)

| Config | Knobs | Verdict | Mechanism toggled |
|---|---|---|---|
| `MC_DeferredAcp_Green` | `PerTxn / NoHooks / Strict` | **GREEN — all hold** | correct mechanism |
| `MC_DeferredAcp_Red_SharedOverlay` | `Shared / NoHooks / Strict` | **RED** — `INV_FailClosedActive` *and* `INV_NoCrossTxnLeak` violated | one projection shared by both txns, not one view each; txn B reads off txn A's uncommitted projection → access B's own commit would never grant |
| `MC_DeferredAcp_Red_RollbackHooks` | `PerTxn / RunHooks / Strict` | **RED** — `INV_RollbackNoOp` violated | rollback applies the projected writes (as if a discard committed them) → committed ACP mutated though the txn aborted |
| `MC_DeferredAcp_Red_OwnerBypass` | `PerTxn / NoHooks / Any` | **RED** — `INV_FailClosedActive` violated | overlay grants a projected `Registered{owner}` to ANY authenticated identity (the `is_creator` check dropped) → a stranger reads what committed denies |

The `SharedOverlay` case violates `INV_NoCrossTxnLeak` even checked alone (verified) — the
dedicated isolation invariant is not subsumed by the headline; it pins the cross-txn-leak
property by name.

## Run / verify

```bash
cd proofs/tla
export JAVA=/opt/homebrew/opt/openjdk/bin/java   # or any JDK 11+; the tlc wrapper also honors JAVA_HOME
./tools/tlc -metadir states/b_dacp_green -config MC_DeferredAcp_Green.cfg              MC_DeferredAcp_Green.tla
./tools/tlc -metadir states/b_dacp_so    -config MC_DeferredAcp_Red_SharedOverlay.cfg MC_DeferredAcp_Red_SharedOverlay.tla
./tools/tlc -metadir states/b_dacp_rb    -config MC_DeferredAcp_Red_RollbackHooks.cfg MC_DeferredAcp_Red_RollbackHooks.tla
./tools/tlc -metadir states/b_dacp_ob    -config MC_DeferredAcp_Red_OwnerBypass.cfg   MC_DeferredAcp_Red_OwnerBypass.tla
```

Observed: GREEN reports "No error has been found" (120420 distinct states). Each RED reports
the named invariant "is violated" with a concrete two-txn counterexample trace.

## Boundaries

- **Bounded instance:** 2 txns, 1 doc, 2 candidate owners + the anonymous principal, ≤2
  projection ops per txn. The doc starts committed-Registered to `u1` (genuinely protected),
  so any over-grant to `u2`/anon or any cross-txn leak is a real violation. This is the minimal
  witnessing shape; conclusions are structural (the gate, the isolation boundary, the
  commit-vs-rollback split), not quantity-sensitive.
- **Abstracted:** the actual Zanzibar/`DocumentACP` relation evaluation is collapsed into the
  `Grant` rule for public-absence backends (Unregistered ⇒ anyone; Registered{owner} ⇒ owner-only). Native Vera's deny-on-absence behavior and branchable collection inheritance are not modeled. Multi-relation policies,
  and the bearer token a settlement runs under
  are out of scope — modeled as an atomic, total commit application. The relation engine itself
  is the Acp slice's job and is exercised by `--test acp`.
- **Modeled as atomic:** commit applies the transaction-local projection in one TLA+ action.
  Storage transaction atomicity and isolation are assumed engine boundaries, not discharged
  by this model. Settling an owed registration with a remote provider after durability is
  `AcpOutbox.tla`. No atomic remote application is claimed.
- **Out of scope here:** the per-request identity/JWT authentication that produces
  `Identity::Authenticated(did)` (the Auth/Jwt slices); the dual-path User-vs-Commits gating
  (the Commits slice). This slice assumes those gates and models only the deferred overlay on
  top of committed ACP.
- **Integrator TODO:** no automated model↔code conformance harness yet (repo-wide policy,
  `proofs/README.md` "Model ≠ code"). Keep anchors in step manually if `txn/context.rs` changes
  the projection-first gate or the owner check, or if `DbTxn::acp_registration_projection`
  stops reading through the transaction's own view.
- **Integrator TODO (run-all):** the four `MC_DeferredAcp_*` configs below
  must run in `proofs/tla/run-all.sh` (it now picks up every top-level
  `*.cfg`, taking the module and verdict from each cfg's `\* run-all:` header
  or its name, so no row is added to the script itself):

```
  "MC_DeferredAcp_Green.cfg              MC_DeferredAcp_Green.tla              GREEN"  # deferred-acp overlay correct
  "MC_DeferredAcp_Red_SharedOverlay.cfg  MC_DeferredAcp_Red_SharedOverlay.tla  RED"   # deferred-acp: cross-txn projection leak
  "MC_DeferredAcp_Red_RollbackHooks.cfg  MC_DeferredAcp_Red_RollbackHooks.tla  RED"   # deferred-acp: hooks run on rollback
  "MC_DeferredAcp_Red_OwnerBypass.cfg    MC_DeferredAcp_Red_OwnerBypass.tla    RED"   # deferred-acp: projected owner-check bypass
```
