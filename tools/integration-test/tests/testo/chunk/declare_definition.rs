//! The chunk collection is listed with its source (`defra.chunk.declare.definition`).

use integration_test::actions::GetCollections;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_collection, CHUNKS};

const CHUNK_VERSION_ID: &str = "bafyreihg47y5zfpyz5ahd64nquof27fvgtunfqxdv7xe23yhrafgba3qzm";

#[test]
#[mantra_macros::req_test("defra.chunk.declare.definition")]
fn the_chunk_collection_is_listed_with_its_source() {
    TestCase::from_actions(vec![
        add_article_collection(),
        GetCollections {
            collection_name: Some(CHUNKS.into()),
            filtered: true,
            expected_data: json!({ "collections": [{
                "ChunkOf": { "Collection": "Article", "Field": "body" },
                "CollectionID": CHUNK_VERSION_ID,
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
                        "FieldID": "bafyreihwtxs2nr52tw7m4anpeeb6nuov32gruxdqoismi4d7tfeyr2mxri",
                        "IsPrimary": true,
                        "Kind": 1,
                        "Name": "_sourceID",
                        "RelationName": "article_article__body_chunk",
                        "Size": 0,
                        "Typ": 1,
                    },
                    {
                        "DefaultValue": null,
                        "FieldID": "bafyreifadfrdhubrvn2n4e7wcyooktriqcwc7eddgcdl5cj5trydbl7lzm",
                        "IsPrimary": false,
                        "Kind": 11,
                        "Name": "hash",
                        "RelationName": null,
                        "Size": 0,
                        "Typ": 1,
                    },
                    {
                        "DefaultValue": null,
                        "FieldID": "bafyreid3fvmlqzz4n2ssl5ru7n4h7f3vdnbve754w452dd7lb3ioilqzoy",
                        "IsPrimary": true,
                        "Kind": {
                            "Array": false,
                            "CollectionID": "bafyreicfdegvlbsluy3ikbeqvbynurynf7mbhs6llmbmofdwjjvkyfjxo4",
                        },
                        "Name": "source",
                        "RelationName": "article_article__body_chunk",
                        "Size": 0,
                        "Typ": 0,
                    },
                    {
                        "DefaultValue": null,
                        "FieldID": "bafyreibihrituppcjs44cdxohnicv2ky6ugx45bhtjoxizzmqcoygukfb4",
                        "IsPrimary": false,
                        "Kind": 11,
                        "Name": "text",
                        "RelationName": null,
                        "Size": 0,
                        "Typ": 1,
                    },
                ],
                "Indexes": [{
                    "Fields": [
                        { "Descending": false, "Name": "_sourceID" },
                        { "Descending": false, "Name": "hash" },
                    ],
                    "ID": 1,
                    "Kind": 0,
                    "KindDescription": { "Unique": false },
                    "Name": "Article__body_chunk__sourceID_ASC",
                    "Unique": false,
                }],
                "IsActive": true,
                "IsBranchable": false,
                "IsEmbeddedOnly": false,
                "IsMaterialized": true,
                "IsPlaceholder": false,
                "Name": CHUNKS,
                "PreviousVersion": null,
                "VectorEmbeddings": [],
                "VersionID": CHUNK_VERSION_ID,
            }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
