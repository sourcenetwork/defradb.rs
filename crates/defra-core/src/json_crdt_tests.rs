use super::*;
use serde_json::json;

/// A block of a field's DAG. Its priority is one above its highest parent.
struct SimBlock {
    id: Vec<u8>,
    /// Indexes of earlier blocks.
    parents: Vec<usize>,
    priority: u64,
    ops: Vec<JsonOp>,
}

struct Sim {
    blocks: Vec<SimBlock>,
}

impl Sim {
    fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    fn write(&mut self, parents: &[usize], ops: Vec<JsonOp>) -> usize {
        let priority = 1 + parents
            .iter()
            .map(|&p| self.blocks[p].priority)
            .max()
            .unwrap_or(0);
        let parent_ids: Vec<Vec<u8>> = parents.iter().map(|&p| self.blocks[p].id.clone()).collect();
        let id = block_id(priority, &parent_ids, &encode_ops(&ops).unwrap());
        self.blocks.push(SimBlock {
            id,
            parents: parents.to_vec(),
            priority,
            ops,
        });
        self.blocks.len() - 1
    }

    fn past(&self, block: usize) -> BTreeSet<Vec<u8>> {
        let mut past = BTreeSet::new();
        let mut stack = self.blocks[block].parents.clone();
        while let Some(next) = stack.pop() {
            if past.insert(self.blocks[next].id.clone()) {
                stack.extend(self.blocks[next].parents.iter().copied());
            }
        }
        past
    }

    /// The frontier after merging `order`, each block once its parents are in.
    fn merge(&self, order: &[usize]) -> Frontier {
        let mut frontier = Frontier::default();
        let mut merged = BTreeSet::new();
        for &b in order {
            assert!(self.blocks[b].parents.iter().all(|p| merged.contains(p)));
            let past = self.past(b);
            let block = &self.blocks[b];
            let delta = frontier.delta(&block.ops, &block.id, block.priority, |id| {
                past.contains(id)
            });
            frontier.compose(delta);
            frontier.settle();
            merged.insert(b);
        }
        frontier
    }
}

fn set(path: &[&str], value: Value) -> JsonOp {
    op(path, JsonEdit::Set(value))
}

fn op(path: &[&str], edit: JsonEdit) -> JsonOp {
    JsonOp {
        path: path.iter().map(|s| (*s).to_owned()).collect(),
        edit,
    }
}

/// What a frontier reads as: its value and its conflicts. Two peers holding
/// the same blocks agree on both, whatever order they merged them in.
fn view(frontier: &Frontier) -> (Option<Value>, Vec<Conflict>) {
    (frontier.materialize(), frontier.conflicts())
}

fn create(sim: &mut Sim, value: Value) -> usize {
    sim.write(&[], diff(None, Some(&value)))
}

#[test]
fn diff_then_merge_gives_the_new_value() {
    let old = json!({"a": {"b": 1, "c": [1, 2]}, "d": "x"});
    let new = json!({"a": {"b": 2, "e": {"f": null}}, "g": true});
    let mut sim = Sim::new();
    let root = create(&mut sim, old.clone());
    assert_eq!(sim.merge(&[root]).value(), old);
    let next = sim.write(&[root], diff(Some(&old), Some(&new)));
    assert_eq!(sim.merge(&[root, next]).value(), new);
    assert!(diff(Some(&new), Some(&new)).is_empty());
}

#[test]
fn disjoint_keys_both_survive_in_any_order() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": 1, "b": 1}));
    let left = sim.write(&[root], vec![set(&["a"], json!(2))]);
    let right = sim.write(&[root], vec![set(&["b"], json!(3))]);
    let one = sim.merge(&[root, left, right]);
    let two = sim.merge(&[root, right, left]);
    assert_eq!(view(&one), view(&two));
    assert_eq!(one.value(), json!({"a": 2, "b": 3}));
    assert!(one.conflicts().is_empty());
}

