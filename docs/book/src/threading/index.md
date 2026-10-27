# Threading

defradb.rs's execution architecture is thread-per-core: one pinned worker per CPU core,
each running its own executor, with the network on a single separate runtime. This part
explains why that architecture was chosen and how each piece works.

- [Why thread-per-core](why.md): the reasoning, and what it buys.
- [The worker pool](pool.md): workers, inboxes, routing, document affinity, I/O queues.
- [The network edge](edge.md): the one tokio runtime and what lives on it.
- [One thread: wasm and single-core](single.md): the same code on one thread.
