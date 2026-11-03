//! Generated from the conformance scenarios of tests/integration/query/one_to_one_to_many/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one_to_many.join")]
fn query_one_to_one_to_many_from_secondary_on_one_to_many() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Indicator {
                        name: String
                        observable: Observable @primary
                    }

                    type Observable {
                        name: String
                        indicator: Indicator
                        observations: [Observation]
                    }

                    type Observation {
                        name: String
                        observable: Observable
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Observable".into(),
                doc_map: Some(json!({ "name": "Observable1" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Indicator".into(),
                doc_map: Some(json!({ "_observableID": m::r("doc1_0"), "name": "Indicator1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Observation".into(),
                doc_map: Some(json!({ "_observableID": m::r("doc1_0"), "name": "Observation1" })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                                                Indicator {
                                                    name
                                                    observable {
                                                        name
                                                        observations {
                                                            name
                                                        }
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({
                    "Indicator": [
                        {
                            "name": "Indicator1",
                            "observable": {
                                "name": "Observable1",
                                "observations": [{ "name": "Observation1" }],
                            },
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.one_to_one_to_many.join")]
fn query_one_to_one_to_many_from_secondary_on_one_to_one() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Indicator {
                        name: String
                        observable: Observable @primary
                    }

                    type Observable {
                        name: String
                        indicator: Indicator
                        observations: [Observation]
                    }

                    type Observation {
                        name: String
                        observable: Observable
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Observable".into(),
                doc_map: Some(json!({ "name": "Observable1" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Indicator".into(),
                doc_map: Some(json!({ "_observableID": m::r("doc1_0"), "name": "Indicator1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Observation".into(),
                doc_map: Some(json!({ "_observableID": m::r("doc1_0"), "name": "Observation1" })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                                                Observation {
                                                    name
                                                    observable {
                                                        name
                                                        indicator {
                                                            name
                                                        }
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({
                    "Observation": [
                        {
                            "name": "Observation1",
                            "observable": {
                                "indicator": { "name": "Indicator1" },
                                "name": "Observable1",
                            },
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.one_to_one_to_many.join")]
fn query_one_to_one_to_many_from_secondary() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Indicator {
                        name: String
                        observable: Observable
                    }

                    type Observable {
                        name: String
                        indicator: Indicator @primary
                        observations: [Observation]
                    }

                    type Observation {
                        name: String
                        observable: Observable
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Indicator".into(),
                doc_map: Some(json!({ "name": "Indicator1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Observable".into(),
                doc_map: Some(json!({ "_indicatorID": m::r("doc0_0"), "name": "Observable1" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Observation".into(),
                doc_map: Some(json!({ "_observableID": m::r("doc1_0"), "name": "Observation1" })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                                                Indicator {
                                                    name
                                                    observable {
                                                        name
                                                        observations {
                                                            name
                                                        }
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({
                    "Indicator": [
                        {
                            "name": "Indicator1",
                            "observable": {
                                "name": "Observable1",
                                "observations": [{ "name": "Observation1" }],
                            },
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
