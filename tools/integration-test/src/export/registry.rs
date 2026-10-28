use serde_json::Value;

use super::convert::{from_wire, to_wire, v, Variant};
use super::wire::WireStep;
use crate::actions::*;

/// What the generator needs to write an action as Rust.
pub struct Described {
    /// The action's Rust type.
    pub type_name: &'static str,
    /// An instance of it with nothing set, as JSON: its keys are the fields
    /// that are not optional.
    pub empty: Value,
    /// Fields the op's name implies, which the step does not spell out.
    pub implied: Vec<(String, Value)>,
    /// The action's field that holds the step's expectation.
    pub expect_field: &'static str,
}

/// The one table that maps each action to the ops the scenario corpus calls
/// it. An action with options stands for several ops, told apart by its
/// fields. The JSON export writes the corpus name through this table, and the
/// generator reads it the other way.
macro_rules! registry {
    ($($op:ident => [$($variant:expr),+ $(,)?]),* $(,)?) => {
        /// Each action and the names of its ops in the scenario corpus.
        pub fn corpus_names() -> Vec<(&'static str, &'static str)> {
            let mut names = Vec::new();
            $(for variant in [$($variant),+] {
                names.push((stringify!($op), variant.op));
            })*
            names
        }

        impl Actions {
            /// The action as the scenario steps it exports to. A parallel group
            /// exports its children as consecutive group steps.
            pub fn wires(&self) -> Vec<WireStep> {
                match self {
                    Self::Connect(_) | Self::IntrospectionRequest(_) => Vec::new(),
                    Self::Parallel(group) => group.actions.iter().flat_map(Actions::wires).collect(),
                    $(Self::$op(action) => vec![to_wire(action, &[$($variant),+])],)*
                }
            }
        }

        /// The Rust type behind a scenario op.
        pub fn describe(op: &str) -> Option<Described> {
            $({
                let variants: Vec<Variant> = vec![$($variant),+];
                if let Some(variant) = variants.iter().find(|v| v.op == op) {
                    let empty = serde_json::to_value($op::default()).ok()?;
                    let implied = variant
                        .implied
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), serde_json::from_str(v).unwrap_or(Value::Null)))
                        .collect();
                    let expect_field = match variant.renames.first() {
                        Some(("expectedResults", _)) => "expected_results",
                        _ => "expected_data",
                    };
                    return Some(Described { type_name: stringify!($op), empty, implied, expect_field });
                }
            })*
            None
        }

        /// Reads a scenario step as the op's action and writes it back; the
        /// step must survive.
        pub fn check_typed(step: &WireStep) -> Result<(), String> {
            $({
                let variants: Vec<Variant> = vec![$($variant),+];
                if variants.iter().any(|v| v.op == step.op) {
                    return from_wire::<$op>(step, &variants).map(drop);
                }
            })*
            Err(format!("no action for op {}", step.op))
        }
    };
}