#[test]
fn one_path_resolves_by_priority_then_block_and_lists_the_loser() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": 1}));
    let left = sim.write(&[root], vec![set(&["a"], json!("left"))]);
    let right = sim.write(&[root], vec![set(&["a"], json!("right"))]);
    let merged = sim.merge(&[root, left, right]);
    assert_eq!(view(&merged), view(&sim.merge(&[root, right, left])));
    let winner = if sim.blocks[left].id > sim.blocks[right].id {
        "left"
    } else {
        "right"
    };
    assert_eq!(merged.value(), json!({ "a": winner }));
    let conflicts = merged.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path, vec!["a".to_owned()]);

    let later = sim.write(&[left], vec![set(&["a"], json!("later"))]);
    let merged = sim.merge(&[root, left, later, right]);
    assert_eq!(
        merged.value(),
        json!({"a": "later"}),
        "higher priority wins"
    );
}

#[test]
fn an_update_beats_a_concurrent_delete_above_it() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": {"b": 1, "c": 1}}));
    let delete = sim.write(&[root], vec![op(&["a"], JsonEdit::Delete)]);
    let update = sim.write(&[root], vec![set(&["a", "b"], json!(2))]);
    let merged = sim.merge(&[root, delete, update]);
    assert_eq!(view(&merged), view(&sim.merge(&[root, update, delete])));
    assert_eq!(merged.value(), json!({"a": {"b": 2}}));
    let conflicts = merged.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].edit, JsonEdit::Delete);
}

#[test]
fn a_delete_never_brings_a_path_back() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": {"b": 1}}));
    let outer = sim.write(&[root], vec![op(&["a"], JsonEdit::Delete)]);
    let inner = sim.write(&[root], vec![op(&["a", "b"], JsonEdit::Delete)]);
    let merged = sim.merge(&[root, outer, inner]);
    assert_eq!(view(&merged), view(&sim.merge(&[root, inner, outer])));
    assert_eq!(merged.value(), json!({}));
}

#[test]
fn a_type_clash_resolves_by_the_same_order_and_the_loser_takes_its_children() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": {"b": 1}}));
    let value = sim.write(&[root], vec![set(&["a"], json!(5))]);
    let child = sim.write(&[root], vec![set(&["a", "c"], json!(2))]);
    let merged = sim.merge(&[root, value, child]);
    assert_eq!(view(&merged), view(&sim.merge(&[root, child, value])));
    let expected = if sim.blocks[value].id > sim.blocks[child].id {
        json!({"a": 5})
    } else {
        json!({"a": {"c": 2}})
    };
    assert_eq!(merged.value(), expected);
    assert_eq!(merged.conflicts().len(), 1);
}

#[test]
fn a_losing_block_that_sets_several_paths_below_is_one_conflict() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({}));
    let other = sim.write(&[root], vec![set(&["b"], json!(1))]);
    let value = sim.write(&[other], vec![set(&["a"], json!(5))]);
    let child = sim.write(
        &[root],
        vec![set(&["a", "c"], json!(2)), set(&["a", "d"], json!(3))],
    );
    let merged = sim.merge(&[root, other, value, child]);
    assert_eq!(
        view(&merged),
        view(&sim.merge(&[root, child, other, value]))
    );
    assert_eq!(merged.value(), json!({"a": 5, "b": 1}));
    let conflicts = merged.conflicts();
    assert_eq!(conflicts.len(), 1, "{conflicts:?}");
    assert_eq!(conflicts[0].block, sim.blocks[child].id);
}

#[test]
fn an_edit_that_saw_the_one_above_it_is_not_a_conflict() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({}));
    let value = sim.write(&[root], vec![set(&["a"], json!(5))]);
    let child = sim.write(&[value], vec![set(&["a", "b"], json!(1))]);
    let merged = sim.merge(&[root, value, child]);
    assert_eq!(merged.value(), json!({"a": {"b": 1}}));
    assert!(merged.conflicts().is_empty());
}

