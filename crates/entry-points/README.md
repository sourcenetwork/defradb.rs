# Entry points

The ways a program gets a running DefraDB: the `defradb` binary, a node to embed
in a Rust application, and the C ABI. A crate here assembles the levels below
into something a user starts; nothing in the workspace builds on top of it.

| Crate | What |
|-------|------|
| [ffi](ffi) | C-compatible FFI bindings |
| [cli](cli) | The `defradb` command-line interface |
| [defra-node](defra-node) | The node assembly: `NodeBuilder`, pinc, the FFI base |
