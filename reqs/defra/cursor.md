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
