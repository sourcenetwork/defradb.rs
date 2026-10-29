# `go.query.commits.filter.fieldname-eq`: Equality on fieldName

- **Parents:** ["go.query.commits.filter"]
- **Scenarios:** ["query.commits.with-doc-idand-field", "query.commits.with-doc-idand-unknown-field", "query.commits.with-doc-idand-unknown-field-id", "query.commits.with-field", "query.commits.with-field-id"]

`filter: {fieldName: {_eq: "age"}}` (alone or with `docID`) returns only the `age` field commit; a field ID string such as `"1"` or `"999999"`, or an unknown name such as `"not a field"`, matches nothing and returns an empty list rather than an error.

# `go.query.commits.filter.composite`: Filtering on the composite block

- **Parents:** ["go.query.commits.filter"]
- **Scenarios:** ["query.commits.with-composite-field", "query.commits.with-composite-field-id-with-returned-collection-version-id", "query.commits.with-doc-idand-composite-field", "query.commits.with-filter-field-name-not-equal-composite-returns-field-commits"]
- **Go-Bug-Suspect:** "TestQueryCommitsWithCompositeFieldIdWithReturnedCollectionVersionID is marked 'for documentation reasons only. This is not desired behaviour (Users should not be specifying field ids)', though it now filters on '_C'; the comment looks stale"

`filter: {fieldName: {_eq: "_C"}}` returns only the composite commit (with its `collectionVersionId`), with or without `docID`; `{fieldName: {_neq: "_C"}}` returns the `age` and `name` field commits.

# `go.query.one_to_one_to_many.join`: Two-hop resolution in both directions

- **Parents:** ["go.query.one_to_one_to_many"]
- **Scenarios:** ["query.one_to_one_to_many.", "query.one_to_one_to_many.from-secondary", "query.one_to_one_to_many.from-secondary-on-one-to-many", "query.one_to_one_to_many.from-secondary-on-one-to-one"]

With `Indicator`–`Observable` one-to-one (either side primary) and `Observable { observations: [Observation] }`, `Observation { observable { indicator } }` and `Indicator { observable { observations } }` both resolve.
