//! DAG-CBOR of the blocks a document write builds (an LWW, counter or
//! composite delta), written straight into one buffer of the exact size.
//!
//! The bytes are the ones `serde_ipld_dagcbor` writes for the same block: map
//! keys in its canonical order (shorter first, then bytewise), integers in
//! their shortest form, a CID as tag 42 over its bytes behind a zero byte.
//! What is left out is its buffering: serde's encoder sorts every map by
//! writing each entry into a buffer of its own, and copies every CID twice,
//! a dozen allocations per field block. Here the key order is fixed by the
//! types, so each block is one pass that counts its length and one that
//! writes it. Any other delta goes through serde.

use cid::Cid;

use crate::block::{
    Block, CompositeDeltaPayload, CounterDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use crate::Result;

const UNSIGNED: u8 = 0;
const NEGATIVE: u8 = 1;
const BYTES: u8 = 2;
const TEXT: u8 = 3;
const ARRAY: u8 = 4;
const MAP: u8 = 5;
const TAG: u8 = 6;

/// The DAG-CBOR tag of a CID.
const CID_TAG: u64 = 42;

/// Where an encoding pass puts its bytes: a counter for the sizing pass, the
/// buffer for the writing pass.
trait Sink {
    fn put(&mut self, bytes: &[u8]);
    fn cid(&mut self, cid: &Cid) -> Result<()>;
}

struct Length(usize);

impl Sink for Length {
    fn put(&mut self, bytes: &[u8]) {
        self.0 += bytes.len();
    }

    fn cid(&mut self, cid: &Cid) -> Result<()> {
        self.0 += cid.encoded_len();
        Ok(())
    }
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }

    fn cid(&mut self, cid: &Cid) -> Result<()> {
        cid.write_bytes(self)?;
        Ok(())
    }
}

