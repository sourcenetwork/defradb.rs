//! A whole-history replay that folds its tree at critical blocks.
//!
//! The tree keeps one node per character ever inserted, deleted ones too, so
//! a replay that kept it whole would hold the field's history, not its text.
//! A critical block is one every other block comes before or after: every
//! block after it sees all of the tree, and its text alone decides what they
//! do (`INV_CriticalReplay`). So past a critical block the replay folds the
//! tree down to its text, kept as a string, with one span node standing for
//! it, as [`super::merge_text`] does for the critical block it starts from.
//! The tree then holds only what the blocks since the last fold added.
//!
//! The contract is `proofs/lean/TextCrdt/Compact.lean` (`run_compact`) and
//! `INV_CriticalReplay` in `proofs/tla/TextCrdt.tla`.

use defra_core::{Error, Result};

use super::{blank, Ancestry, History, Order, TextBlock, Tree, BASE, ROOT};

/// The nodes a tree may hold above an eighth of its folded text's characters
/// before the next critical block folds it. Folding costs a walk of the tree
/// and the text, so the tree is let grow by a share of the text first.
const FOLD_SLACK: usize = if cfg!(test) { 4 } else { 4096 };

/// The text a Text field's blocks produce. `blocks` is the whole history: every
/// parent a block names is in it. Any order of `blocks` gives the same text.
pub fn replay(blocks: &[TextBlock]) -> Result<String> {
    replay_folding(blocks).map(|(text, _)| text)
}

/// [`replay`], and the most nodes its tree held at once.
pub(super) fn replay_folding(blocks: &[TextBlock]) -> Result<(String, usize)> {
    let history = History::of(blocks, false)?;
    let critical = critical_steps(&history.ancestry);
    let steps = history.order.len();
    let mut tree = Tree::with_placeholder(history.ids, 0);
    let mut folded = String::new();
    let mut folded_chars = 0;
    let mut most = tree.nodes.len();
    for (step, &index) in history.order.iter().enumerate() {
        let mut next_offset = 0;
        for op in &blocks[index].ops {
            tree.apply(op, step, &history.ancestry[step], &mut next_offset)?;
        }
        most = most.max(tree.nodes.len());
        if critical[step] && step + 1 < steps && tree.nodes.len() > FOLD_SLACK + folded_chars / 8 {
            folded = tree.text(&folded)?;
            folded_chars = folded.chars().count();
            tree.fold(folded_chars);
        }
    }
    Ok((tree.text(&folded)?, most))
}

/// Which topological steps hold a critical block: every block before it is
/// among its ancestors, and every block after it has it among theirs.
pub(super) fn critical_steps(ancestry: &[Ancestry]) -> Vec<bool> {
    let steps = ancestry.len();
    let mut critical = vec![false; steps];
    // What every block after the current step has before it: a prefix of the
    // steps, cut down further by the bitmaps of those that saw only some.
    let mut prefix = usize::MAX;
    let mut common: Option<Vec<u64>> = None;
    for step in (0..steps).rev() {
        let later_see_it = step < prefix
            && common
                .as_ref()
                .is_none_or(|bits| bits[step / 64] & (1 << (step % 64)) != 0);
        critical[step] =
            later_see_it && matches!(ancestry[step], Ancestry::Prefix(end) if end == step);
        match &ancestry[step] {
            Ancestry::Prefix(end) => prefix = prefix.min(*end),
            Ancestry::Set(bits) => {
                common = Some(match common.take() {
                    None => bits.clone(),
                    Some(mut held) => {
                        held.truncate(bits.len());
                        for (word, theirs) in held.iter_mut().zip(bits) {
                            *word &= theirs;
                        }
                        held
                    }
                });
            }
        }
    }
    critical
}

impl Tree<'_> {
    /// The text every node shows, deleted ones skipped. Placeholder pieces
    /// read the text of the last fold, `folded`: in reading order they are
    /// its characters in turn, since a cut keeps the pieces in order.
    fn text(&self, folded: &str) -> Result<String> {
        let full = match &self.order {
            Some(order) => order.nodes().collect(),
            None => self.traverse(None),
        };
        let mut rest = folded;
        let mut text = String::with_capacity(folded.len());
        for node in full {
            let n = &self.nodes[node];
            if n.block != BASE {
                if n.deleted_by.is_empty() {
                    text.push(n.ch);
                }
                continue;
            }
            let mut chars = rest.char_indices();
            if chars.nth(n.len - 1).is_none() {
                return Err(folded_mismatch());
            }
            let end = chars.next().map_or(rest.len(), |(at, _)| at);
            let (held, after) = rest.split_at(end);
            if n.deleted_by.is_empty() {
                text.push_str(held);
            }
            rest = after;
        }
        if !rest.is_empty() {
            return Err(folded_mismatch());
        }
        Ok(text)
    }

    /// Throw every node away but the root and one span of `chars` placeholder
    /// characters for the text just folded.
    fn fold(&mut self, chars: usize) {
        self.nodes.truncate(1);
        self.nodes[ROOT].left.clear();
        self.nodes[ROOT].right.clear();
        let mut order = Vec::new();
        if chars > 0 {
            self.nodes.push(blank(BASE, chars));
            self.nodes[ROOT].right.push(1);
            order.push(1);
        }
        self.order = Some(Order::new(order, |_| chars));
    }
}

fn folded_mismatch() -> Error {
    Error::MergeError("a folded Text replay lost track of its text".into())
}
