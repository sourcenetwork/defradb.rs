//! Which P2P transport the in-process nodes run: `DEFRA_TRANSPORT`, libp2p
//! when unset. Only `vinc` changes anything today, and it needs the `vinc`
//! feature.

/// The transport `DEFRA_TRANSPORT` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportChoice {
    Libp2p,
    Vinc,
}

impl TransportChoice {
    pub const ENV: &'static str = "DEFRA_TRANSPORT";

    pub fn from_env() -> Self {
        match std::env::var(Self::ENV).as_deref() {
            Ok("vinc") => Self::Vinc,
            Ok("libp2p") | Ok("") | Err(_) => Self::Libp2p,
            Ok(other) => panic!("{} names an unknown transport: {other}", Self::ENV),
        }
    }

    pub fn is_vinc(self) -> bool {
        self == Self::Vinc
    }
}
