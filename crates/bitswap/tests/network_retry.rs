use std::future::poll_fn;
use std::time::Duration;

use bitswap::network::{Network, NetworkError, OutEvent, SendError};
use bitswap::BitswapMessage;
use libp2p::PeerId;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[tokio::test(start_paused = true)]
async fn backoff_sleeps_between_every_pair_of_attempts() {
    let (network, mut events) = Network::new(PeerId::random());
    let (attempts, mut seen) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            if let OutEvent::SendMessage { response, .. } = poll_fn(|cx| events.poll_next(cx)).await
            {
                let _ = attempts.send(Instant::now());
                let _ = response.send(Err(SendError::Other("refused".into())));
            }
        }
    });

    let backoff = Duration::from_secs(1);
    let outcome = network
        .send_message_with_retry_and_timeout(
            PeerId::random(),
            BitswapMessage::new(false),
            3,
            Duration::from_secs(60),
            backoff,
        )
        .await;

    assert!(matches!(outcome, Err(NetworkError::SendFailed { errors, .. }) if errors.len() == 3));
    let times: Vec<Instant> = std::iter::from_fn(|| seen.try_recv().ok()).collect();
    assert_eq!(times.len(), 3);
    assert_eq!(times[1] - times[0], backoff);
    assert_eq!(times[2] - times[1], backoff);
}
