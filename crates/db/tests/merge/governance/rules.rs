//! Rules as code: a wasm module named by the version's rule tag judges its
//! composites and definitions through the step protocol, under a budget.
//!
//! The guests are hand-written WAT built in each test, so a dynamic
//! constant such as a CID can be embedded. Each exports `memory`, a bump
//! `alloc`, and `judge`, and answers with a constant CBOR response chosen
//! by the request's `step`, which the host places at byte offset 6.

use db::merge::governance::rule::{BlockstoreModules, RuleBudget, WasmRules};
use sha2::Digest as _;

use super::definition::{definition_block, patch_block};
use super::*;

/// `{"verdict":"accept"}`
const ACCEPT: &str = r#"\a1\67verdict\66accept"#;
/// `{"verdict":"reject","reason":"forged"}`
const REJECT: &str = r#"\a2\67verdict\66reject\66reason\66forged"#;

/// A guest answering `first` at step 0 and `then` at every later step.
/// Responses are CBOR bytes in WAT string syntax, without the length prefix.
fn guest(first: &str, then: &str) -> Vec<u8> {
    let first_len = wat_len(first);
    let then_len = wat_len(then);
    let wat = format!(
        r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 1024) "{first_prefix}{first}")
  (data (i32.const 2048) "{then_prefix}{then}")
  (func (export "alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $ptr))
  (func (export "judge") (param $ptr i32) (param $len i32) (result i32)
    (if (result i32)
      (i32.eqz (i32.load8_u (i32.add (local.get $ptr) (i32.const 6))))
      (then (i32.const 1024))
      (else (i32.const 2048)))))"#,
        first_prefix = le_u32(first_len),
        then_prefix = le_u32(then_len),
    );
    wat::parse_str(&wat).expect("guest WAT")
}

/// A guest that never returns: the budget is what stops it.
fn looping_guest() -> Vec<u8> {
    wat::parse_str(
        r#"(module
  (memory (export "memory") 1)
  (func (export "alloc") (param i32) (result i32) (i32.const 4096))
  (func (export "judge") (param i32) (param i32) (result i32)
    (loop $forever (br $forever))
    (i32.const 0)))"#,
    )
    .expect("guest WAT")
}

/// The byte length of a WAT string literal: escapes `\xx` count once.
fn wat_len(s: &str) -> u32 {
    let mut n = 0u32;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
            chars.next();
        }
        n += 1;
    }
    n
}

fn le_u32(n: u32) -> String {
    n.to_le_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect()
}

/// `{"verdict":"need","keys":[key]}` in WAT string syntax.
fn need(key: &str) -> String {
    assert!(key.len() < 256);
    let key_len = if key.len() < 24 {
        format!("\\{:02x}", 0x60 + key.len())
    } else {
        format!("\\78\\{:02x}", key.len())
    };
    format!(r#"\a2\67verdict\64need\64keys\81{key_len}{key}"#)
}

fn module_cid(bytes: &[u8]) -> Cid {
    use cid::multihash::Multihash;
    let digest = sha2::Sha256::digest(bytes);
    Cid::new_v1(0x55, Multihash::wrap(0x12, &digest).unwrap())
}

/// A node whose `Notes` are judged by `module`, with `Grants` carrying an
/// `@immutable` `writer` for lookups. Returns the node and the version ID
/// `Notes` composites must name.
async fn rules_node(module: &[u8], budget: RuleBudget) -> (Node, &'static str) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(
        DB::open_from_arc_with_options(store.clone(), DbOptions::default())
            .await
            .unwrap(),
    );
    let mut writer = FieldDescription::new("2", "writer", FieldKind::string());
    writer.immutable = true;
    db.create_collection(CollectionVersion::new(
        "Grants",
        "col-grants",
        "col-grants",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            writer,
        ],
    ))
    .await
    .unwrap();
    let rule = module_cid(module);
    let notes = query::parse_sdl(&format!(
        r#"type Notes @governed(root: "root-a", rule: "{rule}") {{ grant: String }}"#
    ))
    .unwrap()
    .remove(0);
    let version_id: &'static str = Box::leak(notes.version_id.clone().into_boxed_str());
    db.create_collection(notes).await.unwrap();

    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    blockstore.put(&rule, module).await.unwrap();
    let rules =
        WasmRules::with_budget(Arc::new(BlockstoreModules::new(blockstore.clone())), budget)
            .unwrap();
    db.set_merge_governance(
        MergeGovernance::new(["Notes", "Ledgers"]).with_validator(Arc::new(rules)),
    );
    let handler = Arc::new(DbMergeHandler::new(db.clone(), blockstore.clone()));
    handler.install_local_commit_release();
    let sink = Arc::new(RecordingSink::default());
    handler.set_redriven_merge_sink(sink.clone());
    (
        Node {
            db,
            blockstore,
            handler,
            sink,
        },
        version_id,
    )
}

#[tokio::test]
async fn a_rule_module_accepts_and_rejects() {
    let writer = signer();
    let (node, notes) = rules_node(&guest(ACCEPT, ACCEPT), RuleBudget::default()).await;
    let note = genesis(notes, "grant", "anything", &writer);
    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);

    let (node, notes) = rules_node(&guest(REJECT, REJECT), RuleBudget::default()).await;
    let note = genesis(notes, "grant", "anything", &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::rejected("forged")
    );
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
}

