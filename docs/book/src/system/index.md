# The System

This part is the map of the whole. Three chapters:

- **Who owns what**: the layers, from the client over the network edge, through the
  defradb.rs workers, down to [regolith](https://sourcenetwork.github.io/regolith/) and the device. Read this first.
- **Design doctrine**: the invariants every change to defradb.rs must preserve, and the
  reasoning behind them. This is the "why" chapter.
- **A request, end to end**: one GraphQL mutation traced from the client to durable,
  visible state and back, through routing, remote inputs, the transaction, the commit
  ticket, and (when it loses a race) the conflict re-run.

After this part, the rest of the book zooms into each box: the storage engine, then
transactions, reads, writes, documents, queries, access control, threading, replication,
and the remaining subsystems.
