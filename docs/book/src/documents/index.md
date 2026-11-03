# Documents and CRDTs

A document in defradb.rs is two things at once: a *local value* (the materialized fields
a query reads) and a *DAG of blocks* (the replicated, content-addressed history). Both
exist for every document, and they answer different questions: the value answers "what
is it now, here"; the DAG answers "how did it get here, and can others learn the same
history".

- [The document model](model.md): DocIDs, short IDs, the stored format, document parts.
- [Blocks and the DAG](blocks.md): field blocks, composite blocks, signatures,
  encryption links, and the block builder.
- [CRDT types](crdt.md): LWW registers, counters, composites, and the priority algebra.
- [Head sets](head-sets.md): how a document's current heads are tracked without locks.
- [The arrival journal](arrival-journal.md): commit-ordered document arrivals, the dense
  prefix, and the `_documentArrivals` API.
