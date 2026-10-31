mod backup;
mod block;
mod collection;
mod dac;
mod document;
mod encrypted_index;
mod expect;
mod index;
mod lens;
mod list_actions;
mod nac;
mod node;
mod parallel;
mod peer;
mod query;
mod replicator;
pub mod shared;
mod txn;
mod view;
mod wait;

pub use backup::*;
pub use block::*;
pub use collection::*;
pub use dac::*;
pub use document::*;
pub use encrypted_index::*;
pub use index::*;
pub use lens::*;
pub use list_actions::ListActions;
pub use nac::*;
pub use node::*;
pub use parallel::Parallel;
pub use peer::*;
pub use query::*;
pub use replicator::*;
pub use txn::*;
pub use view::*;
pub use wait::Wait;

use serde::{Deserialize, Serialize};
use testo::derive::Action;
use testo::prelude::*;

use crate::client_type::ClientType;
use crate::clients::{Cli, Http};
use crate::test_case::State;

/// Builds [`Actions`] from two lists. Each action in the first runs with its
/// own body for every client: it gets an `Http…` and a `Cli…` variant, which
/// compile only once the action implements `HttpRun` and `CliRun`, and the
/// arm that swaps it for the client's version. An action in the second list
/// runs the same way under every client.
macro_rules! client_actions {
    (clients: [$($client:ident),* $(,)?], shared: [$($shared:ident),* $(,)?] $(,)?) => {
        paste::paste! {
            $(
                pub type [<Http $client>] = Http<$client>;
                pub type [<Cli $client>] = Cli<$client>;
            )*

            /// Every action a test can run, named after the action Go's suite has for it.
            #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Action)]
            #[TState = "State"]
            pub enum Actions {
                $(
                    $client($client),
                    [<Http $client>]([<Http $client>]),
                    [<Cli $client>]([<Cli $client>]),
                )*
                $($shared($shared),)*
            }

            impl Actions {
                /// The version of this action `client` runs, when it has its own.
                fn swap(&self, client: ClientType) -> Option<Self> {
                    match (client, self) {
                        $(
                            (
                                ClientType::RustHttp | ClientType::GoHttp | ClientType::GoShinzo,
                                Self::$client(action),
                            ) => Some(Self::[<Http $client>](Http(action.clone()))),
                            (ClientType::Cli, Self::$client(action)) => {
                                Some(Self::[<Cli $client>](Cli(action.clone())))
                            }
                        )*
                        _ => None,
                    }
                }

                /// The node a per-client action runs against: its flattened
                /// `node` field, else the default node.
                pub(crate) fn target_node(&self) -> Option<String> {
                    let value = match self {
                        $(Self::$client(action) => serde_json::to_value(action).ok()?,)*
                        _ => return None,
                    };
                    Some(
                        value
                            .get("node")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(crate::scenario::DEFAULT_NODE)
                            .to_owned(),
                    )
                }

                /// The action a client's version wraps.
                pub fn base(&self) -> Option<Self> {
                    match self {
                        $(
                            Self::[<Http $client>](Http(action)) | Self::[<Cli $client>](Cli(action)) => {
                                Some(Self::$client(action.clone()))
                            }
                        )*
                        _ => None,
                    }
                }
            }
        }
    };
}

client_actions! {
    clients: [
        ActivePeers,
        AddCollection,
        AddCollectionSubscription,
        AddDoc,
        AddDocumentSubscription,
        AddLens,
        AddReplicator,
        ConfigureMigration,
        ConnectPeers,
        DeleteCollectionSubscription,
        DeleteDoc,
        DeleteDocumentSubscription,
        DeleteReplicator,
        DeleteWithFilter,
        DisconnectPeers,
        GetCollections,
        IntrospectionRequest,
        ListLenses,
        ListP2PCollections,
        ListP2PDocuments,
        ListReplicators,
        PatchCollection,
        PeerInfo,
        Request,
        SetActiveCollectionVersion,
        Truncate,
        TxnBegin,
        TxnCommit,
        TxnDiscard,
        UpdateDoc,
        UpdateWithFilter,
    ],
    shared: [
        Connect,
        Parallel,
        AddDACActorRelationship,
        AddDACPolicy,
        AddNACActorRelationship,
        AddView,
        Await,
        Benchmark,
        Close,
        DeleteCollection,
        DeleteDACActorRelationship,
        DeleteEncryptedIndex,
        DeleteIndex,
        DeleteNACActorRelationship,
        DisableNAC,
        ExportBackup,
        GetNACStatus,
        GetNodeIdentity,
        ImportBackup,
        ListActions,
        ListAllEncryptedIndexes,
        ListEncryptedIndexes,
        ListIndexes,
        NewEncryptedIndex,
        NewIndex,
        NewNode,
        PurgeDocs,
        ReEnableNAC,
        RefreshViews,
        SaveDocs,
        Restart,
        Start,
        SubscriptionRequest,
        SyncBranchableCollection,
        SyncCollectionVersions,
        SyncDocs,
        VerifyBlockSignature,
        Wait,
        WaitForIndexReady,
        WaitForPeersEvents,
        WaitForSESync,
        WaitForSync,
    ],
}

impl Actions {
    /// This action as `client` runs it: its own version for that client when
    /// it has one, and a parallel group with each child swapped.
    pub(crate) fn for_client(&self, client: ClientType) -> Self {
        match self {
            Self::Parallel(group) => Self::Parallel(Parallel {
                actions: group.actions.iter().map(|a| a.for_client(client)).collect(),
                ..group.clone()
            }),
            other => other.swap(client).unwrap_or_else(|| other.clone()),
        }
    }
}
