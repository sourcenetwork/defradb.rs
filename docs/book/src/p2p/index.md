# Replication

Documents replicate as blocks, over two transport stacks behind one sync model. This
part covers the transports, the push/pull mechanisms, and the discovery and admission
machinery around them.

- [Transports and the peer](transports.md): iroh/QUIC and libp2p, the endpoint task,
  the peer identity.
- [Push and pull](sync.md): PushLog gossip, doc sync, DAG fetching, CAR transfer,
  and how merges run on the pool.
- [Providers, admission, budgets](providers.md): signed service records, allowlists,
  inbound budgets, backpressure.
