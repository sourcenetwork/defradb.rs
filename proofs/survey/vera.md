# Survey: `crates/engine/vera/`

## Purpose
Vera ACP client: implements `DocumentACP`/`VeraProvider` against native vera.rs. Builds and
signs requests, queries policies/relationships, and gates document access. Adds an optional
access-decision cache and a proof-validated light-client cache invalidated by chain
height / module-state-root advance.

## State machines
- **Access-decision cache lifecycle** (`access_cache.rs` + `dac.rs::check_access`): TTL expiry +
  eager `invalidate_policy` on every set/delete relationship; only *positive* decisions cached.
- **Light-client cache invalidation** (`vera_rs/provider.rs::run_light_client_observer`): on
  `module_state_root` change at a new height, `invalidate_stale` + publish events. Core proof
  logic lives in the external `acp-light-client` git dep (assumed boundary).
- EVM nonce reservation (`reserve_nonce_at_or_after`): CAS loop — plumbing.

## Candidates
| name | kind | property | already-modeled | priority |
|---|---|---|---|---|
| Access-cache stale-revocation | TLA+ | a revoked positive decision is never served from cache after relationship mutation | YES — `MC_Acp_Green` / `MC_Acp_StaleCache_Red` model `access_cache.rs` exactly | n/a |
| Light-client proof cache | TLA+/Lean | cache entry trusted only if validated against current module-state-root | no (logic in external `acp-light-client` dep) | low |

## Verdict
Mostly **plumbing/glue**: tx building, ABI/JSON (de)serialization, REST/RPC IO, bearer-token
construction. The one security-critical state machine, the access-decision cache under
revocation, is **already proven** by the existing Acp slice (which cites this crate's
`access_cache.rs` line-by-line). Unavailable ⇒ fail-closed is an assumed boundary in
the Acp/Auth models. Light-client proof validation is an external-dependency boundary.

**model_worthy: false** — no high/medium candidate that isn't already covered or external.
