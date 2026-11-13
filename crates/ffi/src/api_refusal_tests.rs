//! What the view, lens, encrypted-index, query and batch calls refuse.

use std::ptr;

use crate::test_support::{add_collection, close, cstr, err, node, ok, ok_json, take};

#[test]
fn a_view_needs_a_query_and_sdl_that_parses() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let view_query = cstr("User { name }");
    let sdl = cstr("type UserView { name: String }");

    err(unsafe { crate::view::defra_view_add(node, ptr::null(), sdl.as_ptr(), ptr::null(), 0) });
    // A null SDL reads as Go's empty string: it defines no view.
    let added = ok_json(unsafe {
        crate::view::defra_view_add(node, view_query.as_ptr(), ptr::null(), ptr::null(), 0)
    });
    assert_eq!(added, serde_json::json!([]));
    let bad_sdl = cstr("type {");
    err(unsafe {
        crate::view::defra_view_add(node, view_query.as_ptr(), bad_sdl.as_ptr(), ptr::null(), 0)
    });
    close(node);
}

#[test]
fn lens_calls_need_a_node_and_a_lens() {
    let lens = cstr(r#"{"Path": "/path/to/transform.wasm"}"#);
    err(unsafe { crate::lens::defra_lens_add(0, 0, lens.as_ptr()) });
    err(crate::lens::defra_lens_list(0, 0));

    let node = node();
    err(unsafe { crate::lens::defra_lens_add(node, 0, ptr::null()) });
    close(node);
}

#[test]
fn an_encrypted_index_needs_a_field_that_exists_and_only_one_per_field() {
    let node = node();
    add_collection(node, "type Account { email: String }");
    let collection = cstr("Account");

    let missing = cstr("nonexistent");
    let error = err(unsafe {
        crate::encrypted_index::defra_encrypted_index_new(
            node,
            collection.as_ptr(),
            missing.as_ptr(),
            ptr::null(),
            0,
        )
    });
    assert!(error.contains("non-existent field"), "{error}");

    let email = cstr("email");
    let new = || unsafe {
        crate::encrypted_index::defra_encrypted_index_new(
            node,
            collection.as_ptr(),
            email.as_ptr(),
            ptr::null(),
            0,
        )
    };
    ok(new());
    let error = err(new());
    assert!(error.contains("already exists"), "{error}");

    let unknown = cstr("Unknown");
    err(unsafe {
        crate::encrypted_index::defra_encrypted_index_new(
            node,
            unknown.as_ptr(),
            email.as_ptr(),
            ptr::null(),
            0,
        )
    });
    close(node);
}

#[test]
fn deleting_an_encrypted_index_that_does_not_exist_fails() {
    let node = node();
    add_collection(node, "type Employee { salary: Int }");
    let collection = cstr("Employee");
    let field = cstr("salary");
    err(unsafe {
        crate::encrypted_index::defra_encrypted_index_delete(
            node,
            collection.as_ptr(),
            field.as_ptr(),
            0,
        )
    });
    let listed = ok_json(unsafe {
        crate::encrypted_index::defra_encrypted_index_list(node, collection.as_ptr(), 0)
    });
    assert_eq!(listed, serde_json::json!([]), "{listed}");
    close(node);
}

#[test]
fn a_query_needs_a_request() {
    let node = node();
    let (status, outcome) = take(unsafe {
        crate::query::defra_query_execute(node, ptr::null(), 0, ptr::null(), ptr::null())
    });
    assert!(
        status == 1
            || outcome
                .as_deref()
                .is_ok_and(|value| value.contains("errors")),
        "a null request runs nothing: {status} {outcome:?}"
    );
    close(node);
}

/// A batch call on a handle that names nothing says so, as every other call
/// does, rather than reporting a missing signing identity.
#[test]
fn batch_calls_refuse_an_invalid_handle() {
    assert_eq!(
        err(crate::batch::defra_batch_start(0, 0)),
        crate::target::ERR_INVALID_HANDLE
    );
    assert_eq!(
        err(crate::batch::defra_batch_sign(0, 0)),
        crate::target::ERR_INVALID_HANDLE
    );
}
