//! Text CRDT: a Fugue tree replayed from one Text field's blocks.
//!
//! Every character is a node of a tree. A block's ops count positions in the
//! block's parent version, the text its parents produce. An insert hangs its
//! first character on the character before its position (the left origin):
//! as the left origin's right child when it has none in that version, and
//! otherwise as the left child of the node right after it in that version's
//! order. Each later character of the same insert is the right child of the
//! one before. Siblings are ordered by ID, the block's CID then the
//! character's offset in the block. Reading the tree in order, skipping
//! deleted characters, gives the text. A delete only marks its characters, so
//! concurrent edits that point at them still find them.
//!
//! [`replay`] rebuilds a field's text from all its blocks; the tree is thrown
//! away after, and the stored value is the text. A block whose parents are the
//! field's current heads needs no replay: its parent version is the stored
//! text, so [`TextOp::apply_all`] on it gives the same result.
//!
//! The contract is `proofs/tla/TextCrdt.tla` and `proofs/lean/TextCrdt`:
//! convergence, replay equality and intent preservation.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use defra_core::{Error, Result, TextOp};
use rapidhash::{HashMapExt, RapidHashMap};

/// One block of a Text field's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlock {
    /// The block's CID bytes: its ID, and its characters' sibling order.
    pub id: Vec<u8>,
    /// The CID bytes of its parents in the field's DAG.
    pub parents: Vec<Vec<u8>>,
    /// Its edits, in order.
    pub ops: Vec<TextOp>,
}

/// The tree's root: not a character, never shown.
const ROOT: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

struct Node {
    /// The topological position of the block that inserted it.
    block: usize,
    /// Its offset among the characters its block inserts.
    offset: u32,
    ch: char,
    /// The topological positions of the blocks that delete it.
    deleted_by: Vec<usize>,
    left: Vec<usize>,
    right: Vec<usize>,
}

/// The blocks before one block in the DAG.
#[derive(Clone)]
enum Ancestry {
    /// Every block at a topological position below this one.
    Prefix(usize),
    /// The positions set in this bitmap.
    Set(Vec<u64>),
}

impl Ancestry {
    fn contains(&self, position: usize) -> bool {
        match self {
            Ancestry::Prefix(end) => position < *end,
            Ancestry::Set(bits) => bits
                .get(position / 64)
                .is_some_and(|word| word & (1 << (position % 64)) != 0),
        }
    }

    fn bits(&self, len: usize) -> Vec<u64> {
        match self {
            Ancestry::Set(bits) => bits.clone(),
            Ancestry::Prefix(end) => {
                let mut bits = vec![0u64; len.div_ceil(64)];
                for position in 0..*end {
                    bits[position / 64] |= 1 << (position % 64);
                }
                bits
            }
        }
    }
}

/// Where a node sits in an [`Order`]: its chunk, and its index there.
#[derive(Debug, Clone, Copy, Default)]
struct Spot {
    chunk: usize,
    index: usize,
}

/// Nodes per chunk of an [`Order`]; a chunk splits in two past twice this.
/// Tests use tiny chunks so their small texts split them often.
const CHUNK: usize = if cfg!(test) { 2 } else { 512 };

/// Nodes in reading order, in chunks that count their visible nodes, so
/// finding the character at a position walks the chunks and then one chunk,
/// not the whole text.
struct Order {
    chunks: Vec<Chunk>,
    /// The visible nodes in all chunks.
    visible: usize,
}

struct Chunk {
    nodes: Vec<usize>,
    visible: usize,
}

impl Order {
    fn new<F>(nodes: Vec<usize>, alive: F) -> Self
    where
        F: Fn(usize) -> bool,
    {
        let mut chunks: Vec<Chunk> = nodes
            .chunks(CHUNK)
            .map(|nodes| Chunk {
                visible: nodes.iter().filter(|&&node| alive(node)).count(),
                nodes: nodes.to_vec(),
            })
            .collect();
        if chunks.is_empty() {
            chunks.push(Chunk {
                nodes: Vec::new(),
                visible: 0,
            });
        }
        let visible = chunks.iter().map(|chunk| chunk.visible).sum();
        Self { chunks, visible }
    }

    fn at(&self, spot: Spot) -> usize {
        self.chunks[spot.chunk].nodes[spot.index]
    }

    /// The node at `spot`, or none at the end.
    fn node_from(&self, spot: Spot) -> Option<usize> {
        self.chunks.get(spot.chunk)?.nodes.get(spot.index).copied()
    }

