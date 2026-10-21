use super::*;

fn block(id: u8, parents: &[u8], ops: Vec<TextOp>) -> TextBlock {
    TextBlock {
        id: vec![id],
        parents: parents.iter().map(|&p| vec![p]).collect(),
        ops,
    }
}

fn ins(pos: u64, text: &str) -> TextOp {
    TextOp::Insert {
        pos,
        text: text.into(),
    }
}

fn del(pos: u64, len: u64) -> TextOp {
    TextOp::Delete { pos, len }
}

#[test]
fn a_linear_history_is_its_ops_applied_in_turn() {
    let blocks = [
        block(1, &[], vec![ins(0, "hello")]),
        block(2, &[1], vec![ins(5, " world")]),
        block(3, &[2], vec![del(0, 1), ins(0, "J")]),
        block(4, &[3], vec![del(5, 99)]),
    ];
    assert_eq!(replay(&blocks).unwrap(), "Jello");
}

#[test]
fn concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order() {
    let base = block(1, &[], vec![ins(0, "ab")]);
    let alice = block(2, &[1], vec![ins(1, "xyz")]);
    let bob = block(3, &[1], vec![ins(1, "123")]);
    let text = replay(&[base, alice, bob]).unwrap();
    assert_eq!(text, "axyz123b");
}

#[test]
fn an_edit_concurrent_with_a_delete_still_finds_its_spot() {
    let base = block(1, &[], vec![ins(0, "abc")]);
    let cut = block(2, &[1], vec![del(1, 1)]);
    let after_b = block(3, &[1], vec![ins(2, "!")]);
    assert_eq!(replay(&[base, cut, after_b]).unwrap(), "a!c");
}

#[test]
fn concurrent_deletes_of_one_character_remove_it_once() {
    let base = block(1, &[], vec![ins(0, "abc")]);
    let one = block(2, &[1], vec![del(1, 1)]);
    let two = block(3, &[1], vec![del(0, 2)]);
    assert_eq!(replay(&[base, one, two]).unwrap(), "c");
}

#[test]
fn a_history_with_a_missing_parent_or_a_cycle_is_an_error() {
    assert!(replay(&[block(2, &[1], vec![])]).is_err());
    assert!(replay(&[block(1, &[2], vec![]), block(2, &[1], vec![])]).is_err());
}

/// A small deterministic generator, so a failure names its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// A random DAG of `count` blocks from `peers` peers that sync at random. Each
/// block's ops count in its own parent version, as a real writer's do.
fn history(seed: u64, peers: usize, count: usize) -> Vec<TextBlock> {
    history_of(seed, peers, count, false)
}

/// [`history`], and with `past_end` some ops also reach past the end of their
/// text: a clear of the whole text, or an insert at the end.
fn history_of(seed: u64, peers: usize, count: usize, past_end: bool) -> Vec<TextBlock> {
    let mut rng = Rng(seed);
    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut known: Vec<Vec<usize>> = vec![Vec::new(); peers];
    for n in 0..count {
        let peer = rng.below(peers as u64) as usize;
        if rng.below(3) == 0 {
            let other = rng.below(peers as u64) as usize;
            let theirs = known[other].clone();
            for b in theirs {
                if !known[peer].contains(&b) {
                    known[peer].push(b);
                }
            }
        }
        let parents = heads(&blocks, &known[peer]);
        let mut text = version(&blocks, &known[peer]);
        let mut ops = Vec::new();
        for _ in 0..=rng.below(2) {
            let len = text.chars().count() as u64;
            let op = if past_end && rng.below(8) == 0 {
                del(0, u64::MAX)
            } else if past_end && rng.below(8) == 0 {
                ins(u64::MAX, "$")
            } else if len > 0 && rng.below(3) == 0 {
                del(rng.below(len), 1 + rng.below(3))
            } else {
                let run: String = (0..=rng.below(3))
                    .map(|_| char::from(b'a' + rng.below(26) as u8))
                    .collect();
                ins(rng.below(len + 1), &run)
            };
            text = TextOp::apply_all(&text, std::slice::from_ref(&op));
            ops.push(op);
        }
        // IDs are hash-like: unrelated to the order blocks were written.
        let id = vec![(rng.next() % 251) as u8, n as u8];
        blocks.push(TextBlock {
            id,
            parents: parents.iter().map(|&p| blocks[p].id.clone()).collect(),
            ops,
        });
        known[peer].push(n);
    }
    blocks
}

