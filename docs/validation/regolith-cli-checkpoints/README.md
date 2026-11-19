# Independent CLI checkpoint validation

All ten checkpoints below the proof PR passed a fresh CLI build, external basic
(56), ACP (204; 16 ignored), encryption (25), and counter conformance (9; 8 ignored).
`manifest.json` records the actual tested source, CLI SHA-256, pinned Go revision,
raw compressed logs, and corresponding stack revision after the test-only restack.
Each log begins with its tested revision. Checksums cover compressed and original logs.

The reported source differences are the two schema test files fixed after CI exposed
a preexisting fixture collision. At the mechanisms checkpoint, there is also the
removal of an unused `BTreeMap` import in `crates/database/db/src/txn/context.rs`; no runtime
behavior changes. The intermediate lock-removal history repairs preserve its final
implementation. These checks are not claimed to rerun after documentation or test-only changes.

`run-validation.py` records the original commands and local paths; the interrupted
initial run was resumed for the final three revisions in the manifest. Adapt paths
and supply the exact intended revisions when reproducing. Performance and remote CI
are separate gates documented in the handoff.
