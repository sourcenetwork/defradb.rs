# `go.shinzo.p2p.replication_filter.field_values`: The fields the filter is given

- **Parents:** ["go.shinzo.p2p.replication_filter"]
- **Scenarios:** ["db::merge::replication_filter::the_filter_is_given_the_values_the_commit_writes", "db::merge::replication_filter::an_encrypted_or_undecodable_value_is_left_out"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`fields` maps each field written in the head commit to its CBOR-decoded value, read from the head block itself and from the field blocks it links. A non-negative integer decodes as an unsigned 64-bit integer: a head linking `blockNumber` = 25951718 and `address` = `"0xabc"` gives `{blockNumber: 25951718 (uint64), address: "0xabc"}`. A field whose block is encrypted, whose value does not decode, or that has not arrived is left out rather than failing the document. defradb.rs asks the filter at merge, once the commit's blocks are local and before anything of the document is stored; a refusal is the head's terminal skip `refused by the replication filter`.
