//! HNSW nodes and metadata: link additions and counts.

use regolith::MergeOperator;

use crate::merge::{vector, DefraMergeOperator};
use crate::vector::{decode_meta, decode_node, encode_meta, encode_node, Meta, Node, NodeId};

#[test]
fn links_preserve_structure_and_counts_preserve_routing() {
    let node = Node::new(NodeId(7), vec![0.25, 0.75], 1);
    let base = encode_node(&node);
    let a = vector::links(0, 4);
    let b = vector::links(0, 9);
    let self_link = vector::links(0, 7);
    let result = DefraMergeOperator
        .full_merge(b"node", Some(&base), &[&a, &b, &a, &self_link])
        .unwrap();
    let stored = decode_node(&result).unwrap();
    assert_eq!(stored.vector, node.vector);
    assert_eq!(stored.layers, vec![vec![NodeId(4), NodeId(9)], vec![]]);
    assert_eq!(stored.id, node.id);
    assert!(!stored.deleted);
    assert!(!DefraMergeOperator.touches(b"node", &a, &[0]));
    assert!(DefraMergeOperator.touches(b"node", &a, &[1]));
    let meta = Meta {
        entry_point: NodeId(7),
        top_layer: 1,
        live: 2,
        waste: 0,
        rebuilds: 3,
    };
    let added = vector::counts(5, 2);
    let removed = vector::counts(-1, 1);
    let result = DefraMergeOperator
        .full_merge(b"meta", Some(&encode_meta(&meta)), &[&added, &removed])
        .unwrap();
    assert_eq!(
        decode_meta(&result).unwrap(),
        Meta {
            live: 6,
            waste: 3,
            ..meta
        }
    );
    assert!(DefraMergeOperator
        .partial_merge(b"meta", &added, &removed)
        .is_none());
}

#[test]
fn malformed_vector_operands_fail_without_allocating_from_unchecked_lengths() {
    let base = encode_node(&Node::new(NodeId(0), vec![1.0], 0));
    let valid = vector::links(0, 1);
    for end in 0..valid.len() {
        assert!(DefraMergeOperator
            .full_merge(b"node", Some(&base), &[&valid[..end]])
            .is_none());
    }
    assert!(DefraMergeOperator
        .full_merge(b"node", Some(&base), &[&vector::links(u32::MAX, 1)])
        .is_none());
    assert!(DefraMergeOperator
        .full_merge(b"node", None, &[&valid])
        .is_none());
    let mut corrupt = base.clone();
    corrupt.extend_from_slice(b"extra");
    assert!(decode_node(&corrupt).is_err());
}

#[test]
fn vector_operator_folding_and_touches_obey_engine_contracts() {
    use regolith::testing::proptest::prelude::*;
    let operands = (0u32..4, any::<u64>()).prop_map(|(layer, id)| vector::links(layer, id));
    let bases = any::<u64>().prop_map(|id| encode_node(&Node::new(NodeId(id), vec![1.0, 0.0], 3)));
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        Just(b"node".to_vec()),
        bases,
        operands.clone(),
    )
    .unwrap();
    assert!(checked.folds > 0);
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"node".to_vec()),
        operands,
        regolith::testing::proptest::collection::vec(0u32..3, 0..4),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

#[test]
fn encrypted_links_reopen_and_structural_put_terminates_operands() {
    workers::block_on(async {
        use crate::corekv::Store;
        let dir = tempfile::tempdir().unwrap();
        let options = crate::RegolithStoreOptions::default().with_encryption_key([29; 32]);
        {
            let store =
                crate::RegolithStore::open_with_options(dir.path(), options.clone()).unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.set(b"node", &encode_node(&Node::new(NodeId(0), vec![1.0], 0)))
                .await
                .unwrap();
            seed.commit().await.unwrap();
            let mut update = store.new_txn(false).await.unwrap();
            update.merge(b"node", &vector::links(0, 1)).await.unwrap();
            update.commit().await.unwrap();
            store.close().await.unwrap();
        }
        let store = crate::RegolithStore::open_with_options(dir.path(), options).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        assert_eq!(
            decode_node(&txn.get(b"node").await.unwrap().unwrap())
                .unwrap()
                .layers[0],
            vec![NodeId(1)]
        );
        txn.merge(b"node", &vector::links(0, 2)).await.unwrap();
        let mut replacement = Node::new(NodeId(0), vec![0.0, 1.0], 0);
        replacement.layers[0] = vec![NodeId(3)];
        txn.set(b"node", &encode_node(&replacement)).await.unwrap();
        txn.commit().await.unwrap();
        let read = store.new_txn(true).await.unwrap();
        assert_eq!(
            decode_node(&read.get(b"node").await.unwrap().unwrap()).unwrap(),
            replacement
        );
    })
    .unwrap()
}

