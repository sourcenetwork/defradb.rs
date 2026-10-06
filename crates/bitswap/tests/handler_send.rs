use std::io;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use asynchronous_codec::Framed;
use bitswap::handler::{outbound_substream, BitswapHandler, BitswapHandlerIn, HandlerEvent};
use bitswap::network::SendError;
use bitswap::{BitswapCodec, BitswapMessage, ProtocolConfig, ProtocolId, WantType};
use futures::io::{AsyncRead, AsyncWrite};
use futures::{poll, StreamExt};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent};
use tokio::sync::oneshot;
use unsigned_varint::codec::UviBytes;

mod common;
use common::cid_v1;

enum Mode {
    Fail,
    HoldUntil(Arc<AtomicBool>),
}

struct Wire(Mode);

impl AsyncRead for Wire {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Pending
    }
}

impl AsyncWrite for Wire {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &self.0 {
            Mode::Fail => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            Mode::HoldUntil(open) if open.load(Ordering::SeqCst) => Poll::Ready(Ok(buf.len())),
            Mode::HoldUntil(_) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn framed(mode: Mode) -> Framed<Wire, BitswapCodec> {
    Framed::new(
        Wire(mode),
        BitswapCodec::new(UviBytes::default(), ProtocolId::Bitswap120),
    )
}

fn message() -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_entry(cid_v1(b"wanted"), 1, WantType::Block, false);
    message
}

#[tokio::test]
async fn a_flush_failure_is_reported_as_a_send_error() {
    let (response, outcome) = oneshot::channel();
    let mut events = pin!(outbound_substream(
        framed(Mode::Fail),
        (message(), response)
    ));

    let event = events.next().await.unwrap();
    assert!(matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::FailedToSendMessage { .. })
    ));
    assert!(matches!(outcome.await.unwrap(), Err(SendError::Other(_))));
}

#[tokio::test]
async fn a_send_is_reported_only_after_the_frame_is_flushed() {
    let open = Arc::new(AtomicBool::new(false));
    let (response, mut outcome) = oneshot::channel();
    let mut events = pin!(outbound_substream(
        framed(Mode::HoldUntil(open.clone())),
        (message(), response)
    ));

    assert!(poll!(events.next()).is_pending());
    assert!(outcome.try_recv().is_err(), "nothing was flushed yet");

    open.store(true, Ordering::SeqCst);
    assert!(events.next().await.is_none());
    assert!(matches!(outcome.await.unwrap(), Ok(())));
}

fn handler() -> BitswapHandler {
    BitswapHandler::new(ProtocolConfig::default(), Duration::ZERO)
}

fn send(handler: &mut BitswapHandler) {
    let (response, _) = oneshot::channel();
    handler.on_behaviour_event(BitswapHandlerIn::Message(message(), response));
}

#[test]
fn protection_survives_sends_until_unprotected() {
    let mut handler = handler();
    send(&mut handler);
    assert!(
        !handler.connection_keep_alive(),
        "a zero idle timeout expires at once without protection"
    );

    handler.on_behaviour_event(BitswapHandlerIn::Protect);
    send(&mut handler);
    assert!(handler.connection_keep_alive());

    handler.on_behaviour_event(BitswapHandlerIn::Unprotect);
    assert!(handler.connection_keep_alive(), "30 s schedule restored");
    send(&mut handler);
    assert!(
        !handler.connection_keep_alive(),
        "activity arms the idle deadline again"
    );
}
