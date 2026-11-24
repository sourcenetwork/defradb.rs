//! The driver's inputs, in two lanes it waits on through one [`Wake`].
//!
//! The driver can wait on the event lane, behind a slow consumer. Control
//! input (what this node asks for, and what happens to the connections it
//! admitted) never waits, so the endpoint loop never waits on the driver.
//! What peers send waits for nothing either: it is dropped when its lane is
//! full, as gossip may drop any message. Modeled in
//! `proofs/tla/GossipInbox.tla`.

use iroh::EndpointId;

use super::frame::ProtoMessage;
use super::Input;
use crate::iroh::mailbox::{Mailbox, Wake};

pub(super) struct Inbox {
    control: Mailbox<Input>,
    received: Mailbox<(EndpointId, ProtoMessage)>,
    wake: Wake,
}

impl Inbox {
    pub(super) fn new(received: usize) -> Self {
        let wake = Wake::default();
        Self {
            control: Mailbox::unbounded(wake.clone()),
            received: Mailbox::new(received, wake.clone()),
            wake,
        }
    }

    pub(super) fn push(&self, input: Input) {
        let _ = self.control.try_push(input);
    }

    /// Queues a message from `peer`, or returns `false` when its lane is
    /// full and the message is dropped.
    pub(super) fn offer(&self, peer: EndpointId, message: ProtoMessage) -> bool {
        self.received.try_push((peer, message)).is_ok()
    }

    pub(super) fn close(&self) {
        self.control.close();
    }

    /// The next input, control first; `None` once closed and empty.
    pub(super) async fn next(&self) -> Option<Input> {
        loop {
            // Read the flag before looking, so an input queued just before
            // the close is still returned.
            let closed = self.control.is_closed();
            if let Some(input) = self.control.pop() {
                return Some(input);
            }
            if let Some((peer, message)) = self.received.pop() {
                return Some(Input::Received { peer, message });
            }
            if closed {
                return None;
            }
            self.wake.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rt::time::GossipInstant as Instant;
    use iroh_gossip::proto::{self, Command, InEvent, OutEvent, PeerData, TopicId};
    use rand::SeedableRng;

    use super::*;

    fn message() -> ProtoMessage {
        let peer = iroh::SecretKey::generate().public();
        let mut state = proto::State::new(
            iroh::SecretKey::generate().public(),
            PeerData::new(Vec::new()),
            proto::Config::default(),
            rand::rngs::StdRng::seed_from_u64(1),
        );
        let join = InEvent::Command(TopicId::from([7; 32]), Command::Join(vec![peer]));
        let message = state
            .handle(join, Instant::now(), None)
            .find_map(|event| match event {
                OutEvent::SendMessage(_, message) => Some(message),
                _ => None,
            })
            .expect("a join sends a message");
        message
    }

    #[nuclei::test]
    async fn a_full_received_lane_never_holds_up_control() {
        let inbox = Inbox::new(1);
        let peer = iroh::SecretKey::generate().public();
        assert!(inbox.offer(peer, message()));
        assert!(!inbox.offer(peer, message()));

        inbox.push(Input::Join("t".to_string()));
        assert!(matches!(inbox.next().await, Some(Input::Join(_))));
        assert!(matches!(inbox.next().await, Some(Input::Received { .. })));
    }

    #[nuclei::test]
    async fn inputs_queued_before_close_are_still_read() {
        let inbox = Inbox::new(1);
        inbox.push(Input::Quit("t".to_string()));
        inbox.close();
        assert!(matches!(inbox.next().await, Some(Input::Quit(_))));
        assert!(inbox.next().await.is_none());
    }
}
