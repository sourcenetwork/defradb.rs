use identity::{new_token, KeyType, RawIdentity};
use integration_test::TestCluster;
use reqwest::{header, Client, Method, StatusCode};
use serde_json::json;
use std::time::Duration;

use super::rest_transactions::request;

fn client(api: &str, identity: Option<&RawIdentity>, lifetime: u64) -> Client {
    let mut headers = header::HeaderMap::new();
    if let Some(identity) = identity {
        let token = new_token(
            identity,
            Duration::from_secs(lifetime),
            Some(api.strip_prefix("http://").unwrap().to_owned()),
            None,
        )
        .unwrap();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", String::from_utf8(token).unwrap())
                .parse()
                .unwrap(),
        );
    }
    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

#[tokio::test]
async fn rust_transaction_ownership_covers_rest_graphql_and_finalization() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let api = cluster.api_url(0);
    let alice = RawIdentity::from_bytes(KeyType::Secp256k1, &[41; 32]).unwrap();
    let bob = RawIdentity::from_bytes(KeyType::Secp256k1, &[42; 32]).unwrap();
    let owner = client(api, Some(&alice), 3600);
    let other = client(api, Some(&bob), 3600);
    let anonymous = client(api, None, 0);
    let response = owner
        .post(format!("{api}/api/v0/collections"))
        .body("type OwnedItem { name: String }")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    request(
        &other,
        api,
        Method::GET,
        "/collections",
        None,
        None,
        StatusCode::OK,
    )
    .await;

    for commit in [true, false] {
        let begin = request(&owner, api, Method::POST, "/tx", None, None, StatusCode::OK).await;
        let transaction = begin["id"].to_string();
        let name = if commit { "committed" } else { "discarded" };
        let ids = request(
            &owner,
            api,
            Method::POST,
            "/collections/OwnedItem",
            Some(&transaction),
            Some(json!({"name":name})),
            StatusCode::OK,
        )
        .await;
        let document = format!(
            "/collections/OwnedItem/document/{}",
            ids[0].as_str().unwrap()
        );
        let lifecycle = format!("/tx/{transaction}");
        for foreign in [&other, &anonymous] {
            for (method, path, body) in [
                (Method::GET, "/collections", None),
                (Method::GET, document.as_str(), None),
                (
                    Method::POST,
                    "/collections/OwnedItem",
                    Some(json!({"name":"foreign"})),
                ),
            ] {
                request(
                    foreign,
                    api,
                    method,
                    path,
                    Some(&transaction),
                    body,
                    StatusCode::NOT_FOUND,
                )
                .await;
            }
            let response = foreign
                .post(format!("{api}/api/v0/collections"))
                .header("x-defradb-tx", &transaction)
                .body("type ForeignItem { name: String }")
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{}",
                response.text().await.unwrap()
            );
            let result = request(
                foreign,
                api,
                Method::POST,
                "/graphql",
                Some(&transaction),
                Some(json!({"query":"{ OwnedItem { name } }"})),
                StatusCode::OK,
            )
            .await;
            assert!(result["data"].is_null());
            assert!(result["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains("not found"));
            for method in [Method::POST, Method::DELETE] {
                request(
                    foreign,
                    api,
                    method,
                    &lifecycle,
                    None,
                    None,
                    StatusCode::NOT_FOUND,
                )
                .await;
            }
        }
        let renewed = client(api, Some(&alice), 7200);
        let extra = if commit {
            "CommittedSchema"
        } else {
            "DiscardedSchema"
        };
        let response = renewed
            .post(format!("{api}/api/v0/collections"))
            .header("x-defradb-tx", &transaction)
            .body(format!("type {extra} {{ value: String }}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let collections = request(
            &renewed,
            api,
            Method::GET,
            "/collections",
            Some(&transaction),
            None,
            StatusCode::OK,
        )
        .await;
        assert!(collections
            .as_array()
            .unwrap()
            .iter()
            .any(|collection| collection["Name"] == extra));

        let inside = request(
            &renewed,
            api,
            Method::GET,
            &document,
            Some(&transaction),
            None,
            StatusCode::OK,
        )
        .await;
        assert_eq!(inside["name"], name);
        let result = request(
            &renewed,
            api,
            Method::POST,
            "/graphql",
            Some(&transaction),
            Some(json!({"query":"{ OwnedItem { name } }"})),
            StatusCode::OK,
        )
        .await;
        assert!(
            result["errors"].as_array().is_none_or(Vec::is_empty),
            "{result}"
        );
        assert!(result["data"]["OwnedItem"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == name));
        let method = if commit { Method::POST } else { Method::DELETE };
        request(
            &renewed,
            api,
            method,
            &lifecycle,
            None,
            None,
            StatusCode::OK,
        )
        .await;
        let expected = if commit {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        request(&other, api, Method::GET, &document, None, None, expected).await;
    }
}
