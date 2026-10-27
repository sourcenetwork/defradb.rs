use integration_test::TestCluster;

use super::support::{CHUNKS, SCHEMA};

#[tokio::test]
async fn chunk_generates_a_queryable_collection() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();

    let names = client.collection_list().unwrap();
    assert!(names.iter().any(|n| n == CHUNKS), "{names:?}");
    let data = client
        .query(&format!(
            "query {{ {CHUNKS} {{ _docID _sourceID text hash }} }}"
        ))
        .unwrap();
    assert_eq!(data[CHUNKS], serde_json::json!([]));
    let data = client.query("query { Article { _body_chunks } }").unwrap();
    assert_eq!(data["Article"], serde_json::json!([]));
}

#[tokio::test]
async fn chunk_is_refused_off_a_string_field() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    let err = client
        .schema_add("type Counter { n: Int @chunk }")
        .unwrap_err()
        .to_string();
    assert!(err.contains("@chunk"), "{err}");
}

#[tokio::test]
async fn generated_collection_has_no_mutations() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();

    let data = client
        .query("query { __schema { mutationType { fields { name } } } }")
        .unwrap();
    let names: Vec<&str> = data["__schema"]["mutationType"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"add_Article"));
    assert!(!names.iter().any(|n| n.contains(CHUNKS)), "{names:?}");
}

#[tokio::test]
async fn the_chunk_list_is_not_a_mutation_input() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();

    let data = client
        .query(r#"query { __type(name: "ArticleMutationInputArg") { inputFields { name } } }"#)
        .unwrap();
    let names: Vec<&str> = data["__type"]["inputFields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"body"), "{names:?}");
    assert!(!names.contains(&"_body_chunks"), "{names:?}");
}
