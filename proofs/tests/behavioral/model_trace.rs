//! P2P delivery liveness, trace validation: a scenario on real nodes built
//! with `--features iroh,model-trace` logs every delivery step, and TLC
//! replays the recorded run through `P2PDelivery` under the fixed design's
//! knobs (`proofs/tla/Trace_P2PDelivery.tla`).
//! Model: `proofs/tla/P2PDelivery.tla`; collector: `conformance::model_trace`.
//!
//! Ignored by default: it needs `DEFRA_TRACE_BINARY` (a defra built with
//! `cargo build -p cli --features iroh,model-trace`) and a JDK for TLC.
//! Each run leaves its events, module, configuration and TLC output in
//! `target/model-trace/<scenario>/`.

use crate::support;
use conformance::model_trace::{self, Bounds, NodeLog, Trace};
use defra_harness::{BinarySource, DefraClient, TestCluster};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TRACE_BINARY: &str = "DEFRA_TRACE_BINARY";
const SCHEMA: &str = "type User { name: String  age: Int }";
const DELIVERY: Duration = Duration::from_secs(30);
const SETTLE: Duration = Duration::from_secs(1);

fn trace_binary() -> BinarySource {
    let _ = support::release_binary();
    let path = std::env::var_os(TRACE_BINARY).unwrap_or_else(|| {
        panic!("{TRACE_BINARY} must name a defra built with `cargo build -p cli --features iroh,model-trace`")
    });
    BinarySource::Path(PathBuf::from(path))
}

async fn cluster(nodes: usize) -> TestCluster {
    TestCluster::builder()
        .rust_nodes(nodes)
        .with_iroh_transport()
        .with_rust_binary(trace_binary())
        .build()
        .await
        .expect("build iroh cluster")
}

fn address(node: &DefraClient) -> String {
    node.p2p_info().expect("p2p info")[0]
        .as_str()
        .expect("p2p address")
        .to_string()
}

fn create(node: &DefraClient, age: i64) -> String {
    let created = node
        .query(&format!(
            r#"mutation {{ add_User(input: {{name: "traced", age: {age}}}) {{ _docID }} }}"#
        ))
        .expect("create");
    created["add_User"][0]["_docID"]
        .as_str()
        .expect("_docID")
        .to_string()
}

fn update(node: &DefraClient, doc: &str, age: i64) {
    node.query(&format!(
        r#"mutation {{ update_User(docID: "{doc}", input: {{age: {age}}}) {{ _docID }} }}"#
    ))
    .expect("update");
}

async fn wait_for_age(node: &DefraClient, doc: &str, age: i64) {
    let deadline = Instant::now() + DELIVERY;
    loop {
        let users = node
            .query("query { User { _docID age } }")
            .unwrap_or_default();
        let delivered = users["User"].as_array().is_some_and(|users| {
            users
                .iter()
                .any(|user| user["_docID"] == doc && user["age"] == age)
        });
        if delivered {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "age {age} of {doc} did not arrive within {DELIVERY:?}"
        );
        nuclei::time::sleep(Duration::from_millis(100)).await;
    }
}

fn node_logs(cluster: &TestCluster, names: &[&str]) -> Vec<NodeLog> {
    names
        .iter()
        .zip(&cluster.nodes)
        .map(|(name, node)| NodeLog {
            name: name.to_string(),
            log: node
                .rootdir
                .parent()
                .expect("node directory")
                .join("logs/stdout.log"),
        })
        .collect()
}

/// Collects the settled run's events, replays them through TLC and returns
/// the abstracted trace; panics with TLC's output when the replay fails.
async fn replay(
    cluster: &TestCluster,
    names: &[&str],
    repl: &[(&str, &str)],
    doc: &str,
    scenario: &str,
    story: &str,
) -> Trace {
    nuclei::time::sleep(SETTLE).await;
    let logs = node_logs(cluster, names);
    let events = model_trace::read(&logs).expect("read model_trace events");
    let work = support::workspace_root()
        .join("target/model-trace")
        .join(scenario);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("trace directory");
    let lines: Vec<String> = events
        .iter()
        .map(|event| {
            let mut fields = event.fields.clone();
            fields.insert("mt".into(), event.mt.into());
            fields.insert("by".into(), event.node.clone().into());
            serde_json::Value::Object(fields).to_string()
        })
        .collect();
    std::fs::write(work.join("events.jsonl"), lines.join("\n")).expect("write events");
    let trace = Trace::abstracted(&logs, repl, doc, &events, Bounds::default())
        .expect("abstract the trace");
    let tla = support::workspace_root().join("proofs/tla");
    let module = format!("MC_P2PDelivery_Trace_{scenario}");
    let verdict =
        model_trace::replay(&tla, &work, &module, &trace, story).expect("run TLC on the trace");
    assert!(
        verdict.accepted,
        "the recorded run is not a behaviour of P2PDelivery (see {}):\n{}",
        work.display(),
        verdict.output
    );
    trace
}

