# Requirements

This folder describes what DefraDB is supposed to do, written as plain sentences. Each sentence is a requirement: one behaviour, small enough to check with a test. Tests say which requirement they check, so we can see at any time which behaviours are tested and which aren't.

## Why we keep them

They do two jobs.

**They're how we finish the move from Go.** The Rust version has to behave like the Go one before the team can switch. Go's integration tests define that behaviour, but there are thousands of them, and a test name doesn't tell you what it's checking. So every Go test is written up here as the behaviour it proves. That gives us one list of everything DefraDB must do, and a way to see how much of it the Rust code already does.

**They're how we say what a piece of work has to do.** This job doesn't end with the move. More and more of our code is written with AI agents, and an agent does what it's told, so a vague task gets a vague result. A requirement says precisely what done means: what goes in, what comes out, and the exact text of every error. It's checked by a test, so there's no arguing about whether the work is finished. When you hand a task to an agent, or pick one up yourself, the requirement is the brief.

The same list helps people too. A reviewer can see what a change is meant to do, and a newcomer can learn what the database promises without reading its tests.

## Working from requirements

Whether you're a person or an agent, start a task here, before you touch the code:

1. **Find the requirements the task touches.** Search this folder for the feature, or follow the IDs on the tests near the code you're changing.

2. **Write down what should happen.** Edit those requirements, or add new ones, until they describe the behaviour you want. That includes the error cases. Anyone who reads them should know exactly what to build.

3. **Write or update the tests** and tag them with the requirement IDs.

4. **Then change the code** until those tests pass.

The work is done when every requirement it touches is true and has a passing test. To find work that still needs doing, look for requirements without tests: the coverage report below lists them.

## What a requirement looks like

```markdown
# `go.collection_version.define.default_values`: `@default` field values

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/with_default_fields_test.go::TestCollectionVersion_WithDefaultFieldValues", …]

`@default(<kind>: value)` on a scalar field records the default in the collection definition.
A value of the wrong kind fails with `default value is invalid. Field: active, Expected: Boolean, …`.
```

The heading holds the requirement's ID and a short title. The ID is a dotted path, read from general to specific. `go.collection_version.define.default_values` is one part of `go.collection_version.define`, which is one part of `go.collection_version`. The `Parents` line records that link.

Only the requirements at the end of a path (the leaves) are checked by tests. The ones above them just group related behaviour.

The paragraph underneath is the requirement itself. It describes what someone using DefraDB would see: what they send, what comes back, and the exact error text when something is refused. It doesn't describe how the code works inside, so it stays true when the code is rewritten.

Below the heading are a few optional lines:

- **`Go-Tests`** lists the Go tests that prove the behaviour.
- **`Scenarios`** lists our own tests that prove it, once it has some.
- **`Go-Bug-Suspect`** warns that a Go test may be checking a bug. We don't copy Go's bugs. Work out what the right behaviour is and write that down.
- **`Go-Ref`** names the Go branch and commit a requirement is checked against when that isn't `develop` at `root.md`'s commit. `go.shinzo` leaves carry `"shinzo-develop 2a899b860"` and run under `DEFRA_CLIENT=go-shinzo`.
- **`Interop`** marks a wire format a Rust node shares with Go nodes, and says what must match byte for byte.

## The folders

- **`go/`** holds requirements written from Go's integration tests, each listing its Go tests. `root.md` says which Go commit they were taken from. The `shinzo*.md` files hold the `go.shinzo` family, written from the tests Go's `shinzo-develop` branch adds, unit tests included; `shinzo.md` names the commits.

- **`defra/`** holds requirements our own tests already cover. A requirement moves here from `go/` when its tests pass. It keeps its ID, and its `Go-Tests` line becomes a `Scenarios` line. New behaviour that only the Rust version has gets its requirement here from the start.

- **`not-applying/`** lists Go tests that have no requirement because they only check a Go-specific mechanism (a Badger transaction size limit, goroutine plumbing, Go's test harness). Each row names the test and says in one line why defradb.rs doesn't need it. A Go test is either cited by a requirement or listed here, never both.

- **`go-gaps/`** is a one-time audit that compared every `go/` requirement with the Rust tests we had when it was written. Each entry says whether the behaviour was `covered`, `partial`, `missing`, or `diverges` (Rust deliberately does something else), which tests showed it, and what was left unchecked. It's a good place to look for a test that still needs writing.

## Tying a test to a requirement

A test names the requirement it checks with an attribute. The crate the test is in needs `mantra-macros` as a dependency (`mantra-macros = "0.9"` under `[dev-dependencies]`):

```rust
#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_default_field_values() { … }
```

A requirement's ID never changes, so this tag keeps working when the requirement moves from `go/` to `defra/`.

## Keeping them up to date

These files are only useful if they match what the code does today. Update them in the same pull request as the code:

- **You changed how something behaves:** edit the requirement's description, and update the tests that check it.

- **You added something new:** write a requirement for it in `defra/`, and tag the new tests with its ID.

- **You removed something:** delete its requirement, and remove the tag from any tests that still carry it.

- **Your tests now cover a `go/` requirement:** move it into `defra/`. Swap its `Go-Tests` line for a `Scenarios` line that lists your tests, and mark its `go-gaps/` entry as covered.

A test with no requirement, or a requirement with no test, is a gap. Fill it, or say in the pull request why it's fine to leave.

## Seeing what's covered

The requirements are read by [mantra](https://github.com/mhatzl/mantra), a requirements-tracing tool. `mantra.json5` at the repo root tells it where to look. To see which requirements have tests:

```bash
cargo install mantra                                    # once
mantra collect                                          # reads the requirements and test tags into mantra.db
mantra report --formats html --output-dir mantra-report # writes the report; open mantra-report/index.html
```

`mantra.db` and `mantra-report/` are ignored by git.

`mantra.json5` reads the requirements in `go/` and `defra/` and the `req_test` tags under `tools/integration-test/`. It doesn't read test results yet: mantra takes them as nextest's JUnit file together with a Cobertura coverage file, and nothing produces the coverage file today. Until it does, the report shows which requirements have tagged tests, not whether those tests pass.
