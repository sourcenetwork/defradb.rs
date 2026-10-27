# Prepared external inputs across write retries

`crates/query/src/prepared.rs` keys resolved provider answers by the exact
request context; `crates/db/src/write/autocommit/{create,update,delete}.rs`
(with the create pipeline in `crates/db/src/write/create.rs`) turns
`NeedsInput` into an attempt abort, resolves it after the engine releases
the transaction, then re-enters. `Store::transact_retry` retries commit
conflicts inside the attempt owner. The model checks that a conflict retains a
matching memo and that a changed context requires a new answer. RED variants
clear the memo on conflict or permit the provider action inside the active
attempt.

This is a control-flow abstraction. It does not prove provider response
correctness, signature validity, KMS policy, or every external check callsite.
Those remain implementation audit and focused-test obligations.
