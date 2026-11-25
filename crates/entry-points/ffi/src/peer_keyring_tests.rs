//! The host keyring's callbacks, called as the node calls them, and a node
//! that keeps its peer key there.

use std::ffi::{c_char, c_int, CStr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use keyring::Keyring;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

use super::*;

/// A keystore a host keeps: keys by name, and a switch that makes it fail.
struct Store {
    keys: HopscotchMap<String, Vec<u8>, RandomState>,
    locked: AtomicBool,
    writes: AtomicUsize,
}

impl Store {
    fn new() -> Box<Self> {
        Box::new(Self {
            keys: HopscotchMap::with_hasher(RandomState::default()),
            locked: AtomicBool::new(false),
            writes: AtomicUsize::new(0),
        })
    }

    fn context(&self) -> usize {
        self as *const Self as usize
    }
}

/// The store `context` names, and the name it is asked about.
///
/// # Safety
///
/// `context` comes from [`Store::context`] on a live store and `name` is a C
/// string.
unsafe fn store_and_name<'call>(context: usize, name: *const c_char) -> (&'call Store, String) {
    let store = &*(context as *const Store);
    (store, CStr::from_ptr(name).to_str().unwrap().to_owned())
}

const LOCKED: c_int = 7;

unsafe extern "C" fn get(
    context: usize,
    name: *const c_char,
    out_key: *mut u8,
    out_key_capacity: usize,
    out_key_len: *mut usize,
) -> c_int {
    let (store, name) = store_and_name(context, name);
    if store.locked.load(Ordering::SeqCst) {
        return LOCKED;
    }
    let Some(key) = store.keys.get(name.as_str()) else {
        return DEFRA_KEYRING_NOT_FOUND;
    };
    *out_key_len = key.len();
    if key.len() <= out_key_capacity {
        std::ptr::copy_nonoverlapping(key.as_ptr(), out_key, key.len());
    }
    0
}

unsafe extern "C" fn set(
    context: usize,
    name: *const c_char,
    key: *const u8,
    key_len: usize,
) -> c_int {
    let (store, name) = store_and_name(context, name);
    if store.locked.load(Ordering::SeqCst) {
        return LOCKED;
    }
    store.writes.fetch_add(1, Ordering::SeqCst);
    store
        .keys
        .insert(name, std::slice::from_raw_parts(key, key_len).to_vec());
    0
}

unsafe extern "C" fn delete(context: usize, name: *const c_char) -> c_int {
    let (store, name) = store_and_name(context, name);
    if store.locked.load(Ordering::SeqCst) {
        return LOCKED;
    }
    match store.keys.remove(name.as_str()) {
        Some(_) => 0,
        None => DEFRA_KEYRING_NOT_FOUND,
    }
}

fn host_keyring(store: &Store) -> HostKeyring {
    HostKeyring {
        get,
        set,
        delete,
        context: store.context(),
    }
}

