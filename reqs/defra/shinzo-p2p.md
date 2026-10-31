# `go.shinzo.p2p.replication_filter.field_values`: The fields the filter is given

- **Parents:** ["go.shinzo.p2p.replication_filter"]
- **Scenarios:** ["db::merge::replication_filter::the_filter_is_given_the_values_the_commit_writes", "db::merge::replication_filter::an_encrypted_or_undecodable_value_is_left_out"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`fields` maps each field written in the head commit to its CBOR-decoded value, read from the head block itself and from the field blocks it links. A non-negative integer decodes as an unsigned 64-bit integer: a head linking `blockNumber` = 25951718 and `address` = `"0xabc"` gives `{blockNumber: 25951718 (uint64), address: "0xabc"}`. A field whose block is encrypted, whose value does not decode, or that has not arrived is left out rather than failing the document. defradb.rs asks the filter at merge, once the commit's blocks are local and before anything of the document is stored; a refusal is the head's terminal skip `refused by the replication filter`.

# `go.shinzo.p2p.retention.gate`: Refused before the document's blocks are written or walked

- **Parents:** ["go.shinzo.p2p.retention"]
- **Scenarios:** ["db::merge::receive_gate::the_floor_refuses_a_create_whose_height_is_at_hand", "db::merge::receive_gate::a_create_names_its_height_block_and_nothing_else_does", "p2p::receive_gate::a_document_below_the_retention_floor_never_arrives", "p2p::receive_gate::a_document_below_the_floor_is_refused_on_its_height_block_alone"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A pushed create is judged on the height block its head links under the height field, read from the first place that has it: a CAR sent with the push or the local store, on arrival; its own fetched CAR; and, when no CAR comes back whole, the height block alone, fetched before the document is walked block by block. It is refused at the first place, and the skip names where: `retentionBeforeFetch` (on arrival; nothing is stored or fetched), `retentionAfterCARFetch` (fetched CAR; none of it is imported), `retentionAfterHeightFetch` (height block fetched alone; `heightFetches: 1`, and none of the document's other blocks are fetched), or `retentionNoHeight` (the value is not a height, e.g. the string `"100"`). A refused document's root is discharged and it is never merged.

Documents at one height share a height block. When their CARs do not come, it is asked of the network once (`heightFetches: 1`) and the other documents wait on that fetch. A height block nobody serves counts `heightFetchMissed: 1`, and the document is walked; the merge then judges it.
