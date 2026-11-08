# `defra`: Behaviour the Rust version defines

Requirements that start in defradb.rs rather than in a Go integration test.

# `defra.index`: Secondary indexes

- **Parents:** ["defra"]

Index behaviour beyond what Go's integration suite checks.

# `defra.index.unique_writer`: Unique-index errors name only the writer

- **Parents:** ["defra.index"]
- **Scenarios:** ["index::unique_writer::a_unique_index_error_names_the_writer_and_its_value"]

With a unique index on `User.name` and a stored `{"name":"alice","age":1}`, adding `{"name":"alice","age":2}` fails with `can not index a doc's field(s) that violates unique index. DocID: <new document's DocID>, name: alice`. Updating another document's `name` to `alice` fails with `failed to update index: can not index a doc's field(s) that violates unique index. DocID: <updated document's DocID>, name: alice. IndexName: User_name_ASC`. The error never contains the DocID of the document already holding the value, which the writer may not be allowed to read. Go develop builds the same text (`NewErrCanNotIndexNonUniqueFields`, `NewErrUpdateIndex`).

# `defra.collection`: Collection-wide operations

- **Parents:** ["defra"]

Operations on a collection's documents as a whole.

# `defra.collection.truncate_ownership`: Truncate removes document mappings and block ownership

- **Parents:** ["defra.collection"]
- **Scenarios:** ["db::collection::truncate::truncate_removes_doc_id_mappings_and_block_ownership"]

Each block records the DocIDs that own it; field blocks are content-addressed and carry no document identity, so ownership is a set. Before `Truncate`, a document's genesis field block is owned by its DocID. After `Truncate`, the DocID no longer resolves on this node and the block has no owner.

# `defra.save_many`: Save many documents in one transaction

- **Parents:** ["defra"]

`Node::save_docs` creates or updates a batch of documents of one collection in one transaction, through the same request path as a GraphQL mutation (NAC, document ACP, signing, encryption). It is a core API with no HTTP or CLI route.

# `defra.save_many.create_or_update`: Each document is added or updated

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::save_many_updates_a_stored_document_and_adds_a_new_one"]

With `User { name: String, age: Int }` holding Alice (`age: 40`), saving `[{_docID: <alice>, age: 41}, {name: "Bob", age: 50}]` leaves Alice at `age: 41` with her name unchanged and adds Bob. The call returns the saved documents' DocIDs in the order given.

# `defra.save_many.atomic`: One document that fails saves none

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::one_document_that_fails_saves_none"]

When one document of the batch cannot be saved, the call fails with that document's error and no document of the batch is stored, including the ones before it: saving `[{name: "Bob"}, {name: "Carol", nickname: "C"}]` on a `User` with no `nickname` field fails, and neither Bob nor Carol exists afterwards.

# `defra.save_many.deleted`: A deleted document is not saved again

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::a_deleted_document_is_not_saved_again"]

A document whose DocID names a document that was deleted fails the batch with `a document with the given ID has been deleted`, and nothing of the batch is stored. A DocID naming a stored document the caller cannot see fails with `document not found or not authorized to access`, so a save never bypasses document ACP.

# `defra.save_many.transaction`: In a caller's transaction, the batch lands on commit

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::a_batch_in_a_transaction_lands_on_commit", "mutation::save_many::a_discarded_batch_stores_nothing", "mutation::save_many::an_empty_batch_stores_nothing"]

Saved in a caller's transaction, the documents are not visible outside it until it commits, and are never stored if it is discarded. An empty batch succeeds and stores nothing.

# `defra.purge`: Hard delete of documents by DocID

- **Parents:** ["defra"]

