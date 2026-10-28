mod action;
mod add_collection;
mod add_doc;
mod backup;
mod collection;
mod connect;
mod dac;
mod document;
mod encrypted_index;
mod expect;
mod index;
mod introspection_request;
mod lens;
mod migration;
mod nac;
mod new_index;
mod node;
mod parallel;
mod patch_collection;
mod peer;
mod query;
mod replicator;
mod request;
mod scenario;
mod schema;
pub mod shared;
mod signature;
mod subscription;
mod txn;
mod view;
mod wait;

pub use action::*;
pub use add_collection::AddCollection;
pub use add_doc::AddDoc;
pub use backup::*;
pub use collection::*;
pub use connect::Connect;
pub use dac::*;
pub use document::*;
pub use encrypted_index::*;
pub use index::*;
pub use introspection_request::IntrospectionRequest;
pub use lens::*;
pub use migration::*;
pub use nac::*;
pub use new_index::NewIndex;
pub use node::*;
pub use parallel::Parallel;
pub use patch_collection::PatchCollection;
pub use peer::*;
pub use query::*;
pub use replicator::*;
pub use request::Request;
pub use scenario::*;
pub use schema::*;
pub use signature::*;
pub use subscription::*;
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
    ActionAwaitList(ActionAwaitList),
    ActionEventsStart(ActionEventsStart),
    ActionListAfterEventAsync(ActionListAfterEventAsync),
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
    DacRemoteStart(DacRemoteStart),
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
    GetCollectionsFiltered(GetCollectionsFiltered),
    GetNACStatus(GetNACStatus),
    GetNodeIdentity(GetNodeIdentity),
    ImportBackup(ImportBackup),
    IndexReleaseBuildGate(IndexReleaseBuildGate),
    IndexWaitBuildBlocked(IndexWaitBuildBlocked),
    IndexWaitBuildCollections(IndexWaitBuildCollections),
    LensCount(LensCount),
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
    NodeOpenCLI(NodeOpenCLI),
    NodeOpenHTTP(NodeOpenHTTP),
    NodeOpenHTTPSigning(NodeOpenHTTPSigning),
    NodeOpenIndexGated(NodeOpenIndexGated),
    NodeOpenIndexGatedPersistent(NodeOpenIndexGatedPersistent),
    NodeOpenIndexMultiGatedPersistent(NodeOpenIndexMultiGatedPersistent),
    NodeOpenIndexWatermarkGatedPersistent(NodeOpenIndexWatermarkGatedPersistent),
    NodeOpenP2P(NodeOpenP2P),
    NodeOpenP2PC(NodeOpenP2PC),
    NodeOpenP2PIdentity(NodeOpenP2PIdentity),
    NodeOpenP2PKMS(NodeOpenP2PKMS),
    NodeOpenP2PPersistent(NodeOpenP2PPersistent),
    NodeOpenP2PRelease(NodeOpenP2PRelease),
    NodeOpenP2PSE(NodeOpenP2PSE),
    NodeOpenP2PSEPersistent(NodeOpenP2PSEPersistent),
    NodeOpenP2PSigning(NodeOpenP2PSigning),
    NodeOpenPersistent(NodeOpenPersistent),
    NodeOpenSigningEd25519(NodeOpenSigningEd25519),
    NodeOpenSigningSecp256k1(NodeOpenSigningSecp256k1),
    NodeStop(NodeStop),
    NodeStopForRestart(NodeStopForRestart),
    ParallelActions(ParallelActions),
    ParallelDocs(ParallelDocs),
    PatchCollection(PatchCollection),
    PatchCollectionWhileAddingDoc(PatchCollectionWhileAddingDoc),
    PeerEventsStart(PeerEventsStart),
    PeerId(PeerId),
    PeerInfo(PeerInfo),
    PeerInfoValidated(PeerInfoValidated),
    ReEnableNAC(ReEnableNAC),
    RefreshViews(RefreshViews),
    Request(Request),
    Restart(Restart),
    SaveDoc(SaveDoc),
    ScenarioExpectPending(ScenarioExpectPending),
    SetActiveCollectionVersion(SetActiveCollectionVersion),
    SignatureVerifySeed(SignatureVerifySeed),
    Start(Start),
    SubscriptionFinish(SubscriptionFinish),
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
    WaitForMerge(WaitForMerge),
    WaitForPeersEvents(WaitForPeersEvents),
    WaitForSESync(WaitForSESync),
    WaitForUpdate(WaitForUpdate),
    WaitForUpdateFields(WaitForUpdateFields),
    WaitForUpdates(WaitForUpdates),
}
