use serde::{Deserialize, Serialize};

/// A hook that holds an index build back so a test can look at it half done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexGate {
    /// One gate before the build.
    #[default]
    Single,
    /// A gate on each of several collections.
    Multi,
    /// A gate after a non-zero watermark.
    Watermark,
}
