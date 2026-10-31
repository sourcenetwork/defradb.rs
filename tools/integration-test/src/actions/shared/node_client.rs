use serde::{Deserialize, Serialize};

/// How a test reaches a node it started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeClient {
    #[default]
    Cli,
    Http,
    C,
}
