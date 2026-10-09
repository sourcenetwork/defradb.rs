//! Edge selection: the diversity heuristic, SSG's angular pruning, and
//! Vamana's alpha pruning.

use db::index::vector::engine::ann::Candidate;
use db::index::vector::engine::select::Angular;
use db::index::vector::engine::select::EdgeSelector;
use db::index::vector::engine::select::Heuristic;
use db::index::vector::engine::select::RobustPrune;
use db::index::vector::engine::select::DEFAULT_ANGLE_DEGREES;
use db::index::vector::store::NodeId;
use defra_core::vector::Metric;
use defra_core::vector::{dot, squared_euclidean};
use std::sync::Arc;

fn candidate(id: u64, vector: &[f32], base: &[f32]) -> Candidate {
    Candidate {
        id: NodeId(id),
        distance: Metric::Cosine.distance(base, vector),
        vector: Arc::from(vector.to_vec().into_boxed_slice()),
    }
}

/// The angle between `c - base` and `k - base`, computed the obvious way.
fn angle_between(c: &[f32], k: &[f32], base: &[f32]) -> f64 {
    let u: Vec<f32> = c.iter().zip(base).map(|(a, b)| a - b).collect();
    let v: Vec<f32> = k.iter().zip(base).map(|(a, b)| a - b).collect();
    let cos = dot(&u, &v) / (dot(&u, &u).sqrt() * dot(&v, &v).sqrt());
    cos.clamp(-1.0, 1.0).acos().to_degrees()
}

#[test]
fn both_selectors_respect_the_cap() {
    let base = [0.0f32, 0.0];
    let mut corpus = crate::support::Corpus::new(0xED6E);
    let vectors = corpus.vectors(40, 2);
    let candidates: Vec<Candidate> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, v, &base))
        .collect();

    for max in [1usize, 3, 8] {
        assert!(
            Heuristic
                .select(Metric::Cosine, &base, &candidates, max)
                .len()
                <= max
        );
        assert!(
            Angular::default()
                .select(Metric::Cosine, &base, &candidates, max)
                .len()
                <= max
        );
    }
}

/// The invariant SSG is built on: every pair of kept edges is at least `angle`
/// apart.
#[test]
fn angular_pruning_holds_its_angle() {
    let base = [0.0f32, 0.0];
    let mut corpus = crate::support::Corpus::new(0x5563);
    let vectors = corpus.vectors(200, 2);
    let candidates: Vec<Candidate> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, v, &base))
        .collect();

    let selected = Angular::default().select(Metric::Cosine, &base, &candidates, 16);
    assert!(selected.len() > 1, "nothing was kept to compare");

    for (i, a) in selected.iter().enumerate() {
        for b in selected.iter().skip(i + 1) {
            let angle = angle_between(&a.vector, &b.vector, &base);
            assert!(
                angle >= DEFAULT_ANGLE_DEGREES as f64 - 1e-6,
                "kept two edges {angle} degrees apart, below {DEFAULT_ANGLE_DEGREES}"
            );
        }
    }
}

/// Four points on the axes are 90 degrees apart around the origin, so all four
/// survive; a fifth crowding one of them does not.
#[test]
fn a_crowding_edge_is_dropped() {
    let base = [0.0f32, 0.0];
    let spread = [[1.0f32, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]];
    let mut candidates: Vec<Candidate> = spread
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, v, &base))
        .collect();

    let kept = Angular::default().select(Metric::Cosine, &base, &candidates, 8);
    assert_eq!(kept.len(), 4, "orthogonal edges must all survive");

    // Two degrees off the first axis: far inside the 60 degree threshold.
    let crowder = [0.9994f32, 0.0349];
    candidates.push(candidate(5, &crowder, &base));
    let kept = Angular::default().select(Metric::Cosine, &base, &candidates, 8);
    assert!(
        !kept.iter().any(|c| c.id == NodeId(5)),
        "an edge 2 degrees from another was kept"
    );
}

/// A wider angle can only keep fewer edges.
#[test]
fn a_wider_angle_keeps_fewer_edges() {
    let base = [0.0f32, 0.0];
    let mut corpus = crate::support::Corpus::new(0xA9613);
    let vectors = corpus.vectors(300, 2);
    let candidates: Vec<Candidate> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, v, &base))
        .collect();

    let mut previous = usize::MAX;
    for angle in [10.0f32, 30.0, 60.0, 90.0, 120.0] {
        let kept = Angular::new(angle).select(Metric::Cosine, &base, &candidates, 64);
        assert!(
            kept.len() <= previous,
            "at {angle} degrees kept {} after {previous}",
            kept.len()
        );
        previous = kept.len();
    }
}

