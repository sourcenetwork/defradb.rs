//! A patch that adds a version of an existing collection under a version ID
//! it made up is refused, as Go refuses it: a version ID is the CID of a block
//! already in the blockstore.

use integration_test::actions::{AddCollection, GetCollections, PatchCollection};
use integration_test::scenario::m;
use integration_test::test_case::TestCase;
use serde_json::json;

/// The version `AddCollection` gives `Users`; on a first version it is also
/// the collection's ID.
const USERS_VERSION_ID: &str = "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu";

/// A version ID no stored version has.
const NEW_VERSION_ID: &str = "bafyreihiiez4vcgh4rys2zfs74macgwyybchutjslyw2oin747enuywn54";

/// The patch copies `Users` to a new key, gives the copy a version ID of its
/// own and makes it inactive. No block has that ID, so the patch fails and
/// `Users` keeps its one version.
#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn col_version_update_add_inactive_version_of_existing_collection_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: format!(
                    r#"
                    [
                        {{ "op": "copy", "from": "/Users", "path": "/UsersV2" }},
                        {{ "op": "replace", "path": "/UsersV2/VersionID", "value": "{NEW_VERSION_ID}" }},
                        {{ "op": "replace", "path": "/UsersV2/IsActive", "value": false }}
                    ]
                "#
                )
                .into(),
                expected_error: Some(format!(
                    "unknown CID, collection ids cannot be manually defined. VersionID: {NEW_VERSION_ID}"
                )),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "VersionID": USERS_VERSION_ID,
                            "CollectionID": USERS_VERSION_ID,
                            "IsActive": true,
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
