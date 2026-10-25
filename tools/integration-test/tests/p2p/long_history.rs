use std::time::{Duration, Instant};

use integration_test::TestCluster;

#[tokio::test]
async fn rust_rust_long_history_catchup() {
    const UPDATES: usize = 256;
    let cluster = TestCluster::builder()
        .rust_nodes(2)
        .with_p2p()
        .with_extra_rust_args(["--max-merge-depth", "64"])
        .build()
        .await
        .expect("start peers");
    for node in 0..2 {
        cluster
            .wait_for_log(node, "p2p_listening", Duration::from_secs(30))
            .await
            .expect("P2P listener");
    }
    let source = cluster.client(0);
    let target = cluster.client(1);
    for client in [&source, &target] {
        client
            .schema_add("type LongHistory { revision: Int }")
            .unwrap();
    }
    let created = source
        .query("mutation { add_LongHistory(input: {revision: 0}) { _docID } }")
        .unwrap();
    let doc_id = created["add_LongHistory"][0]["_docID"].as_str().unwrap();
    // Batch requests, not history: every aliased mutation creates its own revision.
    // A low traversal budget exercises resumption without the separate CAR-size limit.
    for start in (1..=UPDATES).step_by(100) {
        let mut mutation = String::from("mutation {");
        let end = (start + 100).min(UPDATES + 1);
        for revision in start..end {
            use std::fmt::Write;
            write!(&mut mutation,
                "r{revision}: update_LongHistory(docID: \"{doc_id}\", input: {{revision: {revision}}}) {{ _docID }} "
            ).unwrap();
        }
        mutation.push('}');
        let response = source.query(&mutation).expect("build history");
        assert_eq!(response.as_object().unwrap().len(), end - start);
    }

    let info = target.p2p_info().unwrap();
    let address = info.as_array().unwrap()[0].as_str().unwrap();
    source.p2p_connect(&[address]).unwrap();
    source
        .p2p_replicator_set(&["LongHistory"], address)
        .unwrap();
    source.query(&format!(
            "mutation {{ update_LongHistory(docID: \"{doc_id}\", input: {{revision: {}}}) {{ _docID }} }}", UPDATES + 1
    )).unwrap();

    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let result = target
            .query("query { LongHistory { _docID revision } }")
            .unwrap();
        if result["LongHistory"]
            .as_array()
            .unwrap()
            .iter()
            .any(|document| {
                document["_docID"].as_str() == Some(doc_id)
                    && document["revision"].as_i64() == Some((UPDATES + 1) as i64)
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "long history did not converge: {result}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
