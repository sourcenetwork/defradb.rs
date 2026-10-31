# `go.shinzo.cursor.token.invalid`: A malformed token fails with `invalid cursor`

- **Parents:** ["go.shinzo.cursor.token"]
- **Scenarios:** ["shinzo::cursor::shinzo_a_malformed_cursor_fails_the_request"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "The two integration tests for bad JSON and a missing docID build their token with padded standard base64 (`bm90IGpzb24=`, `eyJrIjp7ImFnZSI6MjV9fQ==`), which the unpadded base64url decoder already rejects, so they never reach the JSON or docID check; only the unit tests do. The Rust tests should encode those payloads with unpadded base64url."

`after` (or `before`) set to an empty string, a string that is not unpadded base64url (`not-valid-base64!!!`), base64url of something that is not JSON (`not json`), or base64url of JSON without a non-empty `d` (`{"k":{"age":25}}`) fails the whole request with `invalid cursor`.

# `go.shinzo.cursor.forward.first`: `first: N` returns the first N rows and page info

- **Parents:** ["go.shinzo.cursor.forward"]
- **Scenarios:** ["shinzo::cursor::shinzo_first_returns_the_first_rows_and_their_page_info"]
- **Go-Ref:** "shinzo-develop 2a899b860"

On User `{name: String, age: Int @index}`, `_cursor { User(first: N, order: {age: ASC}) { name age } _pageInfo { hasNext hasPrev startCursor endCursor } }` returns the first N rows by age as `_cursor.User`, with `hasPrev: false` and `hasNext` true exactly when more rows follow. With 5 rows (25…45): `first: 3` gives Alice, Bob, Carol and `hasNext: true`; `first: 1` gives Alice and `hasNext: true`; `first: 5` gives all five and `hasNext: false`. With fewer rows than N (`first: 5` on 2 rows, `first: 10` on 3, `first: 100` on 10) every row comes back in order with `hasNext: false`. `first: 0` returns `[]` with `hasNext: true` when rows exist. `after: null` is the same as no `after`. An empty collection returns `User: []`, `hasNext: false`, `hasPrev: false`, and `startCursor: null`, `endCursor: null`.

# `go.shinzo.cursor.forward.after`: `after` resumes after its row, page by page

- **Parents:** ["go.shinzo.cursor.forward"]
- **Scenarios:** ["shinzo::cursor::shinzo_after_resumes_page_by_page"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Passing a page's `endCursor` as `after: $cursor` (a `String` variable) returns the next rows after that row, with `hasPrev: true` whenever `after` is given. Seven rows aged 10…70 paged with `first: 3` come back as [10, 20, 30] (`hasNext: true`, `hasPrev: false`), [40, 50, 60] (`hasNext: true`, `hasPrev: true`), [70] (`hasNext: false`, `hasPrev: true`): every row exactly once, in strictly increasing order. Page sizes can change between requests (`first: 3`, then `5`, then `10` over ten rows gives 3, 5 and the last 2), `first: 1` walks one row per page, and a last page that is exactly full reports `hasNext: false`. `after` the last row returns `[]` with `hasNext: false`, `hasPrev: true`, `startCursor: null`, `endCursor: null`.

# `go.shinzo.cursor.backward.last`: `last: N` returns the final N rows

- **Parents:** ["go.shinzo.cursor.backward"]
- **Scenarios:** ["shinzo::cursor::shinzo_last_returns_the_final_rows"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`User(last: 2, order: {age: ASC})` over ages 25…45 returns Dave 40, Eve 45 (ascending), `hasNext: false`, `hasPrev: true`, and valid start and end cursors. `last: 5` over two rows returns both with `hasPrev: false`. The same holds when ordering by an indexed `String` (`last: 2` by name gives Fred, Islam), `Float` (scores 5.1, 6.3) or `DateTime` field (2003-09-25, 2004-12-01).

# `go.shinzo.cursor.backward.before`: `before` returns the rows just before its row

- **Parents:** ["go.shinzo.cursor.backward"]
- **Scenarios:** ["shinzo::cursor::shinzo_before_returns_the_rows_just_before_its_row"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`User(last: N, before: $cursor, order: {age: ASC})` returns up to N rows immediately before the cursor's row, in ascending order, with `hasNext: true` (the cursor's row follows) and `hasPrev` true exactly when earlier rows remain. Over ages 25…45, `last: 2` before the row aged 40 gives [30, 35]; `last: 10` before 35 gives [25, 30] with `hasPrev: false`; `last: 2` before the first row gives `[]`, `hasNext: true`, `hasPrev: false`. Over ages 10…60, walking backward with `last: 2` from the end gives [50, 60], [30, 40], [10, 20] (the last with `hasPrev: false`). Forward and backward requests mix freely: a forward page's `startCursor` used as `before` returns the rows just before that page. The same holds when ordering by an indexed `String`, `Float` or `DateTime` field.

# `go.shinzo.cursor.backward.docid_only_token`: A token without keys still pages backward

- **Parents:** ["go.shinzo.cursor.backward"]
- **Scenarios:** ["shinzo::cursor::shinzo_a_token_without_keys_still_pages_backward"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A token holding only `{"d": <docID of the row aged 50>}` (no `k`) is still accepted: `User(last: 2, before: $cursor, order: {age: ASC})` over ages 10…50 returns [30, 40] with `hasNext: true` and `hasPrev: true`.

# `go.shinzo.cursor.field_kinds`: String, Float and DateTime order fields

- **Parents:** ["go.shinzo.cursor"]
- **Scenarios:** ["shinzo::cursor::shinzo_string_float_and_datetime_order_fields_page_in_order"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Forward pages over an indexed `name: String @index`, `score: Float @index` or `birthday: DateTime @index` field, created in any order, come back sorted and complete: names Addo, Andy, Chris, Fred, Islam paged by 2 give [Addo, Andy], [Chris, Fred], [Islam]; scores 1.5, 2.7, 3.9, 4.2, 5.8 the same way; birthdays given as `2000-01-15T00:00:00-00:00` … `2004-12-01T00:00:00-00:00` come back as those UTC instants in chronological order, including four dates in the same year (Jan 15, Mar 20, Jun 10, Sep 25) split [Jan, Mar], [Jun, Sep].

# `go.shinzo.cursor.index.required`: An order without a matching index is refused

- **Parents:** ["go.shinzo.cursor.index"]
- **Scenarios:** ["shinzo::cursor::shinzo_an_order_without_a_matching_index_is_refused"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The request fails with an error starting `no supporting index for cursor order field` (when the order field has no index at all, Go appends `. Field: <name>`, e.g. `. Field: age`) when: the order field has no index (`age: Int` without `@index`, `order: {age: ASC}`); it is not the first field of any index (`order: {email: ASC}` with only `@index(includes: [{field: "name"}, {field: "age"}])`); the order covers only a prefix of a non-unique composite index (`order: {name: ASC}` on that index); or the order's directions match neither the composite index nor its reverse (`order: [{name: ASC}, {age: ASC}]` on `@index(includes: [{field: "name", direction: DESC}, {field: "age"}])`). Ordering by `_docID` or not ordering at all needs no index.

# `go.shinzo.cursor.index.scan`: A page reads only the rows it needs from the index

- **Parents:** ["go.shinzo.cursor.index"]
- **Scenarios:** ["shinzo::cursor::shinzo_a_page_reads_only_the_rows_it_needs_from_the_index"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`@explain(type: execute)` of a cursor query ordered on an indexed field reports `indexFetches` equal to the page size plus one probe row (for `hasNext`/`hasPrev`), or the whole collection when it is smaller: `first: 3` over five rows fetches 4 (for an `Int`, `String`, `Float`, `DateTime` or two-field composite index alike), `first: 5` over five rows inserted in descending order fetches 5 and returns them ascending, `last: 2` over three rows fetches 3, and `last: 2, before` the row aged 50 over ages 10…50 seeks backward from the cursor and fetches 3, returning [30, 40]. A cursor query with no `order` (`User(first: 3)`) does not use the index (`indexFetches: 0`).

# `go.shinzo.cursor.index.direction`: An index serves both directions

- **Parents:** ["go.shinzo.cursor.index"]
- **Scenarios:** ["shinzo::cursor::shinzo_an_index_serves_both_directions"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A single-field index serves an order in either direction, scanning it forward or in reverse: `order: {age: DESC}` on `age: Int @index` and `order: {age: ASC}` on `age: Int @index(direction: DESC)` both succeed and use the index (`indexFetches: 3` for `first: 3` over three rows, 4 for `first: 3` over five rows on a matching DESC index). Over ages 25…45 paged by `first: 2` with `after`, DESC order gives [45, 40], [35, 30], [25] and ASC order on a DESC index gives [25, 30], [35, 40], [45], each last page with `hasNext: false`.

# `go.shinzo.cursor.composite`: Ordering by every field of a composite index

- **Parents:** ["go.shinzo.cursor"]
- **Scenarios:** ["shinzo::cursor::shinzo_a_composite_order_resumes_within_a_shared_name"]
- **Go-Ref:** "shinzo-develop 2a899b860"

On `User @index(includes: [{field: "name"}, {field: "age"}])` with (Addo 25), (Addo 35), (Andy 30), (Chris 40), (Chris 45), `order: [{name: ASC}, {age: ASC}]` with `first: 3` returns Addo 25, Addo 35, Andy 30 and `hasNext: true`; paging by `first: 2` with `after` gives [Addo 25, Addo 35], [Andy 30, Chris 40], [Chris 45], so rows sharing a name resume correctly by age.

# `go.shinzo.cursor.shape`: Aliases and groupBy inside `_cursor`

- **Parents:** ["go.shinzo.cursor"]
- **Scenarios:** ["shinzo::cursor::shinzo_aliases_and_group_by_inside_a_cursor"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`_cursor { users: User(first: 2, order: {age: ASC}) { name age } _pageInfo { hasNext hasPrev } }` returns the rows under `_cursor.users` (Alice 25, Bob 30, `hasNext: true`, `hasPrev: false`). `User(first: 10, groupBy: [age]) { age }` over ages 25, 25, 35, 45 returns the groups `{age: 25}`, `{age: 35}`, `{age: 45}` (in any order) with `hasNext: false`, `hasPrev: false`.

# `go.shinzo.cursor.forward.filter`: A filter applies before paging

- **Parents:** ["go.shinzo.cursor.forward"]
- **Scenarios:** ["shinzo::cursor::shinzo_a_filter_applies_before_paging", "shinzo::cursor::shinzo_an_unsorted_in_filter_keeps_the_order_without_a_cursor"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`User(first: 2, filter: {age: {_geq: 40}}, order: {age: ASC})` over ages 15…75 returns [45, 55] then, with `after`, [65, 75] and `hasNext: false`; over ages 10…100 with `_geq: 80` it returns [80, 90] then [100]. With `filter: {age: {_in: [55, 25, 45, 35]}}` over ages 25…65 the pages are [25, 35] and [45, 55], so the cursor boundary holds even when the filter's values are unsorted, and 65 never appears.
