use std::ffi::CStr;

use super::*;
use crate::test_support::{add_collection, close, cstr, local_options, node_with, ok_json, query};
use crate::types::defra_new_node_result_free;

fn refused(options: NodeInitOptions) -> String {
    let result = unsafe { defra_new_node_or_error(options) };
    result.expect_err("the node must not start")
}

/// `defra_node_new`'s error, or its handle.
unsafe fn defra_new_node_or_error(options: NodeInitOptions) -> Result<usize, String> {
    let mut result = defra_node_new(options);
    if result.status == 0 {
        return Ok(result.node_ptr);
    }
    let error = CStr::from_ptr(result.error).to_string_lossy().into_owned();
    defra_new_node_result_free(&mut result);
    Err(error)
}

#[test]
fn options_report_what_the_node_runs_with_and_redact_secrets() {
    let key = [7u8; 32];
    let node = node_with(NodeInitOptions {
        enable_signing: 1,
        searchable_encryption_key: key.as_ptr(),
        searchable_encryption_key_len: key.len() as i32,
        max_transaction_retries: 9,
        ..local_options()
    });
    let options = ok_json(defra_node_options(node));
    assert_eq!(options["DisableP2P"], true);
    assert_eq!(options["DB"]["EnableSigning"], true);
    assert_eq!(options["DB"]["MaxTxnRetries"], 9);
    assert_eq!(options["DB"]["SearchableEncryptionKey"], "<redacted>");
    assert_eq!(options["Store"]["BadgerInMemory"], true);
    close(node);
}

#[test]
fn an_option_not_supported_yet_fails_the_node_instead_of_being_ignored() {
    let address = cstr("127.0.0.1:9181");
    let error = refused(NodeInitOptions {
        http_address: address.as_ptr(),
        ..local_options()
    });
    assert!(
        error.contains("not supported yet") && error.contains("HTTP"),
        "{error}"
    );

    let error = refused(NodeInitOptions {
        lens_pool_size: 4,
        ..local_options()
    });
    assert!(error.contains("lens_pool_size"), "{error}");
}

#[test]
fn invalid_options_fail_the_node() {
    let intervals = cstr("5,-1");
    let error = refused(NodeInitOptions {
        replicator_retry_intervals: intervals.as_ptr(),
        ..local_options()
    });
    assert!(error.contains("negative"), "{error}");

    let store_type = cstr("cassandra");
    let path = cstr("unused");
    let error = refused(NodeInitOptions {
        store_type: store_type.as_ptr(),
        db_path: path.as_ptr(),
        ..local_options()
    });
    assert!(error.contains("unknown store type"), "{error}");

    let error = refused(NodeInitOptions {
        enable_node_acp: 1,
        ..local_options()
    });
    assert!(error.contains("node identity"), "{error}");
}

#[test]
fn a_persistent_node_keeps_its_data_across_a_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = cstr(&directory.path().to_string_lossy());
    // Go's store name means regolith.
    let store_type = cstr("badger");
    let options = || NodeInitOptions {
        db_path: path.as_ptr(),
        store_type: store_type.as_ptr(),
        ..local_options()
    };

    let node = node_with(options());
    add_collection(node, "type User { name: String }");
    query(
        node,
        r#"mutation { add_User(input: {name: "kept"}) { _docID } }"#,
    );
    close(node);

    let node = node_with(options());
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"][0]["name"],
        "kept"
    );
    close(node);
}

#[test]
fn an_encrypted_store_reopens_only_with_its_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = cstr(&directory.path().to_string_lossy());
    let key = [3u8; 32];
    let options = |key: &[u8]| NodeInitOptions {
        db_path: path.as_ptr(),
        store_encryption_key: key.as_ptr(),
        store_encryption_key_len: key.len() as i32,
        ..local_options()
    };

    let node = node_with(options(&key));
    add_collection(node, "type User { name: String }");
    close(node);

    let node = node_with(options(&key));
    close(node);

    let other = [9u8; 32];
    refused(options(&other));

    let short = [1u8; 16];
    let error = refused(options(&short));
    assert!(error.contains("32 bytes"), "{error}");
}
