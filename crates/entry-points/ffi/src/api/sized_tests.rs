//! The `_sized` entry points: options read from the caller's size, defaults
//! for what an older caller leaves out, refusals for what this library does
//! not know, and the calls they merged.

use std::mem::size_of;
use std::ptr;

use super::collection::{defra_collection_truncate_sized, DefraCollectionTruncateOptions};
use super::document::{
    defra_document_add_sized, defra_document_delete_sized, defra_document_get_sized,
    DefraDocumentAddOptions, DefraDocumentDeleteOptions, DefraDocumentGetOptions,
};
use super::identity::{defra_identity_new_remote_sized, DefraRemoteIdentityOptions};
use super::node::{defra_version_get_sized, DefraVersionOptions};
use super::schema::{
    defra_backup_export_sized, defra_backup_import_sized, defra_print_dump_sized,
    DefraBackupExportOptions, DefraBackupImportOptions, DefraPrintDumpOptions,
};
use super::timeout;
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node, node_with, ok, ok_json, query, query_as,
    take,
};

fn size_of_val<T>(_: &T) -> u32 {
    size_of::<T>() as u32
}

#[test]
fn null_options_are_every_default() {
    let version = ok(unsafe { defra_version_get_sized(ptr::null(), 0) });
    assert!(!version.is_empty());
}

#[test]
fn options_this_library_does_not_know_are_refused() {
    #[repr(C)]
    struct Newer {
        known: DefraVersionOptions,
        added: u64,
    }
    let newer = Newer {
        known: DefraVersionOptions { full: 0, json: 1 },
        added: 7,
    };
    let error = err(unsafe {
        defra_version_get_sized((&newer as *const Newer).cast(), size_of_val(&newer))
    });
    assert!(
        error.contains("fields this library does not know"),
        "{error}"
    );

    let unset = Newer {
        known: DefraVersionOptions { full: 0, json: 1 },
        added: 0,
    };
    ok_json(unsafe {
        defra_version_get_sized((&unset as *const Newer).cast(), size_of_val(&unset))
    });
}

#[test]
fn an_older_callers_shorter_options_get_defaults_for_the_rest() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let user = cstr("User");
    let json = cstr(r#"{"name": "Ada"}"#);
    let add = DefraDocumentAddOptions {
        collection_name: user.as_ptr(),
        ..zeroed()
    };
    ok(unsafe { defra_document_add_sized(node, json.as_ptr(), &add, size_of_val(&add)) });
    let id = query(node, "{ User { _docID } }")["data"]["User"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();

    let doc_id = cstr(&id);
    let delete = DefraDocumentDeleteOptions {
        collection_name: user.as_ptr(),
        doc_id: doc_id.as_ptr(),
        ..zeroed()
    };
    ok(unsafe { defra_document_delete_sized(node, &delete, size_of_val(&delete)) });

    // `show_deleted` is set, but a caller whose struct ends after
    // `collection_name` never passes it: the library reads it as unset, and
    // the deleted document is not found.
    let get = DefraDocumentGetOptions {
        collection_name: user.as_ptr(),
        show_deleted: 1,
        ..zeroed()
    };
    let older = (size_of::<usize>() * 2) as u32;
    err(unsafe { defra_document_get_sized(node, doc_id.as_ptr(), &get, older) });

    // The whole struct passes it, and the deleted document is answered.
    let found = ok_json(unsafe {
        defra_document_get_sized(node, doc_id.as_ptr(), &get, size_of_val(&get))
    });
    assert_eq!(found["name"], "Ada", "{found}");
    close(node);
}

#[test]
fn a_size_that_cuts_a_field_is_refused() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let user = cstr("User");
    let id = cstr("bae-00000000-0000-0000-0000-000000000000");
    let get = DefraDocumentGetOptions {
        collection_name: user.as_ptr(),
        ..zeroed()
    };
    // 12 bytes is `identity` and half of `collection_name`, a pointer.
    let error = err(unsafe { defra_document_get_sized(node, id.as_ptr(), &get, 12) });
    assert!(error.contains("ends inside a field"), "{error}");
    close(node);
}

