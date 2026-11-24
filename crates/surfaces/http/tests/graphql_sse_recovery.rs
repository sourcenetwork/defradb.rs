use std::{sync::Arc, time::Duration};

use axum::{body::Body, http::Request};
use defra_http::{router::AppStateBuilder, FailingMockExecutor, MockQueryExecutor};
use events::{Bus, ChannelBus, ChannelBusConfig, Message, Update};
use futures::StreamExt;
use query::error::{QueryError, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse, QueryResponseError};
use query::TransactionHandle;
use tower::ServiceExt;

struct DenyingExecutor;

#[async_trait::async_trait]
impl QueryExecutor for DenyingExecutor {
    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        QueryResponse::error(QueryResponseError::from_query_error(
            QueryError::permission_denied("opaque"),
        ))
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(&self, _readonly: bool) -> Result<TransactionHandle, TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn commit_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn discard_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn schema(&self) -> query::error::Result<String> {
        Ok("type Query { ping: String }".into())
    }
}

/// The executor a stream runs on: the open check (the subscription itself,
/// run as a plain request) answers with the error its second field makes,
/// and every scoped query goes to the first.
struct Streams<E>(E, fn() -> QueryError);

/// The open check answers that it needs a stream, as the real executor does
/// for a valid subscription.
fn streams<E>(inner: E) -> Streams<E> {
    Streams(inner, || QueryError::SubscriptionNeedsSse)
}

