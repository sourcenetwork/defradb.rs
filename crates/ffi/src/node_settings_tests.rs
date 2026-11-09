//! Print dump, the NAC purge and the node settings options, called as a C
//! caller calls them.

use std::ffi::{c_char, CStr, CString};
use std::ptr::null;

use serde_json::Value;

use crate::acp::{enable_nac, get_nac_status, purge_nac_state};
use crate::dump::print_dump;
use crate::node::{max_txn_retries, new_node, node_close};
use crate::schema::add_schema;
use crate::state::NODES;
use crate::types::{defra_free_string, FfiResult, NodeInitOptions};

fn cstr(text: &str) -> CString {
    CString::new(text).unwrap()
}

/// The call's value, or its error; both strings freed.
fn take(result: FfiResult) -> Result<String, String> {
    let read = |ptr: *mut c_char| {
        (!ptr.is_null()).then(|| {
            let text = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { defra_free_string(ptr) };
            text
        })
    };
    let value = read(result.value);
    let error = read(result.error);
    match result.status {
        0 => Ok(value.unwrap_or_default()),
        _ => Err(error.unwrap_or_default()),
    }
}

fn node_with(options: NodeInitOptions) -> Result<usize, String> {
    assert!(crate::runtime::init_runtime());
    let result = new_node(options);
    match result.status {
        0 => Ok(result.node_ptr),
        _ => {
            let error = unsafe { CStr::from_ptr(result.error) }
                .to_string_lossy()
                .into_owned();
            unsafe { defra_free_string(result.error) };
            Err(error)
        }
    }
}

fn nac_status(node: usize, identity: &CStr) -> Value {
    serde_json::from_str(&take(unsafe { get_nac_status(node, identity.as_ptr()) }).unwrap())
        .unwrap()
}

#[test]
fn print_dump_lists_the_stores_keys() {
    let node = node_with(NodeInitOptions::default()).unwrap();
    let sdl = cstr("type DumpUser { name: String }");
    take(unsafe { add_schema(node, null(), sdl.as_ptr()) }).unwrap();
    let lines: Vec<String> =
        serde_json::from_str(&take(unsafe { print_dump(node, null()) }).unwrap()).unwrap();
    assert!(!lines.is_empty(), "a collection writes keys");
    node_close(node);
}

#[test]
fn purge_nac_state_resets_nac() {
    assert!(crate::runtime::init_runtime());
    let identity: Value =
        serde_json::from_str(&take(crate::acp::create_identity()).unwrap()).unwrap();
    let owner = cstr(identity["did"].as_str().unwrap());
    let node = node_with(NodeInitOptions::default()).unwrap();
    take(unsafe { enable_nac(node, owner.as_ptr()) }).unwrap();
    assert_eq!(nac_status(node, &owner)["status"], "enabled");
    take(unsafe { purge_nac_state(node, owner.as_ptr()) }).unwrap();
    assert_eq!(nac_status(node, &owner)["status"], "not configured");
    node_close(node);
}

#[test]
fn max_txn_retries_and_chunk_size_reach_the_database() {
    let default = node_with(NodeInitOptions::default()).unwrap();
    let retries = take(max_txn_retries(default)).unwrap();
    assert!(retries.parse::<u32>().unwrap() > 0);
    node_close(default);

    let node = node_with(NodeInitOptions {
        max_txn_retries: 9,
        chunk_size: 4096,
        ..NodeInitOptions::default()
    })
    .unwrap();
    assert_eq!(take(max_txn_retries(node)).unwrap(), "9");
    let chunk = NODES
        .get(node, |state| state.database.options().chunk_size)
        .unwrap();
    assert_eq!(chunk, Some(4096));
    node_close(node);
}

#[test]
fn the_at_rest_key_must_be_32_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = cstr(dir.path().to_str().unwrap());
    let key = [7u8; 32];
    let node = node_with(NodeInitOptions {
        db_path: path.as_ptr(),
        in_memory: 0,
        at_rest_encryption_key: key.as_ptr(),
        at_rest_encryption_key_len: key.len(),
        ..NodeInitOptions::default()
    })
    .unwrap();
    node_close(node);

    let short = [7u8; 16];
    let error = node_with(NodeInitOptions {
        at_rest_encryption_key: short.as_ptr(),
        at_rest_encryption_key_len: short.len(),
        ..NodeInitOptions::default()
    })
    .unwrap_err();
    assert!(error.contains("32 bytes"), "{error}");
}
