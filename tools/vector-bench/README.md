# vector-bench

Tools around `crates/db/tests/vector/vector_disk_cost.rs`, the on-disk vector
index benchmark. Standalone: not a workspace member.

| Tool | What it does |
|------|--------------|
| `convert <dir>` | VectorDBBench parquet (`train`, `test`, `neighbors`) to `base.f32`, `queries.f32`, `gt10.u64` |
| `mkwarm <base.f32> <dims> <count> <out.f32>` | Warm-up queries that are not in the test set |
| `usearch_ref <base.f32> <bench dir> <M> <ef_construction>` | In-memory usearch HNSW on the same queries and truth |
| `usearch_view build\|view ...` | usearch M 32 saved, then served by mmap |
| `capped.sh` | One query process under a cgroup memory cap, swap off, cold cache |

The Cohere 1M corpus:

```bash
mkdir -p cohere && cd cohere
for f in train test neighbors; do curl -sSfO https://assets.zilliz.com/benchmark/cohere_medium_1m/$f.parquet; done
cargo run --release --bin convert -- .
```
