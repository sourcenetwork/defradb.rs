# `go.shinzo.p2p.retention.gate`: Refused before the document's blocks are written or walked

- **Parents:** ["go.shinzo.p2p.retention"]
- **Scenarios:** ["db::merge::receive_gate::the_floor_refuses_a_create_whose_height_is_at_hand", "db::merge::receive_gate::a_create_names_its_height_block_and_nothing_else_does", "p2p::receive_gate::a_document_below_the_retention_floor_never_arrives", "p2p::receive_gate::a_document_below_the_floor_is_refused_on_its_height_block_alone"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A pushed create is judged on the height block its head links under the height field, read from the first place that has it: a CAR sent with the push or the local store, on arrival; its own fetched CAR; and, when no CAR comes back whole, the height block alone, fetched before the document is walked block by block. It is refused at the first place, and the skip names where: `retentionBeforeFetch` (on arrival; nothing is stored or fetched), `retentionAfterCARFetch` (fetched CAR; none of it is imported), `retentionAfterHeightFetch` (height block fetched alone; `heightFetches: 1`, and none of the document's other blocks are fetched), or `retentionNoHeight` (the value is not a height, e.g. the string `"100"`). A refused document's root is discharged and it is never merged.

Documents at one height share a height block. When their CARs do not come, it is asked of the network once (`heightFetches: 1`) and the other documents wait on that fetch. A height block nobody serves counts `heightFetchMissed: 1`, and the document is walked; the merge then judges it.
