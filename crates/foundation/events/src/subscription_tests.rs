use super::*;

#[test]
fn test_subscription_recv() {
    workers::block_on(async {
        let (tx, rx) = async_channel::bounded(10);
        let mut sub = Subscription::new(1, rx);

        // Send a message
        let msg = Message::merge();
        tx.send(msg).await.unwrap();

        // Receive it
        let received = sub.recv().await;
        assert!(received.is_some());
        assert_eq!(received.unwrap().name, crate::event::EventName::Merge);

        // Close the channel
        drop(tx);

        // Receive should return None
        let received = sub.recv().await;
        assert!(received.is_none());
    })
    .unwrap()
}

#[test]
fn test_subscription_try_recv() {
    let (tx, rx) = async_channel::bounded(10);
    let mut sub = Subscription::new(1, rx);

    // No message yet
    assert!(sub.try_recv().is_err());

    // Send a message (blocking send in sync context)
    let msg = Message::merge();
    tx.send_blocking(msg).unwrap();

    // Now we can receive
    let received = sub.try_recv();
    assert!(received.is_ok());
}

#[test]
fn try_take_through_a_shared_reference_delivers_each_message_once() {
    const MESSAGES: usize = 1000;
    let (tx, rx) = async_channel::bounded(MESSAGES);
    let sub = std::sync::Arc::new(Subscription::new(1, rx));
    for _ in 0..MESSAGES {
        tx.send_blocking(Message::merge()).unwrap();
    }
    drop(tx);

    let pollers: Vec<_> = (0..4)
        .map(|_| {
            let sub = std::sync::Arc::clone(&sub);
            std::thread::spawn(move || {
                let mut taken = 0usize;
                loop {
                    match sub.try_take() {
                        Ok(_) => taken += 1,
                        Err(TryRecvError::Empty) => std::thread::yield_now(),
                        Err(TryRecvError::Disconnected) => return taken,
                    }
                }
            })
        })
        .collect();
    let taken: usize = pollers
        .into_iter()
        .map(|poller| poller.join().unwrap())
        .sum();

    assert_eq!(taken, MESSAGES);
    assert_eq!(sub.try_take().err(), Some(TryRecvError::Disconnected));
}
