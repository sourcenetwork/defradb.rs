//! `defradb version` names the build in Go DefraDB's shape: the plain line,
//! the `--full` lines and the `--format json` keys Go prints.

use integration_test::actions::NodeVersion;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

/// The `--full` lines Go fixes, which both runtimes print.
fn go_full_lines() -> Vec<Value> {
    vec![
        m::substring("\n* HTTP API: v1\n"),
        m::substring("\n* P2P multicodec: /defra/0.0.1\n"),
        m::substring("\n* DocID versions: 1"),
    ]
}

/// The `--format json` keys Go fixes, with Go's values.
fn go_json(build: Value) -> Value {
    let mut keys = json!({
        "httpAPI": "v1",
        "docIDVersions": "1",
        "netProtocol": "/defra/0.0.1",
    });
    for key in ["release", "commit", "commitDate"] {
        keys[key] = build.clone();
    }
    keys
}

/// A Rust build names its release, commit and Rust toolchain everywhere.
fn rust_version() -> Value {
    let mut full = go_full_lines();
    full.extend([
        m::substring("\n* Rust: "),
        m::substring("\n* Commit: "),
        m::substring("\n* Go compat: "),
    ]);
    let mut json = go_json(m::nonempty_string());
    json["version"] = m::nonempty_string();
    json["docIdVersions"] = json!("1");
    json["rust"] = m::nonempty_string();
    json["goCompat"] = m::object(json!({ "commit": m::nonempty_string() }));
    json!({
        "runtime": "rust",
        "text": m::all(json!([
            m::substring("defradb "),
            m::substring(" ("),
            m::substring(") built with Rust "),
        ])),
        "full": m::all(json!(full)),
        "json": m::object(json),
    })
}

/// A Go build, whose release and commit stay empty unless its Makefile set
/// them.
fn go_version() -> Value {
    json!({
        "runtime": "go",
        "text": m::all(json!([m::substring("defradb "), m::substring(") built with Go")])),
        "full": m::all(json!(go_full_lines())),
        "json": m::object(go_json(m::any_of(json!([m::nonempty_string(), ""])))),
    })
}

#[test]
#[mantra_macros::req_test("defra.cli.version")]
fn version_names_the_build_in_go_shape() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![NodeVersion {
            expected_data: m::any_of(json!([rust_version(), go_version()])),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