`DB::purge_by_doc_ids_as` (and `DbTransactionRegistry::purge_by_doc_ids_as`, in a caller's transaction) removes documents from this node only: no CRDT operation is written and nothing is replicated. It is checked against the node permission of a collection truncate. A filtered truncate removes each chunk of matching documents through the same purge.

# `defra.purge.removes_document`: A purged document leaves nothing to find

- **Parents:** ["defra.purge"]
- **Scenarios:** ["mutation::purge::purge_removes_the_document", "mutation::purge::purge_removes_the_unique_index_entry"]

After purging alice from a `User { name: String, age: Int }` collection, a full scan of `User` returns no row for alice and no trace of a document that cannot be resolved. With a unique index on `name`, the purged document's index entries are gone too: adding a new document with `name: "alice"` afterwards succeeds instead of failing with `can not index a doc's field(s) that violates unique index`.

# `defra.purge.mappings`: Purge removes every DocID alias of the document

- **Parents:** ["defra.purge"]
- **Scenarios:** ["mutation::purge::purged_doc_id_no_longer_resolves", "db::collection::purge::purge_drops_the_doc_id_mappings_and_owner_edges"]

Once a document is purged its DocID no longer resolves on this node, and neither does any other DocID that pointed at the same document (an older alias). Looking any of them up returns not-found. No block's ownership record names a purged DocID any more, including the blocks a purge keeps.

# `defra.purge.history`: A purge deletes the document's history, unless the collection is branchable

- **Parents:** ["defra.purge"]
- **Scenarios:** ["db::collection::purge::purge_deletes_history_and_keeps_a_block_another_document_owns", "db::collection::purge::purging_every_owner_together_deletes_the_shared_block", "db::collection::purge::a_block_a_merge_adopts_during_the_purge_is_kept", "mutation::purge::a_purged_document_can_be_added_again", "db::collection::purge::purge_keeps_a_branchable_collections_history"]

A purge always deletes every block reachable from the document's heads that no other document owns: for a document created as `{"name":"Alice","age":40}` and updated once, both composite blocks, both first-version field blocks and the updated field block (five blocks) are gone. A field block is content-addressed, so two documents with the same `name` share one `name` block. Purging one of them keeps the shared block; purging the other later removes it. Purging both in one call removes it as well, whether the shared block is a current head or is reached only through an older version. A block another document comes to own while the purge runs (a merge that finds the block already stored) is kept. In a `@branchable` collection the history stays: the collection DAG links every version's composite (one collection block per document commit), so the purge keeps every block of the document's history, in the caller's transaction as in its own, and the collection's `_commits` and its sync stay whole; only the purged owner edges go. There is no option to keep or prune history.

# `defra.purge.no_conflict`: A purge does not fail on a concurrent block store or owner write

- **Parents:** ["defra.purge"]
- **Scenarios:** ["db::collection::purge::a_sync_storing_a_walked_block_again_does_not_fail_the_purge", "db::collection::purge::a_truncate_chunk_reruns_after_a_conflicting_owner_write"]

A purge commits successfully even when another transaction stores one of the blocks the purge is walking (as a P2P sync recreating a block would) and commits first: the attempt runs again. A truncate chunk, filtered or not, does the same when a merge adopts a block it decided to delete.

# `defra.purge.count`: Purged documents are counted once they commit

- **Parents:** ["defra.purge"]
- **Scenarios:** ["db::collection::purge::the_count_names_each_stored_document_once", "mutation::purge::purge_counts_each_stored_document_once", "mutation::purge::a_discarded_purge_removes_nothing", "mutation::purge::a_purge_in_a_transaction_lands_on_commit", "mutation::purge::a_purge_in_a_transaction_is_invisible_until_commit", "db::collection::purge::a_purge_in_a_transaction_conflicts_with_a_definition_change"]

A DocID that names no document is not an error and is not counted: purging `[alice, bob, alice]` succeeds and counts 2. A purge run in a caller's transaction removes nothing until that transaction commits, and nothing at all if it is discarded; it validates the collection's definition as every write there does, so a definition change committed first makes the transaction's commit conflict.

# `defra.orphan_gc`: Reclaiming fetched blocks that never merged

- **Parents:** ["defra"]

Blocks fetched during P2P sync whose merge never happened (an abandoned fetch, a refused or discarded merge) are reclaimed by a periodic sweep. A purge never reaches them, since it walks out from documents. `DB::reclaim_orphan_blocks` takes a cutoff time, a resume key and a scan limit, and reports `next_key`, `scanned`, `reclaimed`, `repaired`, `conflicts` and `unparsed`.

# `defra.orphan_gc.reclaim`: Only unowned blocks fetched before the cutoff are reclaimed

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::block::orphan_gc::only_unowned_blocks_fetched_before_the_cutoff_are_reclaimed"]

With one block fetched an hour before the cutoff, one fetched an hour after it, one fetched with no recorded time, and one merged block, a sweep deletes only the first (block and its waiting state), scans 3 (merged blocks are not scanned), reports 1 reclaimed, 0 repaired, 0 conflicts and no next key. A block past the cutoff that a document owns is kept and is marked merged instead (1 repaired). Three unowned and one owned expired block in one sweep give 3 reclaimed, 1 repaired, 4 scanned.

# `defra.orphan_gc.race`: A merge committing during the sweep keeps its block

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::block::orphan_gc::a_merge_committing_after_the_nomination_keeps_its_block"]

A block nominated by the sweep whose merge commits before the sweep deletes it is kept, and is not counted as reclaimed.

# `defra.orphan_gc.resume`: The sweep resumes, and stops when cancelled

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::block::orphan_gc::the_sweep_resumes_across_calls_and_stops_when_cancelled"]

With five expired blocks and a scan limit of 2, repeated sweeps, each starting from the previous next key, take more than one call, scan each block exactly once (5 in total), reclaim all 5, and end with no next key. A sweep that is already cancelled returns `context canceled`, scans nothing and deletes nothing.

# `defra.orphan_gc.refetch`: Re-receiving a block changes nothing

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::block::orphan_gc::storing_a_block_again_leaves_its_mark_and_a_merged_one_stays_merged", "db::block::orphan_gc::a_merge_racing_a_refetch_always_leaves_the_block_merged"]

A to-merge mark holds the time its block was first fetched. Storing again a block the node already holds writes nothing, as on main: a waiting block keeps its age and a merged one stays merged. A merge and a re-receive of the same block racing each other always leave it merged. A batch put holding one waiting and one merged block leaves both as they were. Go Shinzo renews the age of a waiting block on every re-receive; defradb.rs does not, so a re-put never writes a key the merge also writes and cannot add a conflict.

# `defra.orphan_gc.signature_owner`: A skipped field's signature block is still owned

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::merge::signature_blocks::a_merged_document_owns_its_signature_blocks_and_they_read_as_merged"]

With block signing on, a document `{"name":"John","age":30}` created on a node whose `User` has `age`, merged on a node whose `User` has only `name` (a rolling-upgrade schema skew), merges without error; on the receiver the signature block of the skipped `age` field block is owned by the document's DocID and reads as merged, so the orphan sweep never deletes it.

# `defra.orphan_gc.signature_merged`: Merging a signed block marks its signature block merged

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::merge::signature_blocks::a_merged_document_owns_its_signature_blocks_and_they_read_as_merged"]

A signature block received over P2P starts out marked to-merge. Merging a block whose `signature` link points at it clears that mark in the same transaction, so the orphan sweep never reclaims the signature of a merged block.

# `defra.orphan_gc.config`: When the sweep runs

- **Parents:** ["defra.orphan_gc"]
- **Scenarios:** ["db::block::orphan_gc::the_sweep_is_off_by_default_and_runs_every_five_minutes_for_half_hour_marks", "db::block::orphan_gc::with_the_sweep_on_an_abandoned_fetch_goes_once_its_mark_ages"]

The node configuration's `Libp2pConfig::orphan_sweep` sets whether the sweep runs (off by default), the interval between sweeps (5 minutes by default) and how long a to-merge block is kept (30 minutes by default). The node runs it only on a persistent store; an in-memory store never sweeps. The first sweep waits one TTL; each sweep examines at most 100,000 marks and resumes where the previous one stopped. With the sweep on, a block put through the P2P blockstore and never merged is deleted once its mark ages past the TTL.

# `defra.replication_filter`: An embedder's filter on replicated documents

- **Parents:** ["defra"]

An embedder may give a node a `db::replication_filter::ReplicationFilter` (`DbOptions::with_replication_filter`, `NodeBuilder::with_replication_filter`), asked `allow_replication(collection_id, collection_name, doc_id, fields)` for every replicated document; refusing discards it. `collection_name` is the collection's name, the key a Go filter matches on, and `collection_id` is the ID, which stays the same across a rename. A node without one keeps every document, as before.

# `defra.replication_filter.field_values`: The fields the filter is given

- **Parents:** ["defra.replication_filter"]
- **Scenarios:** ["db::merge::replication_filter::the_filter_is_given_the_values_the_commit_writes", "db::merge::replication_filter::an_encrypted_or_undecodable_value_is_left_out"]

`fields` maps each field written in the head commit to its CBOR-decoded value, read from the head block itself and from the field blocks it links. A non-negative integer decodes as an unsigned 64-bit integer: a head linking `blockNumber` = 25951718 and `address` = `"0xabc"` gives `{blockNumber: 25951718 (uint64), address: "0xabc"}`. A field whose block is encrypted, whose value does not decode, or that has not arrived is left out rather than failing the document, so a filter keeps a document whose deciding field is missing.

# `defra.replication_filter.refusal`: A refused document stores nothing

- **Parents:** ["defra.replication_filter"]
- **Scenarios:** ["db::merge::replication_filter::a_refused_document_is_not_stored", "db::merge::replication_filter::a_refusal_in_a_batch_refuses_only_that_document"]

The merge asks the filter once the commit's blocks are local and before anything of the document is stored. A refusal is the head's terminal skip `refused by the replication filter`: nothing of the document is stored and the head is not merged again. In a batch only the refused event is skipped.

# `defra.replication_filter.branchable`: A branchable collection is not filtered

- **Parents:** ["defra.replication_filter"]
- **Scenarios:** ["db::merge::replication_filter::a_branchable_collection_is_not_filtered"]

A branchable collection's head commits many documents at once, so its documents are not filtered one by one: they merge whatever the filter says.

# `defra.request`: Request validation

- **Parents:** ["defra"]

How a GraphQL request is validated before it runs.

# `defra.request.cyclic_fragments`: Cyclic fragment spreads fail validation

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::cyclic_fragments::fragments_spreading_each_other_fail_validation", "query::cyclic_fragments::a_cycle_no_field_spreads_still_fails_validation", "query::cyclic_fragments::a_fragment_spreading_itself_fails_validation"]

A request whose fragments spread each other in a cycle fails validation with no `data`, instead of recursing, whether or not a field spreads into the cycle, in a query or a subscription: `query { User { ...A } } fragment A on User { name ...B } fragment B on User { name ...A }` and the same fragments under `query { User { name } }` both return `Cannot spread fragment "A" within itself via B.`; `query { User { ...A } } fragment A on User { name ...A }` returns `Cannot spread fragment "A" within itself.` These are graphql-js's and Go's texts; defradb.rs said `circular fragment reference` before, and only when a field spread into the cycle.

# `defra.request.optional_variables`: An unset optional variable drops the field it fills

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::optional_variables::an_empty_field_condition_filters_on_nothing"]

A nullable variable with no default that the request leaves out is not an error and is not `null`: the argument or input field it fills is left out, as Go does. `query ($plot: String, $minRating: Float64) { Book(filter: {plot: {_like: $plot}, rating: {_geq: $minRating}}) { title } }` with only `{"minRating": 4.2}` returns the books rated 4.2 or more, whatever their plot. An explicit `null` is kept, and a missing list item becomes `null`. defradb.rs failed with `Variable "$plot" was not provided` before (#2104).

# `defra.request.mutation_relation_filter`: A filtered update or delete follows relations

- **Parents:** ["defra.request"]
- **Scenarios:** ["mutation::delete_relation_filter::a_delete_filtered_through_a_relation_deletes_what_the_query_finds", "mutation::delete_relation_filter::an_update_filtered_through_a_relation_updates_what_the_query_finds"]

`delete_Person(filter: {authoredBooks: {genre: {_eq: "Dystopia"}}})` deletes and returns exactly the people `Person(filter: {authoredBooks: {genre: {_eq: "Dystopia"}}})` returns, and `update_Person` with the same filter updates them; people the filter does not reach are untouched. defradb.rs matched such filters against the stored document only, where a relation has no value, so it found nothing (#2099).

# `defra.commits.link_delta`: Commit links and heads show their block's delta

- **Parents:** ["defra"]
- **Scenarios:** ["query::commits::with_link_delta::a_composite_link_shows_its_field_delta"]

In `_commits { links { fieldName delta } heads { delta } }`, each link and head shows the base64 `delta` and `collectionVersionId` of the block it points to, as Go does: after an update that sets `genre: "Dystopia"`, the composite commit's `genre` link shows `aER5c3RvcGlh`, and a composite head shows `null`, since composites carry no field delta. An Immutable block shows the delta of the field the link names. defradb.rs showed `null` for every link before (#2625).

# `defra.request.schema_descriptions`: The GraphQL schema describes itself like Go's

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::schema_descriptions::introspection_shows_go_descriptions"]

Introspection describes, in Go DefraDB's words (the only reference so far), every type, field, argument, input field and enum value both schemas share: collection objects (`_docID`, `_deleted`, `_version`, `GROUP` and the aggregates), filter, selector and operator blocks, `Commit`, `Signature`, the `_commits` query, mutations, `Ordering` and the custom scalars. A GraphQL client such as GraphiQL shows them in its docs pane. Exceptions: the built-in `ID`, `Int`, `Float` and `String` scalars keep async-graphql's texts, `docID` on update and delete is one ID in defradb.rs and is described as one, and `BlobOperatorBlock` says Blob where Go says String (#2106).

# `defra.request.relation_field_args`: Introspection shows the arguments of relation fields

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::relation_field_args::relation_fields_show_their_arguments"]

A relation field shows in introspection the arguments the executor accepts on it, with Go DefraDB's names, types and descriptions: a one-to-one or many-to-one field such as `Book.author` takes `filter: AuthorFilterArg`, and a one-to-many field such as `Author.published` takes `docID: [ID!]`, `filter: BookFilterArg`, `groupBy: [BookField!]`, `limit: Int`, `offset: Int` and `order: [BookOrderArg]` (#2649).

# `defra.request.mutation_introspection`: Introspection shows the mutations the parser accepts

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::mutation_introspection::mutations_show_go_arguments_and_types"]

For a collection `Book`, the `Mutation` type shows Go DefraDB's surface with its descriptions: `add_Book(encrypt: Boolean, encryptFields: [BookExplicitField!], input: [BookMutationInputArg!]): [Book]`, `update_Book(docID: [ID!], filter: BookFilterArg, input: BookMutationInputArg): [Book]`, `upsert_Book(add: BookMutationInputArg!, filter: BookFilterArg!, update: BookMutationInputArg!): [Book]` and `delete_Book(docID: [ID!], filter: BookFilterArg): [Book]`, where `BookExplicitField` lists the fields the schema declares. `update` and `delete` also take `docIDs: [ID]`, defradb.rs's own spelling of `docID`. `truncate_Book` takes no `docID`, since the parser does not accept one (#2649).

# `defra.request.subscription_introspection`: Introspection describes the subscriptions

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::subscription_introspection::subscriptions_appear_in_introspection"]

Introspection names `Subscription` as the schema's `subscriptionType`, as Go DefraDB does. Like Go's, it has a field per collection (`Book: [Book]`) with the arguments of a query select (`cid`, `docID`, `filter`, `groupBy`, `limit`, `offset`, `order`, `showDeleted`), and `_commits: [Commit]` with the arguments of the `_commits` query. Subscriptions themselves run over Server-Sent Events (#2649).

# `defra.request.aggregate_introspection`: Introspection shows the aggregate arguments the executor accepts

- **Parents:** ["defra.request"]
- **Scenarios:** ["query::aggregate_introspection::aggregates_show_relation_and_root_arguments"]

As in Go DefraDB, a one-to-many relation field such as `Author.published` is an argument of `COUNT` (typed `Book__CountSelector`) and of `SUM`, `AVG`, `MIN` and `MAX` (typed `Book__NumericSelector`), and the `Query` root has `MIN` and `MAX` of nullable `Float` beside `COUNT`, `SUM` and `AVG`, with one argument per collection. Go's `ScalarAggregateNumericBlock` argument on a plain numeric field is not shown, since the executor does not aggregate a single scalar (#2649).

# `defra.index.range_filter`: A two-sided range filter fetches only the range

- **Parents:** ["defra.index"]
- **Scenarios:** ["index::range_filter::a_closed_range_fetches_only_the_range", "index::range_filter::an_open_range_on_a_descending_index_fetches_only_the_range", "index::range_filter::an_empty_range_fetches_nothing"]

Over the index suite's ten users (ages 20, 23, 28, 30, 32, 33, 42, 44, 48, 55), `User(filter: {age: {_geq: 28, _leq: 33}}) { name }` on `age: Int @index` returns Fred, John, Islam, Andy with `indexFetches: 4`; `filter: {age: {_gt: 28, _lt: 44}}` on `age: Int @index(direction: DESC)` returns Addo, Andy, Islam, John with `indexFetches: 4`; and the empty range `{_gt: 44, _lt: 30}` returns `[]` with `indexFetches: 0`. Both bounds limit the index scan, not just the lower one.

# `defra.txn`: Transactions

- **Parents:** ["defra"]

Transaction behaviour beyond what Go's integration suite checks.

# `defra.txn.no_neighbour_conflict`: Updating one document does not conflict with its neighbour

- **Parents:** ["defra.txn"]
- **Scenarios:** ["txn::neighbour::updating_alice_survives_a_rewrite_of_bob", "txn::neighbour::updating_bob_survives_a_rewrite_of_alice"]

A transaction that reads document alice and updates her `age` to 10 commits successfully even though another writer rewrote every field of bob (the next document in the collection) and committed in between.

# `defra.document`: Building documents from client values

- **Parents:** ["defra"]

How client values become document fields.

# `defra.document.typed_nillable_arrays`: Typed lists fill nillable-element arrays

- **Parents:** ["defra.document"]
- **Scenarios:** ["mutation::add::field_kinds::typed_nillable_arrays::string_and_datetime_lists_fill_nillable_element_arrays"]

A `[String]` field given a string list (`["bafyA", "bafyB", "bafyC"]`) and a `[DateTime]` field given a time list (`2026-07-07T12:00:00Z`, `2026-07-08T12:00:00Z`) are stored with every element present, in order.

# `defra.collection.deterministic_id`: Collection IDs are deterministic

- **Parents:** ["defra.collection"]
- **Scenarios:** ["collection::deterministic_ids::a_collection_beside_a_longer_namesake_gets_one_id"]

Adding `type Foo { name: String } type FooBar { name: String }` to a fresh node gives `Foo` the same version ID on every run, `bafyreigj37qgcjknekn3jqcmpai5ut6gqsfvjgczmu5iopkeeznjojbqri`, even though `Foo` is a name prefix of `FooBar`.

# `defra.replication_filter.receive_gate`: A refused push is not fetched or stored

- **Parents:** ["defra.replication_filter"]
- **Scenarios:** ["db::merge::receive_gate::the_filter_sees_the_values_at_hand_and_keeps_a_head_alone", "db::merge::receive_gate::a_node_without_a_filter_has_no_gate", "p2p::sync::coordinator::access_tests::a_push_the_gate_refuses_on_arrival_stores_nothing", "p2p::sync::coordinator::access_tests::a_fetched_car_the_gate_refuses_is_not_imported", "p2p::sync::coordinator::access_tests::a_two_stream_push_the_gate_refuses_stores_nothing"]

A node with a replication filter also asks it in the P2P receive path. On arrival (gossip, pushlog and a replicator's direct push), before the head is stored or a fetch registered, the filter sees the values of the blocks the head links that the node already stores; a composite head alone gives `{}`, so the filter keeps it. A refusal stores nothing and registers no pending DAG. On a fetched CAR, after CID verification and before import, the filter sees the CAR's values; a refusal imports nothing and discharges the root, so it is not fetched again. The merge asks again as the backstop. A node without a filter installs no gate, and its receive path is unchanged.

# `defra.p2p.gossip_admission`: A gossip head refused at admission is kept or reported

- **Parents:** ["defra"]
- **Scenarios:** ["net::gossip_admission::a_gossip_burst_over_the_admission_limits_loses_no_document", "net::gossip_admission::a_gossip_head_past_the_deferral_budget_is_reported"]

Gossip has no reply and no retry, so a gossip head a node refuses for its per-peer rate limit or its pending-DAG capacity is kept as a durable pending record that takes no slot, and is fetched once a slot frees: a burst over both limits loses no document. These records are bounded by bytes, per source peer (`gossip_deferral_peer_bytes`) and in all (`gossip_deferral_total_bytes`). Past a budget the head is dropped, and the node reports a `p2p-gossip-head-dropped` event naming the peer and the root, so the application can resync it.
