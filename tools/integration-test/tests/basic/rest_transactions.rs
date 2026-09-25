use integration_test::TestCluster;
use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Value};

pub(super) async fn request(
    client: &Client,
    base: &str,
    method: Method,
    path: &str,
    transaction: Option<&str>,
    body: Option<Value>,
    expected: StatusCode,
) -> Value {
    let mut request = client.request(method, format!("{base}/api/v0{path}"));
    if let Some(transaction) = transaction {
        request = request.header("x-defradb-tx", transaction);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, expected, "{path}: {text}");
    if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap()
    }
}

#[tokio::test]
async fn rust_rest_transactions_preserve_isolation() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let api = cluster.api_url(0);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let begin = request(
        &client,
        api,
        Method::POST,
        "/tx",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let transaction = begin["id"].to_string();
    let schema = client
        .post(format!("{api}/api/v0/collections"))
        .header("x-defradb-tx", &transaction)
        .body("type RestItem { name: String }")
        .send()
        .await
        .unwrap();
    assert_eq!(
        schema.status(),
        StatusCode::OK,
        "{}",
        schema.text().await.unwrap()
    );
    let ids = request(
        &client,
        api,
        Method::POST,
        "/collections/RestItem",
        Some(&transaction),
        Some(json!([{"name":"one"},{"name":"two"}])),
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids.as_array().unwrap().len(), 2);
    let path = format!(
        "/collections/RestItem/document/{}",
        ids[0].as_str().unwrap()
    );
    let inside = request(
        &client,
        api,
        Method::GET,
        &path,
        Some(&transaction),
        None,
        StatusCode::OK,
    )
    .await;
    assert_eq!(inside["name"], "one");
    request(
        &client,
        api,
        Method::GET,
        &path,
        None,
        None,
        StatusCode::NOT_FOUND,
    )
    .await;
    request(
        &client,
        api,
        Method::POST,
        &format!("/tx/{transaction}"),
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let committed = request(&client, api, Method::GET, &path, None, None, StatusCode::OK).await;
    assert_eq!(committed["name"], "one");

    let begin = request(
        &client,
        api,
        Method::POST,
        "/tx",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let transaction = begin["id"].to_string();
    request(
        &client,
        api,
        Method::PATCH,
        &path,
        Some(&transaction),
        Some(json!({"name":"changed"})),
        StatusCode::OK,
    )
    .await;
    let inside = request(
        &client,
        api,
        Method::GET,
        &path,
        Some(&transaction),
        None,
        StatusCode::OK,
    )
    .await;
    assert_eq!(inside["name"], "changed");
    let outside = request(&client, api, Method::GET, &path, None, None, StatusCode::OK).await;
    assert_eq!(outside["name"], "one");
    request(
        &client,
        api,
        Method::DELETE,
        &path,
        Some(&transaction),
        None,
        StatusCode::OK,
    )
    .await;
    request(
        &client,
        api,
        Method::GET,
        &path,
        Some(&transaction),
        None,
        StatusCode::NOT_FOUND,
    )
    .await;
    request(
        &client,
        api,
        Method::DELETE,
        &format!("/tx/{transaction}"),
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let restored = request(&client, api, Method::GET, &path, None, None, StatusCode::OK).await;
    assert_eq!(restored["name"], "one");
    request(
        &client,
        api,
        Method::POST,
        "/collections/RestItem",
        Some(&transaction),
        Some(json!({"name":"expired"})),
        StatusCode::NOT_FOUND,
    )
    .await;

    let begin = request(
        &client,
        api,
        Method::POST,
        "/tx?read_only=true",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let transaction = begin["id"].to_string();
    request(
        &client,
        api,
        Method::POST,
        "/collections/RestItem",
        Some(&transaction),
        Some(json!({"name":"forbidden"})),
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    request(
        &client,
        api,
        Method::DELETE,
        &format!("/tx/{transaction}"),
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let rows = request(
        &client,
        api,
        Method::POST,
        "/graphql",
        None,
        Some(json!({"query":"query { RestItem { name } }"})),
        StatusCode::OK,
    )
    .await;
    assert_eq!(rows["data"]["RestItem"].as_array().unwrap().len(), 2);
}