/// The blocks in `set` no other block in it names as a parent.
fn heads(blocks: &[TextBlock], set: &[usize]) -> Vec<usize> {
    set.iter()
        .copied()
        .filter(|&b| {
            !set.iter()
                .any(|&o| blocks[o].parents.contains(&blocks[b].id))
        })
        .collect()
}

fn version(blocks: &[TextBlock], set: &[usize]) -> String {
    let subset: Vec<TextBlock> = set.iter().map(|&b| blocks[b].clone()).collect();
    replay(&subset).unwrap()
}

/// The blocks before `b`, and `b` itself when `with_self`.
fn closure(blocks: &[TextBlock], b: usize, with_self: bool) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack: Vec<usize> = if with_self {
        vec![b]
    } else {
        parents_of(blocks, b)
    };
    while let Some(x) = stack.pop() {
        if !out.contains(&x) {
            out.push(x);
            stack.extend(parents_of(blocks, x));
        }
    }
    out
}

fn parents_of(blocks: &[TextBlock], b: usize) -> Vec<usize> {
    blocks[b]
        .parents
        .iter()
        .map(|p| blocks.iter().position(|o| &o.id == p).unwrap())
        .collect()
}

/// INV_Intent: a block's ops, applied as plain edits to its parent version,
/// give the replay of that version with the block added. This is also what
/// lets the merge skip the replay when a block's parents are the heads.
#[test]
fn every_block_lands_where_its_author_put_it() {
    for seed in 0..200 {
        let blocks = history(seed, 3, 12);
        for b in 0..blocks.len() {
            let before = version(&blocks, &closure(&blocks, b, false));
            let after = version(&blocks, &closure(&blocks, b, true));
            assert_eq!(
                after,
                TextOp::apply_all(&before, &blocks[b].ops),
                "seed {seed}, block {b}"
            );
        }
    }
}

/// INV_PlacedAsIntended: a character sits among the characters its own
/// version holds exactly where its author put it, so the full replay, cut
/// down to the characters of any block's version, is that version's replay.
#[test]
fn every_version_keeps_its_order_inside_every_later_one() {
    for seed in 0..200 {
        let blocks = history(seed, 3, 12);
        let all = characters(&blocks).unwrap();
        for b in 0..blocks.len() {
            let set: Vec<TextBlock> = closure(&blocks, b, true)
                .into_iter()
                .map(|x| blocks[x].clone())
                .collect();
            let order = |chars: Vec<Character<'_>>| -> Vec<(Vec<u8>, u32)> {
                chars
                    .into_iter()
                    .filter(|c| set.iter().any(|s| s.id == c.block))
                    .map(|c| (c.block.to_vec(), c.offset))
                    .collect()
            };
            assert_eq!(
                order(all.clone()),
                order(characters(&set).unwrap()),
                "seed {seed}, block {b}"
            );
        }
    }
}

/// INV_Converge: the same blocks, in any order, give the same text.
#[test]
fn the_same_blocks_in_any_order_give_the_same_text() {
    for seed in 0..200 {
        let mut blocks = history(seed, 3, 12);
        let text = replay(&blocks).unwrap();
        let mut rng = Rng(seed ^ 0x5eed);
        for _ in 0..5 {
            for i in (1..blocks.len()).rev() {
                blocks.swap(i, rng.below(i as u64 + 1) as usize);
            }
            assert_eq!(replay(&blocks).unwrap(), text, "seed {seed}");
        }
    }
}

