//! Generated from the conformance scenarios of tests/integration/index/query_with_compound_filter_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_query_with_index_on_one_to_many_relation_and_filter_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Program {
                      name: String
                      certificationBodyOrg: Organization
                    }

                    type Organization {
                      name: String @index
                      programs: [Program]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Organization".into(),
                doc_map: Some(json!({ "name": "Source Inc." })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_0"), "name": "DefraDB" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_0"), "name": "LensVM" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Organization".into(),
                doc_map: Some(json!({ "name": "ESA" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_1"), "name": "Horizon" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "name": "Zanzi" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Program(
                                            filter: {
                                                _and: [
                                                    { certificationBodyOrg: { name: { _eq: "Source Inc." } } }
                                                ]
                                            }
                                        ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Program": [{ "name": "LensVM" }, { "name": "DefraDB" }] }),
                common: Common { unordered_documents: vec!["/data/Program".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_query_with_index_on_one_to_many_relation_and_filter_no_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Program {
                      name: String
                      certificationBodyOrg: Organization
                    }

                    type Organization {
                      name: String @index
                      programs: [Program]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Program(
                                            filter: {
                                                _and: [
                                                    { certificationBodyOrg: { name: { _eq: "Test" } } }
                                                ]
                                            }
                                        ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Program": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_query_with_index_on_one_to_many_relation_not_filter_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Program {
                      name: String
                      certificationBodyOrg: Organization
                    }

                    type Organization {
                      name: String @index
                      programs: [Program]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Organization".into(),
                doc_map: Some(json!({ "name": "Source Inc." })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_0"), "name": "DefraDB" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Organization".into(),
                doc_map: Some(json!({ "name": "ESA" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_1"), "name": "Horizon" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "name": "Zanzi" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Program(
                                            filter: {
                                                _not: {
                                                    certificationBodyOrg: { name: { _eq: "Source Inc." } }
                                                }
                                            }
                                        ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Program": [{ "name": "Horizon" }, { "name": "Zanzi" }] }),
                common: Common { unordered_documents: vec!["/data/Program".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_query_with_index_on_one_to_many_relation_not_filter_no_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Program {
                      name: String
                      certificationBodyOrg: Organization
                    }

                    type Organization {
                      name: String @index
                      programs: [Program]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Program(
                                            filter: {
                                                _not: {
                                                    certificationBodyOrg: { name: { _eq: "Test" } }
                                                }
                                            }
                                        ) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Program": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_query_with_index_on_one_to_many_relation_or_filter_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Program {
                      name: String
                      certificationBodyOrg: Organization
                    }

                    type Organization {
                      name: String @index
                      programs: [Program]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Organization".into(),
                doc_map: Some(json!({ "name": "Source Inc." })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_0"), "name": "DefraDB" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_0"), "name": "LensVM" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Organization".into(),
                doc_map: Some(json!({ "name": "ESA" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "certificationBodyOrg": m::r("doc1_1"), "name": "Horizon" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Program".into(),
                doc_map: Some(json!({ "name": "Zanzi" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Program(
                                            filter: {
                                                _or: [
                                                    { certificationBodyOrg: { name: { _eq: "Source Inc." } } },
                                                    { name: { _eq: "Zanzi" } }
                                                ]
                                            }
                                        ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Program": [{ "name": "LensVM" }, { "name": "DefraDB" }, { "name": "Zanzi" }],
                }),
                common: Common { unordered_documents: vec!["/data/Program".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_query_with_index_on_one_to_many_relation_or_filter_no_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Program {
                      name: String
                      certificationBodyOrg: Organization
                    }

                    type Organization {
                      name: String @index
                      programs: [Program]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Program(
                                            filter: {
                                                _or: [
                                                    { certificationBodyOrg: { name: { _eq: "Test" } } }
                                                ]
                                            }
                                        ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Program": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
