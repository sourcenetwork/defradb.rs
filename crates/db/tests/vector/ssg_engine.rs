//! The SSG engine: building from HNSW layer 0, and searching one flat layer.

use db::index::vector::engine::ann::EngineKind;
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::flat::Flat;
use db::index::vector::engine::ivf::pq::TRAIN_PER_LIST;
use db::index::vector::engine::ssg::Ssg;
use db::index::vector::engine::ssg::SsgParams;
use db::index::vector::params::Params;
use db::index::vector::params::DEFAULT_M;
use db::index::vector::store::MemoryNodeStore;
use db::index::vector::store::NodeId;
use defra_core::vector::Metric;
use rapidhash::{HashSetExt, RapidHashSet};

const SEED: u64 = 0x0559_6EED;
const DIMENSIONS: usize = 16;

#[test]
fn saturated_backlinks_preserve_reachability() {
    workers::block_on(async {
        for initial in [2, 32] {
            let mut graph = index(SsgParams {
                r: 1,
                ..SsgParams::default()
            });
            let vector = [1.0f32; DIMENSIONS];
            for id in 0..initial {
                graph.insert(NodeId(id), &vector).await.unwrap();
            }
            graph.build().await.unwrap();
            for count in initial..initial + 12 {
                graph.insert(NodeId(count), &vector).await.unwrap();
                let found = graph
                    .search(&vector, count as usize + 1, None)
                    .await
                    .unwrap();
                assert_eq!(
                    found.len(),
                    count as usize + 1,
                    "initial={initial}, inserted={count}"
                );
                for id in 0..=count {
                    let edges = graph.neighbours(NodeId(id)).await.unwrap();
                    assert!(edges.len() <= 1);
                    assert!(!edges.contains(&NodeId(id)));
                }
            }
            for id in 0..initial {
                graph.insert(NodeId(id), &vector).await.unwrap();
            }
            assert_eq!(
                graph
                    .search(&vector, initial as usize + 12, None)
                    .await
                    .unwrap()
                    .len(),
                initial as usize + 12
            );
        }
    })
    .unwrap()
}

fn index(params: SsgParams) -> Ssg<MemoryNodeStore> {
    Ssg::try_new(
        MemoryNodeStore::new(),
        Metric::Cosine,
        Params::new(DEFAULT_M),
        params,
        SEED,
    )
    .expect("valid SSG parameters")
}

async fn filled(params: SsgParams, vectors: &[Vec<f32>]) -> Ssg<MemoryNodeStore> {
    let mut index = index(params);
    for (i, vector) in vectors.iter().enumerate() {
        index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    index
}

async fn exact(vectors: &[Vec<f32>], query: &[f32], k: usize) -> Vec<u64> {
    let mut flat = Flat::new(MemoryNodeStore::new(), Metric::Cosine);
    for (i, vector) in vectors.iter().enumerate() {
        flat.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    flat.search(query, k, None)
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.id.0)
        .collect()
}

#[test]
fn the_kind_is_reported() {
    workers::block_on(async {
        assert_eq!(index(SsgParams::default()).kind(), EngineKind::Ssg);
    })
    .unwrap()
}

#[test]
fn out_of_range_parameters_are_refused() {
    for params in [
        SsgParams {
            r: 0,
            ..SsgParams::default()
        },
        SsgParams {
            pool: 0,
            ..SsgParams::default()
        },
        SsgParams {
            angle: 0,
            ..SsgParams::default()
        },
        // At 180 degrees no two edges can coexist, so every node would keep
        // one and no walk could cross the graph.
        SsgParams {
            angle: 180,
            ..SsgParams::default()
        },
    ] {
        assert!(
            Ssg::try_new(
                MemoryNodeStore::new(),
                Metric::Cosine,
                Params::new(DEFAULT_M),
                params,
                SEED
            )
            .is_err(),
            "{params:?} should be refused"
        );
    }
}

/// Before a build the index answers from the HNSW graph it is built on.
#[test]
fn an_unbuilt_index_answers_from_hnsw() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let index = filled(SsgParams::default(), &vectors).await;

        assert!(!index.is_built().await.unwrap());
        let hits = index.search(vectors[5].as_slice(), 10, None).await.unwrap();
        assert_eq!(hits.len(), 10);
        assert_eq!(hits[0].id, NodeId(6), "a vector is nearest itself");
    })
    .unwrap()
}

