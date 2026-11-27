//! Server-sent event subscriptions to a node's `/api/v0/events`.

mod events;

use std::sync::Arc;
use std::time::{Duration, Instant};

use regolith::sync::Event;
use serde_json::Value;
use tokio::task::JoinHandle;

pub use events::SseEvents;

/// Open an SSE connection to `/api/v0/events?event=topic-peer-event`.
///
/// Waits for the HTTP connection to be established before returning,
/// ensuring the server-side subscription is active.
pub async fn open_peer_events_sse(api_url: &str) -> (JoinHandle<()>, SseEvents) {
    open_events_sse(api_url, "topic-peer-event").await
}

/// Open an SSE connection to `/api/v0/events` with an event filter.
///
/// Waits for the HTTP connection to be established before returning,
/// ensuring the server-side event bus subscription is active and no
/// events will be missed.
pub async fn open_events_sse(api_url: &str, event_filter: &str) -> (JoinHandle<()>, SseEvents) {
    let url = format!("{}/api/v0/events?event={}", api_url, event_filter);
    let events = SseEvents::new();
    let received = events.clone();
    let answered = Arc::new(Event::new());
    let answer = Arc::clone(&answered);

    let handle = tokio::spawn(async move {
        let response = reqwest::Client::new().get(&url).send().await;
        answer.set();
        match response {
            Ok(response) => collect(response, &received).await,
            Err(e) => eprintln!("SSE events request failed: {}", e),
        }
    });

    // The server subscribes before it answers, so no event is missed once
    // the request has its response.
    answered.wait().await;

    (handle, events)
}

async fn collect(response: reqwest::Response, events: &SseEvents) {
    use futures::StreamExt;

    let mut buf = String::new();
    let mut stream = response.bytes_stream();
    while let Some(Ok(chunk)) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buf.find("\n\n") {
            let block = buf[..pos].to_string();
            buf = buf[pos + 2..].to_string();

            let mut event_type = "";
            let mut data = "";
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    event_type = rest.trim();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data = rest.trim();
                }
            }
            if event_type == "next" {
                if let Ok(event) = serde_json::from_str::<Value>(data) {
                    events.push(event);
                }
            }
        }
    }
}

/// Wait until at least `expected_count` events have been collected, or timeout.
///
/// Returns the events collected either way.
pub async fn wait_for_peer_events(
    events: &SseEvents,
    expected_count: usize,
    timeout: Duration,
) -> Vec<Value> {
    let collected = events
        .wait_until(timeout, |events| events.len() >= expected_count)
        .await;
    match collected {
        Ok(events) | Err(events) => Vec::clone(&events),
    }
}

/// Open an SSE connection to `/api/v0/events?event=update`.
///
/// Subscribes to document update events emitted when P2P blocks are merged.
/// Waits for the connection to be established before returning.
pub async fn open_merge_events_sse(api_url: &str) -> (JoinHandle<()>, SseEvents) {
    open_events_sse(api_url, "update").await
}

/// Wait until at least `count` update events have been collected, or panic.
pub async fn wait_for_merge_events(events: &SseEvents, count: usize, timeout: Duration) {
    if let Err(current) = events
        .wait_until(timeout, |events| events.len() >= count)
        .await
    {
        panic!(
            "timed out waiting for {} update events after {:?} (got {}): {:?}",
            count,
            timeout,
            current.len(),
            current
        );
    }
}

/// Open an SSE connection to `/api/v0/events?event=acp-cache-invalidated`.
///
/// Subscribes to ACP cache invalidation events emitted when the light client
/// detects a module state root change and invalidates stale cache entries.
pub async fn open_acp_events_sse(api_url: &str) -> (JoinHandle<()>, SseEvents) {
    open_events_sse(api_url, "acp-cache-invalidated").await
}

fn height(event: &Value) -> Option<u64> {
    event.pointer("/data/height").and_then(Value::as_u64)
}

/// Wait until an `acp-cache-invalidated` event arrives with `height > min_height`.
///
/// Used after Vera ACP mutations (set_relationship, register_object) to wait
/// for DefraDB's light client to pick up the state root change and invalidate
/// its cache, so subsequent queries reflect the new permissions.
pub async fn wait_for_acp_invalidation(events: &SseEvents, min_height: u64, timeout: Duration) {
    let start = Instant::now();
    let above = |events: &[Value]| events.iter().filter_map(height).any(|h| h > min_height);
    match events.wait_until(timeout, above).await {
        Ok(current) => {
            let invalidated = current.iter().filter_map(height).find(|h| *h > min_height);
            eprintln!(
                "[defra-harness]   ACP cache invalidated at height {} (waited {:.2}s)",
                invalidated.unwrap_or_default(),
                start.elapsed().as_secs_f64()
            );
        }
        Err(current) => panic!(
            "timed out waiting for ACP cache invalidation above height {} after {:?} (events: {:?})",
            min_height, timeout, current
        ),
    }
}
