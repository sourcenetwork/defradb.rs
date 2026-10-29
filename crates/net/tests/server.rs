//! A server whose edge stops accepting finishes the requests in flight,
//! closes its idle keep-alive connections, and returns.

mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use net::http::Client;
use nuclei::time::{sleep, timeout};

const LIMIT: Duration = Duration::from_secs(10);

fn client() -> Client {
    Client::builder(edge::testing::edge())
        .no_proxy()
        .build()
        .expect("a client")
}

#[nuclei::test]
async fn a_closing_server_finishes_the_requests_in_flight_then_returns() {
    let scope = edge::testing::edge().scope();
    let (address, server) = support::serve_on(None, scope.edge().clone()).await;
    let idle = client();
    let answer = idle.get(format!("http://{address}/method")).send().await;
    assert_eq!(answer.unwrap().text().await.unwrap(), "GET");
    let slow = nuclei::spawn(async move {
        client()
            .get(format!("http://{address}/slow"))
            .send()
            .await?
            .text()
            .await
    });
    timeout(LIMIT, async {
        let mut wait = Duration::from_millis(1);
        while support::SLOW_STARTED.load(Ordering::SeqCst) == 0 {
            sleep(wait).await;
            wait = (wait * 2).min(Duration::from_millis(50));
        }
    })
    .await
    .expect("the slow request reaches the server");

    scope.close();

    let served = timeout(LIMIT, server)
        .await
        .expect("the server returns once its connections end");
    assert!(served.is_ok(), "{served:?}");
    assert_eq!(slow.await.unwrap(), "late");
}
