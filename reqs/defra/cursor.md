# `defra.cursor`: Cursor pagination

- **Parents:** ["defra"]

A `_cursor { <Collection>(first|after|last|before, order, filter) { … } _pageInfo { hasNext hasPrev startCursor endCursor } }` query pages through a collection with opaque tokens: `startCursor` and `endCursor` name the first and last rows of a page, and `after` or `before` resume from one.

# `defra.cursor.token_encoding`: A token is unpadded base64url of `{"d": docID, "k": keys}`

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::token::a_token_is_its_rows_doc_id_and_order_values", "cursor::token::a_token_with_an_empty_o_still_resumes"]

A token is the JSON object `{"d":"<docID>","k":{<order field>: <value>, …}}`, encoded with base64url without padding, so it never contains `+`, `/` or `=`. It has no other member: the always-empty `"o"` older builds wrote is gone, and `k` is omitted when there are no key values. With User `{name: String, age: Int @index}` holding Alice 25, Bob 30 and Carol 35, `User(first: 2, order: {age: ASC})` returns `startCursor` `{"d":"<Alice's docID>","k":{"age":25}}` and `endCursor` `{"d":"<Bob's docID>","k":{"age":30}}`, encoded. A token an older build issued, `{"d":…,"k":{"age":25},"o":""}`, is still accepted: `first: 2, after` it returns Bob and Carol, the same page as the token without `o`.

# `defra.cursor.in_filter_order`: An unsorted `_in` filter keeps the requested order

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::in_filter::an_unsorted_in_filter_pages_in_order", "cursor::in_filter::an_unsorted_in_filter_keeps_the_order_without_a_cursor"]

The order of the values in an `_in` filter never decides the order of the rows. Over Users aged 25, 35, 45, 55 and 65 with `age: Int @index`, `_cursor { User(first: 2, filter: {age: {_in: [55, 25, 45, 35]}}, order: {age: ASC}) }` returns [25, 35] (`hasNext: true`), and `after` its `endCursor` returns [45, 55] (`hasNext: false`); 65 never appears. Without a cursor, `User(filter: {age: {_in: [55, 25, 45, 35]}}, order: {age: ASC})` returns [25, 35, 45, 55] and `order: {age: DESC}` returns [55, 45, 35, 25], whether `age` has an ascending index, a descending index or none.

# `defra.cursor.paging`: `first`/`after` and `last`/`before` page without gaps or repeats

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::paging::last_returns_the_final_rows", "cursor::paging::no_order_pages_by_doc_id_without_an_index", "cursor::paging::before_excludes_its_boundary", "cursor::paging::pages_through_duplicate_index_keys", "cursor::paging::first_after_follows_doc_id_desc", "cursor::paging::last_before_follows_doc_id_desc"]

`first: n` returns the first n rows in the requested order and `last: n` the last n. `after` a token returns the rows that follow its row and `before` a token the rows that precede it; the token's own row is never in the page. With User `{name: String, age: Int @index}` holding a 20, b 30, c 40 and d 50, `last: 2, before` c's token, `order: [{age: ASC}]` returns [a, b]. With no `order`, rows page by `_docID` and need no index. Rows that share an indexed value still page one by one: four Users aged 30 come back as two pages of two, each user once. With `order: [{_docID: DESC}]` the pages follow the same order as a plain `User(order: {_docID: DESC})` query, forward and backward.

# `defra.cursor.page_info`: `_pageInfo` returns what was selected, under its own names

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::page_info::only_the_selected_fields_appear", "cursor::page_info::field_aliases_are_ignored", "cursor::page_info::a_block_alias_is_ignored"]

`_pageInfo` holds only the fields the query selects: `_pageInfo { hasNext startCursor endCursor }` has no `hasPrev`. Aliases inside it and on the block are ignored, as in Go: `_pageInfo { next: hasNext start: startCursor }` returns `{"hasNext": …, "startCursor": …}`, and `info: _pageInfo { hasNext }` renders under `_pageInfo`.

# `defra.cursor.errors`: A cursor query the engine cannot serve fails with a named error

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::errors::an_order_field_without_an_index_fails", "cursor::errors::a_token_that_is_not_base64_fails", "cursor::errors::first_and_last_together_fail", "cursor::errors::two_collections_in_one_block_fail"]

Ordering by a field no index covers fails with `no supporting index for cursor order field`. A token that does not decode fails with `invalid cursor`. `first` or `after` with `last` or `before` fails with `forward parameters (first/after) cannot be combined with backward parameters (last/before)`. Two collections in one `_cursor` block fail with `_cursor block cannot contain multiple collection queries`.

# `defra.cursor.composite_index`: A composite index serves a cursor by all its fields, or a prefix when unique

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::composite_index::ordering_by_every_field_pages", "cursor::composite_index::a_prefix_of_a_non_unique_index_fails", "cursor::composite_index::a_prefix_of_a_unique_index_pages", "cursor::composite_index::a_prefix_of_a_unique_index_returns_the_lowest_rows", "cursor::composite_index::a_prefix_of_a_unique_index_resumes_after_a_token"]

With an index on User (age, score), `order: [{age: ASC}, {score: ASC}]` pages. If the index is not unique, `order: [{age: ASC}]` alone fails with `no supporting index for cursor order field` (Go's `isUnsupportedCursorCompositePrefix`). If it is unique, `order: [{age: ASC}]` pages: over alice 20, bob 30 and carol 40, `first: 2` returns alice and bob, and `first: 1, after` alice's token returns bob.

# `defra.cursor.seek`: A token resumes by its field's own kind

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::seek::a_date_like_string_pages_as_a_string", "cursor::seek::a_datetime_pages_across_the_boundary"]

A token resumes at the value encoded for the order field's kind. A `String` field holding dates and a `DateTime` field, each indexed and holding jan, feb, mar and apr stamps, both page `first: 2` as [jan, feb] then, `after` the end token, [mar, apr].

# `defra.cursor.filter_index`: Filtering by one index and ordering by another pages consistently

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::filter_index::a_filter_index_and_an_order_index_page_consistently", "cursor::filter_index::order_ties_keep_their_order_across_a_page_boundary"]

With indexes on both `score` and `age`, `filter: {score: {_eq: 90}}, order: [{age: ASC}]` over a 20/90, b 30/90, c 40/90, d 50/80 and e 60/90 returns [a, b], then `after` b's token [c, e]. Rows that tie on the order field keep the order of the query without a token: over six Users with `score` ≥ 80, four of them aged 50, pages of three return the same six rows in the same order as `first: 6`.

# `defra.cursor.reindex`: A `DateTime` index stays usable by a cursor after a lens reindex

- **Parents:** ["defra.cursor"]
- **Scenarios:** ["cursor::reindex::a_datetime_index_stays_cursor_visible_after_a_lens_reindex"]

An index on a `DateTime` field rebuilt by a migration still serves cursor queries with every row: Item `{name: String, created_at: DateTime}` with five rows and an index on `created_at` created after them, patched to a new version and given a `set-default` lens migration from the old one, returns all five rows for `_cursor { Item(first: 100, order: [{created_at: ASC}]) }`, in `created_at` order. Today this fails with `no supporting index for cursor order field` (#2420).
