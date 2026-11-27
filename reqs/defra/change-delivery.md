# `defra.events`: Change events and journals

- **Parents:** ["defra"]

A node tells its clients what changed through event subscriptions and through journals read by cursor. The delivery contract each one keeps is in `docs/book/src/events/index.md`.

# `defra.events.delivery`: The update journal's delivery guarantees

- **Parents:** ["defra.events"]

`_documentUpdates(collection, after, limit)` pages the committed updates of one collection: `head` is the number of rows, `next` the cursor to read after, and each entry names a `cursor`, a `docID` and the `cid` of the update block. A create adds no row; each update that changed its document adds one. Rust-only: Go has no `_documentUpdates`, so none of these requirements has a Go test.

# `defra.events.delivery.resume`: A saved cursor resumes after the node reopens

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::resume::a_saved_cursor_resumes_after_a_reopen_in_commit_order"]

A client reads `_documentUpdates(collection: "User", after: "0")`, keeps `next`, and the node closes and reopens on the same data directory. Reading `after` the kept cursor, and again from each returned `next` until it reaches `head` (a page holds at most 1,024 positions), returns exactly the rows committed after it that the reader may see (`defra.events.delivery.visibility`): none committed before it, none twice, none missing. The journal keeps every position across the reopen, so a hidden row is passed, not lost. With all rows visible and updates to Ann and Bob before the reopen (`head` and `next` "2") and to Cy, Ann and Bob after it, `after: "2"` returns cursors "3", "4" and "5" for Cy, Ann and Bob, with `head` and `next` "5".

# `defra.events.delivery.order`: Rows come back in commit order within one collection

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::resume::a_saved_cursor_resumes_after_a_reopen_in_commit_order"]

The rows of one collection have dense cursors, from the oldest row still kept up to `head` (from 1 until a truncate without a filter drops rows, `defra.events.delivery.retention`), in the order their updates committed: updates to Ann then Bob come back as cursor "1" for Ann and "2" for Bob. Nothing orders rows across collections; each collection has its own cursors.

# `defra.events.delivery.merge-once`: A replicated update appears once

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::merge_once::a_replicated_update_appears_once"]

An update a replicator delivers to another node adds exactly one row to that node's `_documentUpdates`, naming the document and the CID of the update block the sender committed: `head` is "1".

# `defra.events.delivery.beyond-head`: A cursor past the head is refused

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::beyond_head::a_cursor_past_the_head_is_refused"]

