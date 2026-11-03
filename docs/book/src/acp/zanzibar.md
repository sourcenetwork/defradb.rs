# The Zanzibar engine

`crates/zanzibar` is a standalone implementation of Google's Zanzibar permission model,
and `crates/acp` builds DefraDB's access control on it. No global mediator, no special
cases in the query engine: authorization is evaluation of a policy over relationship
tuples.

## The model

Three nouns:

- **Policies** (`Policy`, `Resource`, `Relation`, `Subject`): a policy names resource
  types, relations on them, and permission expressions over those relations. DefraDB's
  DPI format is a small YAML dialect (`crates/acp/src/policy_yaml/`); the default
  document policy (used when a collection carries no explicit one) has an `owner`
  relation, an `admin` computed from `owner`, direct `reader`/`updater`/`deleter`
  relations, and `read`/`update`/`delete` permissions computed as unions over owner,
  admin and the direct relations.
- **Relationship tuples**: `(resource, relation, subject)` facts, stored under
  `a/zanzibar/` as Ordinary keys. Subjects can be actors, wildcards, or usersets
  (a subject that is itself a relation on another object).
- **Relation expressions**: the algebra evaluated to answer "may this identity perform
  this permission on this resource" (`crates/zanzibar/src/expression/`):
  `This` (a direct tuple), `ComputedUserset` (follow to another relation on the same
  resource), `TupleToUserset` (follow a tuple relation to a target, then evaluate a
  computed relation there), and the set operations `Union`, `Intersection`, `Difference`.

## Evaluation

The checker is a goal-tree search with cycle detection
([`crates/zanzibar/src/engine/evaluate.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/zanzibar/src/engine/evaluate.rs)):

- `Union` short-circuits on true; `Intersection` short-circuits on false;
- `Difference` requires the base to be *proven* true and the subtraction *proven*
  false; a cycle-truncated subtraction branch is indeterminate, and indeterminate fails
  closed (`Error::IndeterminateExclusion`), never silently grants;
- every check runs under explicit work limits (depth and step budgets) and a per-check
  memo cache with a charged budget, so a malicious policy cannot make evaluation
  unbounded;
- batch checks share one cache (`check_many`).

Policies are immutable once stored; the node keeps them in a lock-free policy snapshot
(an `Atom`-swapped lookup table, [`crates/acp/src/policy_cache.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/acp/src/policy_cache.rs)) that reads never lock
to consult.

## Policy identity

Policy IDs come from the engine's `Db::allocate` (the same atomic allocator as short
IDs), so two replicas allocate distinct policies without coordination, and a policy body
is content-checked: a different body under an existing ID is a typed error, not a
silent overwrite.

> **Why Zanzibar, and why fail-closed on indeterminate.** Relationship tuples express
> sharing graphs (owner grants editor on a doc to a group object) that boolean flags and
> ACL rows cannot, and the evaluation model is well-studied. The fail-closed rule is
> honesty applied to permissions: when the engine cannot *prove* absence (a cycle cut
> the search), it reports indeterminate, and indeterminate denies. A permission system
> that guesses is a vulnerability with good manners.