/// INV_MatchesReplay: a peer that merges one block at a time, through the
/// fast path when it can and a replay otherwise, ends at the full replay.
#[test]
fn merging_one_block_at_a_time_matches_a_full_replay() {
    for seed in 0..200 {
        let blocks = history(seed, 3, 12);
        let mut merged: Vec<usize> = Vec::new();
        let mut text = String::new();
        for b in 0..blocks.len() {
            let parents = parents_of(&blocks, b);
            let mut current = heads(&blocks, &merged);
            current.sort_unstable();
            let mut sorted = parents.clone();
            sorted.sort_unstable();
            merged.push(b);
            text = if sorted == current {
                TextOp::apply_all(&text, &blocks[b].ops)
            } else {
                version(&blocks, &merged)
            };
        }
        assert_eq!(text, replay(&blocks).unwrap(), "seed {seed}");
    }
}

/// Two peers each typing a run at the same spot: every run stays whole.
#[test]
fn concurrent_runs_never_interleave() {
    for seed in 0..200 {
        let mut rng = Rng(seed);
        let base = block(1, &[], vec![ins(0, "0123456789")]);
        let at = rng.below(11);
        let a = "abcd";
        let b = "WXYZ";
        let first = block(1 + 1 + rng.below(100) as u8, &[1], vec![ins(at, a)]);
        let second = block(150 + rng.below(100) as u8, &[1], vec![ins(at, b)]);
        let text = replay(&[base, first, second]).unwrap();
        assert!(text.contains(a) && text.contains(b), "seed {seed}: {text}");
    }
}

/// A run typed one character per block, forward, stays whole against a
/// concurrent run typed the same way at the same spot.
#[test]
fn runs_typed_a_block_per_character_never_interleave() {
    let base = block(0, &[], vec![ins(0, "[]")]);
    let mut blocks = vec![base];
    let mut last_a = 0u8;
    let mut last_b = 0u8;
    for i in 0..4u8 {
        let a = 10 + i * 2;
        let b = 11 + i * 2;
        blocks.push(block(a, &[last_a], vec![ins(1 + u64::from(i), "a")]));
        blocks.push(block(b, &[last_b], vec![ins(1 + u64::from(i), "b")]));
        last_a = a;
        last_b = b;
    }
    let text = replay(&blocks).unwrap();
    assert!(
        text == "[aaaabbbb]" || text == "[bbbbaaaa]",
        "runs interleaved: {text}"
    );
}

/// A long linear history, then a block concurrent with its last edit: the
/// kept order (checked against the tree on every replay) and the plain edits
/// agree.
#[test]
fn a_long_history_replays_to_its_edits() {
    let mut rng = Rng(7);
    let mut blocks = Vec::new();
    let mut text = String::new();
    for n in 0..300u16 {
        let len = text.chars().count() as u64;
        let op = if len > 10 && rng.below(3) == 0 {
            del(rng.below(len), 1 + rng.below(4))
        } else {
            ins(rng.below(len + 1), "hello")
        };
        text = TextOp::apply_all(&text, std::slice::from_ref(&op));
        blocks.push(TextBlock {
            id: n.to_be_bytes().to_vec(),
            parents: if n == 0 {
                vec![]
            } else {
                vec![(n - 1).to_be_bytes().to_vec()]
            },
            ops: vec![op],
        });
    }
    assert_eq!(replay(&blocks).unwrap(), text);
    blocks.push(TextBlock {
        id: vec![0xff],
        parents: vec![298u16.to_be_bytes().to_vec()],
        ops: vec![ins(0, ">")],
    });
    assert!(replay(&blocks).unwrap().starts_with('>'));
}

/// Whether every block of `blocks` comes before `x` or after it.
fn is_critical(blocks: &[TextBlock], x: usize) -> bool {
    let before = closure(blocks, x, true);
    (0..blocks.len()).all(|b| before.contains(&b) || closure(blocks, b, false).contains(&x))
}