`after` greater than the collection's `head` fails with `update cursor is beyond this node's collection head`, through the in-process client, the HTTP client and the CLI alike; it is never answered with an empty page. With one document and no update (`head` "0"), `after: "1"`, the first cursor past the head, fails this way.

# `defra.events.delivery.retention`: A row stays until its collection is truncated in full

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::retention::a_cursor_before_a_full_truncate_is_no_longer_available", "subscription::delivery::retention::a_filtered_truncate_drops_no_row", "subscription::delivery::retention::a_delete_and_a_purge_drop_no_row", "subscription::delivery::retention::a_document_recreated_after_a_full_truncate_adds_rows_again", "db::merge::chunk_update_journal::a_full_truncate_of_the_parent_drops_its_chunks_update_rows"]

The journal keeps every row until its collection is truncated without a filter: no age or row count drops a row, and a filtered truncate, a delete or a purge drops none. With updates to Ann and Bob, a truncate filtered to Ann leaves `after: "0"` returning cursors "1" for Ann and "2" for Bob, and so does deleting Ann and then purging her. A truncate without a filter drops every row up to the head it reads, the rows of the collection's `@chunk` collections included, and `head` keeps counting from where it was, so a cursor is never handed out twice: after it, updates to Ann and Bob that were cursors "1" and "2" are gone, `after: "2"` returns no entries with `head` and `next` "2", and the next update, to Cy, is cursor "3". A document created again after it with the same content and updated the same way, so with the same DocID and the same block CID as a dropped row, adds a row too: re-creating Ann and updating her to "Ann 2" is cursor "3". When a peer's update of an article's `@chunk` added cursor "1" to its chunk collection, a truncate of the article collection without a filter drops it, and `after: "0"` on the chunk collection fails as `defra.events.delivery.unavailable` says, with `read after cursor 1`.

# `defra.events.delivery.unavailable`: A cursor whose rows were dropped is refused

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::retention::a_cursor_before_a_full_truncate_is_no_longer_available"]

`after` below the oldest cursor a read can still start from fails with `update cursor is no longer available on this node: resync the collection and read after cursor <oldest>`, where `<oldest>` is the cursor just before the oldest row still kept, or `head` when no row is kept; reading after `<oldest>` then returns every row kept, with no gap. It is never answered with the rows that are left, which would skip the dropped ones silently. Through the in-process client, the HTTP client and the CLI alike: once a truncate without a filter dropped cursors "1" and "2", `after: "0"` and `after: "1"` fail with `read after cursor 2`, also after the next update adds cursor "3".

# `defra.events.delivery.visibility`: A row the reader may not see is passed, not shown

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::visibility::hidden_rows_are_passed_and_not_shown", "subscription::delivery::visibility::a_collection_without_a_policy_shows_a_deleted_documents_rows"]

In a collection with a `@policy`, `_documentUpdates` hands a reader only the rows of documents it may read that are neither deleted nor purged. A hidden row still counts: `head` and `next` move past it, the rows that are shown keep their own cursors, so the cursors of a page can skip, and no error is raised. With updates to the owner's Ann and Bob, the owner reads cursors "1" and "2", and another identity reads no entries with `head` and `next` "2". Once the owner deletes Ann, the owner reads only cursor "2", for Bob, with `head` and `next` still "2". In a collection without a policy that is not a `@chunk` collection, every row is shown to every reader, a deleted document's included.

# `defra.events.delivery.chunk`: A chunk's update rows follow its parent

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["chunk::journal_visibility::a_parents_writes_add_no_row_to_its_chunks_update_journal", "db::merge::chunk_update_journal::a_chunks_updates_show_only_to_a_reader_of_its_parent"]

A parent's writes only create chunk documents, so they add no row to its `@chunk` collection's `_documentUpdates`: after Alice creates an article under a policy and updates its body, `_documentUpdates(collection: "Article__body_chunk", after: "0")` has `head` and `next` "0" and no entries for Alice, for Bob and for a request with no identity, while the article's own journal has `head` "1". A row a peer's update block of a chunk adds is shown only to a reader of the chunk's parent whose current text uses that chunk; any other reader is handed no entry, and `head` and `next` still move past the row.

# `defra.events.arrivals`: The arrival journal's delivery guarantees

- **Parents:** ["defra.events"]

`_documentArrivals(collection, after, limit, docID)` pages the documents that arrived in one collection, created locally or merged: `head` is the number of rows, `next` the cursor to read after, and each entry names a `cursor` and a `docID`. Rust-only: Go has no `_documentArrivals`, so none of these requirements has a Go test.

# `defra.events.arrivals.visibility`: An arrival the reader may not see is passed, not shown

- **Parents:** ["defra.events.arrivals"]
- **Scenarios:** ["subscription::delivery::arrivals::hidden_arrivals_are_passed_and_not_shown", "db::read::update_visibility::arrivals::a_deleted_private_documents_arrival_is_shown_to_nobody", "db::read::update_visibility::arrivals::a_purged_private_documents_arrival_is_shown_to_nobody"]

In a collection with a `@policy`, `_documentArrivals` hands a reader only the rows of documents it may read that are neither deleted nor purged, as `_documentUpdates` does (`defra.events.delivery.visibility`). A hidden row still counts: `head` and `next` move past it and no error is raised. With Ann created by the owner and then Bob by another identity, the owner reads cursor "1" for Ann, the other identity cursor "2" for Bob, and a request with no identity no entries, each with `head` and `next` "2". The other identity reading `after: "0", limit: 1` gets no entries with `next` "1", then `after: "1", limit: 1` gets Bob. Once the owner deletes Ann, her row is shown to nobody, the owner included: a delete unregisters the document, and its row must not become readable to everyone. The other identity still reads only Bob, with `head` and `next` "2".

# `defra.events.arrivals.chunk`: A chunk's arrival follows its parent

- **Parents:** ["defra.events.arrivals"]
- **Scenarios:** ["chunk::journal_visibility::a_chunks_arrival_is_passed_for_a_reader_who_may_not_read_its_parent", "chunk::journal_visibility::without_a_policy_only_the_chunks_of_a_live_parents_text_are_shown"]

`_documentArrivals` of a `@chunk` collection hands a reader only the rows of chunks its parent's current text uses, and only when the reader may read that parent; `head` and `next` move past the others. Under a policy, Alice's five-paragraph article shows Alice cursors "1" to "5", and with `limit: 2` cursors "1" and "2" with `next` "2"; Bob and a request with no identity get no entries, with `head` "5" and `next` "2" or "5". Without a policy, an article of two paragraphs shows cursors "1" and "2" to every reader; after its body is replaced by two other paragraphs only cursors "3" and "4" are shown (`head` "4"), and a page of the first two cursors has no entries with `next` "2"; once the article is deleted no reader is shown any entry, with `head` and `next` "4".