#[test]
fn three_writers_converge_whatever_the_order() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": {"b": 1}, "c": [1]}));
    let x = sim.write(&[root], vec![op(&["a"], JsonEdit::Delete)]);
    let y = sim.write(
        &[root],
        vec![set(&["a", "b"], json!(9)), set(&["c"], json!([2]))],
    );
    let z = sim.write(&[x], vec![set(&["a", "d"], json!("z"))]);
    let w = sim.write(&[y, z], vec![op(&["c"], JsonEdit::Delete)]);
    let orders = [
        vec![root, x, y, z, w],
        vec![root, y, x, z, w],
        vec![root, x, z, y, w],
    ];
    let first = sim.merge(&orders[0]);
    for order in &orders[1..] {
        assert_eq!(view(&sim.merge(order)), view(&first));
    }
    assert_eq!(first.value(), json!({"a": {"b": 9, "d": "z"}}));
}

#[test]
fn composing_operands_is_associative_and_matches_applying_them_in_turn() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": 1, "b": {"c": 1}}));
    let base = sim.merge(&[root]);
    let one = base.local_delta(&[set(&["a"], json!(2))], b"one", 2);
    let mut after_one = base.clone();
    after_one.compose(one.clone());
    let two = after_one.local_delta(&[op(&["b"], JsonEdit::Delete)], b"two", 3);
    let mut after_two = after_one.clone();
    after_two.compose(two.clone());
    let three = after_two.local_delta(&[set(&["b", "c"], json!(3))], b"three", 4);

    let mut left = one.clone();
    left.compose(two.clone());
    left.compose(three.clone());
    let mut right = two;
    right.compose(three.clone());
    let mut grouped = one;
    grouped.compose(right);
    assert_eq!(left, grouped);

    let mut stepwise = after_two;
    stepwise.compose(three);
    let mut at_once = base;
    at_once.compose(left);
    at_once.settle();
    stepwise.settle();
    assert_eq!(at_once, stepwise);
    assert_eq!(at_once.value(), json!({"a": 2, "b": {"c": 3}}));
}

#[test]
fn a_frontier_round_trips_through_its_encoding() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": [1, {"b": null}], "c": 1.5}));
    let frontier = sim.merge(&[root]);
    assert_eq!(
        Frontier::decode(&frontier.encode().unwrap()).unwrap(),
        frontier
    );
    let ops = diff(None, Some(&json!({"x": "y"})));
    assert_eq!(decode_ops(&encode_ops(&ops).unwrap()).unwrap(), ops);
}

#[test]
fn path_keys_round_trip_and_only_prefix_their_descendants() {
    let paths: Vec<Path> = vec![
        vec![],
        vec!["a".into()],
        vec!["ab".into()],
        vec!["a".into(), "b".into()],
        vec!["a\0b".into()],
        vec!["a\0".into(), "b".into()],
        vec!["".into(), "".into()],
    ];
    for path in &paths {
        assert_eq!(decode_path(&encode_path(path)).as_ref(), Some(path));
        for other in &paths {
            let prefix = encode_path(other).starts_with(&encode_path(path));
            assert_eq!(prefix, other.starts_with(path), "{path:?} {other:?}");
        }
    }
}

#[test]
fn a_block_id_depends_on_priority_parents_and_ops_but_not_parent_order() {
    let parents = vec![b"p1".to_vec(), b"p2".to_vec()];
    let flipped = vec![b"p2".to_vec(), b"p1".to_vec()];
    assert_eq!(block_id(2, &parents, b"x"), block_id(2, &flipped, b"x"));
    assert_ne!(block_id(2, &parents, b"x"), block_id(3, &parents, b"x"));
    assert_ne!(block_id(2, &parents, b"x"), block_id(2, &parents, b"y"));
    assert_ne!(
        block_id(2, &parents, b"x"),
        block_id(2, &parents[..1], b"x")
    );
}