#[async_trait::async_trait]
impl<E> QueryExecutor for Streams<E>
where
    E: QueryExecutor,
{
    async fn execute(&self, request: QueryRequest) -> QueryResponse {
        match request.query.trim_start().starts_with("subscription") {
            true => QueryResponse::error(QueryResponseError::from_query_error((self.1)())),
            false => self.0.execute(request).await,
        }
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(&self, readonly: bool) -> Result<TransactionHandle, TransactionError> {
        self.0.begin_txn(readonly).await
    }

    async fn commit_txn(&self, handle: &TransactionHandle) -> Result<(), TransactionError> {
        self.0.commit_txn(handle).await
    }

    async fn discard_txn(&self, handle: &TransactionHandle) -> Result<(), TransactionError> {
        self.0.discard_txn(handle).await
    }

    fn abandon_txn(&self, handle: &TransactionHandle) {
        self.0.abandon_txn(handle)
    }

    async fn schema(&self) -> query::error::Result<String> {
        self.0.schema().await
    }
}

fn update() -> Message {
    Message::update(Update::new(
        "doc-1".into(),
        cid::Cid::default(),
        "users".into(),
        Vec::new(),
        false,
        false,
    ))
}

async fn open_stream(
    bus: Arc<ChannelBus>,
    executor: Arc<dyn QueryExecutor>,
) -> axum::response::Response {
    let state = AppStateBuilder::new(executor, edge::testing::edge())
        .with_event_bus(bus)
        .build();
    let response = defra_http::create_router_with_state(state)
        .oneshot(
            Request::post("/api/v0/graphql")
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .body(Body::from(
                    r#"{"query":"subscription { users { _docID name } }"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    response
}

#[test]
fn overflow_ends_the_graphql_stream_before_a_queued_result() {
    edge::testing::edge()
        .block_on(async {
            let bus = Arc::new(ChannelBus::with_config(
                ChannelBusConfig::new().with_event_buffer_size(1),
            ));
            let response =
                open_stream(bus.clone(), Arc::new(streams(MockQueryExecutor::new()))).await;
            bus.publish(update());
            bus.publish(update());
            let mut stream = response.into_body().into_data_stream();
            let frame = nuclei::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .expect("overflow must close the stream")
                .unwrap()
                .unwrap();
            let text = std::str::from_utf8(&frame).unwrap();
            assert!(text.contains("event: complete"), "{text}");
            assert!(stream.next().await.is_none());
        })
        .unwrap();
}

#[test]
fn query_error_is_delivered_even_without_rows() {
    edge::testing::edge()
        .block_on(async {
            let bus = Arc::new(ChannelBus::new());
            let response = open_stream(
                bus.clone(),
                Arc::new(streams(FailingMockExecutor::with_schema_error("unused"))),
            )
            .await;
            bus.publish(update());
            let mut stream = response.into_body().into_data_stream();
            let frame = nuclei::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .expect("query errors must reach the subscriber")
                .unwrap()
                .unwrap();
            let text = std::str::from_utf8(&frame).unwrap();
            assert!(text.contains("event: next"), "{text}");
            let data = text
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            let body: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(body["errors"][0]["message"], "execution failed");
            let complete = stream.next().await.unwrap().unwrap();
            assert!(std::str::from_utf8(&complete)
                .unwrap()
                .contains("event: complete"));
            assert!(stream.next().await.is_none());
        })
        .unwrap();
}

#[test]
fn typed_permission_refusal_keeps_its_code_in_the_graphql_stream() {
    edge::testing::edge()
        .block_on(async {
            let bus = Arc::new(ChannelBus::new());
            let response = open_stream(bus.clone(), Arc::new(streams(DenyingExecutor))).await;
            bus.publish(update());
            let mut stream = response.into_body().into_data_stream();
            let frame = nuclei::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .expect("permission refusal must reach the subscriber")
                .unwrap()
                .unwrap();
            let text = std::str::from_utf8(&frame).unwrap();
            let data = text
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            let body: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(body["errors"][0]["extensions"]["code"], "PERMISSION_DENIED");
        })
        .unwrap();
}

#[test]
fn a_subscription_the_plain_path_refuses_sends_its_error_then_completes() {
    edge::testing::edge()
        .block_on(async {
            let bus = Arc::new(ChannelBus::new());
            let response = open_stream(
                bus.clone(),
                Arc::new(Streams(MockQueryExecutor::new(), || {
                    QueryError::unknown_field(UNKNOWN_FIELD)
                })),
            )
            .await;
            let mut stream = response.into_body().into_data_stream();
            let frame = nuclei::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .expect("a refused open must be reported without an update")
                .unwrap()
                .unwrap();
            let text = std::str::from_utf8(&frame).unwrap();
            assert!(text.contains("event: next"), "{text}");
            let data = text
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            let body: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(body["errors"][0]["message"], UNKNOWN_FIELD);
            assert!(
                body.get("data").is_none_or(serde_json::Value::is_null),
                "{body}"
            );
            let complete = stream.next().await.unwrap().unwrap();
            assert!(std::str::from_utf8(&complete)
                .unwrap()
                .contains("event: complete"));
            assert!(stream.next().await.is_none());
        })
        .unwrap();
}

const UNKNOWN_FIELD: &str =
    "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.";

/// The open check failed for a reason that passes (a collection read that
/// failed, a provider that did not answer in time): the subscription stays
/// open and the next update streams, where Go's `execSSESubscription` would
/// have ended it.
#[test]
fn a_transient_failure_at_open_keeps_the_stream() {
    let transient: [fn() -> QueryError; 3] = [
        || QueryError::execution("failed to read the collection definitions"),
        || QueryError::AcpProviderUnavailable("the ACP provider did not answer in time".into()),
        || QueryError::transaction_retry_exhausted("transaction retries exhausted"),
    ];
    for open in transient {
        edge::testing::edge()
            .block_on(async {
                let bus = Arc::new(ChannelBus::new());
                let response = open_stream(
                    bus.clone(),
                    Arc::new(Streams(MockQueryExecutor::new(), open)),
                )
                .await;
                let mut stream = response.into_body().into_data_stream();
                // The open check has answered once the subscription is
                // waiting; an update published now is the stream's first.
                bus.publish(update());
                let frame = nuclei::time::timeout(Duration::from_secs(5), stream.next())
                    .await
                    .expect("the update must reach a stream whose open check failed transiently")
                    .unwrap()
                    .unwrap();
                let text = std::str::from_utf8(&frame).unwrap();
                assert!(text.contains("event: next"), "{text}");
                let data = text
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap();
                let body: serde_json::Value = serde_json::from_str(data).unwrap();
                assert!(body.get("errors").is_none(), "{body}");
                assert_eq!(body["data"]["users"][0]["name"], "Alice", "{body}");
            })
            .unwrap();
    }
}