    /// The spot right after `spot`: the next node's, or the end.
    fn after(&self, spot: Spot) -> Spot {
        if spot.index + 1 < self.chunks[spot.chunk].nodes.len()
            || spot.chunk + 1 == self.chunks.len()
        {
            Spot {
                chunk: spot.chunk,
                index: spot.index + 1,
            }
        } else {
            Spot {
                chunk: spot.chunk + 1,
                index: 0,
            }
        }
    }

    /// The spot of visible node number `k`, counting from 0; `k` is below
    /// [`Order::visible`].
    fn nth_visible<F>(&self, mut k: usize, alive: F) -> Spot
    where
        F: Fn(usize) -> bool,
    {
        for (chunk, held) in self.chunks.iter().enumerate() {
            if k >= held.visible {
                k -= held.visible;
                continue;
            }
            for (index, &node) in held.nodes.iter().enumerate() {
                if alive(node) {
                    if k == 0 {
                        return Spot { chunk, index };
                    }
                    k -= 1;
                }
            }
        }
        unreachable!("visible node {k} past the end of the text")
    }

    /// Put the new, visible `node` at `spot`, and return where it sits.
    fn insert<F>(&mut self, spot: Spot, node: usize, alive: F) -> Spot
    where
        F: Fn(usize) -> bool,
    {
        let held = &mut self.chunks[spot.chunk];
        held.nodes.insert(spot.index, node);
        held.visible += 1;
        self.visible += 1;
        if held.nodes.len() <= 2 * CHUNK {
            return spot;
        }
        let tail = held.nodes.split_off(CHUNK);
        let visible = tail.iter().filter(|&&node| alive(node)).count();
        held.visible -= visible;
        self.chunks.insert(
            spot.chunk + 1,
            Chunk {
                nodes: tail,
                visible,
            },
        );
        if spot.index < CHUNK {
            spot
        } else {
            Spot {
                chunk: spot.chunk + 1,
                index: spot.index - CHUNK,
            }
        }
    }

    /// The node at `spot` was just deleted.
    fn hide(&mut self, spot: Spot) {
        self.chunks[spot.chunk].visible -= 1;
        self.visible -= 1;
    }

    fn into_nodes(self) -> Vec<usize> {
        self.chunks
            .into_iter()
            .flat_map(|chunk| chunk.nodes)
            .collect()
    }
}

/// The replay state: the tree, and the order of every node in it while every
/// node is visible to the block being replayed.
struct Tree<'a> {
    ids: Vec<&'a [u8]>,
    nodes: Vec<Node>,
    /// Every node in reading order, or `None` after a step that saw only part
    /// of the tree.
    order: Option<Order>,
}

/// What one block sees of the tree: the blocks before it, and itself.
struct Version<'v> {
    ancestry: &'v Ancestry,
    block: usize,
}

impl Version<'_> {
    fn has(&self, block: usize) -> bool {
        block == self.block || self.ancestry.contains(block)
    }
}

impl<'a> Tree<'a> {
    fn cmp_id(&self, a: usize, b: usize) -> Ordering {
        let (a, b) = (&self.nodes[a], &self.nodes[b]);
        self.ids[a.block]
            .cmp(self.ids[b.block])
            .then(a.offset.cmp(&b.offset))
    }

    fn present(&self, node: usize, version: &Version<'_>) -> bool {
        version.has(self.nodes[node].block)
    }

    fn dead(&self, node: usize, version: &Version<'_>) -> bool {
        self.nodes[node]
            .deleted_by
            .iter()
            .any(|&block| version.has(block))
    }

    /// Every node the version holds, deleted ones too, in reading order.
    fn traverse(&self, version: Option<&Version<'_>>) -> Vec<usize> {
        enum Item {
            Visit(usize),
            Emit(usize),
        }
        let keep = |node: &usize| version.is_none_or(|version| self.present(*node, version));
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![Item::Visit(ROOT)];
        while let Some(item) = stack.pop() {
            match item {
                Item::Emit(node) => out.push(node),
                Item::Visit(node) => {
                    let n = &self.nodes[node];
                    stack.extend(
                        n.right
                            .iter()
                            .rev()
                            .filter(|c| keep(c))
                            .map(|&c| Item::Visit(c)),
                    );
                    if node != ROOT {
                        stack.push(Item::Emit(node));
                    }
                    stack.extend(
                        n.left
                            .iter()
                            .rev()
                            .filter(|c| keep(c))
                            .map(|&c| Item::Visit(c)),
                    );
                }
            }
        }
        out
    }

