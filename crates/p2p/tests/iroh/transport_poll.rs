use super::*;
use futures::poll;
use nuclei::time::{timeout, Instant};

/// A transport, its command queue, and the half of its stop signal the
/// endpoint would hold: dropping that half is the endpoint stopping.
fn transport() -> (
    IrohTransport,
    mpsc::Receiver<IrohCommand>,
    oneshot::Sender<()>,
) {
    let (tx, rx) = mpsc::channel(1);
    let (running, stopped) = oneshot::channel();
    (
        IrohTransport::new((tx, stopped.shared()), SecretKey::generate()),
        rx,
        running,
    )
}

fn take_reply(commands: &mut mpsc::Receiver<IrohCommand>) -> oneshot::Sender<Result<()>> {
    match commands.try_recv().expect("expected a wait request") {
        IrohCommand::WaitConnected { reply, .. } => reply,
        _ => panic!("unexpected endpoint command"),
    }
}

#[nuclei::test(start_paused = true)]
async fn poll_returns_the_moment_the_peer_connects() {
    let (transport, mut commands, _running) = transport();
    let peer = PeerId::new("remote".into());
    let start = Instant::now();
    let waiting = transport.poll_until_connected(&peer, Duration::from_secs(5));
    let mut waiting = std::pin::pin!(waiting);

    assert!(poll!(&mut waiting).is_pending());
    take_reply(&mut commands).send(Ok(())).unwrap();

    waiting.await.unwrap();
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[nuclei::test(start_paused = true)]
async fn poll_preserves_endpoint_errors() {
    let (transport, mut commands, _running) = transport();
    let peer = PeerId::new("remote".into());
    let waiting = transport.poll_until_connected(&peer, Duration::from_secs(5));
    let mut waiting = std::pin::pin!(waiting);

    assert!(poll!(&mut waiting).is_pending());
    take_reply(&mut commands)
        .send(Err(Error::Transport("unavailable".into())))
        .unwrap();

    assert!(matches!(waiting.await, Err(Error::Transport(message)) if message == "unavailable"));
}

#[nuclei::test(start_paused = true)]
async fn poll_times_out_when_the_peer_never_connects() {
    let (transport, mut commands, _running) = transport();
    let peer = PeerId::new("remote".into());
    let start = Instant::now();
    let waiting = transport.poll_until_connected(&peer, Duration::from_millis(10));
    let mut waiting = std::pin::pin!(waiting);

    assert!(poll!(&mut waiting).is_pending());
    let pending_reply = take_reply(&mut commands);
    let result = timeout(Duration::from_millis(20), waiting)
        .await
        .expect("wait outlived the connection deadline");

    assert!(matches!(result, Err(Error::ConnectionTimeout(id)) if id == peer.as_str()));
    assert_eq!(start.elapsed(), Duration::from_millis(10));
    assert!(
        pending_reply.is_closed(),
        "the endpoint is told to stop waiting"
    );
}

#[nuclei::test(start_paused = true)]
async fn poll_deadline_bounds_a_full_command_queue() {
    let (transport, _commands, _running) = transport();
    let (reply, _response) = oneshot::channel();
    assert!(transport
        .command_tx
        .try_send(IrohCommand::WaitConnected {
            peer_id: PeerId::new("other".into()),
            reply
        })
        .is_ok());
    let peer = PeerId::new("remote".into());
    let result = timeout(
        Duration::from_millis(20),
        transport.poll_until_connected(&peer, Duration::from_millis(10)),
    )
    .await
    .expect("command queue outlived the connection deadline");

    assert!(matches!(result, Err(Error::ConnectionTimeout(id)) if id == peer.as_str()));
}

#[nuclei::test(start_paused = true)]
async fn poll_stops_on_channel_closure_with_a_pending_reply() {
    let (transport, mut commands, running) = transport();
    let peer = PeerId::new("remote".into());
    let waiting = transport.poll_until_connected(&peer, Duration::from_secs(5));
    let mut waiting = std::pin::pin!(waiting);

    assert!(poll!(&mut waiting).is_pending());
    let _pending_reply = take_reply(&mut commands);
    commands.close();
    drop(running);

    assert!(matches!(
        poll!(&mut waiting),
        std::task::Poll::Ready(Err(Error::ChannelSend))
    ));
}
