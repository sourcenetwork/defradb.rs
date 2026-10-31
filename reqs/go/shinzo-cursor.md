# `go.shinzo.cursor`: Cursor pagination with `_cursor`

- **Parents:** ["go.shinzo"]

A `_cursor { <Collection>(first|after|last|before, order, filter, groupBy) { … } _pageInfo { hasNext hasPrev startCursor endCursor } }` block wraps exactly one collection query and pages through it by opaque cursor tokens instead of offsets. The parser and request validation reject a block with no collection query (`_cursor block must contain exactly one collection query`), more than one (`_cursor block cannot contain multiple collection queries`), a negative `first` or `last` (`first must be non-negative`, `last must be non-negative`), an empty `after` or `before` string (`invalid cursor`), and `first`/`after` mixed with `last`/`before` (`forward parameters (first/after) cannot be combined with backward parameters (last/before)`).

# `go.shinzo.cursor.token`: Cursor tokens

- **Parents:** ["go.shinzo.cursor"]

The opaque string handed out as `startCursor`/`endCursor` and accepted as `after`/`before`.

# `go.shinzo.cursor.forward`: `first` and `after`

- **Parents:** ["go.shinzo.cursor"]

Forward pagination in the requested order.

# `go.shinzo.cursor.backward`: `last` and `before`

- **Parents:** ["go.shinzo.cursor"]

Backward pagination: the page is taken from the end, and rows still come back in the requested order.

# `go.shinzo.cursor.index`: Index use and index requirements

- **Parents:** ["go.shinzo.cursor"]

A cursor query with an `order` must be served by an index on that order.