#[test]
fn a_replay_of_the_whole_dag_matches_merging_it_block_by_block() {
    let mut sim = Sim::new();
    let root = create(&mut sim, json!({"a": {"b": 1}, "c": 1}));
    let x = sim.write(&[root], vec![op(&["a"], JsonEdit::Delete)]);
    let y = sim.write(&[root], vec![set(&["a", "b"], json!(2))]);
    let z = sim.write(&[x, y], vec![set(&["c"], json!(3))]);
    let cid = |b: usize| format!("cid{b}").into_bytes();
    let replayed = replay(
        [root, x, y, z]
            .iter()
            .map(|&b| ReplayBlock {
                cid: cid(b),
                parents: sim.blocks[b].parents.iter().map(|&p| cid(p)).collect(),
                priority: sim.blocks[b].priority,
                data: encode_ops(&sim.blocks[b].ops).unwrap(),
            })
            .collect(),
    )
    .unwrap();
    assert_eq!(view(&replayed).0, view(&sim.merge(&[root, y, x, z])).0);
}

/// `Frontier::delta` as one scan of the whole frontier per op: the
/// definition the indexed delta must match.
fn delta_per_op<F>(
    frontier: &Frontier,
    ops: &[JsonOp],
    block: &[u8],
    priority: u64,
    in_past: F,
) -> Frontier
where
    F: Fn(&[u8]) -> bool,
{
    let mut removes = Removes::new();
    let mut entries = Vec::new();
    for op in ops {
        let mut seen = BTreeSet::new();
        for entry in &frontier.entries {
            if !in_past(&entry.block) {
                continue;
            }
            if entry.path.starts_with(&op.path) {
                removes.insert(
                    (encode_path(&entry.path), entry.block.clone()),
                    (entry.path.clone(), entry.block.clone()),
                );
            } else if op.path.starts_with(&entry.path) {
                seen.insert(entry.block.clone());
            }
        }
        entries.push(Entry {
            path: op.path.clone(),
            block: block.to_vec(),
            priority,
            edit: op.edit.clone(),
            seen: seen.into_iter().collect(),
        });
    }
    Frontier {
        removes: removes.into_values().collect(),
        entries,
    }
}

/// `Frontier::compose` as a rebuild of both lists: the definition the
/// in-place compose must match.
fn compose_by_rebuild(mut frontier: Frontier, later: Frontier) -> Frontier {
    let key = |path: &[String], block: &[u8]| (encode_path(path), block.to_vec());
    let removed: BTreeSet<EntryKey> = later
        .removes
        .iter()
        .map(|(path, block)| key(path, block))
        .collect();
    let mut entries: BTreeMap<EntryKey, Entry> = std::mem::take(&mut frontier.entries)
        .into_iter()
        .map(|entry| (key(&entry.path, &entry.block), entry))
        .filter(|(k, _)| !removed.contains(k))
        .collect();
    for entry in later.entries {
        entries.insert(key(&entry.path, &entry.block), entry);
    }
    let removes: Removes = std::mem::take(&mut frontier.removes)
        .into_iter()
        .chain(later.removes)
        .map(|(path, block)| (key(&path, &block), (path, block)))
        .collect();
    Frontier {
        removes: removes.into_values().collect(),
        entries: entries.into_values().collect(),
    }
}

#[test]
fn an_indexed_merge_matches_one_scan_per_op_and_a_full_rebuild() {
    let mut sim = Sim::new();
    let root = create(
        &mut sim,
        json!({"a": {"b": {"c": 1}, "bb": 2}, "a\u{0}": 3, "ab": 4, "z": [1]}),
    );
    let left = sim.write(&[root], vec![set(&["a", "b", "d"], json!(5))]);
    let right = sim.write(
        &[root],
        vec![op(&["a"], JsonEdit::Map), set(&["q"], json!(6))],
    );
    let frontier = sim.merge(&[root, left, right]);
    let ops = vec![
        set(&["a", "b"], json!(7)),
        op(&["a"], JsonEdit::Delete),
        set(&["a", "b", "c", "e"], json!(8)),
        set(&["a", "b"], json!(9)),
        set(&["ab"], json!(10)),
        op(&[], JsonEdit::Map),
        set(&["missing", "deep"], json!(11)),
    ];
    let left_past = sim.past(left);
    let left_id = sim.blocks[left].id.clone();
    check_merge(&frontier, &ops, |_: &[u8]| true);
    check_merge(&frontier, &ops, |_: &[u8]| false);
    check_merge(&frontier, &ops, |id: &[u8]| {
        left_past.contains(id) || id == left_id.as_slice()
    });
}

