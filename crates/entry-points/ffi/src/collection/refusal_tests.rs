//! What the collection calls refuse, and the selections `describe` makes
//! beyond a name and a version.

use std::ptr;

use serde_json::Value;

use super::*;
use crate::test_support::{add_collection, close, cstr, err, node, ok, ok_json, query, Named};
use crate::types::CollectionOptions;

fn describe(node: usize, options: CollectionOptions) -> Vec<Value> {
    match ok_json(unsafe { defra_collection_describe(node, options, 0) }) {
        Value::Array(versions) => versions,
        other => panic!("describe answers with an array: {other}"),
    }
}

#[test]
fn describe_selects_by_collection_id() {
    let node = node();
    let created = add_collection(node, "type FindMe { data: String } type Other { x: Int }");
    let find_me = created
        .as_array()
        .unwrap()
        .iter()
        .find(|version| version["Name"] == "FindMe")
        .expect("FindMe was added");
    let collection_id = cstr(find_me["CollectionID"].as_str().unwrap());

    let found = describe(
        node,
        CollectionOptions {
            collection_id: collection_id.as_ptr(),
            ..CollectionOptions::default()
        },
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["Name"], "FindMe");
    assert!(
        found[0].to_string().contains("data"),
        "with its fields: {found:?}"
    );
    close(node);
}

#[test]
fn adding_a_collection_refuses_bad_sdl_and_an_unknown_policy() {
    let node = node();
    for sdl in [
        "type {",
        r#"type User @policy(id: "missing", resource: "users") { name: String }"#,
    ] {
        let sdl = cstr(sdl);
        err(unsafe { defra_collection_add(node, sdl.as_ptr(), 0) });
    }
    assert!(describe(node, CollectionOptions::default()).is_empty());
    close(node);
}

#[test]
fn adding_a_collection_refuses_two_relations_with_one_implicit_name() {
    let node = node();
    let sdl = cstr(
        r#"
        type Book { title: String author: Person reviewer: Person }
        type Person { name: String authoredBooks: [Book] reviewedBooks: [Book] }
        "#,
    );
    let error = err(unsafe { defra_collection_add(node, sdl.as_ptr(), 0) });
    assert!(
        error.contains("relation name is not unique within collection"),
        "{error}"
    );
    close(node);
}

#[test]
fn deleting_refuses_an_empty_name() {
    let node = node();
    add_collection(node, "type Kept { field: String }");
    let empty = cstr("");
    err(unsafe { defra_collection_delete(node, empty.as_ptr(), 0, 0) });
    err(unsafe { defra_collection_delete(node, ptr::null(), 0, 0) });
    assert_eq!(describe(node, Named::new("Kept").options()).len(), 1);
    close(node);
}

#[test]
fn deleting_an_unknown_collection_fails_and_leaves_the_others() {
    let node = node();
    add_collection(node, "type Kept { field: String }");
    let unknown = cstr("Missing");
    err(unsafe { defra_collection_delete(node, unknown.as_ptr(), 0, 0) });
    assert_eq!(describe(node, Named::new("Kept").options()).len(), 1);
    close(node);
}

#[test]
fn truncating_with_a_filter_refuses_what_is_not_a_filter() {
    let node = node();
    add_collection(node, "type Filtered { field: String }");
    query(
        node,
        r#"mutation { add_Filtered(input: {field: "kept"}) { _docID } }"#,
    );
    let filtered = Named::new("Filtered");

    for invalid in ["null", "[]", "{", "\"text\""] {
        let filter = cstr(invalid);
        err(unsafe {
            defra_collection_truncate_with_filter(node, filtered.options(), 0, filter.as_ptr())
        });
    }
    assert_eq!(
        query(node, "{ Filtered { field } }")["data"]["Filtered"][0]["field"],
        "kept"
    );
    close(node);
}

#[test]
fn truncating_an_unknown_collection_fails() {
    let node = node();
    err(unsafe { defra_collection_truncate(node, Named::new("Missing").options(), 0) });
    close(node);
}

#[test]
fn set_active_refuses_an_unknown_version() {
    let node = node();
    add_collection(node, "type Active { data: String }");
    let unknown = cstr("nonexistent-version-id");
    err(unsafe {
        defra_collection_set_active(
            node,
            CollectionOptions {
                version: unknown.as_ptr(),
                ..CollectionOptions::default()
            },
            0,
        )
    });
    close(node);
}

#[test]
fn a_patch_refuses_an_unknown_collection_and_what_is_not_a_patch() {
    let node = node();
    add_collection(node, "type PatchTest { field: String }");
    for patch in [
        r#"[{"op":"add","path":"/NonExistent/Fields/-","value":{"Name":"x","Kind":"Int"}}]"#,
        "not valid json",
        "",
    ] {
        let patch = cstr(patch);
        err(unsafe { defra_collection_patch(node, patch.as_ptr(), ptr::null(), 0) });
    }
    let unchanged = describe(node, Named::new("PatchTest").options());
    assert!(
        !unchanged[0].to_string().contains(r#""Name":"x""#),
        "{unchanged:?}"
    );
    close(node);
}

#[test]
fn a_new_version_from_a_patch_is_found_with_inactive_versions() {
    let node = node();
    add_collection(node, "type Versioned { name: String }");
    let patch = cstr(
        r#"[{"op": "add", "path": "/Versioned/Fields/-", "value": {"Name": "age", "Kind": "Int"}}]"#,
    );
    ok(unsafe { defra_collection_patch(node, patch.as_ptr(), ptr::null(), 0) });

    let versioned = cstr("Versioned");
    let every_version = describe(
        node,
        CollectionOptions {
            name: versioned.as_ptr(),
            get_inactive: 1,
            ..CollectionOptions::default()
        },
    );
    assert_eq!(every_version.len(), 2, "{every_version:?}");
    close(node);
}
