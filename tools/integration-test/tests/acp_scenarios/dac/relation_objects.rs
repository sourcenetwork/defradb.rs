//! Generated from the conformance scenarios of tests/integration/acp/dac/relation_objects_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn acp_query_many_to_one_relation_objects_with_identity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: A Policy
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Employee @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "employees"
					) {
						name: String
						salary: Int
						company: Company
					}

					type Company @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "companies"
					) {
						name: String
						capital: Int
						employees: [Employee]
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Public Company", "capital": 100000 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Private Company", "capital": 200000 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PubEmp in PubCompany",
                    "salary": 10000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PubEmp in PrivateCompany",
                    "salary": 20000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PrivateEmp in PubCompany",
                    "salary": 30000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PrivateEmp in PrivateCompany",
                    "salary": 40000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Employee {
                            name
                            company {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Employee": m::unordered(json!([
                        {
                            "company": { "name": "Private Company" },
                            "name": "PrivateEmp in PrivateCompany",
                        },
                        {
                            "company": { "name": "Public Company" },
                            "name": "PubEmp in PubCompany",
                        },
                        {
                            "company": { "name": "Private Company" },
                            "name": "PubEmp in PrivateCompany",
                        },
                        {
                            "company": { "name": "Public Company" },
                            "name": "PrivateEmp in PubCompany",
                        },
                    ])),
                }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn acp_query_many_to_one_relation_objects_with_wrong_identity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: A Policy
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Employee @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "employees"
					) {
						name: String
						salary: Int
						company: Company
					}

					type Company @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "companies"
					) {
						name: String
						capital: Int
						employees: [Employee]
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Public Company", "capital": 100000 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Private Company", "capital": 200000 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PubEmp in PubCompany",
                    "salary": 10000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PubEmp in PrivateCompany",
                    "salary": 20000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PrivateEmp in PubCompany",
                    "salary": 30000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PrivateEmp in PrivateCompany",
                    "salary": 40000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Employee {
                            name
                            company {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Employee": [
                        {
                            "company": { "name": "Public Company" },
                            "name": "PubEmp in PubCompany",
                        },
                        { "company": null, "name": "PubEmp in PrivateCompany" },
                    ],
                }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn acp_query_many_to_one_relation_objects_without_identity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: A Policy
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Employee @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "employees"
					) {
						name: String
						salary: Int
						company: Company
					}

					type Company @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "companies"
					) {
						name: String
						capital: Int
						employees: [Employee]
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Public Company", "capital": 100000 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Private Company", "capital": 200000 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PubEmp in PubCompany",
                    "salary": 10000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PubEmp in PrivateCompany",
                    "salary": 20000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PrivateEmp in PubCompany",
                    "salary": 30000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PrivateEmp in PrivateCompany",
                    "salary": 40000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Employee {
                            name
                            company {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Employee": [
                        {
                            "company": { "name": "Public Company" },
                            "name": "PubEmp in PubCompany",
                        },
                        { "company": null, "name": "PubEmp in PrivateCompany" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn acp_query_one_to_many_relation_objects_with_identity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: A Policy
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Employee @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "employees"
					) {
						name: String
						salary: Int
						company: Company
					}

					type Company @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "companies"
					) {
						name: String
						capital: Int
						employees: [Employee]
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Public Company", "capital": 100000 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Private Company", "capital": 200000 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PubEmp in PubCompany",
                    "salary": 10000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PubEmp in PrivateCompany",
                    "salary": 20000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PrivateEmp in PubCompany",
                    "salary": 30000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PrivateEmp in PrivateCompany",
                    "salary": 40000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Company {
                            name
                            employees {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Company": m::unordered(json!([
                        {
                            "employees": m::unordered(json!([
                                { "name": "PubEmp in PubCompany" },
                                { "name": "PrivateEmp in PubCompany" },
                            ])),
                            "name": "Public Company",
                        },
                        {
                            "employees": m::unordered(json!([
                                { "name": "PrivateEmp in PrivateCompany" },
                                { "name": "PubEmp in PrivateCompany" },
                            ])),
                            "name": "Private Company",
                        },
                    ])),
                }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn acp_query_one_to_many_relation_objects_with_wrong_identity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: A Policy
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Employee @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "employees"
					) {
						name: String
						salary: Int
						company: Company
					}

					type Company @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "companies"
					) {
						name: String
						capital: Int
						employees: [Employee]
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Public Company", "capital": 100000 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Private Company", "capital": 200000 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PubEmp in PubCompany",
                    "salary": 10000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PubEmp in PrivateCompany",
                    "salary": 20000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PrivateEmp in PubCompany",
                    "salary": 30000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PrivateEmp in PrivateCompany",
                    "salary": 40000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Company {
                            name
                            employees {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Company": [
                        {
                            "employees": [{ "name": "PubEmp in PubCompany" }],
                            "name": "Public Company",
                        },
                    ],
                }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn acp_query_one_to_many_relation_objects_without_identity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: A Policy
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Employee @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "employees"
					) {
						name: String
						salary: Int
						company: Company
					}

					type Company @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "companies"
					) {
						name: String
						capital: Int
						employees: [Employee]
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Public Company", "capital": 100000 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Company".into(),
                doc_map: Some(json!({ "name": "Private Company", "capital": 200000 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PubEmp in PubCompany",
                    "salary": 10000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PubEmp in PrivateCompany",
                    "salary": 20000,
                })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_0"),
                    "name": "PrivateEmp in PubCompany",
                    "salary": 30000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Employee".into(),
                doc_map: Some(json!({
                    "company": m::r("doc1_1"),
                    "name": "PrivateEmp in PrivateCompany",
                    "salary": 40000,
                })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Company {
                            name
                            employees {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Company": [
                        {
                            "employees": [{ "name": "PubEmp in PubCompany" }],
                            "name": "Public Company",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