    /// Hang a new node under `parent` on `side`, among its siblings by ID.
    fn attach(&mut self, node: Node, parent: usize, side: Side) -> usize {
        let index = self.nodes.len();
        self.nodes.push(node);
        let kids = match side {
            Side::Left => &self.nodes[parent].left,
            Side::Right => &self.nodes[parent].right,
        };
        let at = kids.partition_point(|&kid| self.cmp_id(kid, index) == Ordering::Less);
        match side {
            Side::Left => self.nodes[parent].left.insert(at, index),
            Side::Right => self.nodes[parent].right.insert(at, index),
        }
        index
    }

    /// Apply one op of the block at topological position `block`, which sees
    /// the blocks in `ancestry`. `next_offset` numbers the block's characters.
    fn apply(&mut self, op: &TextOp, block: usize, ancestry: &Ancestry, next_offset: &mut u32) {
        let version = Version { ancestry, block };
        // A block that follows every block before it sees every node, so the
        // kept order is its version's order.
        let linear = matches!(ancestry, Ancestry::Prefix(end) if *end == block);
        let mut order = match self.order.take() {
            Some(order) if linear => order,
            _ => {
                let full = self.traverse((!linear).then_some(&version));
                Order::new(full, |node| !self.dead(node, &version))
            }
        };
        match op {
            TextOp::Insert { pos, text } => {
                let mut chars = text.chars();
                if let Some(first) = chars.next() {
                    let p = clamp(*pos, order.visible);
                    let (origin, mut at) = match p.checked_sub(1) {
                        None => (ROOT, Spot::default()),
                        Some(k) => {
                            let spot = order.nth_visible(k, |node| !self.dead(node, &version));
                            (order.at(spot), order.after(spot))
                        }
                    };
                    let has_right = self.nodes[origin]
                        .right
                        .iter()
                        .any(|&kid| self.present(kid, &version));
                    let (parent, side) = match order.node_from(at) {
                        Some(next) if has_right => (next, Side::Left),
                        _ => (origin, Side::Right),
                    };
                    // The new node lands right after its left origin: it is
                    // either the origin's only right child, or the left child
                    // of the leftmost node of the origin's right subtree.
                    let mut previous =
                        self.attach(self.node(block, next_offset, first), parent, side);
                    at = order.insert(at, previous, |node| !self.dead(node, &version));
                    for ch in chars {
                        previous =
                            self.attach(self.node(block, next_offset, ch), previous, Side::Right);
                        at = order
                            .insert(order.after(at), previous, |node| !self.dead(node, &version));
                    }
                }
            }
            TextOp::Delete { pos, len } => {
                let start = clamp(*pos, order.visible);
                let end = clamp(pos.saturating_add(*len), order.visible);
                if start < end {
                    let mut spot = order.nth_visible(start, |node| !self.dead(node, &version));
                    let mut left = end - start;
                    while left > 0 {
                        let node = order.at(spot);
                        if !self.dead(node, &version) {
                            self.nodes[node].deleted_by.push(block);
                            order.hide(spot);
                            left -= 1;
                        }
                        spot = order.after(spot);
                    }
                }
            }
        }
        if linear {
            self.order = Some(order);
        }
    }

    fn node(&self, block: usize, next_offset: &mut u32, ch: char) -> Node {
        let offset = *next_offset;
        *next_offset += 1;
        Node {
            block,
            offset,
            ch,
            deleted_by: Vec::new(),
            left: Vec::new(),
            right: Vec::new(),
        }
    }
}

/// `pos` as an index into `len` characters, the end when past it.
fn clamp(pos: u64, len: usize) -> usize {
    usize::try_from(pos).map_or(len, |pos| pos.min(len))
}

/// The text a Text field's blocks produce. `blocks` is the whole history: every
/// parent a block names is in it. Any order of `blocks` gives the same text.
pub fn replay(blocks: &[TextBlock]) -> Result<String> {
    Ok(characters(blocks)?
        .into_iter()
        .filter(|c| !c.deleted)
        .map(|c| c.ch)
        .collect())
}

/// One character of a replayed Text field, deleted ones too.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Character<'a> {
    /// The CID bytes of the block that inserted it.
    block: &'a [u8],
    offset: u32,
    ch: char,
    deleted: bool,
}

