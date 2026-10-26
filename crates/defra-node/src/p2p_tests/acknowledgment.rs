use super::*;

async fn wait_for_acknowledged(
    sender: &EmbeddedNode,
    peer_id: &str,
    collection: &str,
    doc_id: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let acknowledged = sender
            .p2p()
            .expect("sender p2p")
            .replication_acknowledged(peer_id, collection, &[doc_id.to_string()])
            .await
            .expect("replication_acknowledged");
        if acknowledged == [true] {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{collection} {doc_id} was never acknowledged by {peer_id}"
        );
        edge::testing::edge()
            .sleep(Duration::from_millis(100))
            .await;
    }
}

async fn assert_replicator_acknowledges_current_heads(schema: &str, collection: &str) {
    let node0 = EmbeddedNode::builder()
        .with_p2p(test_p2p_config())
        .build()
        .await
        .expect("build node0");
    let node1 = EmbeddedNode::builder()
        .with_p2p(test_p2p_config())
        .build()
        .await
        .expect("build node1");
    node0.add_collection(schema).await.expect("schema on node0");
    node1.add_collection(schema).await.expect("schema on node1");
    install_one_way_replicator(&node0, &node1, &[collection]).await;
    let peer1 = node1
        .p2p()
        .expect("node1 p2p")
        .local_peer_id()
        .await
        .expect("node1 peer id");

    let response = node0
        .execute(&format!(
            r#"mutation {{ add_{collection}(input: {{body: "first"}}) {{ _docID }} }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let doc_id = response.data.expect("data")[format!("add_{collection}")][0]["_docID"]
        .as_str()
        .expect("doc id")
        .to_string();
    wait_for_acknowledged(&node0, &peer1, collection, &doc_id).await;

    let unknown = node0
        .p2p()
        .expect("node0 p2p")
        .replication_acknowledged(
            "not-a-replicator",
            collection,
            std::slice::from_ref(&doc_id),
        )
        .await
        .expect("query unknown peer");
    assert_eq!(unknown, [false]);

    let response = node0
        .execute(&format!(
            r#"mutation {{ update_{collection}(docID: "{doc_id}", input: {{body: "second"}}) {{ _docID }} }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    wait_for_acknowledged(&node0, &peer1, collection, &doc_id).await;
    // PushLog acknowledges durable acceptance before the receiver merges the DAG.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let on_peer = node1
            .execute(&format!(
                r#"{{ {collection}(docID: "{doc_id}") {{ body }} }}"#
            ))
            .await;
        assert!(on_peer.errors.is_empty(), "{:?}", on_peer.errors);
        let data = on_peer.data.expect("data");
        if data[collection][0]["body"] == "second" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "acknowledged heads were never materialized: {data:?}"
        );
        edge::testing::edge()
            .sleep(Duration::from_millis(100))
            .await;
    }

    node0.shutdown().await;
    node1.shutdown().await;
}

#[test]
fn replicator_acknowledges_current_document_heads() {
    edge::testing::edge()
        .block_on(async {
            init_tracing();
            assert_replicator_acknowledges_current_heads("type Note { body: String }", "Note")
                .await;
        })
        .unwrap()
}

#[test]
fn filtered_replicator_acknowledges_branchable_document_heads() {
    edge::testing::edge()
        .block_on(async {
            init_tracing();
            let schema = "type AgentDoc @branchable { agent_did: String @immutable  body: String }";
            let node0 = EmbeddedNode::builder()
                .with_p2p(test_p2p_config())
                .build()
                .await
                .expect("build node0");
            let node1 = EmbeddedNode::builder()
                .with_p2p(test_p2p_config())
                .build()
                .await
                .expect("build node1");
            node0.add_collection(schema).await.expect("schema on node0");
            node1.add_collection(schema).await.expect("schema on node1");
            install_filtered_one_way_replicator(
                &node0,
                &node1,
                &["AgentDoc"],
                agent_did_in_filter(&["did:key:alice"]),
            )
            .await;
            let peer1 = node1
                .p2p()
                .expect("node1 p2p")
                .local_peer_id()
                .await
                .expect("node1 peer id");

            let mut doc_ids = Vec::new();
            for agent_did in ["did:key:alice", "did:key:bob"] {
                let response = node0
                    .execute(&format!(
                        r#"mutation {{ add_AgentDoc(input: {{agent_did: "{agent_did}", body: "first"}}) {{ _docID }} }}"#
                    ))
                    .await;
                assert!(response.errors.is_empty(), "{:?}", response.errors);
                doc_ids.push(
                    response.data.expect("data")["add_AgentDoc"][0]["_docID"]
                        .as_str()
                        .expect("doc id")
                        .to_string(),
                );
            }
            wait_for_acknowledged(&node0, &peer1, "AgentDoc", &doc_ids[0]).await;
            let filtered_out = node0
                .p2p()
                .expect("node0 p2p")
                .replication_acknowledged(&peer1, "AgentDoc", std::slice::from_ref(&doc_ids[1]))
                .await
                .expect("query filtered-out document");
            assert_eq!(
                filtered_out,
                [false],
                "the peer never received this document"
            );

            node0.shutdown().await;
            node1.shutdown().await;
        })
        .unwrap()
}

#[test]
fn replicator_acknowledges_current_branchable_collection_heads() {
    edge::testing::edge()
        .block_on(async {
            init_tracing();
            assert_replicator_acknowledges_current_heads(
                "type Note @branchable { body: String }",
                "Note",
            )
            .await;
        })
        .unwrap()
}

#[test]
fn explicit_replay_acknowledges_only_selected_document_heads() {
    edge::testing::edge()
        .block_on(async {
            init_tracing();
            let node0 = EmbeddedNode::builder()
                .with_p2p(test_p2p_config())
                .build()
                .await
                .unwrap();
            let node1 = EmbeddedNode::builder()
                .with_p2p(test_p2p_config())
                .build()
                .await
                .unwrap();
            for node in [&node0, &node1] {
                node.add_collection(
                    "type Control { value: String } type Note @branchable { body: String }",
                )
                .await
                .unwrap();
            }
            let sender = node0.p2p().unwrap();
            let receiver = node1.p2p().unwrap();
            let sender_addr = wait_for_listen_addr(&node0).await;
            let receiver_addr = wait_for_listen_addr(&node1).await;
            sender.connect_peer(&receiver_addr).await.unwrap();
            wait_for_connected_peer(&node0).await;
            wait_for_connected_peer(&node1).await;
            receiver
                .add_replicator(
                    vec!["Control".into()],
                    Some(&sender_addr),
                    Default::default(),
                    Vec::new(),
                    None,
                )
                .await
                .unwrap();
            sender
                .add_replicator(
                    vec!["Control".into()],
                    Some(&receiver_addr),
                    Default::default(),
                    Vec::new(),
                    None,
                )
                .await
                .unwrap();
            let peer = receiver.local_peer_id().await.unwrap();
            let mut ids = Vec::new();
            for body in ["selected", "unselected"] {
                let response = node0
                    .execute(&format!(
                        r#"mutation {{ add_Note(input: {{body: "{body}"}}) {{ _docID }} }}"#
                    ))
                    .await;
                assert!(response.errors.is_empty(), "{:?}", response.errors);
                ids.push(
                    response.data.unwrap()["add_Note"][0]["_docID"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                );
            }
            assert_eq!(
                sender
                    .document_heads_acknowledged(&peer, "Note", &ids)
                    .await
                    .unwrap(),
                [false, false]
            );
            sender
                .push_documents_to_peer(
                    &peer,
                    vec![defra_p2p_adapter::P2pDocumentRequest {
                        collection: "Note".into(),
                        doc_id: ids[0].clone(),
                    }],
                )
                .await
                .unwrap();
            assert_eq!(
                sender
                    .document_heads_acknowledged(&peer, "Note", &ids)
                    .await
                    .unwrap(),
                [true, false]
            );
            assert_eq!(
                sender
                    .replication_acknowledged(&peer, "Note", &ids)
                    .await
                    .unwrap(),
                [false, false]
            );
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let response = node1.execute("{ Note { _docID body } }").await;
                assert!(response.errors.is_empty(), "{:?}", response.errors);
                let data = response.data.unwrap();
                let rows = data["Note"].as_array().unwrap();
                if !rows.is_empty() {
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0]["_docID"], ids[0]);
                    assert_eq!(rows[0]["body"], "selected");
                    break;
                }
                assert!(Instant::now() < deadline, "selected document never merged");
                edge::testing::edge().sleep(Duration::from_millis(50)).await;
            }
            let response = node0
        .execute(&format!(
            r#"mutation {{ update_Note(docID: "{}", input: {{body: "changed"}}) {{ _docID }} }}"#,
            ids[0]
        ))
        .await;
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            assert_eq!(
                sender
                    .document_heads_acknowledged(&peer, "Note", &ids)
                    .await
                    .unwrap(),
                [false, false]
            );
            sender
                .push_documents_to_peer(
                    &peer,
                    vec![defra_p2p_adapter::P2pDocumentRequest {
                        collection: "Note".into(),
                        doc_id: ids[0].clone(),
                    }],
                )
                .await
                .unwrap();
            assert_eq!(
                sender
                    .document_heads_acknowledged(&peer, "Note", &ids)
                    .await
                    .unwrap(),
                [true, false]
            );
            assert_eq!(
                sender
                    .document_heads_acknowledged("unknown-peer", "Note", &ids)
                    .await
                    .unwrap(),
                [false, false]
            );
            assert_eq!(
                sender
                    .document_heads_acknowledged(&peer, "Note", &["missing".into()])
                    .await
                    .unwrap(),
                [false]
            );
            assert!(sender.get_collections().await.unwrap().is_empty());
            assert!(receiver.get_collections().await.unwrap().is_empty());
            node0.shutdown().await;
            node1.shutdown().await;
        })
        .unwrap()
}
