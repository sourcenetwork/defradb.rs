//! Collections defined together get the same IDs on every node, whatever
//! order they are processed in (Go shinzo-develop #5040,
//! `go.shinzo.collection_id`). `Foo` is a name prefix of `FooBar`, which is
//! what let Go's head scan read the wrong collection.

use integration_test::actions::{AddCollection, GetCollections};

use integration_test::test_case::TestCase;
use serde_json::json;

/// `Foo`'s version ID for this SDL. The same constant under the go-shinzo
/// client checks the Interop line: Go Shinzo derives the same ID.
const FOO_VERSION_ID: &str = "bafyreigj37qgcjknekn3jqcmpai5ut6gqsfvjgczmu5iopkeeznjojbqri";

#[test]
#[mantra_macros::req_test("go.shinzo.collection_id")]
fn a_collection_beside_a_longer_namesake_gets_one_id() {
    for _ in 0..5 {
        TestCase::from_actions(vec![
            AddCollection {
                sdl: "type Foo { name: String }  type FooBar { name: String }".into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Foo".into()),
                filtered: true,
                expected_data: json!({ "collections": [{
                    "CollectionID": FOO_VERSION_ID,
                    "Fields": [
                        {
                            "DefaultValue": null,
                            "FieldID": "bafyreihqzhiz3iwro4jozp6kphq4sosg6ccoqcbiaf7rg5dmvea7aux55a",
                            "IsPrimary": false,
                            "Kind": 1,
                            "Name": "_docID",
                            "RelationName": null,
                            "Size": 0,
                            "Typ": 0,
                        },
                        {
                            "DefaultValue": null,
                            "FieldID": "bafyreiaezc5g33yzhyzcgbyiv476lovyztyoliotzksdfogoep5ktgpedq",
                            "IsPrimary": false,
                            "Kind": 11,
                            "Name": "name",
                            "RelationName": null,
                            "Size": 0,
                            "Typ": 1,
                        },
                    ],
                    "IsActive": true,
                    "IsBranchable": false,
                    "IsEmbeddedOnly": false,
                    "IsMaterialized": true,
                    "IsPlaceholder": false,
                    "Name": "Foo",
                    "PreviousVersion": null,
                    "VectorEmbeddings": [],
                    "VersionID": FOO_VERSION_ID,
                }] }),
                ..Default::default()
            }
            .into(),
        ])
        .execute();
    }
}
