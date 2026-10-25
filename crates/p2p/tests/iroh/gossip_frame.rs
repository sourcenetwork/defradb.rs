//! Frames read back as written, and a frame over the limit is refused from
//! its length alone, as `proofs/lean/GossipFrame/Framing.lean` proves.

use iroh_gossip::proto::{self, Command, InEvent, OutEvent, PeerData, TopicId};
use n0_future::time::Instant;
use rand::SeedableRng;

use super::*;

const ALPN: &[u8] = b"test/gossip-frame";

/// A real protocol message: the join a fresh node sends its bootstrap peer.
fn join_message() -> ProtoMessage {
    let me = iroh::SecretKey::generate().public();
    let peer = iroh::SecretKey::generate().public();
    let mut state = proto::State::new(
        me,
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

async fn uni_stream() -> (SendStream, RecvStream, [iroh::Endpoint; 2]) {
    let bind = || async {
        iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(iroh::RelayMode::Disabled)
            .alpns(vec![ALPN.to_vec()])
            .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .expect("bind addr")
            .bind()
            .await
            .expect("bind endpoint")
    };
    let server = bind().await;
    let client = bind().await;
    let accept = n0_future::task::spawn({
        let server = server.clone();
        async move { server.accept().await.unwrap().await.unwrap() }
    });
    let connection = client.connect(server.addr(), ALPN).await.expect("connect");
    let far_side = accept.await.unwrap();
    let mut send = connection.open_uni().await.expect("open uni");
    // A stream is announced to the far side only once it carries data.
    send.write_all(&[0]).await.unwrap();
    let mut recv = far_side.accept_uni().await.expect("accept uni");
    let mut first = [0u8; 1];
    recv.read_exact(&mut first).await.unwrap();
    (send, recv, [server, client])
}

#[tokio::test]
async fn frames_read_back_as_written_until_the_stream_ends() {
    let (mut send, mut recv, _endpoints) = uni_stream().await;
    let message = join_message();
    for _ in 0..2 {
        write_gossip_frame(&mut send, &message, 4096).await.unwrap();
    }
    send.finish().unwrap();

    let expected = postcard::to_stdvec(&message).unwrap();
    for _ in 0..2 {
        let read = read_gossip_frame(&mut recv, 4096).await.unwrap().unwrap();
        assert_eq!(postcard::to_stdvec(&read).unwrap(), expected);
    }
    assert!(read_gossip_frame(&mut recv, 4096).await.unwrap().is_none());
}

#[tokio::test]
async fn a_frame_over_the_limit_is_refused_before_its_body_arrives() {
    let (mut send, mut recv, _endpoints) = uni_stream().await;
    // Only the length is sent: refusing must not wait for a body.
    send.write_all(&4097u32.to_be_bytes()).await.unwrap();
    assert!(read_gossip_frame(&mut recv, 4096).await.is_err());

    let message = join_message();
    let len = postcard::to_stdvec(&message).unwrap().len();
    assert!(write_gossip_frame(&mut send, &message, len - 1)
        .await
        .is_err());
}
