use bitswap::{ProtocolConfig, ProtocolId};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade};
use libp2p::core::UpgradeInfo;
use multistream_select::{dialer_select_proto, listener_select_proto, Version};
use tokio_util::compat::TokioAsyncReadCompatExt;

const ALL: [ProtocolId; 4] = [
    ProtocolId::Legacy,
    ProtocolId::Bitswap100,
    ProtocolId::Bitswap110,
    ProtocolId::Bitswap120,
];

#[test]
fn default_config_prefers_newest_first() {
    let cfg = ProtocolConfig::default();
    assert_eq!(cfg.max_transmit_size, 2 * 1024 * 1024);
    let names: Vec<_> = cfg.protocol_info().iter().map(|p| p.to_string()).collect();
    assert_eq!(
        names,
        [
            "/ipfs/bitswap/1.2.0",
            "/ipfs/bitswap/1.1.0",
            "/ipfs/bitswap/1.0.0",
            "/ipfs/bitswap"
        ]
    );
}

#[test]
fn names_parse_back() {
    for p in ALL {
        assert_eq!(ProtocolId::try_from_str(p.protocol_name()), Some(p));
        assert_eq!(ProtocolId::try_from(p.protocol_name().as_bytes()), Some(p));
        assert_eq!(p.as_stream_protocol().as_ref(), p.protocol_name());
    }
    assert_eq!(ProtocolId::try_from_str("/ipfs/bitswap/9.9.9"), None);
    assert_eq!(ProtocolId::try_from([0xff, 0xfe]), None);
}

#[test]
fn only_120_supports_have() {
    assert_eq!(
        ALL.map(ProtocolId::supports_have),
        [false, false, false, true]
    );
}

#[test]
fn ordering_follows_version() {
    let mut v = [
        ProtocolId::Bitswap120,
        ProtocolId::Legacy,
        ProtocolId::Bitswap100,
    ];
    v.sort();
    assert_eq!(
        v,
        [
            ProtocolId::Legacy,
            ProtocolId::Bitswap100,
            ProtocolId::Bitswap120
        ]
    );
}

async fn negotiate(listener: ProtocolConfig, dialer: ProtocolConfig) -> (ProtocolId, ProtocolId) {
    let (a, b) = tokio::io::duplex(64 * 1024);
    let server = async move {
        let (protocol, stream) = listener_select_proto(a.compat(), listener.protocol_info())
            .await
            .unwrap();
        let framed = listener.upgrade_inbound(stream, protocol).await.unwrap();
        framed.codec().protocol
    };
    let client = async move {
        let (protocol, stream) =
            dialer_select_proto(b.compat(), dialer.protocol_info(), Version::V1Lazy)
                .await
                .unwrap();
        let framed = dialer.upgrade_outbound(stream, protocol).await.unwrap();
        framed.codec().protocol
    };
    futures::future::join(server, client).await
}

#[tokio::test]
async fn both_default_lists_select_120() {
    let got = negotiate(ProtocolConfig::default(), ProtocolConfig::default()).await;
    assert_eq!(got, (ProtocolId::Bitswap120, ProtocolId::Bitswap120));
}

#[tokio::test]
async fn listener_with_only_100_forces_100() {
    let listener = ProtocolConfig {
        protocol_ids: vec![ProtocolId::Bitswap100],
        ..Default::default()
    };
    let got = negotiate(listener, ProtocolConfig::default()).await;
    assert_eq!(got, (ProtocolId::Bitswap100, ProtocolId::Bitswap100));
}
