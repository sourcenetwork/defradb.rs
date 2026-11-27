# The Zanzibar engine

`crates/building-blocks/zanzibar` is a standalone implementation of Google's Zanzibar permission model,
and `crates/engine/acp` builds DefraDB's access control on it. No global mediator, no special
cases in the query engine: authorization is evaluation of a policy over relationship
tuples.

## The model

Three nouns:

- **Policies** (`Policy`, `Resource`, `Relation`, `Subject`): a policy names resource
  types, relations on them, and permission expressions over those relations. Policies
  are written in acp_core's YAML format, parsed and checked by
  `crates/engine/acp/src/policy_yaml/` ([Writing a policy](#writing-a-policy) below).
- **Relationship tuples**: `(resource, relation, subject)` facts, stored under
  `a/zanzibar/` as Ordinary keys. Subjects can be actors, wildcards, or usersets
  (a subject that is itself a relation on another object).
- **Relation expressions**: the algebra evaluated to answer "may this identity perform
  this permission on this resource" (`crates/building-blocks/zanzibar/src/expression/`):
  `This` (a direct tuple), `ComputedUserset` (follow to another relation on the same
  resource), `TupleToUserset` (follow a tuple relation to a target, then evaluate a
  computed relation there), and the set operations `Union`, `Intersection`, `Difference`.

## Writing a policy

A DAC policy is acp_core's YAML: a `name`, and `resources`, each with `relations` (who
can be named, by `types`) and `permissions` (an `expr` over the resource's relations and
permissions). The rules, as `crates/engine/acp/src/policy_yaml/` applies them:

- **`owner` is implicit and reserved.** Every resource gets an `owner` relation, held by
  the identity that creates the document. A policy that declares a relation named `owner`
  is refused (``'owner` is a reserved relation name``).
- **The owner holds every permission.** Each permission's expression is joined to
  `owner` by union, and a permission with no `expr` grants the owner alone. An expression
  that names the resource's own `owner` is refused (``permission cannot reference `owner`
  relation``, `go.acp.dac.policy.invalid`); `parent->owner`, the owner of the object
  `parent` names, is accepted, as in acp_core. No expression can take a permission away
  from the owner: `reader - blocked` means `owner + (reader - blocked)`.
- **`spec: defra`.** A policy that sets it must give every resource `read` and `write`
  permissions, and its `read` is joined to `write`: whoever holds `write` reads too, so
  there a `read` with no `expr` grants the owner and the `write` holders.
- **Operators.** `+` is union, `-` difference, `&` intersection, and `rel->perm` follows
  relation `rel` to the object it names and checks `perm` there. They have equal
  precedence and apply left to right; parentheses group. This is the grammar of
  acp_core's `PermissionExpr.g4`, the parser Go uses.
- **Relation types.** `actor` admits any actor DID or the all-actors wildcard `*`,
  `actor:individual` one DID only, a resource name an object of that resource (the target
  of `rel->perm`), and `resource->relation` a userset.
- **What a collection needs.** `@policy(id, resource)` accepts a resource that has `read`,
  `update` and `delete` permissions (`go.acp.dac.link.accepted`,
  `go.acp.dac.link.bad-policy`). Whoever can update or delete a document can also read
  it (`DocumentPermission::implies_read_permissions`).
- **`manages`.** A relation's `manages: [rel, ...]` lets its holders add and remove tuples
  of those relations; the owner may always do so, and nobody else may
  (`PolicyResource::get_managers_for_relation`, `go.acp.dac.relationship.manager`).
  Managing a relation grants no read, update or delete by itself. `owner` cannot be
  managed.
- **`create`.** A resource's `create: {parent: <relation>, requires: <permission>}` admits
  a new document only if its creator holds `requires` on the parent object it names. Its
  collection must then pass `parentField` in `@policy`, and only then
  (`defra.acp.parent_registration`; [Parent-authorized document
  creation](transactional.md#parent-authorized-document-creation)).

An example, in which a file is readable by its readers, its editors and the readers of
its directory, except anyone blocked:

```yaml
name: filesystem
resources:
  - name: directory
    relations:
      - name: reader
        types: [actor]
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
  - name: file
    relations:
      - name: reader
        types: [actor]
      - name: editor
        types: [actor]
      - name: blocked
        types: [actor]
      - name: parent
        types: [directory]
    permissions:
      - name: read
        expr: (reader + editor + parent->read) - blocked
      - name: update
        expr: editor - blocked
      - name: delete
```

With collections `type Directory @policy(id: "<id>", resource: "directory")` and
`type File @policy(id: "<id>", resource: "file")`, the `parent` edge is a relationship
whose target names the directory: `defradb client acp document relationship add
--collection File --docID <file> --relation parent --actor directory:<directory>`. A
reader of that directory then reads the file, until they are added to `blocked`; the
owner still reads it if added to `blocked` too.

## Evaluation

The checker is a goal-tree search with cycle detection
([`crates/building-blocks/zanzibar/src/engine/evaluate.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/building-blocks/zanzibar/src/engine/evaluate.rs)):

- `Union` short-circuits on true; `Intersection` short-circuits on false;
- `Difference` requires the base to be *proven* true and the subtraction *proven*
  false; a cycle-truncated subtraction branch is indeterminate, and indeterminate fails
  closed (`Error::IndeterminateExclusion`), never silently grants;
- every check runs under explicit work limits (depth and step budgets) and a per-check
  memo cache with a charged budget, so a malicious policy cannot make evaluation
  unbounded;
- batch checks share one cache (`check_many`).

Policies are immutable once stored; the node keeps them in a lock-free policy snapshot
(an `Atom`-swapped lookup table, [`crates/engine/acp/src/policy_cache.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/engine/acp/src/policy_cache.rs)) that reads never lock
to consult.

## Policy identity

A local ACP names a policy as Go's acp_core does: a SHA-256 of the policy's name,
resources, relations and permissions, hashed again with a counter
(`policy_yaml::generate_policy_id`). The counter comes from the engine's `Db::allocate`
(the same atomic allocator as short IDs, `PersistentZanzibarStore::next_policy_counter`),
so two adds of one body get two IDs (`go.acp.dac.policy.ids`). A policy that uses
`actor:individual` or a `create` rule, which acp_core cannot express, also hashes those
(under `defra/individual-actor/v1` and `defra/parent-registration/v1`), so its ID is not
one acp_core would compute. Vera names its policies
itself ([Vera](vera.md)). A policy body is content-checked: a different body under an
existing ID is a typed error (`a different policy already exists under ID ...`), not a
silent overwrite.

> **Why Zanzibar, and why fail-closed on indeterminate.** Relationship tuples express
> sharing graphs (owner grants editor on a doc to a group object) that boolean flags and
> ACL rows cannot, and the evaluation model is well-studied. The fail-closed rule is
> honesty applied to permissions: when the engine cannot *prove* absence (a cycle cut
> the search), it reports indeterminate, and indeterminate denies. A permission system
> that guesses is a vulnerability with good manners.
