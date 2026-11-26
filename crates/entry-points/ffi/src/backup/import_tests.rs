//! An import's documents are written as `defra_document_add` writes them:
//! signed as the node signs, refused to a caller node access denies, and
//! pushed to a replicator.

use std::ffi::c_int;
use std::ptr;

use serde_json::{json, Value};

use super::defra_backup_import;
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node_with, ok, ok_json, query, query_as,
};
use crate::types::NodeInitOptions;

const USERS: &str = r#"{"User": [{"name": "John"}, {"name": "Bob"}]}"#;

/// A file in a fresh temporary directory holding `content`.
fn file_with(content: &str) -> (tempfile::TempDir, std::ffi::CString) {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("backup.json");
    std::fs::write(&file, content).unwrap();
    let path = cstr(&file.to_string_lossy());
    (directory, path)
}

/// An in-memory node with a `User` collection, signing when `signing` is set.
fn users(signing: bool) -> usize {
    let node = node_with(NodeInitOptions {
        disable_signing: c_int::from(!signing),
        ..local_options()
    });
    add_collection(node, "type User { name: String }");
    node
}

/// The `signature` of every block on `node`.
fn signatures(node: usize) -> Vec<Value> {
    let response = query(node, "{ _commits { signature { type } } }");
    let rows = response["data"]["_commits"]
        .as_array()
        .unwrap_or_else(|| panic!("no commits in {response}"))
        .clone();
    assert!(!rows.is_empty(), "the import wrote blocks");
    rows.into_iter()
        .map(|row| row["signature"].clone())
        .collect()
}

#[test]
fn an_import_on_a_signing_node_signs_every_block() {
    let node = users(true);
    let (_directory, path) = file_with(USERS);
    ok(unsafe { defra_backup_import(node, path.as_ptr(), 0) });
    let signatures = signatures(node);
    assert!(
        signatures.iter().all(|signature| !signature.is_null()),
        "{signatures:?}"
    );
    close(node);
}

#[test]
fn an_import_on_a_node_that_does_not_sign_leaves_its_blocks_unsigned() {
    let node = users(false);
    let (_directory, path) = file_with(USERS);
    ok(unsafe { defra_backup_import(node, path.as_ptr(), 0) });
    let signatures = signatures(node);
    assert!(signatures.iter().all(Value::is_null), "{signatures:?}");
    close(node);
}

#[test]
fn node_access_control_refuses_an_import_to_a_stranger() {
    let identity = || {
        let result = unsafe { defra_identity_new(ptr::null()) };
        assert_eq!(result.status, 0);
        result.identity_ptr
    };
    let (owner, stranger) = (identity(), identity());
    let node = node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    });
    let sdl = cstr("type User { name: String }");
    ok_json(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), owner) });
    let (_directory, path) = file_with(USERS);

    let refusal = err(unsafe { defra_backup_import(node, path.as_ptr(), stranger) });
    assert!(
        refusal.starts_with("not authorized to perform operation"),
        "{refusal}"
    );
    assert_eq!(
        query_as(node, "{ User { name } }", owner)["data"]["User"],
        json!([]),
        "a refused import writes nothing"
    );

    ok(unsafe { defra_backup_import(node, path.as_ptr(), owner) });
    assert_eq!(
        query_as(node, "{ User { name } }", owner)["data"]["User"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    close(node);
    defra_identity_free(owner);
    defra_identity_free(stranger);
}

#[cfg(feature = "p2p")]
mod pinc {
    use std::time::Duration;

    use serde_json::{json, Value};

    use super::{file_with, USERS};
    use crate::backup::defra_backup_import;
    use crate::p2p::{defra_p2p_info, defra_p2p_replicator_add};
    use crate::test_support::{
        add_collection, close, cstr, local_options, node_with, ok, ok_json, query, wait_for,
    };
    use crate::types::NodeInitOptions;

    /// A pinc node on a free loopback port.
    fn p2p_node() -> usize {
        let listen = cstr("127.0.0.1:0");
        node_with(NodeInitOptions {
            disable_p2p: 0,
            listening_addresses: listen.as_ptr(),
            ..local_options()
        })
    }

    /// The loopback address other peers reach `node` at.
    fn address(node: usize) -> String {
        wait_for(Duration::from_secs(10), || {
            let info = ok_json(defra_p2p_info(node, 0));
            info.as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|addr| addr.contains("@127.0.0.1:"))
                .map(str::to_string)
                .ok_or_else(|| format!("the node never listened: {info}"))
        })
    }

    #[test]
    fn an_import_reaches_a_replicator_peer() {
        let source = p2p_node();
        let target = p2p_node();
        for node in [source, target] {
            add_collection(node, "type User { name: String }");
        }
        let collections = cstr("User");
        let target_address = cstr(&address(target));
        ok(unsafe {
            defra_p2p_replicator_add(source, collections.as_ptr(), target_address.as_ptr(), 0)
        });

        let (_directory, path) = file_with(USERS);
        ok(unsafe { defra_backup_import(source, path.as_ptr(), 0) });

        wait_for(Duration::from_secs(20), || {
            let mut names: Vec<String> = query(target, "{ User { name } }")["data"]["User"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|user| user["name"].as_str().map(str::to_string))
                .collect();
            names.sort();
            match names == ["Bob", "John"] {
                true => Ok(()),
                false => Err(format!("the peer still holds {}", json!(names))),
            }
        });

        close(source);
        close(target);
    }
}