/// The deltas this encoder writes.
#[derive(Clone, Copy)]
enum Payload<'block> {
    Lww(&'block LwwDeltaPayload),
    Counter(&'block CounterDeltaPayload),
    Composite(&'block CompositeDeltaPayload),
}

impl<'block> Payload<'block> {
    fn of(delta: &'block CrdtDelta) -> Option<Self> {
        match delta {
            CrdtDelta::Lww(lww) => Some(Self::Lww(lww)),
            CrdtDelta::Counter(counter) => Some(Self::Counter(counter)),
            CrdtDelta::Composite(composite) => Some(Self::Composite(composite)),
            CrdtDelta::Collection(_)
            | CrdtDelta::CollectionSet(_)
            | CrdtDelta::FieldDefinition(_)
            | CrdtDelta::CollectionDefinition(_)
            | CrdtDelta::Immutable(_) => None,
        }
    }
}

/// `block`'s DAG-CBOR, or `None` for a delta this encoder does not write.
pub(crate) fn encode(block: &Block) -> Option<Result<Vec<u8>>> {
    let payload = Payload::of(&block.delta)?;
    let mut length = Length(0);
    if let Err(error) = write_block(&mut length, block, payload) {
        return Some(Err(error));
    }
    let mut out = Vec::with_capacity(length.0);
    Some(write_block(&mut out, block, payload).map(|()| out))
}

/// A data item's head: its major type and its argument, shortest form.
fn head<S>(sink: &mut S, major: u8, value: u64)
where
    S: Sink,
{
    let major = major << 5;
    if let Ok(value) = u8::try_from(value) {
        if value < 24 {
            sink.put(&[major | value]);
        } else {
            sink.put(&[major | 24, value]);
        }
    } else if let Ok(value) = u16::try_from(value) {
        let [a, b] = value.to_be_bytes();
        sink.put(&[major | 25, a, b]);
    } else if let Ok(value) = u32::try_from(value) {
        let [a, b, c, d] = value.to_be_bytes();
        sink.put(&[major | 26, a, b, c, d]);
    } else {
        sink.put(&[major | 27]);
        sink.put(&value.to_be_bytes());
    }
}

fn text<S>(sink: &mut S, value: &str)
where
    S: Sink,
{
    head(sink, TEXT, value.len() as u64);
    sink.put(value.as_bytes());
}

fn bytes<S>(sink: &mut S, value: &[u8])
where
    S: Sink,
{
    head(sink, BYTES, value.len() as u64);
    sink.put(value);
}

fn int<S>(sink: &mut S, value: i64)
where
    S: Sink,
{
    match u64::try_from(value) {
        Ok(value) => head(sink, UNSIGNED, value),
        Err(_) => head(sink, NEGATIVE, (-1 - value) as u64),
    }
}

fn cid<S>(sink: &mut S, cid: &Cid) -> Result<()>
where
    S: Sink,
{
    head(sink, TAG, CID_TAG);
    head(sink, BYTES, cid.encoded_len() as u64 + 1);
    sink.put(&[0]);
    sink.cid(cid)
}

fn link<S>(sink: &mut S, link: &DAGLink) -> Result<()>
where
    S: Sink,
{
    head(sink, MAP, 2);
    text(sink, "link");
    cid(sink, &link.link)?;
    text(sink, "name");
    text(sink, &link.name);
    Ok(())
}

/// The block's map, keys in canonical order: `delta`, `heads`, `links`,
/// `signature`, `encryption`; an absent option has no entry.
fn write_block<S>(sink: &mut S, block: &Block, payload: Payload<'_>) -> Result<()>
where
    S: Sink,
{
    let entries = 1
        + u64::from(block.heads.is_some())
        + u64::from(block.links.is_some())
        + u64::from(block.signature.is_some())
        + u64::from(block.encryption.is_some());
    head(sink, MAP, entries);
    text(sink, "delta");
    delta(sink, payload);
    if let Some(heads) = &block.heads {
        text(sink, "heads");
        head(sink, ARRAY, heads.len() as u64);
        for head in heads {
            cid(sink, head)?;
        }
    }
    if let Some(links) = &block.links {
        text(sink, "links");
        head(sink, ARRAY, links.len() as u64);
        for each in links {
            link(sink, each)?;
        }
    }
    if let Some(signature) = &block.signature {
        text(sink, "signature");
        cid(sink, signature)?;
    }
    if let Some(encryption) = &block.encryption {
        text(sink, "encryption");
        cid(sink, encryption)?;
    }
    Ok(())
}

/// The delta as its one-entry variant map, the payload's keys in canonical
/// order.
fn delta<S>(sink: &mut S, payload: Payload<'_>)
where
    S: Sink,
{
    head(sink, MAP, 1);
    match payload {
        Payload::Lww(lww) => {
            text(sink, "lww");
            head(sink, MAP, 4);
            text(sink, "data");
            bytes(sink, &lww.data);
            text(sink, "priority");
            head(sink, UNSIGNED, lww.priority);
            text(sink, "fieldName");
            text(sink, &lww.field_name);
            text(sink, "collectionVersionID");
            text(sink, &lww.collection_version_id);
        }
        Payload::Counter(counter) => {
            text(sink, "counter");
            head(sink, MAP, 5);
            text(sink, "data");
            bytes(sink, &counter.data);
            text(sink, "nonce");
            int(sink, counter.nonce);
            text(sink, "priority");
            head(sink, UNSIGNED, counter.priority);
            text(sink, "fieldName");
            text(sink, &counter.field_name);
            text(sink, "collectionVersionID");
            text(sink, &counter.collection_version_id);
        }
        Payload::Composite(composite) => {
            text(sink, "composite");
            head(sink, MAP, 3);
            text(sink, "status");
            head(sink, UNSIGNED, u64::from(composite.status));
            text(sink, "priority");
            head(sink, UNSIGNED, composite.priority);
            text(sink, "collectionVersionID");
            text(sink, &composite.collection_version_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use multihash::Multihash;
    use proptest::prelude::*;

    use super::*;
    use crate::block::{CollectionDeltaPayload, DAG_CBOR_CODEC, SHA2_256_CODE};

    /// What serde writes for `block`: the bytes [`encode`] must reproduce.
    fn serde(block: &Block) -> Vec<u8> {
        serde_ipld_dagcbor::to_vec(block).expect("serde encodes every block")
    }

    fn direct(block: &Block) -> Vec<u8> {
        encode(block)
            .expect("an LWW, counter or composite delta")
            .expect("every CID writes into a buffer")
    }

    /// A CIDv1 over DAG-CBOR or raw bytes, or a CIDv0, from one seed byte.
    fn cid_strategy() -> impl Strategy<Value = Cid> {
        (any::<u8>(), 0..3u8).prop_map(|(seed, kind)| {
            let hash = Multihash::<64>::wrap(SHA2_256_CODE, &[seed; 32]).expect("a digest");
            match kind {
                0 => Cid::new_v1(DAG_CBOR_CODEC, hash),
                1 => Cid::new_v1(0x55, hash),
                _ => Cid::new_v0(hash).expect("a SHA2-256 digest"),
            }
        })
    }

    /// Every width a CBOR argument can take, at and around each boundary.
    fn unsigned_strategy() -> impl Strategy<Value = u64> {
        prop_oneof![
            any::<u64>(),
            prop::sample::select(vec![
                0,
                1,
                23,
                24,
                255,
                256,
                65_535,
                65_536,
                u64::from(u32::MAX),
                u64::from(u32::MAX) + 1,
                u64::MAX,
            ]),
        ]
    }

    fn signed_strategy() -> impl Strategy<Value = i64> {
        prop_oneof![
            any::<i64>(),
            prop::sample::select(vec![
                i64::MIN,
                -(1 << 32) - 1,
                -(1 << 32),
                -65_537,
                -65_536,
                -257,
                -256,
                -25,
                -24,
                -1,
                0,
                1,
                i64::MAX,
            ]),
        ]
    }

    fn text_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<char>(), 0..300).prop_map(String::from_iter)
    }

    fn data_strategy() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 0..300)
    }

    fn delta_strategy() -> impl Strategy<Value = CrdtDelta> {
        prop_oneof![
            (
                text_strategy(),
                unsigned_strategy(),
                text_strategy(),
                data_strategy()
            )
                .prop_map(|(field_name, priority, collection_version_id, data)| {
                    CrdtDelta::Lww(LwwDeltaPayload {
                        field_name,
                        priority,
                        collection_version_id,
                        data,
                    })
                }),
            (
                text_strategy(),
                unsigned_strategy(),
                signed_strategy(),
                text_strategy(),
                data_strategy()
            )
                .prop_map(
                    |(field_name, priority, nonce, collection_version_id, data)| {
                        CrdtDelta::Counter(CounterDeltaPayload {
                            field_name,
                            priority,
                            nonce,
                            collection_version_id,
                            data,
                        })
                    }
                ),
            (text_strategy(), unsigned_strategy(), any::<u8>()).prop_map(
                |(collection_version_id, priority, status)| {
                    CrdtDelta::Composite(CompositeDeltaPayload {
                        collection_version_id,
                        priority,
                        status,
                    })
                }
            ),
        ]
    }

    proptest! {
        #[test]
        fn writes_the_bytes_serde_writes(
            delta in delta_strategy(),
            heads in prop::collection::vec(cid_strategy(), 0..4),
            links in prop::collection::vec((text_strategy(), cid_strategy()), 0..4),
            encryption in prop::option::of(cid_strategy()),
            signature in prop::option::of(cid_strategy()),
        ) {
            let links = links
                .into_iter()
                .map(|(name, link)| DAGLink::new(name, link))
                .collect();
            let block = Block::new_with_options(delta, heads, links, encryption, signature);
            let bytes = direct(&block);
            prop_assert_eq!(&bytes, &serde(&block));
            prop_assert_eq!(bytes.len(), bytes.capacity());
        }

        /// Present but empty heads and links are an empty array, as serde
        /// writes them; only the constructor turns them into absent ones.
        #[test]
        fn writes_empty_heads_and_links_as_serde_does(
            delta in delta_strategy(),
            heads in any::<bool>(),
            links in any::<bool>(),
        ) {
            let block = Block {
                delta,
                heads: heads.then(Vec::new),
                links: links.then(Vec::new),
                encryption: None,
                signature: None,
            };
            prop_assert_eq!(direct(&block), serde(&block));
        }
    }

    #[test]
    fn leaves_every_other_delta_to_serde() {
        let block = Block::new(
            CrdtDelta::Collection(CollectionDeltaPayload {
                collection_version_id: "bafkcollection".to_owned(),
                priority: 3,
            }),
            Vec::new(),
            Vec::new(),
        );
        assert!(encode(&block).is_none());
        assert_eq!(
            block.to_dag_cbor().expect("serde encodes it"),
            serde(&block)
        );
    }
}