/// Non-vacuity: the replay walked the whole delivery of every head.
fn assert_delivered_through_the_pool(trace: &Trace, receivers: usize) {
    let heads = trace.heads.len();
    assert!(heads >= 2, "the scenario commits a create and an update");
    for (ev, want) in [
        ("Commit", heads),
        ("Register", heads * receivers),
        ("Schedule", heads * receivers),
        ("Merge", heads * receivers),
    ] {
        assert!(
            trace.count(ev) >= want,
            "trace has {} {ev} events, expected at least {want}",
            trace.count(ev)
        );
    }
    assert!(
        trace
            .steps
            .iter()
            .any(|step| step.ev == "Admit" && step.outcome == "registered"),
        "no push was admitted as a durable record"
    );
}

#[nuclei::test]
#[ignore = "needs DEFRA_TRACE_BINARY (defra built with --features iroh,model-trace) and a JDK"]
async fn two_node_push_replays_through_p2p_delivery() {
    edge::compat(async move {
        let cluster = cluster(2).await;
        let (a, b) = (cluster.client(0), cluster.client(1));
        a.schema_add(SCHEMA).expect("schema a");
        b.schema_add(SCHEMA).expect("schema b");
        let addr_b = address(&b);
        a.p2p_connect(&[addr_b.as_str()]).expect("connect a -> b");
        a.p2p_replicator_set(&["User"], &addr_b)
            .expect("replicator a -> b");

        let doc = create(&a, 1);
        wait_for_age(&b, &doc, 1).await;
        update(&a, &doc, 2);
        wait_for_age(&b, &doc, 2).await;

        let trace = replay(
            &cluster,
            &["a", "b"],
            &[("a", "b")],
            &doc,
            "TwoNode",
            "Two nodes: a replicates to b. a creates a document (h1) and, once b\n\
             has it, updates it (h2). Each head is committed, registered, pushed,\n\
             admitted as a durable record, dispatched and merged on b.",
        )
        .await;
        assert_delivered_through_the_pool(&trace, 1);
    })
    .await
}

#[nuclei::test]
#[ignore = "needs DEFRA_TRACE_BINARY (defra built with --features iroh,model-trace) and a JDK"]
async fn hub_relay_replays_through_p2p_delivery() {
    edge::compat(async move {
        let cluster = cluster(3).await;
        let (a, h, b) = (cluster.client(0), cluster.client(1), cluster.client(2));
        for node in [&a, &h, &b] {
            node.schema_add(SCHEMA).expect("schema");
        }
        let (addr_h, addr_b) = (address(&h), address(&b));
        a.p2p_connect(&[addr_h.as_str()]).expect("connect a -> h");
        h.p2p_connect(&[addr_b.as_str()]).expect("connect h -> b");
        a.p2p_replicator_set(&["User"], &addr_h)
            .expect("replicator a -> h");
        h.p2p_replicator_set(&["User"], &addr_b)
            .expect("replicator h -> b");

        let doc = create(&a, 1);
        wait_for_age(&b, &doc, 1).await;
        update(&a, &doc, 2);
        wait_for_age(&b, &doc, 2).await;

        let trace = replay(
            &cluster,
            &["a", "h", "b"],
            &[("a", "h"), ("h", "b")],
            &doc,
            "HubRelay",
            "Three nodes: a replicates to hub h, and h to b. a creates a document\n\
             (h1) and, once b has it, updates it (h2). h merges each head, and its\n\
             merge leaves the forward intent that becomes h's marker for b.",
        )
        .await;
        assert_delivered_through_the_pool(&trace, 2);
        assert!(
            trace
                .steps
                .iter()
                .any(|step| step.ev == "Register" && step.node == "h" && step.peer == "b"),
            "the hub registered no forward"
        );
    })
    .await
}