/// Every prefix of `ops`, merged onto `frontier`, gives the same frontier
/// both ways, and so does composing the two operands one after the other.
fn check_merge<F>(frontier: &Frontier, ops: &[JsonOp], in_past: F)
where
    F: Fn(&[u8]) -> bool + Copy,
{
    for count in 0..=ops.len() {
        let ops = &ops[..count];
        let indexed = frontier.delta(ops, b"new", 9, in_past);
        let scanned = delta_per_op(frontier, ops, b"new", 9, in_past);
        let mut merged = frontier.clone();
        merged.compose(indexed.clone());
        assert_eq!(
            merged,
            compose_by_rebuild(frontier.clone(), scanned.clone()),
            "{count} ops"
        );
        let mut both = indexed.clone();
        both.compose(merged.delta(ops, b"next", 10, in_past));
        assert_eq!(
            both,
            compose_by_rebuild(scanned, delta_per_op(&merged, ops, b"next", 10, in_past)),
            "{count} ops, composed"
        );
    }
}

#[test]
fn decoding_puts_entries_in_path_order_with_the_last_repeat_winning() {
    let entry = |path: &[&str], block: &[u8], value: i64| Entry {
        path: path.iter().map(|s| (*s).to_owned()).collect(),
        block: block.to_vec(),
        priority: 1,
        edit: JsonEdit::Set(json!(value)),
        seen: Vec::new(),
    };
    let unsorted = Frontier {
        removes: vec![
            (vec!["b".into()], b"x".to_vec()),
            (vec!["a".into()], b"x".to_vec()),
        ],
        entries: vec![
            entry(&["b"], b"x", 1),
            entry(&["a", "c"], b"x", 2),
            entry(&["a"], b"y", 3),
            entry(&["b"], b"x", 4),
        ],
    };
    let decoded = Frontier::decode(&unsorted.encode().unwrap()).unwrap();
    assert_eq!(decoded, compose_by_rebuild(Frontier::default(), unsorted));
    assert_eq!(decoded.entries.len(), 3);
}

#[test]
fn streamed_path_keys_match_built_ones() {
    let paths: Vec<Path> = [
        vec![],
        vec!["a"],
        vec!["a", "b"],
        vec!["a\u{0}"],
        vec!["a", ""],
        vec!["ab"],
        vec!["\u{0}\u{1}"],
    ]
    .into_iter()
    .map(|path| path.into_iter().map(str::to_owned).collect())
    .collect();
    for path in &paths {
        assert_eq!(key_bytes(path).collect::<Vec<u8>>(), encode_path(path));
        for other in &paths {
            let key = encode_path(other);
            assert_eq!(cmp_key(path, &key), encode_path(path).cmp(&key));
            assert_eq!(
                key_starts_with(path, &key),
                encode_path(path).starts_with(&key)
            );
        }
    }
}

#[test]
fn an_update_carries_its_nonce_so_equal_edits_take_different_ids() {
    let ops = diff(None, Some(&json!({"a": {"b": 1}})));
    let first = encode_write(1, &ops).unwrap();
    let second = encode_write(2, &ops).unwrap();
    assert_eq!(decode_ops(&first).unwrap(), ops);
    assert_eq!(decode_ops(&second).unwrap(), ops);
    let parents = vec![b"head".to_vec()];
    assert_ne!(
        block_id(2, &parents, &first),
        block_id(2, &parents, &second)
    );
    assert_ne!(encode_ops(&ops).unwrap(), first);
}