registry! {
    ActionAwaitList => [v("action.awaitList")],
    ActionEventsStart => [v("action.events.start")],
    ActionListAfterEventAsync => [v("action.listAfterEventAsync")],
    ActivePeers => [v("peer.active")],
    AddCollection => [v("schema.addWithMetadata").all(&["expectedResults"]).renames(&[("expectedResults", "expectedData")]), v("schema.add")],
    AddCollectionSubscription => [v("peer.collections.add")],
    AddDACActorRelationship => [v("dac.addActorRelationship")],
    AddDACPolicy => [v("dac.addPolicy")],
    AddDoc => [v("document.createManyEncrypted").all(&["documents", "encryptDoc"]), v("document.createMany").all(&["documents"]), v("document.createEncrypted").all(&["encryptDoc"]), v("document.create")],
    AddDocumentSubscription => [v("peer.documents.add")],
    AddLens => [v("lens.add")],
    AddNACActorRelationship => [v("nac.addActorRelationship")],
    AddReplicator => [v("replicator.add")],
    AddView => [v("view.add")],
    Await => [v("schema.awaitPatch")],
    Benchmark => [v("query.benchmarkIndex")],
    Close => [v("node.close")],
    ConfigureMigration => [v("migration.set")],
    ConnectPeers => [v("peer.connect")],
    DacRemoteStart => [v("dac.remote.start")],
    DeleteCollection => [v("collection.delete")],
    DeleteCollectionSubscription => [v("peer.collections.remove")],
    DeleteDACActorRelationship => [v("dac.deleteActorRelationship")],
    DeleteDoc => [v("document.delete")],
    DeleteDocumentSubscription => [v("peer.documents.remove")],
    DeleteEncryptedIndex => [v("encryptedIndex.delete")],
    DeleteIndex => [v("index.delete")],
    DeleteNACActorRelationship => [v("nac.deleteActorRelationship")],
    DeleteReplicator => [v("replicator.remove")],
    DeleteWithFilter => [v("document.deleteWithFilter")],
    DisableNAC => [v("nac.disable")],
    DisconnectPeers => [v("peer.disconnect")],
    ExportBackup => [v("backup.export")],
    GetCollections => [v("collection.list")],
    GetCollectionsFiltered => [v("collection.listFiltered")],
    GetNACStatus => [v("nac.status")],
    GetNodeIdentity => [v("node.getIdentity")],
    ImportBackup => [v("backup.import")],
    IndexReleaseBuildGate => [v("index.releaseBuildGate")],
    IndexWaitBuildBlocked => [v("index.waitBuildBlocked")],
    IndexWaitBuildCollections => [v("index.waitBuildCollections")],
    LensCount => [v("lens.count")],
    ListActions => [v("action.list")],
    ListAllEncryptedIndexes => [v("encryptedIndex.listAll")],
    ListEncryptedIndexes => [v("encryptedIndex.list")],
    ListIndexes => [v("index.list")],
    ListLenses => [v("lens.listMetadata")],
    ListP2PCollections => [v("peer.collections.list")],
    ListP2PDocuments => [v("peer.documents.list")],
    ListReplicators => [v("replicator.list")],
    NewEncryptedIndex => [v("encryptedIndex.create")],
    NewIndex => [v("index.createConfiguredAsync").all(&["async"]).any(&["index.ordered", "index.vector"]).implied(&[("async", "true")]), v("index.createAsync").all(&["async"]).implied(&[("async", "true")]), v("index.createConfigured").any(&["index.ordered", "index.vector"]), v("index.create")],
    NewNode => [v("node.open")],
    NodeOpenCLI => [v("node.openCLI")],
    NodeOpenHTTP => [v("node.openHTTP")],
    NodeOpenHTTPSigning => [v("node.openHTTPSigning")],
    NodeOpenIndexGated => [v("node.openIndexGated")],
    NodeOpenIndexGatedPersistent => [v("node.openIndexGatedPersistent")],
    NodeOpenIndexMultiGatedPersistent => [v("node.openIndexMultiGatedPersistent")],
    NodeOpenIndexWatermarkGatedPersistent => [v("node.openIndexWatermarkGatedPersistent")],
    NodeOpenP2P => [v("node.openP2P")],
    NodeOpenP2PC => [v("node.openP2PC")],
    NodeOpenP2PIdentity => [v("node.openP2PIdentity")],
    NodeOpenP2PKMS => [v("node.openP2PKMS")],
    NodeOpenP2PPersistent => [v("node.openP2PPersistent")],
    NodeOpenP2PRelease => [v("node.openP2PRelease")],
    NodeOpenP2PSE => [v("node.openP2PSE")],
    NodeOpenP2PSEPersistent => [v("node.openP2PSEPersistent")],
    NodeOpenP2PSigning => [v("node.openP2PSigning")],
    NodeOpenPersistent => [v("node.openPersistent")],
    NodeOpenSigningEd25519 => [v("node.openSigningEd25519")],
    NodeOpenSigningSecp256k1 => [v("node.openSigningSecp256k1")],
    NodeStop => [v("node.stop")],
    NodeStopForRestart => [v("node.stopForRestart")],
    ParallelActions => [v("action.parallel")],
    ParallelDocs => [v("document.parallel")],
    PatchCollection => [v("schema.patchWithLensAsync").all(&["lens", "async"]).implied(&[("async", "true")]), v("schema.patchWithLens").all(&["lens"]), v("schema.patch")],
    PatchCollectionWhileAddingDoc => [v("schema.patchWhileCreating")],
    PeerEventsStart => [v("peer.events.start")],
    PeerId => [v("peer.id")],
    PeerInfo => [v("peer.info")],
    PeerInfoValidated => [v("peer.infoValidated")],
    ReEnableNAC => [v("nac.reEnable")],
    RefreshViews => [v("view.refresh")],
    Request => [v("query.execute")],
    Restart => [v("node.restart")],
    SaveDoc => [v("document.save")],
    ScenarioExpectPending => [v("scenario.expectPending")],
    SetActiveCollectionVersion => [v("collection.setActiveVersion")],
    SignatureVerifySeed => [v("signature.verifySeed")],
    Start => [v("node.startNAC")],
    SubscriptionFinish => [v("subscription.finish")],
    SubscriptionRequest => [v("subscription.start")],
    SyncBranchableCollection => [v("collection.syncBranchable")],
    SyncCollectionVersions => [v("collection.syncVersions")],
    SyncDocs => [v("document.sync")],
    Truncate => [v("collection.truncate")],
    TxnBegin => [v("transaction.begin")],
    TxnCommit => [v("transaction.commit")],
    TxnDiscard => [v("transaction.discard")],
    UpdateDoc => [v("document.update")],
    UpdateWithFilter => [v("document.updateWithFilter")],
    VerifyBlockSignature => [v("signature.verify")],
    Wait => [v("action.wait").all(&["expectedAction"]), v("node.wait")],
    WaitForIndexReady => [v("index.waitBuilt")],
    WaitForMerge => [v("peer.awaitMerge")],
    WaitForPeersEvents => [v("peer.awaitTopics")],
    WaitForSESync => [v("peer.awaitSESync")],
    WaitForUpdate => [v("peer.awaitUpdate")],
    WaitForUpdateFields => [v("peer.awaitUpdateFields")],
    WaitForUpdates => [v("peer.awaitUpdates")],
}
