use serde::{Deserialize, Serialize};

/// The signing key algorithm of a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    #[default]
    Secp256k1,
    Ed25519,
}
