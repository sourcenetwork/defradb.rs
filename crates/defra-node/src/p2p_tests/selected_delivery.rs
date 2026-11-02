use super::*;
use std::collections::BTreeSet;

#[test]
fn selected_delivery_crosses_receiver_capacity_without_losing_the_remainder() {
    edge::testing::edge()
        .block_on(async {
            init_tracing();
            let sender = Node::builder()
                .with_p2p(test_p2p_config())
                .build()
                .await
                .unwrap();
            let receiver = Node::builder()
                .with_p2p(test_p2p_config())
                .build()
                .await
                .unwrap();
            for node in [&sender, &receiver] {
                node.add_collection("type SelectedPushNote { value: Int }")
                    .await
                    .unwrap();
            }
            let count = 1800;
            let inputs = (0..count)
                .map(|value| format!("{{value: {value}}}"))
                .collect::<Vec<_>>()
                .join(",");
            let response = sender
                .execute(&format!(
                    "mutation {{ add_SelectedPushNote(input: [{inputs}]) {{ _docID }} }}"
                ))
                .await;
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            let documents = response.data.as_ref().unwrap()["add_SelectedPushNote"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| defra_p2p_adapter::P2pDocumentRequest {
                    collection: "SelectedPushNote".into(),
                    doc_id: row["_docID"].as_str().unwrap().into(),
                })
                .collect::<Vec<_>>();
            assert_eq!(documents.len(), count);
            let expected: BTreeSet<_> = documents.iter().map(|doc| doc.doc_id.clone()).collect();
            let sender_address = wait_for_listen_addr(&sender).await;
            let receiver_address = wait_for_listen_addr(&receiver).await;
            sender
                .p2p()
                .unwrap()
                .connect_peer(&receiver_address)
                .await
                .unwrap();
            wait_for_connected_peer(&sender).await;
            wait_for_connected_peer(&receiver).await;
            receiver
                .p2p()
                .unwrap()
                .add_replicator(
                    vec!["SelectedPushNote".into()],
                    Some(&sender_address),
                    Default::default(),
                    Vec::new(),
                    None,
                )
                .await
                .unwrap();
            let peer = receiver.p2p().unwrap().local_peer_id().await.unwrap();
            let started = Instant::now();
            let result = sender
                .p2p()
                .unwrap()
                .push_documents_to_peer(&peer, documents)
                .await;
            let acknowledged_at = Instant::now();
            let status = receiver.p2p().unwrap().sync_status().await.unwrap();
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis(),
                pending_high_water = ?status["pending_dag_high_water"],
                capacity_shed = ?status["pending_dag_capacity_shed"],
                "selected capacity result"
            );
            assert!(
                result.is_ok(),
                "selected transfer must resume after a receiver pause: {result:?}"
            );
            // A PushLog ACK admits receiver-owned work; it does not certify a
            // synchronous merge. This healthy loopback fixture must keep making
            // fresh progress and finish within the existing recovery window.
            let recovery_deadline = acknowledged_at
                + Duration::from_secs(p2p::sync::PENDING_RECOVERY_WORST_CASE_SECS);
            let clock = edge::testing::edge();
            let read = || async {
                let rows = receiver
                    .execute(&format!(
                        "{{ SelectedPushNote(limit: {}) {{ _docID }} }}",
                        count + 1
                    ))
                    .await;
                assert!(rows.errors.is_empty(), "{:?}", rows.errors);
                let actual: BTreeSet<_> = rows.data.as_ref().unwrap()["SelectedPushNote"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row["_docID"].as_str().unwrap().to_owned())
                    .collect();
                assert!(actual.is_subset(&expected), "unexpected merged document IDs");
                actual
            };
            let completed = clock
                .timeout(recovery_deadline.saturating_duration_since(Instant::now()), async {
                    let mut seen = BTreeSet::new();
                    loop {
                        let progress = clock
                            .timeout(Duration::from_secs(5), async {
                                loop {
                                    let actual = read().await;
                                    if actual == expected
                                        || actual.difference(&seen).next().is_some()
                                    {
                                        break actual;
                                    }
                                    clock.sleep(Duration::from_millis(20)).await;
                                }
                            })
                            .await;
                        let actual = match progress {
                            Ok(actual) => actual,
                            Err(_) => {
                                let actual = read().await;
                                if actual != expected
                                    && actual.difference(&seen).next().is_none()
                                {
                                    let status = receiver.p2p().unwrap().sync_status().await.unwrap();
                                    panic!("receiver made no fresh merge progress for five seconds: expected={count} actual={} previously_seen={} status={status}", actual.len(), seen.len());
                                }
                                actual
                            }
                        };
                        if actual == expected {
                            break;
                        }
                        seen.extend(actual);
                    }
                })
                .await;
            if completed.is_err() {
                let actual = read().await;
                if actual != expected {
                    let status = receiver.p2p().unwrap().sync_status().await.unwrap();
                    panic!("selected transfer did not converge within the native recovery window: expected={count} actual={} status={status}", actual.len());
                }
            }
            sender.shutdown().await.unwrap();
            receiver.shutdown().await.unwrap();
        })
        .unwrap();
}
