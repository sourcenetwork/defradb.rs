# Building blocks

Self-contained pieces with one job each: a data type, a codec, a protocol, a
client. A crate here is reusable on its own and knows nothing of the database
or its engine.

| Crate | What |
|-------|------|
| [crdt](crdt) | CRDT implementations |
| [inputs](inputs) | Remote inputs: single-flight, batching, a byte-bounded cache, deadlines, retries |
| [orbis](orbis) | Threshold BLS signing (Orbis ring client) |
| [telemetry](telemetry) | OpenTelemetry exporter setup |
| [zanzibar](zanzibar) | Google Zanzibar permission engine |
| [document](document) | Document handling |
| [identity](identity) | Identity and JWT management |
| [net](net) | HTTP, TLS and gRPC on nuclei sockets |
| [chunk-split](chunk-split) | `@chunk` splitter and the parent's chunk list |
