//! Print dump and the node access control purge, called as a C caller calls
//! them.

use std::ptr;

use crate::acp::{defra_acp_nac_purge, defra_acp_nac_status};
use crate::dump::defra_print_dump;
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::test_support::{add_collection, close, err, local_options, node, node_with, ok, ok_json};
use crate::types::NodeInitOptions;

#[test]
fn print_dump_logs_the_stores_keys() {
    let node = node();
    add_collection(node, "type DumpUser { name: String }");
    ok(defra_print_dump(node));
    close(node);
}

#[test]
fn purging_node_access_control_resets_it() {
    let result = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0);
    let owner = result.identity_ptr;
    let node = node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    });
    assert_eq!(ok_json(defra_acp_nac_status(node, owner))["status"], "enabled");

    let error = err(defra_acp_nac_purge(node, 0));
    assert!(error.contains("requires an identity"), "{error}");

    ok(defra_acp_nac_purge(node, owner));
    assert_eq!(
        ok_json(defra_acp_nac_status(node, owner))["status"],
        "not configured"
    );
    close(node);
    defra_identity_free(owner);
}