/// Counts add to the metadata's bookkeeping and saturate, so they are not
/// associative and never fold early. The routing state (part 0: entry point,
/// top layer, rebuilds) keeps its bytes under any run of them, which is what
/// lets a read of the routing part ignore them.
#[test]
fn counts_obey_the_engine_contract_and_leave_routing_alone() {
    use regolith::testing::proptest::collection::vec;
    use regolith::testing::proptest::prelude::*;
    use regolith::testing::proptest::test_runner::{Config, TestCaseError, TestRunner};

    let metas = (
        any::<u64>(),
        0usize..8,
        any::<u64>(),
        any::<u64>(),
        any::<u32>(),
    )
        .prop_map(|(entry, top_layer, live, waste, rebuilds)| Meta {
            entry_point: NodeId(entry),
            top_layer,
            live,
            waste,
            rebuilds,
        });
    let counts = (any::<i64>(), any::<u64>()).prop_map(|(live, waste)| vector::counts(live, waste));
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        Just(b"meta".to_vec()),
        metas.clone().prop_map(|meta| encode_meta(&meta)),
        counts.clone(),
    )
    .unwrap();
    assert_eq!(checked.folds, 0, "saturating counts never fold early");
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"meta".to_vec()),
        counts.clone(),
        vec(0u32..3, 0..4),
    )
    .unwrap();
    assert_eq!(checked.folds, 0);

    let mut runner = TestRunner::new(Config {
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(&(metas, vec(counts, 1..8)), |(meta, operands)| {
            let refs: Vec<&[u8]> = operands.iter().map(Vec::as_slice).collect();
            let merged = DefraMergeOperator
                .full_merge(b"meta", Some(&encode_meta(&meta)), &refs)
                .ok_or_else(|| TestCaseError::fail("counts onto a meta must merge"))?;
            let merged =
                decode_meta(&merged).map_err(|error| TestCaseError::fail(error.to_string()))?;
            if (merged.entry_point, merged.top_layer, merged.rebuilds)
                != (meta.entry_point, meta.top_layer, meta.rebuilds)
            {
                return Err(TestCaseError::fail(format!(
                    "{merged:?} lost the routing state of {meta:?}"
                )));
            }
            if refs
                .iter()
                .any(|operand| DefraMergeOperator.touches(b"meta", operand, &[0]))
            {
                return Err(TestCaseError::fail("a count touches the routing part"));
            }
            Ok(())
        })
        .unwrap();
}

/// Link additions change adjacency (part 1) only: a node's id, vector, layer
/// count and deletion mark keep their bytes under any run of them.
#[test]
fn links_leave_the_structural_part_alone() {
    use regolith::testing::proptest::collection::vec;
    use regolith::testing::proptest::prelude::*;
    use regolith::testing::proptest::test_runner::{Config, TestCaseError, TestRunner};

    let graph_nodes = (
        any::<u64>(),
        vec(-1000i32..1000, 1..4),
        0usize..4,
        any::<bool>(),
    )
        .prop_map(|(id, coordinates, top_layer, deleted)| {
            let coordinates = coordinates.into_iter().map(|x| x as f32).collect();
            let mut built = Node::new(NodeId(id), coordinates, top_layer);
            built.deleted = deleted;
            built
        });
    let mut runner = TestRunner::new(Config {
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(
            &(graph_nodes, vec((0u32..4, any::<u64>()), 1..8)),
            |(base, additions)| {
                let operands: Vec<Vec<u8>> = additions
                    .iter()
                    .filter(|(layer, _)| (*layer as usize) < base.layers.len())
                    .map(|(layer, neighbor)| vector::links(*layer, *neighbor))
                    .collect();
                let refs: Vec<&[u8]> = operands.iter().map(Vec::as_slice).collect();
                let merged = DefraMergeOperator
                    .full_merge(b"node", Some(&encode_node(&base)), &refs)
                    .ok_or_else(|| TestCaseError::fail("links within its layers must merge"))?;
                let merged =
                    decode_node(&merged).map_err(|error| TestCaseError::fail(error.to_string()))?;
                if (
                    merged.id,
                    &merged.vector,
                    merged.layers.len(),
                    merged.deleted,
                ) != (base.id, &base.vector, base.layers.len(), base.deleted)
                {
                    return Err(TestCaseError::fail(format!(
                        "{merged:?} lost the structure of {base:?}"
                    )));
                }
                if refs
                    .iter()
                    .any(|operand| DefraMergeOperator.touches(b"node", operand, &[0]))
                {
                    return Err(TestCaseError::fail("a link touches the structural part"));
                }
                Ok(())
            },
        )
        .unwrap();
}