/// The module asks for a lookup on step 0 and accepts on step 1: the host
/// fetched what it named and ran it again.
#[tokio::test]
async fn a_rule_module_is_run_again_with_what_it_asked_for() {
    let writer = signer();
    let key = format!("find:Grants:writer:{}", hex::encode(b"\x65alice"));
    let (node, notes) = rules_node(&guest(&need(&key), ACCEPT), RuleBudget::default()).await;
    let note = genesis(notes, "grant", "anything", &writer);
    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
}

/// A key the host cannot satisfy is a defer naming it: the composite waits
/// for that arrival and is re-driven when it merges.
#[tokio::test]
async fn a_rule_module_needing_an_unheld_composite_defers_on_it() {
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let key = format!("fields:{}", grant.cid);
    let (node, notes) = rules_node(&guest(&need(&key), ACCEPT), RuleBudget::default()).await;
    let note = genesis(notes, "grant", "anything", &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip(format!("composite {} not held", grant.cid))
    );
    assert_eq!(node.handler.deferred_composites(), 1);

    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(node.handler.deferred_composites(), 0);
    assert!(
        node.forwarded().contains(&note.cid),
        "the note was not re-driven"
    );
}

/// A module that never returns is stopped by its fuel: an error, not a
/// verdict, and the composite stays unmerged.
#[tokio::test]
async fn a_rule_module_that_never_returns_is_stopped_by_its_budget() {
    let writer = signer();
    let (node, notes) = rules_node(
        &looping_guest(),
        RuleBudget {
            fuel: 100_000,
            ..RuleBudget::default()
        },
    )
    .await;
    let note = genesis(notes, "grant", "anything", &writer);
    note.store(&node).await;
    let outcome = node
        .handler
        .handle_block(
            &note.cid,
            note.bytes(),
            BlockMetadata::normal("", "", &writer.did, Some("peer"), false),
        )
        .await;
    assert!(outcome.is_err(), "a looping rule produced a verdict");
    assert!(node.forwarded().is_empty());
}

/// A version naming a module this node does not hold defers, and the sweep
/// re-judges it: the block's arrival merges nothing, so nothing else would.
#[tokio::test]
async fn a_rule_module_not_held_defers_until_it_is() {
    let writer = signer();
    let module = guest(ACCEPT, ACCEPT);
    let (node, notes) = rules_node(&module, RuleBudget::default()).await;
    // Forget the module: the rule tag still names it.
    let rule = module_cid(&module);
    node.blockstore.delete(&rule).await.unwrap();
    let note = genesis(notes, "grant", "anything", &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip(format!("rule module {rule} not held"))
    );
    node.blockstore.put(&rule, &module).await.unwrap();
    assert_eq!(node.handler.sweep_unmerged_governed().await, 1);
    assert!(
        node.forwarded().contains(&note.cid),
        "the note did not merge once its rule was held"
    );
}

/// An initial definition is judged by the module its own rule tag names:
/// there is no rule in force before it.
#[tokio::test]
async fn a_rule_module_judges_definitions_too() {
    let (node, _) = rules_node(&guest(ACCEPT, ACCEPT), RuleBudget::default()).await;
    let accepting = module_cid(&guest(ACCEPT, ACCEPT));
    let initial = definition_block(
        "Ledgers",
        &["_docID", "!writer"],
        Some("root-a"),
        false,
        Some(&accepting.to_string()),
    );
    assert_eq!(initial.merge(&node).await, MergeOutcome::Merged);

    let rejecting = guest(REJECT, REJECT);
    let rejecting_cid = module_cid(&rejecting);
    node.blockstore
        .put(&rejecting_cid, &rejecting)
        .await
        .unwrap();
    // The same name under another root is a different collection, an
    // initial definition judged by the module its own tag names.
    let refused = definition_block(
        "Ledgers",
        &["_docID", "!writer"],
        Some("root-b"),
        false,
        Some(&rejecting_cid.to_string()),
    );
    assert_eq!(refused.merge(&node).await, MergeOutcome::rejected("forged"));
    assert!(!node.holds_version(&refused.cid).await);
    let held = node.db.get_collection("Ledgers").unwrap().unwrap();
    assert_eq!(
        held.schema().governance_rule.as_deref(),
        Some(accepting.to_string().as_str())
    );
}

/// A patch that changes the rule is judged by the rule in force, the one the
/// version it supersedes names, so a patch cannot admit itself by naming a
/// permissive module. Here the accepting rule admits a change to the
/// rejecting one, and the rejecting rule then refuses the change back.
#[tokio::test]
async fn a_rule_change_is_judged_by_the_rule_in_force() {
    let accepting = guest(ACCEPT, ACCEPT);
    let rejecting = guest(REJECT, REJECT);
    let (node, _) = rules_node(&accepting, RuleBudget::default()).await;
    let rejecting_cid = module_cid(&rejecting);
    node.blockstore
        .put(&rejecting_cid, &rejecting)
        .await
        .unwrap();

    let initial = definition_block(
        "Ledgers",
        &["_docID", "!writer"],
        Some("root-a"),
        false,
        Some(&module_cid(&accepting).to_string()),
    );
    assert_eq!(initial.merge(&node).await, MergeOutcome::Merged);

    let to_rejecting = patch_block(&initial, &["note"], Some(&rejecting_cid.to_string()));
    assert_eq!(
        to_rejecting.merge(&node).await,
        MergeOutcome::Merged,
        "the rule in force, which accepts everything, did not admit the change"
    );

    let back = patch_block(
        &to_rejecting,
        &["more"],
        Some(&module_cid(&accepting).to_string()),
    );
    assert_eq!(
        back.merge(&node).await,
        MergeOutcome::rejected("forged"),
        "the rule now in force, which rejects everything, admitted a change"
    );
    assert!(!node.holds_version(&back.cid).await);
}
