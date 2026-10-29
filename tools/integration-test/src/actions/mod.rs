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

use crate::test_case::State;

/// Every action a test can run, named after the action Go's suite has for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Action)]
#[TState = "State"]
pub enum Actions {
    Connect(Connect),
    IntrospectionRequest(IntrospectionRequest),
    Parallel(Parallel),
    ActivePeers(ActivePeers),
    AddCollection(AddCollection),
    AddCollectionSubscription(AddCollectionSubscription),
    AddDACActorRelationship(AddDACActorRelationship),
    AddDACPolicy(AddDACPolicy),
    AddDoc(AddDoc),
    AddDocumentSubscription(AddDocumentSubscription),
    AddLens(AddLens),
    AddNACActorRelationship(AddNACActorRelationship),
    AddReplicator(AddReplicator),
    AddView(AddView),
    Await(Await),
    Benchmark(Benchmark),
    Close(Close),
    ConfigureMigration(ConfigureMigration),
    ConnectPeers(ConnectPeers),
    DeleteCollection(DeleteCollection),
    DeleteCollectionSubscription(DeleteCollectionSubscription),
    DeleteDACActorRelationship(DeleteDACActorRelationship),
    DeleteDoc(DeleteDoc),
    DeleteDocumentSubscription(DeleteDocumentSubscription),
    DeleteEncryptedIndex(DeleteEncryptedIndex),
    DeleteIndex(DeleteIndex),
    DeleteNACActorRelationship(DeleteNACActorRelationship),
    DeleteReplicator(DeleteReplicator),
    DeleteWithFilter(DeleteWithFilter),
    DisableNAC(DisableNAC),
    DisconnectPeers(DisconnectPeers),
    ExportBackup(ExportBackup),
    GetCollections(GetCollections),
    GetNACStatus(GetNACStatus),
    GetNodeIdentity(GetNodeIdentity),
    ImportBackup(ImportBackup),
    ListActions(ListActions),
    ListAllEncryptedIndexes(ListAllEncryptedIndexes),
    ListEncryptedIndexes(ListEncryptedIndexes),
    ListIndexes(ListIndexes),
    ListLenses(ListLenses),
    ListP2PCollections(ListP2PCollections),
    ListP2PDocuments(ListP2PDocuments),
    ListReplicators(ListReplicators),
    NewEncryptedIndex(NewEncryptedIndex),
    NewIndex(NewIndex),
    NewNode(NewNode),
    PatchCollection(PatchCollection),
    PeerInfo(PeerInfo),
    ReEnableNAC(ReEnableNAC),
    RefreshViews(RefreshViews),
    Request(Request),
    Restart(Restart),
    SetActiveCollectionVersion(SetActiveCollectionVersion),
    Start(Start),
    SubscriptionRequest(SubscriptionRequest),
    SyncBranchableCollection(SyncBranchableCollection),
    SyncCollectionVersions(SyncCollectionVersions),
    SyncDocs(SyncDocs),
    Truncate(Truncate),
    TxnBegin(TxnBegin),
    TxnCommit(TxnCommit),
    TxnDiscard(TxnDiscard),
    UpdateDoc(UpdateDoc),
    UpdateWithFilter(UpdateWithFilter),
    VerifyBlockSignature(VerifyBlockSignature),
    Wait(Wait),
    WaitForIndexReady(WaitForIndexReady),
    WaitForPeersEvents(WaitForPeersEvents),
    WaitForSESync(WaitForSESync),
    WaitForSync(WaitForSync),
}
