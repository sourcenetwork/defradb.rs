//! The request time limit runs on nuclei's clock: an answer in time passes
//! through, a late one fails with `Elapsed`.

use std::convert::Infallible;
use std::time::Duration;

use net::timeout::{Elapsed, TimeoutLayer};
use tower::{service_fn, Layer, ServiceExt};

fn answering_after(
    delay: Duration,
) -> impl tower::Service<(), Response = &'static str, Error = Infallible> {
    service_fn(move |()| async move {
        nuclei::time::sleep(delay).await;
        Ok::<_, Infallible>("answer")
    })
}

#[nuclei::test(start_paused = true)]
async fn an_answer_in_time_passes_through() {
    let service =
        TimeoutLayer::new(Duration::from_secs(5)).layer(answering_after(Duration::from_secs(1)));

    assert_eq!(service.oneshot(()).await.unwrap(), "answer");
}

#[nuclei::test(start_paused = true)]
async fn a_late_answer_fails_with_elapsed() {
    let service =
        TimeoutLayer::new(Duration::from_secs(1)).layer(answering_after(Duration::from_secs(5)));

    let error = service.oneshot(()).await.unwrap_err();

    assert!(error.is::<Elapsed>());
}
