# query-plan — formal-modelability survey

## Purpose
Query plan tree + planner + execution-boundary abstractions (extracted from `query`
per #670). Implements Volcano-iterator plan nodes (scan, select, limit, joins,
aggregates, cursor, mutation create/update/delete/upsert, permission/SE filters),
the planner (builder, index selection, mapping), and the read/write boundary traits
(`DocFetcher`, `DocMutator`). `txn/` holds plan-layer transaction primitives: the
`TransactionContext` trait, registry, and the **deferred-ACP-mutation overlay**.

## State machines
- **Deferred-ACP overlay** (`txn/context.rs`): an explicit txn's ACP registrations are
  its own writes in its ACP namespace (a local provider bound to the transaction, or the
  owed registration an external provider's create records). Access checks read that
  projection first (`check_doc_access_with_projection`), so a not-yet-committed
  registration already gates reads to the owner; the writes reach committed ACP state only
  when the storage txn commits. This is a two-phase (projected → committed) security
  state machine with isolation + commit-ordering concerns.
- **UpsertNode** (`plan/mutation/upsert.rs`): 0-match→create / 1-match→update / >1→error
  branching. Deterministic, single-shot; integration tests cover it.
- **PlanNode lifecycle** (init→start→next*→close): per-node iterator protocol; mechanical.

## Candidates
| name | kind | property | already-modeled | priority |
|------|------|----------|-----------------|----------|
| Deferred-ACP overlay consistency | TLA+ | Within a txn, a projected `Registered{owner}` gates reads to that owner and `Unregistered` applies the backend's public-access rule, exactly as the post-commit ACP state would; on commit the projection applies (a registered doc becomes ACP-registered); on rollback no ACP state changes; a concurrent txn never observes another's uncommitted projection. No reader gains access the committed state would deny (fail-closed across the projected→committed boundary). | `DeferredAcp` (Commits/Acp slices reference `check_doc_access_with_projection` only as the gate call-site) | medium |

The existing `DeferredAcp` model assumes a backend where unregistered objects are
public, as in the local and Cosmos implementations. Native Vera denies missing
registrations; the model does not establish that behavior. The Rust overlay reads
`DocumentACP::unregistered_documents_are_public()` and has separate regression
coverage for native denial and branchable collection grants.

## Verdict
**Model-worthy: yes — one medium candidate.** The deferred-ACP overlay is a genuine
security state machine (uncommitted projection that gates access + commit-time hook
application) not covered by the existing Commits (dual-path gating) or Acp (Zanzibar
soundness + revocation cache) slices, which both assume already-committed ACP state.
Everything else in the crate — Volcano plan nodes, upsert branching, planner/index
selection, fetcher/mutator boundary traits — is deterministic plumbing validated by
the integration suite; no model warranted.