/// Every character `blocks` insert, in reading order.
fn characters(blocks: &[TextBlock]) -> Result<Vec<Character<'_>>> {
    let order = topological(blocks)?;
    let position: RapidHashMap<&[u8], usize> = order
        .iter()
        .enumerate()
        .map(|(position, &index)| (blocks[index].id.as_slice(), position))
        .collect();
    let mut tree = Tree {
        ids: order
            .iter()
            .map(|&index| blocks[index].id.as_slice())
            .collect(),
        nodes: vec![Node {
            block: usize::MAX,
            offset: 0,
            ch: '\0',
            deleted_by: Vec::new(),
            left: Vec::new(),
            right: Vec::new(),
        }],
        order: Some(Order::new(Vec::new(), |_| true)),
    };
    let mut ancestry: Vec<Ancestry> = Vec::with_capacity(order.len());
    for (step, &index) in order.iter().enumerate() {
        let block = &blocks[index];
        let parents: Vec<usize> = block
            .parents
            .iter()
            .map(|parent| position[parent.as_slice()])
            .collect();
        let before = ancestry_of(&parents, &ancestry, step);
        let mut next_offset = 0;
        for op in &block.ops {
            tree.apply(op, step, &before, &mut next_offset);
        }
        ancestry.push(before);
    }
    let all = Ancestry::Prefix(order.len());
    let version = Version {
        ancestry: &all,
        block: usize::MAX,
    };
    let full = match tree.order.take() {
        Some(order) => {
            let order = order.into_nodes();
            // The kept order stands in for the tree; the tests hold it to it.
            #[cfg(test)]
            assert_eq!(order, tree.traverse(None), "kept order left the tree");
            order
        }
        None => tree.traverse(None),
    };
    Ok(full
        .into_iter()
        .map(|node| {
            let n = &tree.nodes[node];
            Character {
                block: tree.ids[n.block],
                offset: n.offset,
                ch: n.ch,
                deleted: tree.dead(node, &version),
            }
        })
        .collect())
}

/// The blocks before the block at topological position `step`, whose parents
/// are at `parents`.
fn ancestry_of(parents: &[usize], ancestry: &[Ancestry], step: usize) -> Ancestry {
    // A block whose parent is the step before, and that step saw everything
    // before it, sees everything: the common case of a linear history.
    if let [parent] = parents {
        if *parent + 1 == step
            && matches!(ancestry[*parent], Ancestry::Prefix(end) if end == *parent)
        {
            return Ancestry::Prefix(step);
        }
    }
    let mut bits = vec![0u64; step.div_ceil(64)];
    for &parent in parents {
        for (word, theirs) in bits.iter_mut().zip(ancestry[parent].bits(step)) {
            *word |= theirs;
        }
        bits[parent / 64] |= 1 << (parent % 64);
    }
    let count: usize = bits.iter().map(|word| word.count_ones() as usize).sum();
    if count == step {
        Ancestry::Prefix(step)
    } else {
        Ancestry::Set(bits)
    }
}

/// `blocks` in an order where every block follows its parents, ties broken by
/// ID so the order is the same on every peer.
fn topological(blocks: &[TextBlock]) -> Result<Vec<usize>> {
    let mut index: RapidHashMap<&[u8], usize> = RapidHashMap::with_capacity(blocks.len());
    for (i, block) in blocks.iter().enumerate() {
        if index.insert(block.id.as_slice(), i).is_some() {
            return Err(Error::MergeError(
                "a Text field's history holds a block twice".into(),
            ));
        }
    }
    let mut waiting = vec![0usize; blocks.len()];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); blocks.len()];
    for (i, block) in blocks.iter().enumerate() {
        for parent in &block.parents {
            let Some(&p) = index.get(parent.as_slice()) else {
                return Err(Error::MergeError(
                    "a Text field's history is missing a parent block".into(),
                ));
            };
            waiting[i] += 1;
            children[p].push(i);
        }
    }
    let mut ready: BTreeSet<(&[u8], usize)> = blocks
        .iter()
        .enumerate()
        .filter(|(i, _)| waiting[*i] == 0)
        .map(|(i, block)| (block.id.as_slice(), i))
        .collect();
    let mut order = Vec::with_capacity(blocks.len());
    while let Some(first) = ready.pop_first() {
        let i = first.1;
        order.push(i);
        for &child in &children[i] {
            waiting[child] -= 1;
            if waiting[child] == 0 {
                ready.insert((blocks[child].id.as_slice(), child));
            }
        }
    }
    if order.len() != blocks.len() {
        return Err(Error::MergeError(
            "a Text field's history has a cycle".into(),
        ));
    }
    Ok(order)
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
