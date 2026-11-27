# Survey: `crates/engine/vera/`

## Purpose
Vera ACP client: implements `DocumentACP`/`VeraProvider` against native vera.rs. Builds and
signs requests, queries policies/relationships, and gates document access. Keeps no
access-decision cache of its own: a decision is reused only under the verified state it was
read at, through the proof-validated light-client cache invalidated by chain height /
module-state-root advance.

## State machines
- **Remote decisions** (`decisions.rs` + `dac.rs::check_doc_access`): one call per decision
  tuple in flight; only a decision keyed by a verified state is reused, every other is asked
  again.
- **Light-client cache invalidation** (`vera_rs/provider.rs::run_light_client_observer`): on
  `module_state_root` change at a new height, `invalidate_stale` + publish events. Core proof
  logic lives in the external `acp-light-client` git dep (assumed boundary).
- EVM nonce reservation (`reserve_nonce_at_or_after`): CAS loop — plumbing.

## Candidates
| name | kind | property | already-modeled | priority |
|---|---|---|---|---|
| Light-client proof cache | TLA+/Lean | cache entry trusted only if validated against current module-state-root | no (logic in external `acp-light-client` dep) | low |

## Verdict
Mostly **plumbing/glue**: tx building, ABI/JSON (de)serialization, REST/RPC IO, bearer-token
construction. Revocation consistency is covered by the Acp slice; with no decision cache
here there is no stale positive answer to model. Unavailable ⇒ fail-closed is an assumed
boundary in the Acp/Auth models. Light-client proof validation is an external-dependency boundary.

**model_worthy: false** — no high/medium candidate that isn't already covered or external.
