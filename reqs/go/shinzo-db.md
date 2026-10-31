# `go.shinzo.merge`: Merging replicated documents

- **Parents:** ["go.shinzo"]

`DB.Merge(event)` merges one replicated head; `DB.MergeBatchWithTxn(events)` merges many and returns one outcome per event, in the caller's order: `MergeCommitted` (stored), `MergeRejected` (refused by the retention rule) or `MergeDropped` (not stored). Callers relay only committed events onward.

# `go.shinzo.merge.batch_isolation`: One event that cannot merge does not stop the others

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMergeBatch_OneEventCannotMerge_OthersStillLand", "internal/db/merge_test.go::TestMergeBatch_FailureInLaterChunk_ReportsFailureAgainstTheRightEvent"]
- **Go-Ref:** "shinzo-develop 2a899b860"

In a batch where one event's head has a parent this node never received, every other event is stored and its outcome is `MergeCommitted`; the bad event's outcome is `MergeDropped` and it is not readable. This holds wherever the bad event sits: first in a batch of two, or at position 9 of 18. The returned error contains `could not find <missing CID>` and the bad event's DocID, wrapped as `merge event dropped: <cause>. DocID: <docID>, CID: <head CID>`. The outcomes agree with what a read returns for each document.

# `go.shinzo.merge.retry`: Retries are bounded, and exhaustion is reported as a failure

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMerge_ExhaustedRetries_ReportsFailure", "internal/db/merge_test.go::TestMergeBatch_ExhaustedRetries_ReportsNothingMerged", "internal/db/merge_test.go::TestMerge_ZeroMaxRetriesStillAttempts", "internal/db/merge_test.go::TestMergeBatch_ZeroMaxRetriesStillAttempts"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Go's text is `reached maximum transaction reties` (a typo for retries); a Rust node should match the text clients match on, so decide which spelling the requirement fixes."

When every attempt to commit a merge conflicts, `Merge` returns an error containing `maximum transaction` (Go: `merge event dropped: reached maximum transaction reties: <conflict>. DocID: <docID>, CID: <head CID>`), and the batch path reports `[MergeDropped]` with an error; in both cases the document is not readable. A retry budget (`MaxTxnRetries`) of 0 still makes one attempt: merging a head whose block is absent fails with `failed to load block for merge`, not with exhausted retries, on both paths.

# `go.shinzo.block_owner`: Which documents own a block

- **Parents:** ["go.shinzo"]

Each block records the DocIDs that own it. Field blocks are content-addressed and carry no document identity, so ownership is a set.
