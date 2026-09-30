# GovernanceHeads: the blocks a validator never sees (TLA+ design)

`GovernanceMerge.tla` is the governed merge path for composites: verdict, defer,
re-drive, sweep. Two other block kinds reach a governed collection and the
validator judges neither directly. A **collection block** installs a head over
the composites it links. A **definition** stores a version over the one it
patches. What each may do rests on verdicts already taken on the composites and
versions it names, so the question this module asks is whether the host reads
those verdicts correctly, and whether a block that arrives before what it names
is ever lost.

| Module | Asks |
|---|---|
| `GovernanceHeads.tla` | is a head ever installed over a composite no verdict accepted; is a patch that arrives before its base ever lost |
| `MC_GovernanceHeads_*` | today's ordering, the ordering before it, and the terminal drop before it |

## Source anchors

`crates/db/src/merge/` unless said otherwise.

- `governance/collection_block.rs:21` `CollectionBlockVerdict`: a block's verdict
  is derived from its links, installed when every linked composite and every
  parent has merged, rejected when one is known rejected, otherwise deferred on
  each missing CID. `:78` and `:94`: "merged" is the in-process merged set or the
  blockstore's durable **merged mark**. `BlockVerdict`, `PushBlock`,
  `RedriveBlock`.
- `merge_handler/composite.rs:252`: the replicated-write-into-a-downsample-source
  check, and `:269` `judge_governed`. Before this change the check came first and
  answered with a terminal skip. `PushWrite` under `SkipBeforeJudge`.
- `crates/p2p/src/sync/replication/handlers.rs:429`: a terminal skip is marked
  merged. That mark is what `:94` above reads, which is the whole finding.
- `merge_handler/definition.rs:52-68`: a nameless patch whose base is not held
  waits on the base's CID (`PushDef` under `OrphanTerminal = FALSE`); before,
  `terminal_skip`, which marked it merged and dropped it from the unmerged set
  (`OrphanTerminal = TRUE`). `:372`: a stored version releases waiters on its
  own CID. `RedriveDef`.
- `governance/sweep.rs:127`: the sweep's definition arm re-drives a patch of a
  version not held, so a restart does not lose it either.

## The three runs

| Config | Skip before judge | Orphan terminal | States | Verdict |
|---|---|---|---|---|
| `MC_GovernanceHeads_Today` | no | no | 125 | GREEN |
| `MC_GovernanceHeads_Red_SkipBeforeJudge` | yes | no | 2 | RED `INV_MergedIsJudged` |
| `MC_GovernanceHeads_Red_OrphanTerminal` | no | yes | 125 | RED `L_DefsStored` |

The instance is one composite the validator rejects, configured as a
local-only downsample source, with a collection block linking it; and an
initial definition with a patch of it, which may arrive first.

## Result 1: a head is installed only over accepted composites

`Today` is GREEN on `INV_HeadOverAccepted` (every installed head's links were
accepted by the validator), `INV_MergedIsJudged` (the merged mark of a governed
composite is a verdict's), `INV_RejectedLinkNoHead`, and the liveness that an
installable block is eventually installed. This is the property D-48 in the
Fefra repository recorded as open when the link rule replaced the refusal; it
holds, given that the mark means what the link check takes it to mean.

## Result 2: the skip before the verdict broke it, in two states

`Red_SkipBeforeJudge` is the ordering as `composite.rs` had it: a replicated
write into a local-only downsample source is answered with a terminal skip
before `judge_governed` runs. The replication handler marks a terminal skip
merged; the collection-block link check reads that mark; so a governed
composite the validator would have **rejected** is marked merged without a
verdict, and a block linking it installs a head over it. TLC finds it in two
states: push the write.

The fix is to take the verdict first for a governed collection, and apply the
skip only on an accept. A reject quarantines as it should; an accept is then
skipped as before, and the mark it leaves means "accepted", which is what the
link check needs it to mean. The reorder is in `composite.rs:252-269`. This was
a reviewer's finding on #1846 with "low likelihood" against it; the model turns
that into an ordering rule that is now cheap to keep.

## Result 3: a patch that arrives first was lost

`Red_OrphanTerminal` is `definition.rs` before the change: a nameless patch
whose base is not held was skipped as terminal, which marked it merged and
dropped it from the unmerged set, so nothing re-drove it when the base arrived.
`L_DefsStored` fails. `Today` waits on the base's CID instead, releases on the
base's store, and the sweep re-drives it after a restart; every patch whose
chain of bases arrives is stored.

The behaviour change this brought for **ungoverned** collections is stated in
`governed-collections.md` §6: a nameless patch says nothing about its
governance until its base is held, so it waits whether or not the collection
turns out to be governed, where before it was dropped.

## Not covered

1. **Parents.** A block's parents are collapsed into its links; the code checks
   both the same way.
2. **Batches.** A collection block in a batch reads the batch's own merged set
   as well as the durable mark (`collection_block.rs:78`); modelled as one set.
3. **Restarts.** The sweep's definition arm is cited but not modelled; the
   contract module's `Restart` covers the deferral index's loss.
4. **What the validator does.** `Verdict` is an oracle per composite. The
   composite path itself is `GovernanceMerge.tla`.
