use super::{best_per_parent, top_with_ties};

fn hit(parent: &str, score: f64) -> (String, f64) {
    (parent.to_string(), score)
}

#[test]
fn a_parent_keeps_its_best_chunk_whatever_order_its_chunks_come_in() {
    let best = best_per_parent([hit("a", 0.2), hit("a", 0.9), hit("a", 0.5)]);
    assert_eq!(best, vec![hit("a", 0.9)]);
}

#[test]
fn each_parent_appears_once_in_the_order_it_was_first_seen() {
    let best = best_per_parent([hit("b", 0.3), hit("a", 0.8), hit("b", 0.7), hit("c", 0.1)]);
    assert_eq!(best, vec![hit("b", 0.7), hit("a", 0.8), hit("c", 0.1)]);
}

#[test]
fn no_chunks_rank_no_parents() {
    assert!(best_per_parent(Vec::new()).is_empty());
}

#[test]
fn the_top_keeps_every_parent_tied_with_the_kth_in_order() {
    let best = vec![hit("a", 0.5), hit("b", 0.9), hit("c", 0.7), hit("d", 0.7)];
    assert_eq!(
        top_with_ties(best, 2),
        vec![hit("b", 0.9), hit("c", 0.7), hit("d", 0.7)]
    );
}

#[test]
fn a_top_no_longer_than_k_keeps_everything() {
    let best = vec![hit("a", 0.5), hit("b", 0.9)];
    assert_eq!(top_with_ties(best.clone(), 2), best);
}

#[test]
fn a_nan_score_keeps_every_parent() {
    let best = vec![hit("a", f64::NAN), hit("b", 0.9), hit("c", 0.1)];
    assert_eq!(top_with_ties(best, 1).len(), 3);
}
