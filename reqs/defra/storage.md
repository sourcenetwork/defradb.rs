# `defra`: Behaviour the Rust version defines

Requirements that start in defradb.rs rather than in a Go integration test.

# `defra.index`: Secondary indexes

- **Parents:** ["defra"]

Index behaviour beyond what Go's integration suite checks.

# `defra.index.unique_writer`: Unique-index errors name only the writer

- **Parents:** ["defra.index"]
- **Scenarios:** ["index::unique_writer::a_unique_index_error_names_the_writer_and_its_value"]

With a unique index on `User.name` and a stored `{"name":"alice","age":1}`, adding `{"name":"alice","age":2}` fails with `can not index a doc's field(s) that violates unique index. DocID: <new document's DocID>, name: alice`. Updating another document's `name` to `alice` fails with `failed to update index: can not index a doc's field(s) that violates unique index. DocID: <updated document's DocID>, name: alice. IndexName: User_name_ASC`. The error never contains the DocID of the document already holding the value, which the writer may not be allowed to read. Go develop builds the same text (`NewErrCanNotIndexNonUniqueFields`, `NewErrUpdateIndex`).
