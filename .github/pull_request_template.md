## Relevant issue(s)

Resolves #

## Description

(*replace*) Summarize the change, its motivation and context, and any new dependencies it needs.
Every PR needs an issue that describes the problem simply enough for a human to understand. Create one if none exists.

## Tasks

- [ ] The PR title follows the conventional style checked by [validate-title.sh](https://github.com/sourcenetwork/defradb.rs/blob/main/.github/scripts/validate-title.sh): `label(scope)!: Capitalized summary`, 60 characters or fewer.
- [ ] `cargo nextest run`, `cargo test --doc`, `cargo clippy --all --all-targets -- -D warnings` and `cargo fmt --all` pass.
- [ ] If this touches core behavior, `cargo nextest run -p integration-test` passes.
- [ ] Go parity impact is considered (see `CLAUDE.md`).
- [ ] Limitations are discussed: threats to validity, misuse, broken assumptions, resource requirements, ...

## How has this been tested?

(*replace*) Describe the tests that verify the change and how to reproduce them.
Show the evidence where you can: a link to a workflow run, or the output of the local run.

Platform(s) tested on:
- *(modify the list accordingly)*
- Linux
- macOS
