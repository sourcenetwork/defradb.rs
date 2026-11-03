# Keys, families and classes

Every byte defradb.rs writes has a family, and every family has a class. The class decides
how the engine treats the key: whether writes to it conflict, whether reads of it are
validated, whether identical bytes are the same write. This chapter is the complete map.

## The key classes

[regolith](https://sourcenetwork.github.io/regolith/) defines four behavioural classes; defradb.rs assigns each key family to one by
implementing `KeyClassifier` ([`crates/storage/src/backends/regolith/classifier.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/classifier.rs)):

- **Ordinary**: last-writer-wins keys. A read of an Ordinary key inside a write
  transaction is validated at commit (an identical rewrite is not a change), and two
  different puts of the same key race; one wins.
- **ContentAddressed**: the key *is* the content's hash (a CID). Identical writes are
  literally the same bytes, so both commit; different bytes under one block key are
  rejected. A read that found the key is validated only for removal.
- **Log**: a commit-ordered, engine-assigned append-only sequence. Writes never conflict;
  the engine assigns positions in commit order (the arrival journal).
- **CommutativePrefix**: keys whose writes commute within a prefix, so a scan of the
  prefix is not recorded as a validated range. (defradb.rs's families are classified
  Ordinary, ContentAddressed or Log today; the class exists in the engine's vocabulary
  and appears in the read rules.)

The classifier is one lookup: it finds the key's family in a static table
(`families::family_of`) and maps the family's `Class` to the engine class. A
`ContentAddressed` family key that does not parse as a CID falls back to Ordinary; a key
of no family, cut short or malformed, stays Ordinary. Unknown keys are never rejected:
they are simply ordinary.

## The family table

The namespace byte comes first (`d` datastore, `b` blockstore, `h` headstore, `s`
systemstore, `p` peerstore, `e` encstore, `a` acpstore; [`crates/storage/src/namespace.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/namespace.rs)),
then the family prefix. First match wins; longer prefixes sort before shorter ones
([`crates/storage/src/backends/regolith/families.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/families.rs)):

| Prefix | Class | What lives there |
|---|---|---|
| `bm` | Ordinary | block merge markers |
| `b` | ContentAddressed | blocks, keyed by CID |
| `s/arrival/` | Log | arrival journal: head, rows, once keys |
| `h/hd/` | Ordinary | document head sets |
| `h/hc/` | Ordinary | collection head sets |
| `h/rc/` | Ordinary | collection block publication guards |
| `h/p/` | Ordinary | commit priority index, per document |
| `h/r/` | Ordinary | earlier publication guards (dropped by the head-set migration) |
| `d/d/` | Ordinary | document values |
| `d/del/` | Ordinary | deletion markers |
| `d/v/` | Ordinary | schema versions |
| `d/` | Ordinary | index entries and field values |
| `a/zanzibar/` | Ordinary | Zanzibar relationship tuples |
| `a/acp/` | Ordinary | the earlier ACP store |

## Why head sets are Ordinary, not special

A head set (the set of a document's current DAG heads) lives under one Ordinary key per
document, but its *writes* are blind merges of a set operand, and its *reads* are
`get_parts(key, &[])`, which validates nothing. The class of a key is the engine's
default behaviour; the merge operand and the parts mechanism narrow it to exactly what the
decision needs. This is the general pattern: the class sets the ceiling of strictness,
the mechanism picks the floor.

## What the classes buy

The classifier's tests demonstrate the contract in four sentences
(`classifier/tests.rs`):

- Two identical ContentAddressed block writes both commit: same CID, same bytes, nothing
  to race.
- Two different decisions on one Ordinary key race; the second commit gets a typed
  `Conflict`.
- A decision that read a block (found it) conflicts with a later delete of that block:
  the read is validated for removal only.
- Disjoint-part document writes never meet: the operand's `touches` says which parts it
  changed, and a projected read is validated only against those.

> **Why one static table.** Classification is on the hot path of every write and every
> validated read. One table, ordered longest-prefix-first, is one binary search over a
> handful of entries, and a 256-entry first-byte index jumps straight to the candidates
> ([`crates/storage/src/backends/regolith/families.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/families.rs)). It also makes the layout auditable: this page is the complete list of
> what defradb.rs stores.
