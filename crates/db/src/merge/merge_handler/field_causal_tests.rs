use super::is_critical;
use rapidhash::RapidHashMap;

fn dag(edges: &[(u8, &[u8])]) -> RapidHashMap<u8, Vec<u8>> {
    edges
        .iter()
        .map(|(id, parents)| (*id, parents.to_vec()))
        .collect()
}

/// The blocks under `starts` in `all`, `starts` included.
fn under(all: &RapidHashMap<u8, Vec<u8>>, starts: &[u8]) -> RapidHashMap<u8, Vec<u8>> {
    let mut found = RapidHashMap::default();
    let mut stack = starts.to_vec();
    while let Some(next) = stack.pop() {
        if found.contains_key(&next) {
            continue;
        }
        stack.extend(all[&next].iter().copied());
        found.insert(next, all[&next].clone());
    }
    found
}

/// A root 0, two concurrent blocks 1 and 2 on it, a merge 3 of both, and 4
/// on 1 alone. 1 is under both tips, 3 and 4, but 2 is neither before nor
/// after it, so 1 is not critical. A block merged while 2 was already a head
/// is one like that, so the walk must never pick it.
#[test]
fn a_block_under_every_tip_with_a_concurrent_one_is_not_critical() {
    let all = dag(&[(0, &[]), (1, &[0]), (2, &[0]), (3, &[1, 2]), (4, &[1])]);
    let tips = [3, 4];
    assert!(!is_critical(&under(&all, &tips), &under(&all, &[1]), &1));
    assert!(is_critical(&under(&all, &tips), &under(&all, &[0]), &0));
}

/// In a chain every block is critical; a second root breaks that.
#[test]
fn a_chain_is_critical_everywhere_and_a_second_root_is_not() {
    let all = dag(&[(0, &[]), (1, &[0]), (2, &[1])]);
    for at in 0..3 {
        assert!(is_critical(&all, &under(&all, &[at]), &at), "block {at}");
    }
    let forest = dag(&[(0, &[]), (1, &[0]), (9, &[]), (2, &[1, 9])]);
    assert!(!is_critical(&forest, &under(&forest, &[1]), &1));
    assert!(is_critical(&forest, &under(&forest, &[2]), &2));
}
