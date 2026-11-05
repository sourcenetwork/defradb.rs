//! KV snapshot frames in the core (`go.shinzo.kv_snapshot`). Importing Go
//! Shinzo's snapshots is `tools/integration-test/tests/shinzo/kv_snapshot.rs`.

use db::kv_snapshot::frame::{read_entries, Entry};
use db::kv_snapshot::KvSnapshotError;

/// One frame as Go Shinzo's `writeKVEntry` writes it: `store` and `key` as
/// the frame's key.
fn frame(store: u8, key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut bytes = ((key.len() + 1) as u32).to_be_bytes().to_vec();
    bytes.push(store);
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
    bytes
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.format")]
fn frames_read_back_and_a_cut_frame_is_an_error() {
    let mut bytes = frame(b'd', b"/k", b"value");
    bytes.extend(frame(b'b', b"cid", b""));
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(b"after the end mark");
    let mut entries = read_entries(&bytes[..]);
    assert_eq!(
        entries.next_entry().unwrap(),
        Some(Entry {
            store: b'd',
            key: b"/k".to_vec(),
            value: b"value".to_vec()
        })
    );
    assert_eq!(entries.next_entry().unwrap().unwrap().store, b'b');
    assert_eq!(entries.next_entry().unwrap(), None);

    // A clean end between frames is a complete stream; a cut frame is not.
    let whole = bytes.len() - b"after the end mark".len() - 4;
    assert!(read_entries(&bytes[..whole])
        .next_entry()
        .unwrap()
        .is_some());
    for cut in [2, 6, 8, 14] {
        let mut entries = read_entries(&bytes[..cut]);
        assert!(
            matches!(entries.next_entry(), Err(KvSnapshotError::Truncated)),
            "cut at {cut}"
        );
    }
}
