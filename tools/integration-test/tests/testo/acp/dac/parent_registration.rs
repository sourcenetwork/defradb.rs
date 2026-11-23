//! A resource with a `create` rule admits a document only under a parent the
//! creator holds the rule's permission on (`defra.acp.parent_registration`).

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc,
    DeleteDACActorRelationship, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

/// Go's policies have no `create` rule, so its client cannot add this one.
const CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

const POLICY: &str = r#"
name: gated
resources:
- name: nodes
  permissions:
  - name: delete
  - name: read
  - name: update
- name: jobs
  permissions:
  - name: delete
  - name: read
  - name: update
  relations:
  - name: parent
    types:
    - nodes
  create:
    parent: parent
    requires: owner
"#;

fn as_identity(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

fn add_policy() -> Actions {
    AddDACPolicy {
        policy: POLICY.into(),
        expected_data: json!({ "policyID": m::nonempty_string() }),
        common: Common {
            capture: captures([("policy", "/policyID")]),
            ..as_identity(1)
        },
        ..Default::default()
    }
    .into()
}

fn collection(name: &str, directive_arguments: &str, fields: &str) -> Text {
    Text::concat([
        Text::from(format!("type {name} @policy(id: \"")),
        Text::reference("policy"),
        Text::from(format!("\", {directive_arguments}) {{ {fields} }}")),
    ])
}

fn add_collections() -> [Actions; 2] {
    [
        AddCollection {
            sdl: collection("Node", r#"resource: "nodes""#, "name: String"),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: collection(
                "Job",
                r#"resource: "jobs", parentField: "nodeID""#,
                "nodeID: String name: String",
            ),
            ..Default::default()
        }
        .into(),
    ]
}

fn add_node(name: &str, owner: u64, capture: &'static str) -> Actions {
    AddDoc {
        collection: "Node".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..as_identity(owner)
        },
        ..Default::default()
    }
    .into()
}

fn add_job(doc: Value, creator: Common, expected_error: Option<&str>) -> Actions {
    AddDoc {
        collection: "Job".into(),
        doc_map: Some(doc),
        expected_error: expected_error.map(Into::into),
        common: creator,
        ..Default::default()
    }
    .into()
}

fn jobs_seen_by(reader: u64, expected: Value) -> Actions {
    Request {
        request: "query { Job { name } }".into(),
        expected_data: json!({ "Job": expected }),
        common: as_identity(reader),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.acp.parent_registration.create")]
fn a_document_is_registered_only_under_a_parent_its_creator_owns() {
    let [nodes, jobs] = add_collections();
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy().into(),
            nodes.into(),
            jobs.into(),
            add_node("edge", 1, "node").into(),
            add_job(
                json!({ "nodeID": m::r("node"), "name": "stranger" }),
                as_identity(2),
                Some("parent requires 'owner'"),
            )
            .into(),
            add_job(
                json!({ "nodeID": m::r("node"), "name": "anonymous" }),
                Common::default(),
                Some(
                    "parent-authorized registration requires an authenticated creator and ACP provider",
                ),
            )
            .into(),
            add_job(
                json!({ "name": "orphan" }),
                as_identity(1),
                Some("missing parent document ID in 'nodeID'"),
            )
            .into(),
            add_job(
                json!({ "nodeID": m::r("node"), "name": "build" }),
                as_identity(1),
                None,
            )
            .into(),
            jobs_seen_by(1, json!([{ "name": "build" }])).into(),
            jobs_seen_by(2, json!([])).into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.parent_registration.parent_field")]
fn a_collection_names_a_parent_field_exactly_when_its_resource_has_a_create_rule() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy().into(),
            AddCollection {
                sdl: collection("Job", r#"resource: "jobs""#, "nodeID: String name: String"),
                expected_error: Some(
                    "parentField must be supplied exactly when resource 'jobs' declares a create rule"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: collection(
                    "Node",
                    r#"resource: "nodes", parentField: "name""#,
                    "name: String",
                ),
                expected_error: Some(
                    "parentField must be supplied exactly when resource 'nodes' declares a create rule"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: collection(
                    "Job",
                    r#"resource: "jobs", parentField: "missing""#,
                    "nodeID: String name: String",
                ),
                expected_error: Some("parent field 'missing' does not exist on 'Job'".into()),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.parent_registration.immutable")]
fn an_update_cannot_move_a_registered_document_to_another_parent() {
    let [nodes, jobs] = add_collections();
    let update = |doc: Value, expected_error: Option<&str>| -> Actions {
        UpdateDoc {
            collection: "Job".into(),
            doc_id: Text::reference("job"),
            doc_map: doc,
            expected_error: expected_error.map(Into::into),
            common: as_identity(1),
            ..Default::default()
        }
        .into()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy().into(),
            nodes.into(),
            jobs.into(),
            add_node("edge", 1, "node").into(),
            add_node("core", 1, "other").into(),
            AddDoc {
                collection: "Job".into(),
                doc_map: Some(json!({ "nodeID": m::r("node"), "name": "build" })),
                common: Common {
                    capture: captures([("job", "/docID")]),
                    ..as_identity(1)
                },
                ..Default::default()
            }
            .into(),
            update(
                json!({ "nodeID": m::r("other") }),
                Some("parent field 'nodeID' of a registered document cannot change"),
            )
            .into(),
            update(
                json!({ "nodeID": null }),
                Some("parent field 'nodeID' of a registered document cannot change"),
            )
            .into(),
            update(json!({ "nodeID": m::r("node"), "name": "deploy" }), None).into(),
            Request {
                request: "query { Job { name nodeID } }".into(),
                expected_data: json!({ "Job": [{ "name": "deploy", "nodeID": m::r("node") }] }),
                common: as_identity(1),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.parent_registration.last_parent")]
fn a_registered_document_retains_at_least_one_parent_link() {
    let [nodes, jobs] = add_collections();
    let target = |node: &str| Text::concat([Text::from("nodes:"), Text::reference(node)]);
    let unlink = |node: &str, last: bool| DeleteDACActorRelationship {
        collection: "Job".into(),
        doc_id: Text::reference("job"),
        relation: "parent".into(),
        target_actor: Some(target(node)),
        expected_error: last
            .then(|| "invalid relation: cannot delete the last parent relation".into()),
        expected_data: if last {
            Value::Null
        } else {
            json!({ "recordFound": true })
        },
        common: as_identity(1),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy().into(),
            nodes.into(),
            jobs.into(),
            add_node("edge", 1, "first").into(),
            add_node("core", 1, "second").into(),
            add_job(
                json!({ "nodeID": m::r("first"), "name": "build" }),
                Common {
                    capture: captures([("job", "/docID")]),
                    ..as_identity(1)
                },
                None,
            )
            .into(),
            unlink("first", true).into(),
            jobs_seen_by(1, json!([{ "name": "build" }])).into(),
            AddDACActorRelationship {
                collection: "Job".into(),
                doc_id: Text::reference("job"),
                relation: "parent".into(),
                target_actor: Some(target("second")),
                expected_data: json!({ "existedAlready": false }),
                common: as_identity(1),
                ..Default::default()
            }
            .into(),
            unlink("first", false).into(),
            unlink("second", true).into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}
