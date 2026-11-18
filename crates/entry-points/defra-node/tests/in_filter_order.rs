use defra_node::Node;
use serde_json::json;

/// An unsorted `_in` keeps the requested order over an ASC index, a DESC
/// index and no index (#2486).
#[test]
fn an_unsorted_in_filter_keeps_the_requested_order() {
    workers::block_on(async {
        for sdl in [
            "type User { name: String age: Int @index }",
            "type User { name: String age: Int @index(direction: DESC) }",
            "type User { name: String age: Int }",
        ] {
            let node = Node::builder().build().await.unwrap();
            node.add_collection(sdl).await.unwrap();
            for age in [25, 35, 45, 55, 65] {
                let created = node
                    .execute(&format!(
                        r#"mutation {{ create_User(input: {{name: "u{age}", age: {age}}}) {{ _docID }} }}"#
                    ))
                    .await;
                assert!(!created.has_errors(), "{:?}", created.errors);
            }
            for (order, want) in [("ASC", [25, 35, 45, 55]), ("DESC", [55, 45, 35, 25])] {
                let result = node
                    .execute(&format!(
                        "{{ User(filter: {{age: {{_in: [55, 25, 45, 35, 25]}}}}, order: {{age: {order}}}) {{ age }} }}"
                    ))
                    .await;
                assert!(!result.has_errors(), "{:?}", result.errors);
                let want: Vec<_> = want.iter().map(|age| json!({ "age": age })).collect();
                assert_eq!(result.data.unwrap()["User"], json!(want), "{sdl} {order}");
            }
            node.shutdown().await.unwrap();
        }
    })
    .unwrap()
}
