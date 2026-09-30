## Relevant issue(s)

Resolves #

## Description

(*replace*) Summarize the change, its motivation and context, and any new dependencies it needs.
Create an issue for it if none exists.

## Tasks

- [ ] The PR title follows the conventional style checked by [validate-title.sh](.github/scripts/validate-title.sh): `label(scope)!: Capitalized summary`, 60 characters or fewer.
- [ ] `cargo test`, `cargo clippy --all --all-targets -- -D warnings` and `cargo fmt --all` pass.
- [ ] If this touches core behavior, `cargo test -p integration-test` passes.
- [ ] Go parity impact is considered (see `CLAUDE.md`).
- [ ] Limitations are discussed: threats to validity, misuse, broken assumptions, resource requirements, ...

## How has this been tested?

(*replace*) Describe the tests that verify the change and how to reproduce them.

Platform(s) tested on:
- *(modify the list accordingly)*
- Linux
- macOS
