//! Which splitting rules a `@chunk` field's chunks were cut with.

use thiserror::Error;

/// One set of splitting rules. Two nodes cut a text the same way only on the same version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SplitterVersion {
    V1,
}

impl SplitterVersion {
    /// The version new `@chunk` fields are declared with.
    pub const CURRENT: Self = Self::V1;
    /// Every version this build can split with.
    pub const ALL: [Self; 1] = [Self::V1];

    /// The number a collection definition stores.
    pub const fn number(self) -> u32 {
        match self {
            Self::V1 => 1,
        }
    }
}

impl TryFrom<u32> for SplitterVersion {
    type Error = UnknownSplitterVersion;

    fn try_from(version: u32) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|known| known.number() == version)
            .ok_or(UnknownSplitterVersion { version })
    }
}

/// A definition names splitting rules this build lacks, so it cannot cut or check its chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "@chunk splitter version {version} is not implemented by this node, which knows {}. Upgrade the node to write or check these chunks",
    known_versions()
)]
pub struct UnknownSplitterVersion {
    pub version: u32,
}

fn known_versions() -> String {
    SplitterVersion::ALL
        .iter()
        .map(|known| known.number().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