/// INV_CriticalReplay: merging a head into the text of every other block,
/// replayed from any critical block over placeholders for its text, gives the
/// full replay. Deletes and concurrency before the critical block, and ops
/// that reach past the end, included.
#[test]
fn merging_from_any_critical_block_matches_a_full_replay() {
    let mut checked = 0;
    for seed in 0..600 {
        // Small DAGs of few peers have critical blocks often enough to count.
        let peers = 2 + (seed % 2) as usize;
        let count = 4 + (seed % 9) as usize;
        let blocks = history_of(seed, peers, count, seed % 3 == 1);
        let all: Vec<usize> = (0..blocks.len()).collect();
        let want = replay(&blocks).unwrap();
        for new in heads(&blocks, &all) {
            let rest: Vec<usize> = all.iter().copied().filter(|&b| b != new).collect();
            let current = version(&blocks, &rest);
            for x in (0..blocks.len()).filter(|&x| x != new && is_critical(&blocks, x)) {
                let base = closure(&blocks, x, true);
                let after: Vec<TextBlock> = all
                    .iter()
                    .filter(|b| !base.contains(b))
                    .map(|&b| blocks[b].clone())
                    .collect();
                let len = version(&blocks, &base).chars().count();
                let got = merge_text(&current, len, &after, &blocks[new].id).unwrap();
                assert_eq!(
                    got.as_deref(),
                    Some(want.as_str()),
                    "seed {seed}, new {new}, x {x}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} merges had a critical block");
}

/// A stored text that is not what the blocks leave, or a block that already
/// builds on the one merged, sends the caller to a full replay.
#[test]
fn a_merge_that_does_not_fit_the_stored_text_asks_for_a_full_replay() {
    let after = [
        block(2, &[1], vec![ins(1, "x")]),
        block(3, &[1], vec![ins(0, "y")]),
    ];
    assert_eq!(
        merge_text("axb", 2, &after, &[3]).unwrap().as_deref(),
        Some("yaxb")
    );
    assert_eq!(merge_text("ab", 2, &after, &[3]).unwrap(), None);
    assert_eq!(merge_text("azb", 2, &after, &[3]).unwrap(), None);
    assert_eq!(merge_text("axbc", 2, &after, &[3]).unwrap(), None);
    let built_on = [
        block(2, &[1], vec![ins(1, "x")]),
        block(3, &[2], vec![ins(0, "y")]),
    ];
    assert_eq!(merge_text("yaxb", 2, &built_on, &[2]).unwrap(), None);
    assert!(merge_text("ab", 2, &after, &[9]).is_err());
}

/// Edits inside the critical block's text, and deletes across its edges,
/// split the placeholder span without losing or moving a character.
#[test]
fn edits_inside_and_across_the_span_match_a_full_replay() {
    let blocks = vec![
        block(1, &[], vec![ins(0, "abcdefghij")]),
        block(2, &[1], vec![ins(3, "X"), del(5, 3), ins(0, "<")]),
        block(3, &[1], vec![del(1, 2), ins(7, "Y"), del(0, 1)]),
        block(4, &[2], vec![del(2, 4), ins(4, "Z"), ins(9, "!")]),
    ];
    let all: Vec<usize> = (0..blocks.len()).collect();
    let want = replay(&blocks).unwrap();
    for new in heads(&blocks, &all) {
        let rest: Vec<usize> = all.iter().copied().filter(|&b| b != new).collect();
        let got = merge_text(&version(&blocks, &rest), 10, &blocks[1..], &blocks[new].id).unwrap();
        assert_eq!(got.as_deref(), Some(want.as_str()), "new {new}");
    }
}

/// A concurrent merge over a long text builds a handful of nodes, one span
/// piece per edge an edit lands on, not one per character.
#[test]
fn a_merge_over_a_long_text_splits_its_span_only_where_edits_land() {
    let base = 1_000_000;
    let letter = |i: usize| char::from(b'a' + (i % 26) as u8);
    let after = [
        block(2, &[1], vec![ins(500_000, "x"), del(10, 5)]),
        block(3, &[1], vec![ins(999_999, "y"), del(0, 1)]),
    ];
    let (tree, _) = build(&after, Some(base)).unwrap();
    assert!(tree.nodes.len() <= 12, "{} nodes", tree.nodes.len());

    let mut current = String::new();
    let mut want = String::new();
    for i in 0..base {
        if !(10..15).contains(&i) {
            current.push(letter(i));
            if i != 0 {
                want.push(letter(i));
            }
        }
        if i == 499_999 {
            current.push('x');
            want.push('x');
        }
        if i == 999_998 {
            want.push('y');
        }
    }
    let got = merge_text(&current, base, &after, &[3]).unwrap();
    assert!(got.as_deref() == Some(want.as_str()), "merged text differs");
}
