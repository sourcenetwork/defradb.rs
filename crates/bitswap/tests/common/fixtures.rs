use std::collections::BTreeMap;
use std::path::PathBuf;

use asynchronous_codec::{Decoder, Encoder};
use bitswap::{
    BitswapCodec, BitswapHandlerError, BitswapMessage, Block, BlockPresenceType, ProtocolId,
    WantType,
};
use bytes::{Bytes, BytesMut};
use cid::Cid;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use unsigned_varint::codec::UviBytes;

/// Loads a golden fixture, expanding every `data_pattern` back into `data_hex`.
pub fn load(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden fixture missing at {}: {e}", path.display()));
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("bad fixture {}: {e}", path.display()));
    match expand(value) {
        Value::Array(cases) => cases,
        _ => panic!("fixture {} is not an array", path.display()),
    }
}

/// The generator's deterministic block data: byte `i` is `i * 31 + 7 + seed`.
pub fn pattern(len: usize, seed: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 + 7 + seed) as u8).collect()
}

fn expand(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(expand).collect()),
        Value::Object(fields) => {
            let mut out = Map::new();
            for (key, field) in fields {
                if key == "data_pattern" {
                    let len = field["len"].as_u64().unwrap() as usize;
                    let seed = field["seed"].as_u64().unwrap() as usize;
                    out.insert("data_hex".into(), hex::encode(pattern(len, seed)).into());
                } else {
                    out.insert(key, expand(field));
                }
            }
            Value::Object(out)
        }
        other => other,
    }
}

pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str()
        .unwrap_or_else(|| panic!("missing string {k}"))
}

pub fn cid(v: &Value) -> Cid {
    Cid::try_from(s(v, "cid")).unwrap()
}

pub fn codec(protocol: &str) -> BitswapCodec {
    let id = ProtocolId::try_from_str(protocol).unwrap_or_else(|| panic!("protocol {protocol}"));
    let mut l = UviBytes::default();
    l.set_max_len(2 * 1024 * 1024);
    BitswapCodec::new(l, id)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn want(v: &Value) -> WantType {
    match s(v, "want_type") {
        "block" => WantType::Block,
        "have" => WantType::Have,
        o => panic!("want_type {o}"),
    }
}

/// Rebuilds a message from the generator's recorded `build` ops.
pub fn replay(ops: &[Value]) -> BitswapMessage {
    let mut m = BitswapMessage::new(false);
    for op in ops {
        match s(op, "op") {
            "new" => m = BitswapMessage::new(op["full"].as_bool().unwrap()),
            "add_entry" => {
                m.add_entry(
                    cid(op),
                    op["priority"].as_i64().unwrap() as i32,
                    want(op),
                    op["send_dont_have"].as_bool().unwrap(),
                );
            }
            "cancel" => {
                m.cancel(cid(op));
            }
            "add_block" => m.add_block(Block::new(
                Bytes::from(hex::decode(s(op, "data_hex")).unwrap()),
                cid(op),
            )),
            "add_have" => m.add_have(cid(op)),
            "add_dont_have" => m.add_dont_have(cid(op)),
            "set_pending_bytes" => m.set_pending_bytes(op["value"].as_i64().unwrap() as i32),
            o => panic!("unknown op {o}"),
        }
    }
    m
}

pub fn encode(protocol: &str, message: BitswapMessage) -> Vec<u8> {
    let mut out = BytesMut::new();
    codec(protocol).encode(message, &mut out).unwrap();
    out.to_vec()
}

pub fn decode(protocol: &str, frame: &[u8]) -> Result<BitswapMessage, BitswapHandlerError> {
    let mut buf = BytesMut::from(frame);
    codec(protocol)
        .decode(&mut buf)?
        .map(|(m, _)| m)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into())
}

/// The fixture's canonical form: every list sorted by cid string.
pub fn canonical(m: &BitswapMessage) -> Value {
    let wantlist: BTreeMap<String, Value> = m
        .wantlist()
        .map(|e| {
            (
                e.cid.to_string(),
                json!({
                    "cid": e.cid.to_string(),
                    "priority": e.priority,
                    "want_type": if e.want_type == WantType::Block { "block" } else { "have" },
                    "cancel": e.cancel,
                    "send_dont_have": e.send_dont_have,
                }),
            )
        })
        .collect();
    let blocks: BTreeMap<String, Value> = m
        .blocks()
        .map(|b| {
            (
                b.cid().to_string(),
                json!({ "cid": b.cid().to_string(), "data_hex": hex::encode(b.data()) }),
            )
        })
        .collect();
    let presences: BTreeMap<String, Value> = m
        .block_presences()
        .map(|p| {
            (
                p.cid.to_string(),
                json!({
                    "cid": p.cid.to_string(),
                    "type": if p.typ == BlockPresenceType::Have { "have" } else { "dont_have" },
                }),
            )
        })
        .collect();
    json!({
        "full": m.full(),
        "pending_bytes": m.pending_bytes(),
        "wantlist": wantlist.into_values().collect::<Vec<_>>(),
        "blocks": blocks.into_values().collect::<Vec<_>>(),
        "presences": presences.into_values().collect::<Vec<_>>(),
    })
}

/// Asserts `ours` is the golden frame: exact bytes from `frame_hex`, or the same length and
/// digest when the fixture stored `frame_sha256` for a large frame.
pub fn assert_same_frame(case: &Value, ours: &[u8], what: &str) {
    if let Some(golden) = case["frame_hex"].as_str() {
        assert_eq!(hex::encode(ours), golden, "{what}: bytes differ");
    } else {
        assert_eq!(
            ours.len() as u64,
            case["frame_len"].as_u64().unwrap(),
            "{what}: frame length differs"
        );
        assert_eq!(
            sha256_hex(ours),
            s(case, "frame_sha256"),
            "{what}: frame digest differs"
        );
    }
}