fn keyring_options(store: &Store) -> NodeInitOptions {
    NodeInitOptions {
        peer_keyring_get: Some(get),
        peer_keyring_set: Some(set),
        peer_keyring_delete: Some(delete),
        peer_keyring_context: store.context(),
        ..NodeInitOptions::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.node.ffi_peer_keyring.callbacks")]
fn keys_round_trip_through_the_host_callbacks() {
    let store = Store::new();
    let keyring = host_keyring(&store);

    keyring.set("peer-key", &[3; 64]).unwrap();
    assert_eq!(&keyring.get("peer-key").unwrap()[..], &[3; 64]);
    keyring.delete("peer-key").unwrap();

    assert!(matches!(
        keyring.get("peer-key"),
        Err(keyring::Error::NotFound(name)) if name == "peer-key"
    ));
    assert!(matches!(
        keyring.delete("peer-key"),
        Err(keyring::Error::NotFound(_))
    ));
    assert!(matches!(
        keyring.list(),
        Err(keyring::Error::SystemKeyringListNotSupported)
    ));
    assert!(matches!(
        keyring.get("../peer-key"),
        Err(keyring::Error::InvalidKeyName(_))
    ));
}

#[test]
#[mantra_macros::req_test("defra.node.ffi_peer_keyring.callbacks")]
fn a_host_failure_is_an_error_never_a_missing_key() {
    let store = Store::new();
    let keyring = host_keyring(&store);
    store.locked.store(true, Ordering::SeqCst);

    for error in [
        keyring.get("peer-key").unwrap_err(),
        keyring.set("peer-key", &[1]).unwrap_err(),
        keyring.delete("peer-key").unwrap_err(),
    ] {
        assert!(
            matches!(&error, keyring::Error::SystemKeyring(message) if message.contains("failed with status 7")),
            "{error}"
        );
    }

    store.locked.store(false, Ordering::SeqCst);
    store
        .keys
        .insert("big".to_owned(), vec![0; DEFRA_KEYRING_MAX_KEY_LEN + 1]);
    let error = keyring.get("big").unwrap_err().to_string();
    assert!(
        error.contains("host keyring key 'big' is 4097 bytes, more than the 4096 a key may hold"),
        "{error}"
    );
}

#[test]
#[mantra_macros::req_test("defra.node.ffi_peer_keyring.callbacks")]
fn the_three_callbacks_are_set_together() {
    let store = Store::new();
    assert!(from_options(&NodeInitOptions::default()).unwrap().is_none());
    assert!(from_options(&keyring_options(&store)).unwrap().is_some());

    let partial = [
        NodeInitOptions {
            peer_keyring_get: None,
            ..keyring_options(&store)
        },
        NodeInitOptions {
            peer_keyring_set: None,
            ..keyring_options(&store)
        },
        NodeInitOptions {
            peer_keyring_delete: None,
            ..keyring_options(&store)
        },
    ];
    for options in partial {
        assert_eq!(
            from_options(&options).err().as_deref(),
            Some("peer_keyring_get, peer_keyring_set and peer_keyring_delete must be set together")
        );
    }
}

/// Nodes that run libp2p, which a build without the libp2p feature does not
/// have.
#[cfg(feature = "libp2p")]
mod node {
    use std::time::Duration;

    use super::*;
    use crate::node::defra_node_new;
    use crate::p2p::defra_p2p_info;
    use crate::test_support::{close, cstr, local_options, node_with, ok_json, text, wait_for};
    use crate::types::defra_new_node_result_free;

    /// A libp2p node on `path`, listening on a free local port.
    fn options(path: &CStr, listen: &CStr) -> NodeInitOptions {
        NodeInitOptions {
            db_path: path.as_ptr(),
            disable_p2p: 0,
            listening_addresses: listen.as_ptr(),
            ..local_options()
        }
    }

    fn with_keyring(options: NodeInitOptions, store: &Store) -> NodeInitOptions {
        NodeInitOptions {
            peer_keyring_get: Some(get),
            peer_keyring_set: Some(set),
            peer_keyring_delete: Some(delete),
            peer_keyring_context: store.context(),
            ..options
        }
    }

    fn refused(options: NodeInitOptions) -> String {
        let mut result = unsafe { defra_node_new(options) };
        assert_eq!(result.status, 1, "the node must not start");
        let error = text(result.error);
        unsafe { defra_new_node_result_free(&mut result) };
        error
    }

    /// The peer ID `node`'s first address names.
    fn peer_id(node: usize) -> String {
        wait_for(Duration::from_secs(10), || {
            let info = ok_json(defra_p2p_info(node, 0));
            info.as_array()
                .and_then(|addresses| addresses.first())
                .and_then(serde_json::Value::as_str)
                .and_then(|address| address.rsplit("/p2p/").next())
                .map(str::to_owned)
                .ok_or_else(|| format!("the node never listened: {info}"))
        })
    }

    #[test]
    #[mantra_macros::req_test("defra.node.ffi_peer_keyring.identity")]
    fn a_node_keeps_its_peer_key_in_the_host_keyring() {
        let directory = tempfile::tempdir().unwrap();
        let path = cstr(&directory.path().join("node").to_string_lossy());
        let listen = cstr("/ip4/127.0.0.1/tcp/0");
        let store = Store::new();

        let first = node_with(with_keyring(options(&path, &listen), &store));
        let identity = peer_id(first);
        close(first);
        let key = store
            .keys
            .get(keyring::PEER_KEY)
            .expect("the key is in the host keyring");
        assert_eq!(key.len(), 64);
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);

        let again = node_with(with_keyring(options(&path, &listen), &store));
        assert_eq!(peer_id(again), identity, "a restart keeps the identity");
        close(again);
        assert_eq!(
            store.writes.load(Ordering::SeqCst),
            1,
            "and writes no new key"
        );

        let other = tempfile::tempdir().unwrap();
        let other_path = cstr(&other.path().to_string_lossy());
        let from_key = node_with(NodeInitOptions {
            p2p_private_key: key.as_ptr(),
            p2p_private_key_len: key.len() as c_int,
            ..options(&other_path, &listen)
        });
        assert_eq!(
            peer_id(from_key),
            identity,
            "the identity is the keyring's key"
        );
        close(from_key);
    }

    #[test]
    #[mantra_macros::req_test("defra.node.ffi_peer_keyring.fail_closed")]
    fn a_keyring_failure_stops_the_node() {
        let directory = tempfile::tempdir().unwrap();
        let path = cstr(&directory.path().to_string_lossy());
        let listen = cstr("/ip4/127.0.0.1/tcp/0");
        let store = Store::new();
        store.locked.store(true, Ordering::SeqCst);

        let error = refused(with_keyring(options(&path, &listen), &store));
        assert!(
            error.contains("failed to read peer key from keyring: system keyring error: host keyring get of 'peer-key' failed with status 7"),
            "{error}"
        );

        store.locked.store(false, Ordering::SeqCst);
        let node = node_with(with_keyring(options(&path, &listen), &store));
        close(node);
        let error = refused(options(&path, &listen));
        assert!(
            error.contains("this database requires its application-owned peer keyring"),
            "{error}"
        );
    }

    #[test]
    #[mantra_macros::req_test("defra.node.ffi_peer_keyring.fail_closed")]
    fn a_plaintext_peer_key_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = cstr(&directory.path().to_string_lossy());
        let listen = cstr("/ip4/127.0.0.1/tcp/0");
        let plaintext = node_with(options(&path, &listen));
        close(plaintext);
        let store = Store::new();

        let error = refused(with_keyring(options(&path, &listen), &store));
        assert!(
            error.contains(
                "plaintext peerstore /p2p/local-peer-key requires explicit keyring migration"
            ),
            "{error}"
        );
        assert!(store.keys.is_empty(), "nothing is written to the keyring");
    }

    #[test]
    #[mantra_macros::req_test("defra.node.ffi_peer_keyring.callbacks")]
    fn a_node_refuses_part_of_a_keyring() {
        let store = Store::new();
        let error = refused(NodeInitOptions {
            peer_keyring_delete: None,
            ..with_keyring(local_options(), &store)
        });
        assert!(
            error.contains(
                "peer_keyring_get, peer_keyring_set and peer_keyring_delete must be set together"
            ),
            "{error}"
        );
    }
}