#[test]
fn building_marks_the_index_built() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;

        let report = index.build().await.unwrap();
        assert_eq!(report.nodes, 300);
        assert!(report.edges > 0);
        assert!(index.is_built().await.unwrap());
    })
    .unwrap()
}

/// The invariant the pruning exists for: no node keeps more than `r` edges.
#[test]
fn no_node_exceeds_the_degree_cap() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x11);
        let vectors = corpus.clustered(400, DIMENSIONS, 10, 0.2);
        let params = SsgParams {
            r: 8,
            ..SsgParams::default()
        };
        let mut index = filled(params, &vectors).await;
        index.build().await.unwrap();

        for i in 1..=400u64 {
            let degree = index.neighbours(NodeId(i)).await.unwrap().len();
            assert!(degree <= 8, "node {i} kept {degree} edges, above r=8");
        }
    })
    .unwrap()
}

/// A stranded node can never be returned, so the connectivity pass must leave
/// every node reachable from the entry point.
#[test]
fn every_node_is_reachable_from_the_entry_point() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x22);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        let report = index.build().await.unwrap();

        let mut visited: RapidHashSet<NodeId> = RapidHashSet::new();
        let mut stack = vec![report.state.entry_point];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            for neighbour in index.neighbours(id).await.unwrap() {
                if !visited.contains(&neighbour) {
                    stack.push(neighbour);
                }
            }
        }

        assert_eq!(
            visited.len(),
            300,
            "{} of 300 nodes are unreachable ({} were reattached)",
            300 - visited.len(),
            report.reattached
        );
    })
    .unwrap()
}

#[test]
fn a_built_index_finds_the_nearest_neighbours() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x33);
        let vectors = corpus.clustered(600, DIMENSIONS, 12, 0.15);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();

        let mut matched = 0usize;
        let queries = 20;
        for i in 0..queries {
            let query = &vectors[i * 11 % vectors.len()];
            let got: Vec<u64> = index
                .search(query.as_slice(), 10, None)
                .await
                .unwrap()
                .into_iter()
                .map(|n| n.id.0)
                .collect();
            let want = exact(&vectors, query, 10).await;
            matched += got.iter().filter(|id| want.contains(id)).count();
        }

        let recall = matched as f64 / (queries * 10) as f64;
        assert!(recall >= 0.85, "recall@10 was {recall:.4}");
    })
    .unwrap()
}

#[test]
fn a_deleted_document_stops_ranking() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x44);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();

        assert!(index.delete(NodeId(1)).await.unwrap());
        let hits = index.search(vectors[0].as_slice(), 10, None).await.unwrap();
        assert!(!hits.iter().any(|n| n.id == NodeId(1)), "{hits:?}");
    })
    .unwrap()
}

/// `Admit` is trait-level, so a filtered walk must still return a full `k`.
#[test]
fn a_filter_excludes_without_shortening_the_answer() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x55);
        let vectors = corpus.clustered(500, DIMENSIONS, 10, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();

        let admit = |id: NodeId| id.0.is_multiple_of(2);
        let hits = index
            .search_where(vectors[7].as_slice(), 8, None, &admit)
            .await
            .unwrap();

        assert!(
            hits.iter().all(|n| n.id.0.is_multiple_of(2)),
            "a filter leaked"
        );
        assert_eq!(hits.len(), 8, "the filter shortened the answer");
    })
    .unwrap()
}

#[test]
fn results_are_ordered_nearest_first() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x66);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();

        let hits = index.search(vectors[3].as_slice(), 10, None).await.unwrap();
        assert!(hits.windows(2).all(|w| w[0].distance <= w[1].distance));
    })
    .unwrap()
}

