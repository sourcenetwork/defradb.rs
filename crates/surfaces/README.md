# Surfaces

The interfaces a client talks to: an API served over a wire or to a host
runtime. A crate here exposes the database to the outside world; it translates
requests into database calls, and adds no database behaviour of its own.

| Crate | What |
|-------|------|
| [http](http) | HTTP API server |
| [wasm](wasm) | Browser client (WebAssembly) |
| [p2p-adapter](p2p-adapter) | The P2P operation surface every edge calls, and its adapters |
