//! Generated from the conformance scenarios of tests/integration/collection_version/one_one_many_self_ref_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.relations.self_ref_ids")]
fn collection_version_with_one_one_one_many_self_ref() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Dev_RC_Domain {
                        routes: [Dev_RC_RedirectRoute]
                        firstRoute: Dev_RC_RedirectRoute @primary @relation(name: "domain_first_route")
                    }

                    type Dev_RC_RedirectRoute {
                        firstForDomain: Dev_RC_Domain @relation(name: "domain_first_route")

                        domain: Dev_RC_Domain
                        after: Dev_RC_RedirectRoute
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Dev_RC_Domain",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": m::optional(json!(null)),
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "_firstRouteID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "domain_first_route",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "firstRoute",
                                    "Kind": { "RelativeID": "1", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "domain_first_route",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "routes",
                                    "Kind": { "RelativeID": "1", "Array": true },
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "dev_rc_domain_dev_rc_redirectroute",
                                    "Size": 0,
                                })),
                            ],
                        })),
                        m::object(json!({
                            "Name": "Dev_RC_RedirectRoute",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": m::optional(json!(null)),
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "_afterID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dev_rc_redirectroute_dev_rc_redirectroute",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "_domainID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dev_rc_domain_dev_rc_redirectroute",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "_firstForDomainID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "domain_first_route",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "after",
                                    "Kind": { "RelativeID": "1", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dev_rc_redirectroute_dev_rc_redirectroute",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "domain",
                                    "Kind": { "RelativeID": "0", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dev_rc_domain_dev_rc_redirectroute",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "firstForDomain",
                                    "Kind": { "RelativeID": "0", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "domain_first_route",
                                    "Size": 0,
                                })),
                            ],
                        })),
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
