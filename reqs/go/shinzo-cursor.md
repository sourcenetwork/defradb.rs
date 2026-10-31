# `go.shinzo.cursor`: Cursor pagination with `_cursor`

- **Parents:** ["go.shinzo"]

A `_cursor { <Collection>(first|after|last|before, order, filter, groupBy) { … } _pageInfo { hasNext hasPrev startCursor endCursor } }` block wraps exactly one collection query and pages through it by opaque cursor tokens instead of offsets. The parser and request validation reject a block with no collection query (`_cursor block must contain exactly one collection query`), more than one (`_cursor block cannot contain multiple collection queries`), a negative `first` or `last` (`first must be non-negative`, `last must be non-negative`), an empty `after` or `before` string (`invalid cursor`), and `first`/`after` mixed with `last`/`before` (`forward parameters (first/after) cannot be combined with backward parameters (last/before)`).

# `go.shinzo.cursor.token`: Cursor tokens

- **Parents:** ["go.shinzo.cursor"]

The opaque string handed out as `startCursor`/`endCursor` and accepted as `after`/`before`.

# `go.shinzo.cursor.token.encoding`: A token is unpadded base64url of `{"d": docID, "k": keys}`

- **Parents:** ["go.shinzo.cursor.token"]
- **Go-Tests:** ["internal/cursor/cursor_test.go::TestEncodeDecode_RoundtripWithKeys", "internal/cursor/cursor_test.go::TestEncodeDecode_RoundtripWithoutKeys", "internal/cursor/cursor_test.go::TestEncodeDecode_RoundtripDocIDOnly", "internal/cursor/cursor_test.go::TestEncode_URLSafeCharacters", "internal/cursor/cursor_test.go::TestEncodeDecode_VeryLongDocID", "internal/cursor/cursor_test.go::TestEncodeDecode_SpecialCharactersInKeys", "internal/cursor/cursor_test.go::TestEncodeDecode_NumericKeysPreservation", "internal/cursor/cursor_test.go::TestEncodeDecode_EmptyKeysVsNilKeys"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "cursor token bytes: a token issued by a Go Shinzo node must decode on a Rust node and the reverse, so clients can page across either"

A token is the JSON object `{"d":"<docID>","k":{<order field>: <value>, …}}`, encoded with base64url without padding (Go `base64.RawURLEncoding`), so it never contains `+`, `/` or `=`. `k` is omitted when there are no key values: an empty or absent key map decodes as no keys, and a non-empty one decodes to the same entries. Decoding returns exactly what was encoded: a 504-character docID (`"bae-"` plus 500 characters), key strings with Unicode (`Café 中文`), newlines, tabs and quotes, and numeric keys `25`, `3.14`, `0`, `-42.5` and `9007199254740991` all round-trip unchanged.

# `go.shinzo.cursor.token.keys`: Returned tokens carry the order field values of their row

- **Parents:** ["go.shinzo.cursor.token"]
- **Go-Tests:** ["query/cursor/with_first_after_test.go::TestCursorWithFirstAfter_CursorsAreNonEmptyStrings", "query/cursor/with_first_after_test.go::TestCursorWithFirstAfter_CursorsEncodeIndexKeyValues"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the `k` map names each order field by its field name and holds its JSON value (Int as a JSON number, DateTime as an RFC 3339 string)"
- **Go-Bug-Suspect:** "Go decodes `k` values as JSON float64 and converts back with int64(float) only for nillable Int and parses RFC 3339 only for nillable DateTime fields (internal/planner/scan.go buildCursorSeekKey): an Int beyond 2^53 resumes at the wrong index position, and a non-nillable Int!/DateTime! order field gets a float or string seek value. Untested in Go; write the right behaviour (exact values for every kind)."

With User `{name: String, age: Int @index}` holding Alice 25, Bob 30, Carol 35, `User(first: 2, order: {age: ASC})` returns `startCursor` and `endCursor` as non-empty tokens with a non-empty `d` whose `k` holds `{"age": 25}` and `{"age": 30}`: the first and last rows actually returned, not the input cursor.

# `go.shinzo.cursor.forward`: `first` and `after`

- **Parents:** ["go.shinzo.cursor"]

Forward pagination in the requested order.

# `go.shinzo.cursor.backward`: `last` and `before`

- **Parents:** ["go.shinzo.cursor"]

Backward pagination: the page is taken from the end, and rows still come back in the requested order.

# `go.shinzo.cursor.deleted_docs`: Deleted documents and cursors

- **Parents:** ["go.shinzo.cursor"]
- **Go-Tests:** ["query/cursor/with_edge_cases_test.go::TestCursorEdgeCase_DeletedCursorDocContinuesToNext", "query/cursor/with_edge_cases_test.go::TestCursorEdgeCase_DeletedResultDocExcludedFromResults", "query/cursor/with_edge_cases_test.go::TestCursorEdgeCase_AllRemainingDocsDeletedReturnsEmpty", "query/cursor/with_edge_cases_test.go::TestCursorEdgeCase_DeletedBeforeCursorAtEndHasNoNextPage"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A deleted document never appears in a page, and a cursor whose row was deleted still marks a position. Over ages 25, 30, 35: deleting Bob (30) after taking his `endCursor` makes `first: 2, after` that cursor return [35] with `hasNext: false`, `hasPrev: true`; deleting Bob and Carol after taking Alice's cursor makes `first: 10, after` it return `[]`, `hasNext: false`, `hasPrev: true`, null cursors. Deleting Carol (35) from ages 25…45 makes `first: 5` return [25, 30, 40, 45] with `hasNext: false`. Deleting Eve (45) after taking her `endCursor` makes `last: 2, before` it return [35, 40] with `hasNext: false`, `hasPrev: true`.

# `go.shinzo.cursor.index`: Index use and index requirements

- **Parents:** ["go.shinzo.cursor"]

A cursor query with an `order` must be served by an index on that order.

# `go.shinzo.cursor.explain`: Explain output of a cursor query

- **Parents:** ["go.shinzo.cursor"]
- **Go-Tests:** ["explain/default/with_cursor_test.go::TestDefaultExplainCursorQueryWithFirstOnly", "explain/default/with_cursor_test.go::TestDefaultExplainCursorQueryWithFirstAndAfterNull", "explain/debug/with_cursor_test.go::TestDebugExplainCursorQueryWithFirst", "explain/debug/with_cursor_test.go::TestDebugExplainCursorQueryWithFirstAndAfterNull", "explain/execute/with_cursor_test.go::TestExecuteExplainCursorQueryWithFirst", "explain/execute/with_cursor_test.go::TestExecuteExplainCursorQueryWithFirstReturnsAllWhenFewerExist", "query/cursor/with_backward_explain_test.go::TestCursorBackwardExplain_SimpleShowsLastBeforeFields"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Every explain type shows the plan `selectTopNode` → `cursorNode` → `selectNode` → `scanNode`. Simple explain gives `cursorNode` the attributes `first`, `after`, `last`, `before` and `cursorValue`: `User(first: 5, order: {age: ASC})` and `User(first: 3, after: null, …)` show `first: 5` (or `3`) and the other four `null`; `User(last: 2, …)` shows `last: 2` and `first`, `after`, `before` `null`. When a cursor is given, `cursorValue` is `{docID: <d>}` plus `keys: <k>` when the token has keys. Execute explain of `User(first: 2, order: {age: ASC}) { name }` over three rows reports `executionSuccess: true`, `sizeOfResult: 1`, `planExecutions: 2`, `cursorNode.iterations: 3`, `selectNode.iterations: 3`, `filterMatches: 3`, and `scanNode` `iterations: 3`, `docFetches: 3`, `fieldFetches: 6`, `indexFetches: 3`; with `first: 10` the iteration counts are 4 and the fetch counts are the same.

