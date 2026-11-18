//! The `_sized` entry points: options read from the caller's size, defaults
//! for what an older caller leaves out, refusals for what this library does
//! not know, and the calls they merged.

use std::mem::size_of;
use std::ptr;

use super::collection::{defra_collection_truncate_sized, DefraCollectionTruncateOptions};
use super::document::{
    defra_document_add_sized, defra_document_get_sized, DefraDocumentAddOptions,
    DefraDocumentGetOptions,
};
use super::identity::{defra_identity_new_remote_sized, DefraRemoteIdentityOptions};
use super::node::{defra_version_get_sized, DefraVersionOptions};
use super::schema::{defra_print_dump_sized, DefraPrintDumpOptions};
use super::timeout;
use crate::test_support::{add_collection, close, cstr, err, node, ok, ok_json, query, take};

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

    // A caller whose struct ends after `collection_name`.
    let get = DefraDocumentGetOptions {
        collection_name: user.as_ptr(),
        show_deleted: 1,
        ..zeroed()
    };
    let older = (size_of::<usize>() * 2) as u32;
    let doc_id = cstr(&id);
    let found = ok_json(unsafe { defra_document_get_sized(node, doc_id.as_ptr(), &get, older) });
    assert_eq!(found["name"], "Ada", "{found}");
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