/// A wider angle keeps fewer edges, so the graph gets sparser.
#[test]
fn a_wider_angle_builds_a_sparser_graph() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x77);
        let vectors = corpus.clustered(400, DIMENSIONS, 10, 0.2);

        let mut previous = u64::MAX;
        for angle in [20u32, 60, 100] {
            let params = SsgParams {
                angle,
                ..SsgParams::default()
            };
            let mut index = filled(params, &vectors).await;
            let report = index.build().await.unwrap();
            assert!(
                report.edges <= previous,
                "at {angle} degrees kept {} edges after {previous}",
                report.edges
            );
            previous = report.edges;
        }
    })
    .unwrap()
}

#[test]
fn an_empty_index_builds_nothing() {
    workers::block_on(async {
        let mut index = index(SsgParams::default());
        assert!(index.build().await.is_err());
        assert!(!index.is_built().await.unwrap());
    })
    .unwrap()
}

#[test]
fn k_zero_returns_nothing() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x88);
        let vectors = corpus.clustered(200, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();
        assert!(index
            .search(vectors[0].as_slice(), 0, None)
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}

/// A document written after a build must be searchable, or the index silently
/// stops covering new writes until someone rebuilds it.
#[test]
fn a_document_written_after_the_build_is_searchable() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0x99);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();

        let late = vectors[3].clone();
        index.insert(NodeId(9_999), &late).await.unwrap();

        let hits = index.search(late.as_slice(), 10, None).await.unwrap();
        assert!(
            hits.iter().any(|n| n.id == NodeId(9_999)),
            "the late document was not found: {hits:?}"
        );
    })
    .unwrap()
}

#[test]
fn should_build_is_false_on_an_empty_index() {
    workers::block_on(async {
        let index = index(SsgParams::default());
        assert!(!index.should_build().await.unwrap());
    })
    .unwrap()
}

#[test]
fn should_build_is_false_once_built() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0xAA);
        let vectors = corpus.clustered(300, DIMENSIONS, 8, 0.2);
        let mut index = filled(SsgParams::default(), &vectors).await;
        index.build().await.unwrap();

        assert!(!index.should_build().await.unwrap());
    })
    .unwrap()
}

/// The regression #1463 closes, mirrored from the IVF-PQ engine: crossing the
/// threshold must make `should_build` answer `true`.
#[test]
fn should_build_becomes_true_once_the_threshold_is_crossed() {
    workers::block_on(async {
        let params = SsgParams {
            r: 8,
            ..SsgParams::default()
        };
        let threshold = 8u64 * u64::from(TRAIN_PER_LIST);
        let mut corpus = crate::support::Corpus::new(SEED ^ 0xBB);
        let vectors = corpus.clustered(threshold as usize, DIMENSIONS, 8, 0.2);
        let mut index = index(params);

        for (i, vector) in vectors.iter().take(threshold as usize - 1).enumerate() {
            index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
        }
        assert!(
            !index.should_build().await.unwrap(),
            "one short of the threshold"
        );

        index
            .insert(NodeId(threshold), &vectors[threshold as usize - 1])
            .await
            .unwrap();
        assert!(index.should_build().await.unwrap(), "at the threshold");
    })
    .unwrap()
}

/// `live_count_at_least` must answer exactly what a full count would, at
/// targets on both sides of the true count.
#[test]
fn live_count_at_least_agrees_with_live_count() {
    workers::block_on(async {
        let mut corpus = crate::support::Corpus::new(SEED ^ 0xCC);
        let vectors = corpus.clustered(150, DIMENSIONS, 8, 0.2);
        let index = filled(SsgParams::default(), &vectors).await;

        let true_count = index.live_count().await.unwrap();
        assert_eq!(true_count, 150);

        for target in [0u64, 1, 50, 149, 150, 151, 500] {
            assert_eq!(
                index.live_count_at_least(target).await.unwrap(),
                true_count >= target,
                "target={target}, true_count={true_count}"
            );
        }
    })
    .unwrap()
}
