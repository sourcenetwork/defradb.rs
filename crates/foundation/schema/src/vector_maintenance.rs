//! When a vector index applies a write to its graph.
//!
//! Go's wire form (sourcenetwork/defradb#5326): the `Maintenance` member of a
//! vector index description, omitted for `SYNC`. `SYNC` reads back as the
//! omitted form, so there is one stored form for it and every description
//! stored before the modes existed is unchanged.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// When a vector index's graph is updated after a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VectorIndexMaintenance {
    /// The graph is updated in the writing transaction.
    #[default]
    Sync,
    /// The write records the change in a pending set and a background drain
    /// applies it to the graph later. Searches score the pending set too, so a
    /// committed write is searchable right away.
    Deferred,
}

impl VectorIndexMaintenance {
    /// Every mode, the default first.
    pub const ALL: &'static [Self] = &[Self::Sync, Self::Deferred];

    /// The name in `@index(vector: {maintenance:})` and on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "SYNC",
            Self::Deferred => "DEFERRED",
        }
    }

    /// The mode a name selects; the empty name is Go's unset form.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "" => Some(Self::Sync),
            _ => Self::ALL.iter().copied().find(|mode| mode.as_str() == name),
        }
    }

    pub fn is_sync(&self) -> bool {
        *self == Self::Sync
    }

    pub fn is_deferred(self) -> bool {
        self == Self::Deferred
    }
}

impl Serialize for VectorIndexMaintenance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Go's create path refuses an unknown mode with this text
/// (`NewErrVectorIndexUnknownMaintenance`).
impl<'de> Deserialize<'de> for VectorIndexMaintenance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Self::from_name(&name).ok_or_else(|| {
            D::Error::custom(format!(
                "vector index with unknown maintenance. Maintenance: {name}"
            ))
        })
    }
}