#[test]
fn truncate_takes_its_filter_as_an_option() {
    let node = node();
    add_collection(node, "type User { name: String }");
    query(
        node,
        r#"mutation { add_User(input: [{name: "A"}, {name: "B"}, {name: "C"}]) { _docID } }"#,
    );
    let user = cstr("User");
    let filter = cstr(r#"{"name": {"_eq": "A"}}"#);
    let filtered = DefraCollectionTruncateOptions {
        collection_name: user.as_ptr(),
        filter_json: filter.as_ptr(),
        ..zeroed()
    };
    ok(unsafe { defra_collection_truncate_sized(node, &filtered, size_of_val(&filtered)) });
    assert_eq!(
        query(node, "{ User { _docID } }")["data"]["User"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let every = DefraCollectionTruncateOptions {
        collection_name: user.as_ptr(),
        ..zeroed()
    };
    ok(unsafe { defra_collection_truncate_sized(node, &every, size_of_val(&every)) });
    assert_eq!(
        query(node, "{ User { _docID } }")["data"]["User"],
        serde_json::json!([])
    );
    close(node);
}

#[test]
fn a_remote_identity_needs_exactly_one_signer() {
    let did = cstr("did:key:z");
    let none: DefraRemoteIdentityOptions = zeroed();
    let mut refused =
        unsafe { defra_identity_new_remote_sized(did.as_ptr(), &none, size_of_val(&none)) };
    assert_ne!(refused.status, 0);
    unsafe { crate::types::defra_new_identity_result_free(&mut refused) };

    unsafe extern "C" fn sign(
        _: usize,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: *const u8,
        _: usize,
        _: *mut u8,
        _: usize,
        _: *mut usize,
    ) -> std::ffi::c_int {
        1
    }
    unsafe extern "C" fn sign_async(
        _: usize,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: *const u8,
        _: usize,
        _: usize,
    ) -> std::ffi::c_int {
        1
    }
    let both = DefraRemoteIdentityOptions {
        sign: Some(sign),
        sign_async: Some(sign_async),
        ..zeroed()
    };
    let mut refused =
        unsafe { defra_identity_new_remote_sized(did.as_ptr(), &both, size_of_val(&both)) };
    assert_ne!(refused.status, 0);
    unsafe { crate::types::defra_new_identity_result_free(&mut refused) };
}

#[test]
fn a_sync_timeout_is_milliseconds() {
    assert!(matches!(timeout(0), Ok(None)));
    let Ok(Some(limit)) = timeout(1500) else {
        panic!("1500 ms is a timeout");
    };
    assert_eq!(limit.to_str().unwrap(), "1500ms");
    let (_, outcome) = take(timeout(-1).unwrap_err());
    assert!(outcome.unwrap_err().contains("must not be negative"));
}

#[test]
fn a_sync_timeout_too_long_for_a_go_duration_is_the_longest_one() {
    for ms in [i64::MAX, i64::MAX / 1_000_000 + 1, 10_000_000_000_000] {
        let Ok(Some(limit)) = timeout(ms) else {
            panic!("{ms} ms is a timeout");
        };
        let limit = limit.to_str().unwrap();
        let nanos = crate::duration::parse_go_duration(limit)
            .unwrap_or_else(|error| panic!("{ms} ms: {error}"));
        // About 292 years: within a millisecond of `i64::MAX` nanoseconds (the
        // parser works in floating point, so the last digits may round).
        assert!(nanos > i64::MAX - 1_000_000, "{ms} ms: {nanos} ns");
    }
}

#[test]
fn the_dump_takes_a_caller() {
    let node = node();
    let none = DefraPrintDumpOptions { identity: 0 };
    ok(unsafe { defra_print_dump_sized(node, &none, size_of_val(&none)) });
    let unknown = DefraPrintDumpOptions { identity: 0xdead };
    err(unsafe { defra_print_dump_sized(node, &unknown, size_of_val(&unknown)) });
    close(node);
}

/// Every option at its default, as `{0}` gives a C caller.
fn zeroed<T>() -> T
where
    T: crate::options::ZeroDefault,
{
    // SAFETY: `ZeroDefault` promises all-zero bytes are a valid `T`.
    unsafe { std::mem::zeroed() }
}

/// A document access control policy: a document's owner reads it, and
/// nobody else does without a relationship.
const OWNER_POLICY: &str = r#"
name: Owner Policy
resources:
  - name: users
    relations:
      - name: viewer
    permissions:
      - name: read
        expr: viewer
"#;

/// A new identity's handle.
fn new_identity() -> usize {
    let result = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0);
    result.identity_ptr
}

/// A node whose `User` collection is under `OWNER_POLICY`, added by `owner`.
fn node_with_owned_users(owner: usize) -> usize {
    let node = node_with(local_options());
    let policy = cstr(OWNER_POLICY);
    let added =
        ok_json(unsafe { crate::acp::defra_acp_dac_policy_add(node, owner, policy.as_ptr()) });
    let policy_id = added["PolicyID"].as_str().expect("the policy has an ID");
    let sdl = cstr(&format!(
        r#"type User @policy(id: "{policy_id}", resource: "users") {{ name: String }}"#
    ));
    ok(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), owner) });
    node
}

