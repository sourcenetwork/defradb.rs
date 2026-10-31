# `go.shinzo.retention.rule`: The merge refuses what the rule refuses

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_merge_refuses_what_the_rule_refuses", "db::merge::retention::a_floor_of_zero_refuses_nothing_by_height", "db::merge::retention::an_uncovered_collection_or_a_node_without_a_rule_stores_everything"]
- **Go-Ref:** "shinzo-develop 2a899b860"

For `type Log { blockNumber: Int address: String }` with field `blockNumber` and floor 100, merging a replicated document:

- `blockNumber: 100` (at the floor) is refused with `replicated document is at or below the retention floor`;
- `blockNumber: 101` is stored;
- no `blockNumber`, or `blockNumber: -5`, is refused with `replicated document has no valid retention height` (a null, non-integer or negative value has no valid height);
- with a floor of 0 or less, nothing is refused by height (`blockNumber: 0` is stored), but a document with no valid height is still refused;
- a collection the rule does not cover, or a node with no rule, stores everything.

Both refusals read `<reason>: refused by the retention rule`, as Go's wrapped errors do. A refused document is not stored, its head is not marked merged, and its outcome is `Rejected`, never a dropped event. The `rejectedBelowFloor` and `rejectedNoHeight` counters are `go.shinzo.telemetry.merge.report`.

# `go.shinzo.retention.scope`: Branchable and unknown collections are not judged

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_floor_resolves_for_a_covered_collection_only", "db::merge::retention::a_branchable_collection_is_not_judged"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A covered collection resolves to its field and floor (`blockNumber`, 100). A `@branchable` collection is never judged, because one of its heads commits many documents at once. A collection ID this node does not hold, or a node with no rule, is not judged either.

# `go.shinzo.retention.delete`: A delete is applied below the floor

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_delete_is_applied_below_the_floor"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With floor 100, a replicated delete of a document whose `blockNumber` is 5 merges without error and its head is marked merged: deleted documents are not judged.

# `go.shinzo.retention.batch`: A refusal in a batch refuses only that document

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_refusal_in_a_batch_refuses_only_that_document"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Merging a batch of three Logs at heights 101, 100 and 150 against floor 100 returns the outcomes `[Merged, Rejected, Merged]`; the 101 and 150 documents are stored, the 100 one is not, and no drop is recorded.
