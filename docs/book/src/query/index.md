# Querying

defradb.rs is queried in GraphQL. The pipeline from a GraphQL string to results is
classical (parse, plan, execute), and the interesting parts are where defradb.rs's
storage decisions show up: index selection against part layouts, permission filters as
plan nodes, and history queries that walk the DAG.

- [The GraphQL pipeline](pipeline.md): parse, map, plan, execute; ACP placement; pooled
  execution.
- [Indexes](indexes.md): secondary, unique, full-text and vector indexes, and how they
  are maintained.
- [History: `_commits` and time travel](history.md): commit queries and versioned reads.