/// The JSON `node` exports to `file` as `identity`.
fn export_as(node: usize, file: &std::path::Path, identity: usize) -> serde_json::Value {
    let path = cstr(&file.to_string_lossy());
    let opts = DefraBackupExportOptions {
        identity,
        ..zeroed()
    };
    ok(unsafe { defra_backup_export_sized(node, path.as_ptr(), &opts, size_of_val(&opts)) });
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

#[test]
fn a_backup_export_runs_as_the_caller() {
    let owner = new_identity();
    let node = node_with_owned_users(owner);
    query_as(
        node,
        r#"mutation { add_User(input: {name: "Ada"}) { _docID } }"#,
        owner,
    );
    let directory = tempfile::tempdir().unwrap();

    let as_owner = export_as(node, &directory.path().join("owner.json"), owner);
    assert_eq!(as_owner["User"][0]["name"], "Ada", "{as_owner}");

    let as_nobody = export_as(node, &directory.path().join("nobody.json"), 0);
    let exported = as_nobody["User"].as_array().map_or(0, Vec::len);
    assert_eq!(
        exported, 0,
        "a caller with no identity exports no owned document: {as_nobody}"
    );

    close(node);
    defra_identity_free(owner);
}

#[test]
fn a_backup_import_runs_as_the_caller() {
    let owner = new_identity();
    let other = new_identity();
    let source = node_with_owned_users(owner);
    query_as(
        source,
        r#"mutation { add_User(input: {name: "Ada"}) { _docID } }"#,
        owner,
    );
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("backup.json");
    export_as(source, &file, owner);
    close(source);

    let target = node_with_owned_users(owner);
    let path = cstr(&file.to_string_lossy());
    let opts = DefraBackupImportOptions { identity: owner };
    ok(unsafe { defra_backup_import_sized(target, path.as_ptr(), &opts, size_of_val(&opts)) });

    // The import created the document as `owner`, who reads it; a document
    // imported with no identity would have no owner, and `other` would read
    // it too.
    let read = |who| query_as(target, "{ User { name } }", who)["data"]["User"].clone();
    assert_eq!(read(owner), serde_json::json!([{"name": "Ada"}]));
    assert_eq!(read(other), serde_json::json!([]));

    close(target);
    defra_identity_free(owner);
    defra_identity_free(other);
}
