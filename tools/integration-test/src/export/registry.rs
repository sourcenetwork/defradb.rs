use serde_json::Value;

use super::convert::{all_safe, from_wire, to_wire, v, Variant};
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
                    Self::Parallel(group) if group.children.is_none() && !group.expect_pending => {
                        group.actions.iter().flat_map(Actions::wires).collect()
                    }
                    $(Self::$op(action) => vec![to_wire(action, &[$($variant),+])],)*
                    client => client.base().map(|base| base.wires()).unwrap_or_default(),
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
                        Some(("expectedCount", _)) => "expected_count",
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
    ActivePeers => [v("peer.active")],
    AddCollection => [v("schema.addWithMetadata").all(&["expectedResults"]).renames(&[("expectedResults", "expectedData")]), v("schema.add")],
    AddCollectionSubscription => [v("peer.collections.add")],
    AddDACActorRelationship => [v("dac.addActorRelationship")],
    AddDACPolicy => [v("dac.addPolicy")],
    AddDoc => [v("document.createManyEncrypted").all(&["documents", "encryptDoc"]), v("document.createMany").all(&["documents"]), v("document.createEncrypted").all(&["encryptDoc"]), v("document.save").all(&["upsert"]).implied(&[("upsert", "true")]), v("document.create")],
    AddDocumentSubscription => [v("peer.documents.add")],
    AddLens => [v("lens.add")],
    AddNACActorRelationship => [v("nac.addActorRelationship")],
    AddReplicator => [v("replicator.add")],
    AddView => [v("view.add")],
    Await => [v("schema.awaitPatch")],
    Benchmark => [v("query.benchmarkIndex")],
    Close => [v("node.stopForRestart").all(&["keepData"]).implied(&[("keepData", "true")]), v("node.close")],
    ConfigureMigration => [v("migration.set")],
    ConnectPeers => [v("peer.connect")],
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
    GetCollections => [v("collection.listFiltered").all(&["filtered"]).implied(&[("filtered", "true")]), v("collection.list")],
    GetNACStatus => [v("nac.status")],
    GetNodeIdentity => [v("node.getIdentity")],
    ImportBackup => [v("backup.import")],
    ListActions => [v("action.awaitList").all(&["eventually"]).implied(&[("eventually", "{}")]), v("action.listAfterEventAsync").all(&["expectedAction"]), v("action.events.start").all(&["listen"]).implied(&[("listen", "true")]), v("action.list")],
    ListAllEncryptedIndexes => [v("encryptedIndex.listAll")],
    ListEncryptedIndexes => [v("encryptedIndex.list")],
    ListIndexes => [v("index.list")],
    ListLenses => [v("lens.count").all(&["expectedCount"]).renames(&[("expectedCount", "expectedData")]), v("lens.listMetadata")],
    ListP2PCollections => [v("peer.collections.list")],
    ListP2PDocuments => [v("peer.documents.list")],
    ListReplicators => [v("replicator.list")],
    NewEncryptedIndex => [v("encryptedIndex.create")],
    NewIndex => [v("index.createConfiguredAsync").all(&["async"]).any(&["index.ordered", "index.vector"]).implied(&[("async", "true")]), v("index.createAsync").all(&["async"]).implied(&[("async", "true")]), v("index.createConfigured").any(&["index.ordered", "index.vector"]), v("index.create")],
    NewNode => [v("node.openP2PSEPersistent").all(&["network", "enableSearchableEncryption", "databaseDir"]).implied(&[("network", "true"), ("enableSearchableEncryption", "true"), ("databaseDir", "\"\"")]), v("node.openP2PSE").all(&["network", "enableSearchableEncryption"]).implied(&[("network", "true"), ("enableSearchableEncryption", "true")]), v("node.openP2PPersistent").all(&["network", "databaseDir"]).implied(&[("network", "true"), ("databaseDir", "\"\"")]), v("node.openP2PRelease").all(&["network", "releaseVersion"]).implied(&[("network", "true")]), v("node.openP2PSigning").all(&["network", "enableSigning"]).implied(&[("network", "true"), ("enableSigning", "true")]), v("node.openP2PKMS").all(&["network", "kms"]).implied(&[("network", "true"), ("kms", "true")]), v("node.openP2PIdentity").all(&["network", "withIdentity"]).implied(&[("network", "true"), ("withIdentity", "true")]), v("node.openP2PC").all(&["network", "clientType"]).implied(&[("network", "true"), ("clientType", "\"C\"")]), v("node.openP2P").all(&["network"]).implied(&[("network", "true")]), v("node.openIndexMultiGatedPersistent").all(&["indexGate=\"Multi\"", "databaseDir"]).implied(&[("indexGate", "\"Multi\""), ("databaseDir", "\"\"")]), v("node.openIndexWatermarkGatedPersistent").all(&["indexGate=\"Watermark\"", "databaseDir"]).implied(&[("indexGate", "\"Watermark\""), ("databaseDir", "\"\"")]), v("node.openIndexGatedPersistent").all(&["indexGate", "databaseDir"]).implied(&[("indexGate", "\"Single\""), ("databaseDir", "\"\"")]), v("node.openIndexGated").all(&["indexGate"]).implied(&[("indexGate", "\"Single\"")]), v("node.openHTTPSigning").all(&["clientType", "enableSigning"]).implied(&[("clientType", "\"Http\""), ("enableSigning", "true")]), v("node.openHTTP").all(&["buckets"]), v("node.openCLI").all(&["clientType"]).implied(&[("clientType", "\"Cli\"")]), v("node.openSigningEd25519").all(&["enableSigning", "keyType=\"Ed25519\""]).implied(&[("enableSigning", "true"), ("keyType", "\"Ed25519\"")]), v("node.openSigningSecp256k1").all(&["enableSigning", "keyType=\"Secp256k1\""]).implied(&[("enableSigning", "true"), ("keyType", "\"Secp256k1\"")]), v("dac.remote.start").all(&["isDocumentAcpTest"]).implied(&[("isDocumentAcpTest", "true")]), v("node.openPersistent").all(&["databaseDir"]).implied(&[("databaseDir", "\"\"")]), v("node.open")],
    Parallel => [v("scenario.expectPending").all(&["expectPending"]).implied(&[("expectPending", "true")]), v("action.parallel").when(all_safe), v("document.parallel")],
    PatchCollection => [v("schema.patchWhileCreating").all(&["collection", "document"]), v("schema.patchWithLensAsync").all(&["lens", "async"]).implied(&[("async", "true")]), v("schema.patchWithLens").all(&["lens"]), v("schema.patch")],
    PeerInfo => [v("peer.infoValidated").all(&["validated"]).implied(&[("validated", "true")]), v("peer.id").all(&["idOnly"]).implied(&[("idOnly", "true")]), v("peer.info")],
    PurgeDocs => [v("document.purge")],
    ReEnableNAC => [v("nac.reEnable")],
    SaveDocs => [v("document.saveMany")],
    RefreshViews => [v("view.refresh")],
    Request => [v("query.execute")],
    Restart => [v("node.stop").all(&["stop"]).implied(&[("stop", "true")]), v("node.restart")],
    SetActiveCollectionVersion => [v("collection.setActiveVersion")],
    Start => [v("node.startNAC")],
    SubscriptionRequest => [v("subscription.start").all(&["query"]), v("subscription.finish")],
    SyncBranchableCollection => [v("collection.syncBranchable")],
    SyncCollectionVersions => [v("collection.syncVersions")],
    SyncDocs => [v("document.sync")],
    Truncate => [v("collection.truncate")],
    TxnBegin => [v("transaction.begin")],
    TxnCommit => [v("transaction.commit")],
    TxnDiscard => [v("transaction.discard")],
    UpdateDoc => [v("document.update")],
    UpdateWithFilter => [v("document.updateWithFilter")],
    VerifyBlockSignature => [v("signature.verifySeed").all(&["signerSeed"]), v("signature.verify")],
    Wait => [v("action.wait").all(&["expectedAction"]), v("node.wait")],
    WaitForIndexReady => [v("index.waitBuildCollections").all(&["expectedCount"]), v("index.waitBuilt").all(&["collection"]), v("index.waitBuildBlocked").all(&["blocked"]).implied(&[("blocked", "true")]), v("index.releaseBuildGate").all(&["releaseGate"]).implied(&[("releaseGate", "true")])],
    WaitForPeersEvents => [v("peer.events.start").all(&["start"]).implied(&[("start", "true")]), v("peer.awaitTopics")],
    WaitForSESync => [v("peer.awaitSESync")],
    WaitForSync => [v("peer.awaitMerge").all(&["docID", "blockCID"]), v("peer.awaitUpdateFields").all(&["updatedFields"]).implied(&[("updatedFields", "true")]), v("peer.awaitUpdates").all(&["docIDs"]), v("peer.awaitUpdate")],
}
