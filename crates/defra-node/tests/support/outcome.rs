//! The result type of the integration tests: the typed error of each call a
//! test propagates with `?`. Included by each test crate with
//! `#[path = "support/outcome.rs"] mod outcome;`.
#![allow(dead_code)]

#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error(transparent)]
    Embedded(#[from] defra_node::Error),
    #[error(transparent)]
    Acp(#[from] acp::Error),
    #[error(transparent)]
    Zanzibar(#[from] zanzibar::error::Error),
    #[error(transparent)]
    Cid(#[from] cid::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
    #[error(transparent)]
    Db(#[from] db::Error),
    #[error(transparent)]
    Identity(#[from] identity::Error),
    #[error(transparent)]
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    SliceLength(#[from] std::array::TryFromSliceError),
    #[error(transparent)]
    Peer(#[from] defra_p2p_adapter::P2PError),
    #[error("{0}")]
    Remote(String),
    #[cfg(feature = "libp2p")]
    #[error(transparent)]
    PeerIdParse(#[from] libp2p::identity::ParseError),
    #[cfg(feature = "libp2p")]
    #[error(transparent)]
    PeerKeyDecode(#[from] libp2p::identity::DecodingError),
    #[cfg(feature = "libp2p")]
    #[error(transparent)]
    PeerKeyType(#[from] libp2p::identity::OtherVariantError),
}

impl From<String> for TestError {
    fn from(message: String) -> Self {
        Self::Remote(message)
    }
}

pub type Result<T = (), E = TestError> = std::result::Result<T, E>;
