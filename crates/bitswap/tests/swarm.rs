use std::time::Duration;

use asynchronous_codec::Framed;
use bitswap::{
    Bitswap, BitswapCodec, BitswapMessage, Block, Config, ProtocolConfig, ProtocolId, WantType,
};
use futures::{SinkExt, StreamExt};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_stream as stream;
use nuclei::time::timeout;
use unsigned_varint::codec::UviBytes;

mod common;
use common::block_v1;
use common::mem_store::MemStore;

const DEADLINE: Duration = Duration::from_secs(10);

fn build<B: NetworkBehaviour>(behaviour: impl FnOnce(PeerId) -> B) -> (PeerId, Swarm<B>) {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let swarm = net::libp2p::swarm(&keypair, behaviour(peer_id), Duration::from_secs(60)).unwrap();
    (peer_id, swarm)
}

async fn listen<B: NetworkBehaviour>(swarm: &mut Swarm<B>) -> Multiaddr {
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

fn framed(stream: libp2p::swarm::Stream) -> Framed<libp2p::swarm::Stream, BitswapCodec> {
    let mut length_codec = UviBytes::default();
    length_codec.set_max_len(ProtocolConfig::default().max_transmit_size);
    Framed::new(
        stream,
        BitswapCodec::new(length_codec, ProtocolId::Bitswap120),
    )
}

fn want_block(block: &Block) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_entry(block.cid, 1, WantType::Block, true);
    message
}

async fn connect_probe(
    server_addr: Multiaddr,
) -> (stream::Control, stream::IncomingStreams, PeerId) {
    let (_, mut probe) = build(|_| stream::Behaviour::new());
    let mut control = probe.behaviour().new_control();
    let incoming = control
        .accept(ProtocolId::Bitswap120.as_stream_protocol())
        .unwrap();
    probe.dial(server_addr).unwrap();
    let server_id = loop {
        if let SwarmEvent::ConnectionEstablished { peer_id, .. } = probe.select_next_some().await {
            break peer_id;
        }
    };
    nuclei::spawn(async move {
        loop {
            probe.select_next_some().await;
        }
    })
    .detach();
    (control, incoming, server_id)
}

#[nuclei::test(threads = 2)]
async fn server_replies_on_a_new_stream_per_message() {
    async {
        let first = block_v1(b"first block");
        let second = block_v1(b"second block");
        let store = MemStore::new(&[first.clone(), second.clone()]);

        let (_, mut server) = build(|id| Bitswap::new(id, store, Config::default()));
        let server_addr = listen(&mut server).await;
        nuclei::spawn(async move {
            loop {
                server.select_next_some().await;
            }
        })
        .detach();

        let (mut control, mut incoming, server_id) = connect_probe(server_addr).await;

        let outbound = control
            .open_stream(server_id, ProtocolId::Bitswap120.as_stream_protocol())
            .await
            .unwrap();
        let mut outbound = framed(outbound);

        outbound.send(want_block(&first)).await.unwrap();
        let (peer, reply_stream) = timeout(DEADLINE, incoming.next()).await.unwrap().unwrap();
        assert_eq!(peer, server_id);
        let mut reply_stream = framed(reply_stream);
        let (message, protocol) = timeout(DEADLINE, reply_stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(protocol, ProtocolId::Bitswap120);
        assert_eq!(
            message.blocks().map(|b| b.cid).collect::<Vec<_>>(),
            [first.cid]
        );
        assert!(
            timeout(DEADLINE, reply_stream.next())
                .await
                .unwrap()
                .is_none(),
            "the server closes the reply stream after one message"
        );

        outbound.send(want_block(&second)).await.unwrap();
        let (_, second_stream) = timeout(DEADLINE, incoming.next()).await.unwrap().unwrap();
        let mut second_stream = framed(second_stream);
        let (message, protocol) = timeout(DEADLINE, second_stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(protocol, ProtocolId::Bitswap120);
        assert_eq!(
            message.blocks().map(|b| b.cid).collect::<Vec<_>>(),
            [second.cid]
        );
        assert!(timeout(DEADLINE, second_stream.next())
            .await
            .unwrap()
            .is_none());
    }
    .await
}

#[nuclei::test(threads = 2)]
async fn missing_block_is_answered_with_dont_have_over_the_wire() {
    async {
        let missing = block_v1(b"not stored");
        let store = MemStore::new(&[]);

        let (_, mut server) = build(|id| Bitswap::new(id, store, Config::default()));
        let server_addr = listen(&mut server).await;
        nuclei::spawn(async move {
            loop {
                server.select_next_some().await;
            }
        })
        .detach();

        let (mut control, mut incoming, server_id) = connect_probe(server_addr).await;

        let mut outbound = framed(
            control
                .open_stream(server_id, ProtocolId::Bitswap120.as_stream_protocol())
                .await
                .unwrap(),
        );
        outbound.send(want_block(&missing)).await.unwrap();

        let (_, reply_stream) = timeout(DEADLINE, incoming.next()).await.unwrap().unwrap();
        let (message, _) = timeout(DEADLINE, framed(reply_stream).next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            message.dont_haves().copied().collect::<Vec<_>>(),
            [missing.cid]
        );
    }
    .await
}
