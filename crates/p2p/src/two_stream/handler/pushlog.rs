//! PushLog request and response methods.

use libp2p::PeerId;
use tokio::sync::oneshot;

use crate::codec::write_message;
use crate::error::{Error, Result};
use crate::message::{PushLogReply, PushLogRequest};

use super::{PendingResponseKey, TwoStreamHandler};

impl TwoStreamHandler {
    /// Send a request to a peer and return a receiver for the response.
    ///
    /// Opens a stream on the request protocol, sends the request, and returns
    /// the oneshot receiver. The caller MUST drop the handler lock before
    /// awaiting the receiver — otherwise the response handler cannot route
    /// the reply back (deadlock).
    pub async fn start_request(
        &mut self,
        peer_id: PeerId,
        request: PushLogRequest,
    ) -> Result<(String, oneshot::Receiver<PushLogReply>)> {
        let message_id = request.message_id.clone();
        let pending_key = PendingResponseKey::new(peer_id, message_id.clone());

        // Create response channel
        let (tx, rx) = oneshot::channel();

        // Register pending response
        self.pending.register_pushlog(pending_key.clone(), tx);

        // Open stream and send request
        let stream = self
            .control
            .open_stream(peer_id, Self::retry_request_protocol())
            .await;
        // No payload has been sent when negotiation fails. I/O failures
        // belong to the durable retry owner, not protocol fallback.
        let stream = match stream {
            Err(libp2p_stream::OpenStreamError::UnsupportedProtocol(_)) => {
                self.control
                    .open_stream(peer_id, Self::request_protocol())
                    .await
            }
            result => result,
        };
        let mut stream = stream.map_err(|e| {
            // Clean up pending on failure
            drop(self.pending.take_pushlog(&pending_key));
            Error::Transport(format!("failed to open stream: {}", e))
        })?;

        write_message(&mut stream, &request).await.map_err(|e| {
            // Clean up pending on failure
            drop(self.pending.take_pushlog(&pending_key));
            Error::CborSerialization(format!("failed to write request: {}", e))
        })?;

        tracing::debug!(
            peer_id = %peer_id,
            message_id = %message_id,
            doc_id = %request.doc_id,
            "Sent PushLog request on two-stream protocol"
        );

        Ok((message_id, rx))
    }

    /// Send a response to a peer.
    ///
    /// This opens a new stream on the response protocol and sends the reply.
    pub async fn send_response(&mut self, peer_id: PeerId, response: PushLogReply) -> Result<()> {
        let message_id = response.message_id.clone();

        tracing::info!(
            peer_id = %peer_id,
            message_id = %message_id,
            pubkey_len = response.pubkey.len(),
            "Opening response stream for two-stream protocol"
        );

        // Open stream and send response
        let mut stream = self
            .control
            .open_stream(peer_id, Self::response_protocol())
            .await
            .map_err(|e| Error::Transport(format!("failed to open response stream: {}", e)))?;

        write_message(&mut stream, &response)
            .await
            .map_err(|e| Error::CborSerialization(format!("failed to write response: {}", e)))?;

        tracing::info!(
            peer_id = %peer_id,
            message_id = %message_id,
            "Sent PushLog response on two-stream protocol"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use libp2p::{swarm::SwarmEvent, SwarmBuilder};
    use std::time::Duration;

    #[tokio::test]
    async fn retry_after_protocol_negotiates_and_falls_back_to_legacy() {
        tokio::time::timeout(Duration::from_secs(15), async {
            for negotiated in [true, false] {
                let sender_key = libp2p::identity::Keypair::generate_ed25519();
                let receiver_key = libp2p::identity::Keypair::generate_ed25519();
                let swarm = |key| {
                    SwarmBuilder::with_existing_identity(key).with_tokio()
                        .with_tcp(Default::default(), libp2p::noise::Config::new, libp2p::yamux::Config::default).unwrap()
                        .with_behaviour(|_| libp2p_stream::Behaviour::new()).unwrap().build()
                };
                let mut sender = swarm(sender_key.clone());
                let mut receiver = swarm(receiver_key.clone());
                let receiver_id = *receiver.local_peer_id();
                let mut sender_handler = TwoStreamHandler::new(sender.behaviour().new_control());
                let mut responses = sender.behaviour().new_control().accept(TwoStreamHandler::response_protocol()).unwrap();
                let mut receiver_handler = TwoStreamHandler::new(receiver.behaviour().new_control());
                let protocol = if negotiated { TwoStreamHandler::retry_request_protocol() } else { TwoStreamHandler::request_protocol() };
                let mut requests = receiver.behaviour().new_control().accept(protocol).unwrap();
                receiver.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
                let address = loop {
                    if let SwarmEvent::NewListenAddr { address, .. } = receiver.select_next_some().await { break address; }
                };
                sender.dial(address).unwrap();
                loop {
                    tokio::select! {
                        event = sender.select_next_some() => if matches!(event, SwarmEvent::ConnectionEstablished { .. }) { break; },
                        _ = receiver.select_next_some() => {}
                    }
                }
                let sender_task = tokio::spawn(async move { loop { sender.select_next_some().await; } });
                let receiver_task = tokio::spawn(async move { loop { receiver.select_next_some().await; } });
                let pending = sender_handler.pending_responses();
                let response_task = tokio::spawn(async move {
                    let (peer, stream) = responses.next().await.unwrap();
                    TwoStreamHandler::handle_response_stream(&pending, peer, stream, 65536, Duration::from_secs(5)).await.unwrap();
                });
                let request_task = tokio::spawn(async move {
                    let (peer, stream) = requests.next().await.unwrap();
                    let event = TwoStreamHandler::handle_request_stream(peer, stream, 65536, Duration::from_secs(5)).await.unwrap();
                    let crate::two_stream::event::TwoStreamEvent::InboundRequest { request, .. } = event else { panic!("expected PushLog") };
                    assert!(!request.supports_retry_after);
                    assert!(!request.supports_same_stream_reply);
                    let mut reply = PushLogReply::error(&request.message_id, crate::error::RATE_LIMITED_MESSAGE)
                        .with_retry_after(negotiated, Duration::from_millis(1234));
                    crate::signing::sign_message(&receiver_key, &mut reply).unwrap();
                    receiver_handler.send_response(peer, reply).await.unwrap();
                });
                let mut request = PushLogRequest::new("doc".into(), bytes::Bytes::new(), "collection".into(), "creator".into(), bytes::Bytes::new());
                crate::signing::sign_message(&sender_key, &mut request).unwrap();
                let (_, reply) = sender_handler.start_request(receiver_id, request).await.unwrap();
                let reply = reply.await.unwrap();
                assert_eq!(reply.retry_after_ms, negotiated.then_some(1234));
                assert_eq!(reply.err_message.as_deref(), Some(crate::error::RATE_LIMITED_MESSAGE));
                request_task.await.unwrap();
                response_task.await.unwrap();
                sender_task.abort();
                receiver_task.abort();
            }
        }).await.expect("protocol negotiation completes");
    }
}
