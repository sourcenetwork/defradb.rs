//! A parent's score is its best accepted chunk's.

use rapidhash::{HashMapExt, RapidHashMap};

/// Each parent once with its best score, in the order parents first appear.
pub(crate) fn best_per_parent<I>(hits: I) -> Vec<(String, f64)>
where
    I: IntoIterator<Item = (String, f64)>,
{
    let mut best: Vec<(String, f64)> = Vec::new();
    let mut position: RapidHashMap<String, usize> = RapidHashMap::new();
    for (parent, score) in hits {
        match position.get(&parent) {
            Some(&at) => {
                if score > best[at].1 {
                    best[at].1 = score;
                }
            }
            None => {
                position.insert(parent.clone(), best.len());
                best.push((parent, score));
            }
        }
    }
    best
}

#[cfg(test)]
#[path = "best_tests.rs"]
mod tests;
