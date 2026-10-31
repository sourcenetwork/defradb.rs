# `go.shinzo.p2p`: Shinzo P2P replication

- **Parents:** ["go.shinzo"]

How a Shinzo node announces, fetches, serves, admits and counts replicated documents: the `car` request/reply exchange, CAR building and import, inbound admission, the replication filter, the retention gate, and the counters an operator reads from the `p2p stats` log lines.

# `go.shinzo.p2p.message`: Direct-stream message framing

- **Parents:** ["go.shinzo.p2p"]

Rules every request/reply message on a direct libp2p stream follows (`rep`, `car` and the other comm channels).

# `go.shinzo.p2p.message.stream_close`: An inbound stream is closed once read

- **Parents:** ["go.shinzo.p2p.message"]
- **Go-Tests:** ["internal/db/p2p/message/message_test.go::TestReceive_ClosesStream"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "replies travel on a new stream to the `_resp` endpoint, so a receiver never holds the request stream open"

After reading an inbound message the receiver closes the stream, whether or not the bytes decode (a body of `not cbor` fails to decode and its stream is still closed). Replies go out on a new stream, so a peer that sends many requests never runs into its own stream limit at the receiver.

# `go.shinzo.p2p.message.late_replies`: Late and duplicate replies are discarded

- **Parents:** ["go.shinzo.p2p.message"]
- **Go-Tests:** ["internal/db/p2p/message/message_test.go::TestReceive_FullResponseChan_DoesNotBlock", "internal/db/p2p/message/message_test.go::TestSend_ReplyAfterTimeout_DoesNotPanic"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A request whose context ends before its reply arrives fails with `timeout waiting for response`. A reply that arrives after that, or a second validly signed reply carrying the same message ID, is discarded: receiving it succeeds, nothing waits on it, and the node keeps handling later messages.

# `go.shinzo.p2p.car_exchange`: The `car` request/reply exchange

- **Parents:** ["go.shinzo.p2p"]

An update is announced by its head CID and root block only. A receiver that needs the head asks a peer for the CAR rooted at it over the `car` channel, and falls back to walking the DAG block by block for any head no peer serves.

# `go.shinzo.p2p.car_exchange.protocol`: `car` channel endpoints, messages and caps

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_exchange_test.go::TestServeCARs_AnswersAtMostTheCap", "internal/db/p2p/car_exchange_test.go::TestServeCARs_AnswersABatchLargerThanTheSenderBatchSize"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "protocol IDs `/defradb/car_req/0.0.1` and `/defradb/car_resp/0.0.1`; CBOR map keys of CARRequest and CARReply; the 1024-head and 8 MiB reply caps"

Requests go to `/defradb/car_req/0.0.1` and replies come back on a new stream to `/defradb/car_resp/0.0.1`. Both are CBOR maps keyed by Go field name, with the message metadata flattened in: a request is `{Version, MessageID, SenderID, Pubkey, Signature?, ErrMessage?, CIDs}` where `CIDs` is an array of binary CIDs, and a reply is `{Version, MessageID, SenderID, Pubkey, Signature?, ErrMessage?, CARs}` where `CARs` is an array whose entries are byte strings, or CBOR `null` for a head the server left unserved (a nil slice, which `fxamacker/cbor` encodes as `null`) (`Signature` and `ErrMessage` are omitted when empty). `Version` is `/defradb/0.0.1`; the reply repeats the request's `MessageID`; each message is signed by its sender over its own CBOR encoding with `Signature` left out.

`CARs` is parallel to a prefix of the request's `CIDs`. A server answers at most 1024 heads (16 × the 64-document sender batch) per request: a request for 1029 heads gets a reply of exactly 1024 entries, and a request for 256 copies of one held head gets 256 entries, the last a CAR rooted at that head. The CAR bytes in one reply total at most 8 MiB (8388608).

# `go.shinzo.p2p.car_exchange.serve`: What a server puts in each reply entry

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_exchange_test.go::TestServeCARs_ServesHeldHeadsAndLeavesTheRestEmpty", "internal/db/p2p/car_exchange_test.go::TestServeCARs_DoesNotServeANonDocumentHead"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "an empty entry means not served; a reply shorter than the request means the heads past its end were not attempted"

For each requested head, in order, the server puts the CAR rooted at it, or an empty entry when the bytes are not a CID, the node does not hold the block, the requesting peer may not read it (the same access check bitswap applies), the block is not a document (composite or field) block, the CAR cannot be built, or the CAR alone exceeds 8 MiB. One bad head never fails the reply. A request for `[held head, absent head, "not a cid"]` gets three entries: a CAR rooted at the held head, then two empty ones; a request for a held block that does not decode as a document block gets `[empty]`.

The reply ends early, shorter than the request, at the first CAR that would take the reply past 8 MiB, or when the server's context ends; the heads past its end were not attempted. Each head's outcome is counted under `CAR serve outcomes` as `served`, `deferred`, `badCID`, `notHeld`, `readFailed`, `noAccess`, `notDocument`, `tooLarge`, `buildFailed` or `context` (the example above counts `served: 1, notHeld: 1, badCID: 1`, and `carBuilt` rises by 1).

# `go.shinzo.p2p.car_exchange.cache`: A CAR several peers ask for is built once

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_cache_test.go::TestCARCache_BuildsOnceThenServesFromCache", "internal/db/p2p/car_cache_test.go::TestCARCache_ConcurrentRequestSharesTheBuild", "internal/db/p2p/car_cache_test.go::TestCARCache_DoesNotKeepAFailedBuild", "internal/db/p2p/car_cache_test.go::TestCARCache_EvictsLeastRecentlyUsedPastTheBudget", "internal/db/p2p/car_cache_test.go::TestCARCache_DoesNotKeepACAROverTheWholeBudget", "internal/db/p2p/car_cache_test.go::TestServeCARs_SecondRequestForAHeadIsACacheHit"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The serving node keeps recently built CARs by head, up to 64 MiB in total, evicting the least recently used. Two peers asking for the same head, one after the other or at the same time, get byte-identical CARs from one build: `carBuilt` rises by 1 and `carCacheHits` by 1. A failed build is not kept, so the next request builds again; a CAR larger than the whole budget is built but not kept, and no reply carries one, since a CAR over the 8 MiB reply cap is counted `tooLarge` and left out; a CAR left out of a reply by the byte budget stays cached for the follow-up request.

# `go.shinzo.p2p.car_exchange.sources`: Which peers are asked, in which order

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_exchange_test.go::TestCARSources", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_AsksTheCreatorBeforeTheRelay", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_AsksTheRelayOnlyForHeadsTheCreatorDidNotServe", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_SkipsTheRequestWithoutHeadsOrPeer"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the peer named in the pushlog's `Creator` field receives the first `car` request"

A receiver asks the update's creator (the pushlog's `Creator`) first, then the relay (the peer that delivered the message) only for the heads the creator did not serve, and only when the relay is a different peer. Itself is never asked. With creator `c`, relay `r` the order is `[c, r]`; with creator = relay = `c` it is `[c]`; with no creator, or the creator being this node, it is `[r]`. When the creator serves every head the relay is not asked. No request is sent when there are no heads, or no peer other than this node to ask; every head then falls back to the DAG walk.

Outcomes are counted under `CAR fetch outcomes` as the source label followed by the outcome, e.g. `creatorFetched: 1, creatorNotServed: 1, relayFetched: 1`.

# `go.shinzo.p2p.car_exchange.reply_handling`: How a receiver uses a reply

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_exchange_test.go::TestFetchCARs_KeepsOnlyCARsRootedAtTheirHead", "internal/db/p2p/car_exchange_test.go::TestCARRootIs", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_AsksAgainForHeadsPastAShortReply", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_StopsOnAReplyThatAnswersNothing", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_FailedRequestLeavesEveryEntryNil"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a CAR is accepted only if it is a CAR whose only root is the requested head; a short reply is followed by a request for the remaining heads"

A CAR is used only if it parses and its single root is the head asked for; otherwise the head counts as `RootMismatch` and goes on to the next source. An empty entry counts as `NotServed`. For heads `[h1, h2, h3]` and a reply `[CAR rooted at h1, CAR rooted at h3, empty]`, only h1 is kept: `carFetched` rises by 1, `carFetchMissed` by 2, and the outcomes are `creatorFetched: 1, creatorRootMismatch: 1, creatorNotServed: 1`.

A reply shorter than the request is followed, from the same peer, by a request for the heads past its end (3 heads asked, 1 answered, then 2 asked). A reply that carries an `ErrMessage`, no entries, or more entries than heads asked counts every head it was asked for as `BadReply` and ends the asking of that peer. A request that fails leaves every head without a CAR. `carFetchMissed` counts exactly the heads that fall back to the DAG walk.

# `go.shinzo.p2p.car_exchange.backoff`: Failing peers are deprioritised, then passed over

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_exchange_test.go::TestFetchCARs_PrefersAReadyRelayOverARecentlyFailedCreator", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_AsksABackedOffCreatorTheRelayCannotServe", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_PassesOverAPeerThatKeepsFailing", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_ForgetsBackoffOnceThePeerAnswers", "internal/db/p2p/car_exchange_test.go::TestBackOffCARPeer_GrowsTheWindowAndCaps", "internal/db/p2p/car_exchange_test.go::TestCARPeerAvailability_ForgetsAnExpiredWindow", "internal/db/p2p/car_exchange_test.go::TestFetchCARs_DoesNotBackOffAPeerWhenThisNodeCancels", "internal/db/p2p/car_exchange_test.go::TestCARRequestFailure", "internal/db/p2p/car_exchange_test.go::TestCARFetchFailedPeer", "internal/db/p2p/car_exchange_test.go::TestCARStreamResourceLimited"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Resource-limit and dial failures are recognised by substring: `error code: 4098` also matches codes such as 40981, and any error text containing `dial` counts as `DialFailed`."

Each `car` request runs under a 10 s deadline. A failed request is classified, in this order: this node's own context ended → `Canceled`; the peer reset the stream with libp2p's StreamResourceLimitExceeded (4098, printed `error code: 4098` or `code: 0x1002`) → `ResourceLimited`; deadline passed → `Timeout`; a dial error → `DialFailed`; anything else → `RequestFailed`. Only `RequestFailed`, `Timeout`, `DialFailed` and `ResourceLimited` count against the peer; `Canceled`, `QueueFull`, `BadReply`, `NotServed` and `Fetched` do not.

A peer's first counted failure backs it off for at most 250 ms; each consecutive failure doubles the window, capped at 30 s. Inside its window a peer with fewer than 3 consecutive failures is asked after every ready source, and its outcomes count under the source label plus `Retry` (`creatorRetryFetched`); with 3 or more it is not asked at all, and the heads it would have been asked for count as `creatorBackoff`. So a creator that failed once is asked after the relay, and is still asked when the relay serves nothing (the head does not fall back to the walk). A peer that sends a well-formed reply, or whose window has expired, is ready again; a `BadReply` neither counts against the peer nor clears its backoff. A request this node abandoned (`creatorCanceled`) adds no failure and leaves any existing backoff unchanged.

# `go.shinzo.p2p.car_exchange.slots`: At most four requests in flight per peer

- **Parents:** ["go.shinzo.p2p.car_exchange"]
- **Go-Tests:** ["internal/db/p2p/car_exchange_test.go::TestAcquireCARSlot_HandsOutUpToTheLimit", "internal/db/p2p/car_exchange_test.go::TestAcquireCARSlot_WaitsForASlotToComeFree", "internal/db/p2p/car_exchange_test.go::TestAcquireCARSlot_ReturnsCanceledWhenThisNodeGivesUp"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A node has at most 4 `car` requests open to any one peer; the limit is per peer, so a fifth request to another peer goes out at once. A fifth request to the same peer waits for one of the four to finish and then goes out; if none finishes within 2 s it is not sent and its heads count as `QueueFull`. A request whose own context ends while waiting counts as `Canceled`. Neither outcome backs the peer off.

# `go.shinzo.p2p.car_build`: What a served CAR contains

- **Parents:** ["go.shinzo.p2p"]
- **Go-Tests:** ["internal/db/p2p/car_build_test.go::TestGenerateCAR_CountsNoMissingLinkWhenTheCARIsAbandoned", "internal/db/p2p/car_build_test.go::TestGenerateCAR_CountsEveryMissingLinkWhenTheCARShips"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "CAR v1 with the head as its only root, carrying the head, every block reachable through `Links`, and each block's signature and encryption blocks; never `Heads`"

A CAR is a CAR v1 whose only root is the head. It carries the head and every block reachable from it through block `Links` (field blocks and their links), plus the signature block and encryption block each of those blocks names, and nothing reachable only through `Heads`, so it holds one commit and none of the document's history. A linked block that is held but does not decode as a document block is included with its links unexplored, and counted under `carMissingLinks` (two such links → `carMissingLinks: 2`, `carBuilt: 1`). If any block to include cannot be read the whole CAR is abandoned: no CAR is served, `carFailed` rises by 1, `car failures` counts `blockRead: 1`, and `carMissingLinks` is unchanged. Build failures are counted under `car failures` as `rootLink`, `walk`, `carWriter`, `blockRead` or `carPut`.

# `go.shinzo.p2p.car_import`: Importing a received CAR

- **Parents:** ["go.shinzo.p2p"]
- **Go-Tests:** ["internal/db/p2p/car_import_test.go::TestImportCAR_DoesNotUnmergeABlockTheNodeAlreadyHas", "internal/db/p2p/car_import_test.go::TestImportCAR_CountsTheBlocksAnAbandonedImportWrote"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Every block in a received CAR is stored unless the node already holds it; a block it already holds and has merged stays merged. A CAR whose root block is not among its blocks is refused with `root block not found in CAR file` (and one with no root with `no root CIDs in CAR file`). An abandoned import adds 1 to `carImportFailed` and adds the number of blocks it newly wrote to `carImportOrphanBlocks`: with two carried blocks it is 2 when neither was held, 1 when one was, 0 when both were. Each import attempt adds 1 to `carImports`, and failures are counted under `CAR import failures` as `carReader`, `carNoRoots`, `carNextBlock`, `carPut` or `carNoRootBlock`.

# `go.shinzo.p2p.ingest`: Admitting and accounting for pushed documents

- **Parents:** ["go.shinzo.p2p"]

How a node takes in pushlog messages from pubsub and replicators, and accounts for every document they carry.

# `go.shinzo.p2p.ingest.inbound_cap`: A batch carries at most 1024 documents

- **Parents:** ["go.shinzo.p2p.ingest"]
- **Go-Tests:** ["internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_RejectsAnOversizedBatch"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "pushlog CBOR keys `DocID`, `CID`, `CollectionID`, `Creator`, `Block`, `car` and `documents` (entries `DocID`, `CID`, `Block`, `car`); at most 1024 `documents` entries"

A pushlog is a CBOR map `{Version, MessageID, SenderID, Pubkey, Signature?, ErrMessage?, DocID, CID, CollectionID, Creator, Block, car?, documents?}`, where `car` is the CAR rooted at `CID` and `documents` is a batch of `{DocID, CID, Block, car?}` entries that replaces the single-document fields; `car` and `documents` are omitted when empty. A message whose `documents` array has 1025 or more entries fails to decode and is not queued; one with exactly 1024 entries is accepted.

# `go.shinzo.p2p.ingest.admission`: Inbound messages are admitted against a byte budget

- **Parents:** ["go.shinzo.p2p.ingest"]
- **Go-Tests:** ["internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_OverBudgetDropsBeforeDecode", "internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_ByteBudgetReleasedAfterProcessing", "internal/db/p2p/p2p_test.go::TestProcessMessageWorker_ReleasesByteBudget", "internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_QueueFullCountedSeparately", "internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_ChargesForEachDocument", "internal/db/p2p/p2p_test.go::TestPubSubMessageHandler_QueueFullReleasesTheWholeReservation"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Pubsub messages waiting to be processed are bounded both by count (50000) and by bytes. A message is charged its wire size plus a fixed per-entry cost for each `documents` entry it carries. A message that does not fit the byte budget is dropped before it is decoded and counted under `droppedOverBudget`; one that fits the bytes but finds the queue full is dropped and counted under `droppedQueueFull`, each such message once (with a one-message queue the first message fills it and each later one is counted); either way it holds no budget afterwards. The wire size is checked before the message is decoded; the per-entry charge is known only after decoding, so a batch whose entries exceed the remaining budget is dropped after it is decoded, even when its wire size fits several times over. A message's charge is returned once it has been processed, so with a budget of exactly one message a second message is dropped while the first is queued and admitted again after it is processed. When either drop count is non-zero in a 30 s interval, the node logs `dropped inbound pubsub messages` at error level with `overBudget` and `queueFull`.

# `go.shinzo.p2p.ingest.outcomes`: Every arrival ends merged, skipped or dropped

- **Parents:** ["go.shinzo.p2p.ingest"]
- **Go-Tests:** ["internal/db/p2p/ingeststats_test.go::TestBatchIsCountedWhenNoDocumentSurvives", "internal/db/p2p/ingeststats_test.go::TestBatchOfHeldDocumentsCountsNoDrops", "internal/db/p2p/ingeststats_test.go::TestBatchAccountsForEveryDocument", "internal/db/p2p/ingeststats_test.go::TestSingleDocumentOutcomesAreCounted", "internal/db/p2p/ingeststats_test.go::TestDocSyncOutcomesAreCounted", "internal/db/p2p/ingeststats_test.go::TestArrivalForAHeadAlreadyInFlightIsSkipped", "internal/db/p2p/ingeststats_test.go::TestArrivalRefusedByAFullSyncQueueIsDropped"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Every document an arrival carries, pushed singly, in a batch, or pulled by document sync, is counted exactly once as merged (`docsMerged`), skipped (`docsSkipped`, a deliberate non-merge) or dropped (`docsDropped`, a loss), so merged + skipped + dropped equals documents received. Each batch counts under `batches` even when none of its documents survives, and under `batchesWithDrops` when at least one was dropped.

- A head CID that does not parse is a drop (`invalidCID`): a batch of 40 such entries gives 40 drops, 1 batch, 1 batch with drops.
- A head the node already holds merged is a skip (`alreadyMerged`), never a drop: a batch of 5 held documents gives 5 skips and 0 batches with drops.
- A request with no `CID` and no `documents` fails with `push log request carries no document` and is neither a drop nor a skip.
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

Every 30 s the node logs `p2p stats` at info with `queueDepth`, `queueBytes`, `droppedOverBudget`, `droppedQueueFull`, `batches`, `batchesWithDrops`, `docsMerged`, `docsDropped`, `docsSkipped`, `msgsIn`, `carImports`, `carImportFailed`, `carImportOrphanBlocks`, `carBuilt`, `carFailed`, `carMissingLinks`, `carFetched`, `carFetchMissed`, `carCacheHits`, `syncDAGCalls`, `heightFetches` and `heightFetchMissed`. Then, each only when non-empty, one info line per reason set with one field per reason: `car failures`, `syncDAG failures`, `document drops`, `document skips`, `CAR import failures`, `CAR fetch outcomes`, `CAR serve outcomes`. Every count is for that interval only and starts again from zero. Drops and skips are separate: two `importCAR` drops, one `syncDAG` drop, one `alreadyMerged` skip and one `filtered` skip report `docsDropped: 3`, `docsSkipped: 2`, `document drops` `{importCAR: 2, syncDAG: 1}` and `document skips` `{alreadyMerged: 1, filtered: 1}`. Reasons come from fixed sets, so a peer cannot add fields by varying its errors.

# `go.shinzo.p2p.telemetry.sync_dag_reason`: A failed DAG walk names its step

- **Parents:** ["go.shinzo.p2p.telemetry"]
- **Go-Tests:** ["internal/db/p2p/syncdagreason_test.go::TestSyncDAGReason_NamesTheStepThatFailed"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A failed DAG walk is counted under `syncDAG failures` by the step that failed: storing the root block `storeRoot`, generating a block link `blockLink`, checking whether a block merged `isMerged`, verifying a signature `verifySig`, fetching or retrieving encryption keys `encKeys`, loading a linked block `loadLink`, decoding a linked block `decodeLink`, a cancelled or expired context `context`, anything else `other`. A step that failed because the context was cancelled is counted under its step (a load stopped by cancellation is `loadLink`).

# `go.shinzo.p2p.replication_filter`: The pluggable replication filter

- **Parents:** ["go.shinzo.p2p"]

A node may be given a `ReplicationFilter`, asked `AllowReplication(collectionID, docID, fields)` for every pushed document before any of its blocks are stored; returning false discards the document.

# `go.shinzo.p2p.replication_filter.field_values`: The fields the filter is given

- **Parents:** ["go.shinzo.p2p.replication_filter"]
- **Go-Tests:** ["internal/db/p2p/replication_filter_test.go::TestExtractFieldsFromBlock_ReadsValuesFromTheLinkedFieldBlocks", "internal/db/p2p/replication_filter_test.go::TestExtractFieldsFromBlock_SkipsAnEncryptedFieldBlock", "internal/db/p2p/replication_filter_test.go::TestExtractFieldsFromBlock_SkipsAValueThatDoesNotDecode"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`fields` maps each field written in the head commit to its CBOR-decoded value, read from the head block itself and from the field blocks it links that are present in the document's CAR. A non-negative integer decodes as an unsigned 64-bit integer: a head linking `blockNumber` = 25951718 and `address` = `"0xabc"` gives `{blockNumber: 25951718 (uint64), address: "0xabc"}`. A field whose block is encrypted, whose value does not decode, or that has not arrived is left out rather than failing the document.

# `go.shinzo.p2p.replication_filter.after_fetch`: A document without its CAR is filtered twice

- **Parents:** ["go.shinzo.p2p.replication_filter"]
- **Go-Tests:** ["internal/db/p2p/replication_filter_test.go::TestArrivalIsFilteredOnTheFieldValuesInItsCAR"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A document pushed with its CAR is filtered once on the full field values; refused, it is a `filtered` skip and nothing is stored. A document pushed without its CAR is filtered first on what the head block alone carries (`{}` for a composite head), and, if allowed and a CAR is fetched, again on the fetched CAR's values; refused then, it is a `filteredAfterFetch` skip and nothing is stored. With a filter refusing `blockNumber` ≤ cutoff and a document at 100, cutoff 100 refuses and cutoff 99 stores it, on both the single-document and the batch path. A refusal at the first check is final.

# `go.shinzo.p2p.retention`: The retention floor for replicated documents

- **Parents:** ["go.shinzo.p2p"]

A node may be given a `RetentionRule` naming, per collection, a height field and a floor. A replicated document created at or below the floor, or with no valid height, is refused: `replicated document is at or below the retention floor` or `replicated document has no valid retention height`, both wrapping `refused by the retention rule`. Only creates are judged (a head of priority 1); updates, deletes and branchable collections are not.

# `go.shinzo.p2p.retention.gate`: Refused before the document's blocks are written or walked

- **Parents:** ["go.shinzo.p2p.retention"]
- **Go-Tests:** ["internal/db/p2p/retention_test.go::TestRetentionGateRefusesOnTheFirstHeightItCanRead", "internal/db/p2p/retention_test.go::TestRetentionGateReadsTheHeightFromASiblingsCAR", "internal/db/p2p/retention_test.go::TestRetentionGateKeepsTheRestOfABatch", "internal/db/p2p/retention_test.go::TestRetentionGateAsksForASharedHeightBlockOnce"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A pushed create is judged on the height block its head links under the height field, read from the first place that has it: the CAR sent with it or a CAR sent or fetched for another document in the same message, the local store, its own fetched CAR, and last the height block alone fetched over bitswap. It is refused at the first place, and the skip names where: `retentionBeforeFetch` (sent CAR or local store; no `car` request is made), `retentionAfterCARFetch` (fetched CAR; 1 request), `retentionAfterHeightFetch` (height block fetched alone; 1 request and `heightFetches: 1`), or `retentionNoHeight` (the value is not a height, e.g. the string `"100"`). A refused document writes no block, is never walked (`syncDAGCalls` stays 0) and is never merged.

Documents at one height share a height block, so a CAR fetched for one refuses its siblings too, with no height fetch. A document refused in a batch leaves the others to go on: with one document at the floor (height block held locally) and one above it, only the second is asked for and merged. Documents that share a height block nobody serves fetch it once (`heightFetches: 1`, `heightFetchMissed: 1`) and are each walked.

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

A document the merge refuses with a retention error is counted as a `retentionAtMerge` skip, not a drop (`docsDropped` and `batchesWithDrops` stay 0, `docsMerged` stays 0), and no update event is published or relayed for it, on both the single-document and batch paths.

# `go.shinzo.p2p.replicator_backfill`: Adding a replicator pushes every document

- **Parents:** ["go.shinzo.p2p"]
- **Go-Tests:** ["internal/db/p2p/replicator_backfill_test.go::TestPushHeadsForAllDocs_SkipsPrimaryKeysWithNoDocument"]
- **Go-Ref:** "shinzo-develop 2a899b860"

When a replicator is added, the node pushes the heads of every document in the collection. A stored primary key with no document behind it (left by a purge, for example) is skipped: the backfill succeeds and goes on to the documents after it, rather than failing or stopping at that key.
