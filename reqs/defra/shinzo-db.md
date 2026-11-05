# `go.shinzo.retention.rule`: The merge refuses what the rule refuses

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_merge_refuses_what_the_rule_refuses", "db::merge::retention::a_floor_of_zero_refuses_nothing_by_height", "db::merge::retention::an_uncovered_collection_or_a_node_without_a_rule_stores_everything", "db::merge::retention::a_document_is_judged_as_its_head_leaves_it", "db::merge::retention::a_document_that_gains_a_height_above_the_floor_is_kept", "db::merge::retention::an_update_to_the_floor_is_refused_and_the_document_keeps_its_height", "db::merge::retention::a_null_height_is_no_height"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The merge judges the document a pushed head's walk ends with, once: the walk merges the head's unmerged older versions first, in the same attempt, and a refusal discards them all. So a Log created at `blockNumber: 50` and updated to 150 is stored at 150, whether the update arrives alone or after the create was refused on its own; a Log created with no `blockNumber` and updated to 150 is stored too, as Go keeps both. An update that moves a stored Log to 100 is refused and the Log stays at its earlier height.

For `type Log { blockNumber: Int address: String }` with field `blockNumber` and floor 100, merging a replicated document:

- `blockNumber: 100` (at the floor) is refused with `replicated document is at or below the retention floor`;
- `blockNumber: 101` is stored;
- no `blockNumber`, or `blockNumber: -5`, is refused with `replicated document has no valid retention height` (a null, non-integer or negative value has no valid height; a nillable Int read back as null is no height);
- with a floor of 0 or less, nothing is refused by height (`blockNumber: 0` is stored), but a document with no valid height is still refused;
- a collection the rule does not cover, or a node with no rule, stores everything.

Both refusals read `<reason>: refused by the retention rule`, as Go's wrapped errors do. A refused document is not stored, its head is not marked merged (merging it again judges it again), and its outcome is `Rejected`, never a dropped event. A node takes any rule (`NodeBuilder::with_retention`), so a floor can move with the chain. The `rejectedBelowFloor` and `rejectedNoHeight` counters are `go.shinzo.merge.telemetry.report`.

# `go.shinzo.retention.scope`: Branchable and unknown collections are not judged

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_floor_resolves_for_a_covered_collection_only", "db::merge::retention::a_branchable_collection_is_not_judged"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A covered collection resolves to its field and floor (`blockNumber`, 100). A `@branchable` collection is never judged, because one of its heads commits many documents at once. A collection ID this node does not hold, or a node with no rule, is not judged either.

# `go.shinzo.retention.delete`: A delete is applied below the floor

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_delete_is_applied_below_the_floor", "db::merge::retention::an_update_concurrent_with_a_delete_merges_into_the_deleted_document"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With floor 100, a replicated delete of a document whose `blockNumber` is 5 merges without error and its head is marked merged: deleted documents are not judged. An update made concurrently with the delete that moves the height to 50 merges too: the merged document is deleted, so it is not judged.

# `go.shinzo.retention.batch`: A refusal in a batch refuses only that document

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_refusal_in_a_batch_refuses_only_that_document"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Merging a batch of three Logs at heights 101, 100 and 150 against floor 100 returns the outcomes `[Merged, Rejected, Merged]`; the 101 and 150 documents are stored, the 100 one is not, and no drop is recorded.
