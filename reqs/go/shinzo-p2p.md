# `go.shinzo.p2p`: Shinzo P2P replication

- **Parents:** ["go.shinzo"]

How a Shinzo node admits and counts replicated documents: inbound admission, the replication filter, the retention gate, and the counters an operator reads from the `p2p stats` log lines.

# `go.shinzo.p2p.message`: Direct-stream message framing

- **Parents:** ["go.shinzo.p2p"]

Rules every request/reply message on a direct libp2p stream follows (`rep` and the other comm channels).

# `go.shinzo.p2p.message.late_replies`: Late and duplicate replies are discarded

- **Parents:** ["go.shinzo.p2p.message"]
- **Go-Tests:** ["internal/db/p2p/message/message_test.go::TestReceive_FullResponseChan_DoesNotBlock", "internal/db/p2p/message/message_test.go::TestSend_ReplyAfterTimeout_DoesNotPanic"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A request whose context ends before its reply arrives fails with `timeout waiting for response`. A reply that arrives after that, or a second validly signed reply carrying the same message ID, is discarded: receiving it succeeds, nothing waits on it, and the node keeps handling later messages.

# `go.shinzo.p2p.ingest`: Admitting and accounting for pushed documents

- **Parents:** ["go.shinzo.p2p"]

How a node takes in pushlog messages from pubsub and replicators, and accounts for every document they carry.

# `go.shinzo.p2p.ingest.admission`: Inbound messages are admitted against a byte budget

- **Parents:** ["go.shinzo.p2p.ingest"]
- **Go-Tests:** ["internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_OverBudgetDropsBeforeDecode", "internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_ByteBudgetReleasedAfterProcessing", "internal/db/p2p/p2p_test.go::TestProcessMessageWorker_ReleasesByteBudget", "internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_QueueFullCountedSeparately"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Pubsub messages waiting to be processed are bounded both by count (50000) and by bytes. A message is charged its wire size. A message that does not fit the byte budget is dropped before it is decoded and counted under `droppedOverBudget`; one that fits the bytes but finds the queue full is dropped and counted under `droppedQueueFull`, each such message once (with a one-message queue the first message fills it and each later one is counted); either way it holds no budget afterwards. A message's charge is returned once it has been processed, so with a budget of exactly one message a second message is dropped while the first is queued and admitted again after it is processed. When either drop count is non-zero in a 30 s interval, the node logs `dropped inbound pubsub messages` at error level with `overBudget` and `queueFull`.

# `go.shinzo.p2p.ingest.outcomes`: Every arrival ends merged, skipped or dropped

- **Parents:** ["go.shinzo.p2p.ingest"]
- **Go-Tests:** ["internal/db/p2p/ingeststats_test.go::TestSingleDocumentOutcomesAreCounted", "internal/db/p2p/ingeststats_test.go::TestDocSyncOutcomesAreCounted", "internal/db/p2p/ingeststats_test.go::TestArrivalForAHeadAlreadyInFlightIsSkipped", "internal/db/p2p/ingeststats_test.go::TestArrivalRefusedByAFullSyncQueueIsDropped"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Every document an arrival carries, pushed or pulled by document sync, is counted exactly once as merged (`docsMerged`), skipped (`docsSkipped`, a deliberate non-merge) or dropped (`docsDropped`, a loss), so merged + skipped + dropped equals documents received.

- A head CID that does not parse is a drop (`invalidCID`).
- A head the node already holds merged is a skip (`alreadyMerged`), never a drop.
- A request with no `CID` fails with `push log request carries no document` and is neither a drop nor a skip.
- An arrival for a head already being processed is a skip (`inFlight`) and returns no error; an arrival for a different head of the same document is processed.
- An arrival refused because the sync queue is full fails with `sync queue is full` and is a drop (`syncQueueFull`); the head is not left marked in flight, so a repeat is refused the same way.
- In document sync, a head repeated within one response is a skip (`duplicateHead`), a DAG that cannot be fetched is a drop (`syncDAG`), and a merge refused by the retention rule is a skip (`retentionAtMerge`).

Drop reasons are `accessError`, `blockDecode`, `cidMismatch`, `generateLink`, `importCAR`, `invalidCID`, `isMergedError`, `mergeFailed`, `syncDAG`, `syncQueueFull`; skip reasons are `alreadyMerged`, `duplicateHead`, `filtered`, `filteredAfterFetch`, `inFlight`, `noAccess`, `retentionBeforeFetch`, `retentionAfterCARFetch`, `retentionAfterHeightFetch`, `retentionAtMerge`, `retentionNoHeight`.

# `go.shinzo.p2p.telemetry`: The P2P stats report

- **Parents:** ["go.shinzo.p2p"]

Counters a Shinzo node reports in its logs every 30 seconds.

# `go.shinzo.p2p.telemetry.report`: Interval counters, with drops and skips apart

- **Parents:** ["go.shinzo.p2p.telemetry"]
- **Go-Tests:** ["internal/db/p2p/dropstats_test.go::TestDropAndSkipAreCountedApart", "internal/db/p2p/dropstats_test.go::TestDropReasonsDrainResets", "internal/db/p2p/dropstats_test.go::TestReportDrainsDropsAndSkips"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Every 30 s the node logs `p2p stats` at info with `queueDepth`, `queueBytes`, `droppedOverBudget`, `droppedQueueFull`, `docsMerged`, `docsDropped`, `docsSkipped`, `msgsIn`, `heightFetches` and `heightFetchMissed`. Then, each only when non-empty, one info line per reason set with one field per reason: `document drops` and `document skips`. Every count is for that interval only and starts again from zero. Drops and skips are separate: two `importCAR` drops, one `syncDAG` drop, one `alreadyMerged` skip and one `filtered` skip report `docsDropped: 3`, `docsSkipped: 2`, `document drops` `{importCAR: 2, syncDAG: 1}` and `document skips` `{alreadyMerged: 1, filtered: 1}`. Reasons come from fixed sets, so a peer cannot add fields by varying its errors.

# `go.shinzo.p2p.telemetry.sync_dag_reason`: A failed DAG walk names its step

- **Parents:** ["go.shinzo.p2p.telemetry"]
- **Go-Tests:** ["internal/db/p2p/syncdagreason_test.go::TestSyncDAGReason_NamesTheStepThatFailed"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A failed DAG walk is counted under `syncDAG failures` by the step that failed: storing the root block `storeRoot`, generating a block link `blockLink`, checking whether a block merged `isMerged`, verifying a signature `verifySig`, fetching or retrieving encryption keys `encKeys`, loading a linked block `loadLink`, decoding a linked block `decodeLink`, a cancelled or expired context `context`, anything else `other`. A step that failed because the context was cancelled is counted under its step (a load stopped by cancellation is `loadLink`).

# `go.shinzo.p2p.replication_filter`: The pluggable replication filter

- **Parents:** ["go.shinzo.p2p"]

A node may be given a `ReplicationFilter`, asked `AllowReplication(collectionID, docID, fields)` for every pushed document before any of its blocks are stored; returning false discards the document.

# `go.shinzo.p2p.replication_filter.after_fetch`: A pushed document is filtered on arrival and on its fetched CAR

- **Parents:** ["go.shinzo.p2p.replication_filter"]
- **Go-Tests:** ["internal/db/p2p/replication_filter_test.go::TestArrivalIsFilteredOnTheFieldValuesInItsCAR"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A pushed document is filtered first on what the head block alone carries (`{}` for a composite head); refused, it is a `filtered` skip and nothing is stored. If allowed and a CAR is fetched, it is filtered again on the fetched CAR's values; refused then, it is a `filteredAfterFetch` skip and nothing is stored. With a filter refusing `blockNumber` ≤ cutoff and a document at 100, cutoff 100 refuses and cutoff 99 stores it. A refusal at the first check is final.

# `go.shinzo.p2p.retention`: The retention floor for replicated documents

- **Parents:** ["go.shinzo.p2p"]

A node may be given a `RetentionRule` naming, per collection, a height field and a floor. A replicated document created at or below the floor, or with no valid height, is refused: `replicated document is at or below the retention floor` or `replicated document has no valid retention height`, both wrapping `refused by the retention rule`. Only creates are judged (a head of priority 1); updates, deletes and branchable collections are not.

# `go.shinzo.p2p.retention.gate`: Refused before the document's blocks are written or walked

- **Parents:** ["go.shinzo.p2p.retention"]
- **Go-Tests:** ["internal/db/p2p/retention_test.go::TestRetentionGateRefusesOnTheFirstHeightItCanRead", "internal/db/p2p/retention_test.go::TestRetentionGateAsksForASharedHeightBlockOnce"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A pushed create is judged on the height block its head links under the height field, read from the first place that has it: the local store, its own fetched CAR, and last the height block alone. It is refused at the first place, and the skip names where: `retentionBeforeFetch` (local store), `retentionAfterCARFetch` (fetched CAR), `retentionAfterHeightFetch` (height block fetched alone; `heightFetches: 1`), or `retentionNoHeight` (the value is not a height, e.g. the string `"100"`). A refused document is never walked and never merged.

Documents that share a height block nobody serves fetch it once (`heightFetches: 1`, `heightFetchMissed: 1`) and are each walked.

# `go.shinzo.p2p.retention.lets_through`: What the gate cannot refuse goes on

- **Parents:** ["go.shinzo.p2p.retention"]
- **Go-Tests:** ["internal/db/p2p/retention_test.go::TestRetentionGateLeavesWhatItCannotRefuse"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A document above the floor (101 against 100), an update (priority 2) at the floor, and a document in a collection the rule does not cover are merged without a skip. A create whose height block no peer serves is not refused by the gate: `heightFetchMissed` rises by 1 and the document goes on to the DAG walk (which then fails on the same missing block and counts as a drop).

# `go.shinzo.p2p.retention.height_value`: What counts as a height

- **Parents:** ["go.shinzo.p2p.retention"]
- **Go-Tests:** ["internal/db/p2p/retention_test.go::TestHeightIn"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A height block is used only if its bytes hash to the linked CID, it decodes as a plain (unencrypted) LWW register block, and it is for the height field; otherwise the gate cannot read it and does not refuse (bytes of another block, not a block, another field such as `logIndex`, an encrypted value, a counter, or a value that does not decode). A readable block holding a non-negative integer up to 9223372036854775807 is that height (7 → 7). A readable block holding a negative number, a non-integer such as `"7"`, or an integer above 9223372036854775807 has no valid height, and the document is refused with `retentionNoHeight`.

# `go.shinzo.p2p.retention.at_merge`: A refusal at merge is a skip, never relayed

- **Parents:** ["go.shinzo.p2p.retention"]
- **Go-Tests:** ["internal/db/p2p/retention_test.go::TestRetentionRejectionAtMergeIsASkip"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A document the merge refuses with a retention error is counted as a `retentionAtMerge` skip, not a drop (`docsDropped` and `docsMerged` stay 0), and no update event is published or relayed for it.

# `go.shinzo.p2p.replicator_backfill`: Adding a replicator pushes every document

- **Parents:** ["go.shinzo.p2p"]
- **Go-Tests:** ["internal/db/p2p/replicator_backfill_test.go::TestPushHeadsForAllDocs_SkipsPrimaryKeysWithNoDocument"]
- **Go-Ref:** "shinzo-develop 2a899b860"

When a replicator is added, the node pushes the heads of every document in the collection. A stored primary key with no document behind it (left by a purge, for example) is skipped: the backfill succeeds and goes on to the documents after it, rather than failing or stopping at that key.
