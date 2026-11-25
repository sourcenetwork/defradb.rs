# Replication

Documents replicate as blocks, over pinc. This
part covers the transports, the push/pull mechanisms, and the discovery and admission
machinery around them.

- [Transports and the peer](transports.md): pinc, the endpoint task, hubs and topics,
  the peer identity.
- [Push and pull](sync.md): PushLog gossip, doc sync, DAG fetching, CAR transfer,
  and how merges run on the pool.
- [Providers, admission, budgets](providers.md): peer records and exchange, allowlists,
  inbound budgets, backpressure.
