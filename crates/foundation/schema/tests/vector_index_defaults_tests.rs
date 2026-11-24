//! Go's defaults for a vector index description that leaves parts unset.

use schema::{DistanceMetric, HnswParams, VectorAlgorithm, VectorIndexDescription};

/// Go's index API writes an unset algorithm and metric as `""`, meaning the
/// defaults; an unknown name is still refused.
#[test]
fn an_empty_algorithm_or_metric_means_the_default() {
    let vector: VectorIndexDescription = serde_json::from_str(
        r#"{"Fields":["v"],"Algorithm":"","Metric":"","Dimensions":3,"HNSW":{"M":0,"EfConstruction":0,"EfSearch":0}}"#,
    )
    .unwrap();
    assert_eq!(vector.algorithm, VectorAlgorithm::Hnsw);
    assert_eq!(vector.metric, DistanceMetric::Cosine);

    let named: VectorIndexDescription =
        serde_json::from_str(r#"{"Algorithm":"FLAT","Metric":"DOT","Dimensions":3}"#).unwrap();
    assert_eq!(named.algorithm, VectorAlgorithm::Flat);
    assert_eq!(named.metric, DistanceMetric::Dot);

    assert!(serde_json::from_str::<VectorIndexDescription>(r#"{"Metric":"MANHATTAN"}"#).is_err());
}

/// A zero HNSW parameter, and a missing HNSW block, mean Go's defaults.
#[test]
fn zero_hnsw_parameters_take_the_defaults() {
    let mut vector =
        VectorIndexDescription::with_defaults(VectorAlgorithm::Hnsw, DistanceMetric::Cosine, 3);
    vector.hnsw = Some(HnswParams {
        m: 0,
        ef_construction: 0,
        ef_search: 200,
    });
    assert_eq!(
        vector.normalized().hnsw,
        Some(HnswParams {
            m: HnswParams::default().m,
            ef_construction: HnswParams::default().ef_construction,
            ef_search: 200,
        })
    );

    vector.hnsw = None;
    assert_eq!(vector.normalized().hnsw, Some(HnswParams::default()));
}