/// The expanded inner products must agree with materialising `c - base`, or the
/// optimization changed the answer.
#[test]
fn the_expanded_form_matches_the_naive_one() {
    let mut corpus = crate::support::Corpus::new(0x9A17E);
    let base = corpus.vector(32);
    let vectors = corpus.vectors(60, 32);

    for pair in vectors.chunks(2).filter(|c| c.len() == 2) {
        let (c, k) = (&pair[0], &pair[1]);

        let u: Vec<f32> = c.iter().zip(&base).map(|(a, b)| a - b).collect();
        let v: Vec<f32> = k.iter().zip(&base).map(|(a, b)| a - b).collect();
        let naive = dot(&u, &v);

        let bb = dot(&base, &base);
        let expanded = dot(c, k) - dot(c, &base) - dot(k, &base) + bb;

        assert!(
            (naive - expanded).abs() <= 1e-4 * naive.abs().max(1.0),
            "expanded {expanded} against naive {naive}"
        );
    }
}

/// Nearest-first is what both strategies assume, so the first kept edge is
/// always the nearest candidate.
#[test]
fn the_nearest_candidate_is_always_kept() {
    let base = [0.0f32, 0.0];
    let mut corpus = crate::support::Corpus::new(0x11157);
    let vectors = corpus.vectors(50, 2);
    let candidates: Vec<Candidate> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, v, &base))
        .collect();
    let nearest = candidates
        .iter()
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
        .unwrap()
        .id;

    for kept in [
        Heuristic.select(Metric::Cosine, &base, &candidates, 8),
        Angular::default().select(Metric::Cosine, &base, &candidates, 8),
    ] {
        assert_eq!(kept[0].id, nearest);
    }
}

#[test]
fn an_empty_candidate_set_selects_nothing() {
    let base = [1.0f32, 0.0];
    assert!(Heuristic.select(Metric::Cosine, &base, &[], 8).is_empty());
    assert!(Angular::default()
        .select(Metric::Cosine, &base, &[], 8)
        .is_empty());
}

/// A candidate sitting exactly on the base has no direction from it, so there
/// is no angle to measure and it cannot be used to prune others.
#[test]
fn a_candidate_on_the_base_is_skipped() {
    let base = [1.0f32, 2.0];
    let candidates = vec![
        candidate(1, &base, &base),
        candidate(2, &[5.0, 2.0], &base),
        candidate(3, &[1.0, 9.0], &base),
    ];
    let kept = Angular::default().select(Metric::Cosine, &base, &candidates, 8);
    assert!(!kept.iter().any(|c| c.id == NodeId(1)));
    assert_eq!(kept.len(), 2);
}

fn unit(vector: &[f32]) -> Vec<f32> {
    Metric::Cosine.prepare(vector)
}

/// With no slack, Vamana's pruning is HNSW's diversity heuristic: both keep a
/// candidate only when it is nearer the base than to every edge already kept.
#[test]
fn robust_prune_without_slack_is_the_heuristic() {
    let mut corpus = crate::support::Corpus::new(0xA1FA);
    let base = unit(&corpus.vectors(1, 8)[0]);
    let candidates: Vec<Candidate> = corpus
        .vectors(120, 8)
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, &unit(v), &base))
        .collect();

    for max in [4usize, 16, 64] {
        let ids = |kept: Vec<Candidate>| kept.into_iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(
            ids(RobustPrune::new(1.0).select(Metric::Cosine, &base, &candidates, max)),
            ids(Heuristic.select(Metric::Cosine, &base, &candidates, max)),
        );
    }
}

/// The alpha-RNG property FreshVamana's recall stability rests on: a kept edge
/// `c` is never `alpha` times closer to an earlier kept edge than to the base.
#[test]
fn robust_prune_holds_the_alpha_rng_property() {
    let mut corpus = crate::support::Corpus::new(0x5A1F);
    let base = unit(&corpus.vectors(1, 16)[0]);
    let candidates: Vec<Candidate> = corpus
        .vectors(300, 16)
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, &unit(v), &base))
        .collect();

    let alpha = 1.2;
    let kept = RobustPrune::new(alpha).select(Metric::Cosine, &base, &candidates, 32);
    assert!(kept.len() > 1, "nothing was kept to compare");
    assert!(kept.len() <= 32);
    for (i, c) in kept.iter().enumerate() {
        for k in &kept[..i] {
            let to_kept = squared_euclidean(&k.vector, &c.vector).sqrt();
            let to_base = squared_euclidean(&base, &c.vector).sqrt();
            assert!(
                alpha * to_kept > to_base,
                "kept {:?} although {:?} covers it",
                c.id,
                k.id
            );
        }
    }
}

/// Slack is what keeps a FreshVamana graph dense enough to survive updates:
/// on the same candidates it never keeps fewer edges than no slack does.
#[test]
fn robust_prune_slack_keeps_more_edges() {
    let mut corpus = crate::support::Corpus::new(0xDE45);
    let base = unit(&corpus.vectors(1, 16)[0]);
    let candidates: Vec<Candidate> = corpus
        .vectors(300, 16)
        .iter()
        .enumerate()
        .map(|(i, v)| candidate(i as u64 + 1, &unit(v), &base))
        .collect();

    let tight = RobustPrune::new(1.0).select(Metric::Cosine, &base, &candidates, 64);
    let slack = RobustPrune::new(1.2).select(Metric::Cosine, &base, &candidates, 64);
    assert!(
        slack.len() > tight.len(),
        "{} vs {}",
        slack.len(),
        tight.len()
    );
}
