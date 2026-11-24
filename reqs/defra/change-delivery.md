# `defra.events`: Change events and journals

- **Parents:** ["defra"]

A node tells its clients what changed through event subscriptions and through journals read by cursor. The delivery contract each one keeps is in `docs/book/src/events/index.md`.

# `defra.events.delivery`: The update journal's delivery guarantees

- **Parents:** ["defra.events"]

`_documentUpdates(collection, after, limit)` pages the committed updates of one collection: `head` is the number of rows, `next` the cursor to read after, and each entry names a `cursor`, a `docID` and the `cid` of the update block. A create adds no row; each update that changed its document adds one. Rust-only: Go has no `_documentUpdates`, so none of these requirements has a Go test.

# `defra.events.delivery.resume`: A saved cursor resumes after the node reopens

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::resume::a_saved_cursor_resumes_after_a_reopen_in_commit_order"]

A client reads `_documentUpdates(collection: "User", after: "0")`, keeps `next`, and the node closes and reopens on the same data directory. Reading `after` the kept cursor returns exactly the rows committed after it: none committed before it, none twice, none missing. With updates to Ann and Bob before the reopen (`head` and `next` "2") and to Cy, Ann and Bob after it, `after: "2"` returns cursors "3", "4" and "5" for Cy, Ann and Bob, with `head` and `next` "5".

# `defra.events.delivery.order`: Rows come back in commit order within one collection

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::resume::a_saved_cursor_resumes_after_a_reopen_in_commit_order"]

The rows of one collection have dense cursors, 1 up to `head`, in the order their updates committed: updates to Ann then Bob come back as cursor "1" for Ann and "2" for Bob. Nothing orders rows across collections; each collection has its own cursors.

# `defra.events.delivery.merge-once`: A replicated update appears once

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::merge_once::a_replicated_update_appears_once"]

An update a replicator delivers to another node adds exactly one row to that node's `_documentUpdates`, naming the document and the CID of the update block the sender committed. The row stays single when the update reaches the node again: after the receiving node reopens and syncs the document from its peer, `head` is still "1".

# `defra.events.delivery.beyond-head`: A cursor past the head is refused

- **Parents:** ["defra.events.delivery"]
- **Scenarios:** ["subscription::delivery::beyond_head::a_cursor_past_the_head_is_refused"]

`after` greater than the collection's `head` fails with `update cursor is beyond this node's collection head`, through the in-process client, the HTTP client and the CLI alike; it is never answered with an empty page. With one document and no update, `after: "2"` fails this way.
