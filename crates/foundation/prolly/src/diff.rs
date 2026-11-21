//! The entries two trees disagree on, found by walking down only where their
//! hashes differ.

use std::{cmp::Ordering, sync::Arc};

use crate::{
    error::Result,
    hash::Hash,
    node::{Item, Node},
    store::Store,
    tree::load,
    tree::Tree,
};

/// One key the two trees disagree on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The key.
    pub key: Vec<u8>,
    /// Its value in the left tree, `None` when the left tree lacks it.
    pub left: Option<Vec<u8>>,
    /// Its value in the right tree, `None` when the right tree lacks it.
    pub right: Option<Vec<u8>>,
}

/// What is left to walk of one tree, in key order: a subtree not opened yet,
/// or an entry. A root keeps the node read for its level, so opening it reads
/// nothing more.
enum Elem {
    Subtree {
        hash: Hash,
        level: u8,
        node: Option<Arc<Node>>,
    },
    Entry(Item),
}

/// One tree's walk: the next element is on top of the stack.
struct Walk<'a, S> {
    store: &'a S,
    stack: Vec<Elem>,
}

impl<'a, S> Walk<'a, S>
where
    S: Store,
{
    fn new(store: &'a S, tree: &Tree) -> Result<Self> {
        let mut stack = Vec::new();
        if let Some(hash) = tree.root {
            let node = load(store, &hash)?;
            stack.push(Elem::Subtree {
                hash,
                level: node.level,
                node: Some(node),
            });
        }
        Ok(Self { store, stack })
    }

    /// Replaces the subtree on top with its children.
    fn open(&mut self) -> Result<()> {
        let Some(Elem::Subtree { hash, node, .. }) = self.stack.pop() else {
            return Ok(());
        };
        let node = match node {
            Some(node) => node,
            None => load(self.store, &hash)?,
        };
        for item in node.items.iter().rev() {
            self.stack.push(match node.level {
                0 => Elem::Entry(item.clone()),
                level => Elem::Subtree {
                    hash: Hash::from_slice(&item.value)?,
                    level: level - 1,
                    node: None,
                },
            });
        }
        Ok(())
    }

    /// Takes the entry on top.
    fn take(&mut self) -> Option<Item> {
        match self.stack.pop() {
            Some(Elem::Entry(item)) => Some(item),
            _ => None,
        }
    }
}

/// What the walk does next.
enum Step {
    Done,
    SkipBoth,
    Open { left: bool, right: bool },
    Take(Ordering),
}

/// The keys where `left` and `right` differ, in key order. Subtrees with the
/// same hash at the same place in both walks hold the same entries and are
/// skipped unopened, so the nodes read grow with the difference, not with the
/// trees. Equal roots return at once, without reading either store.
pub fn diff<A, B>(
    left_store: &A,
    left: &Tree,
    right_store: &B,
    right: &Tree,
) -> Result<Vec<Difference>>
where
    A: Store,
    B: Store,
{
    if left.root == right.root {
        return Ok(Vec::new());
    }
    let mut lhs = Walk::new(left_store, left)?;
    let mut rhs = Walk::new(right_store, right)?;
    let mut differences = Vec::new();
    loop {
        let step = match (lhs.stack.last(), rhs.stack.last()) {
            (None, None) => Step::Done,
            (
                Some(Elem::Subtree {
                    hash: a, level: la, ..
                }),
                Some(Elem::Subtree {
                    hash: b, level: lb, ..
                }),
            ) => {
                if a == b {
                    Step::SkipBoth
                } else {
                    Step::Open {
                        left: la >= lb,
                        right: lb >= la,
                    }
                }
            }
            (Some(Elem::Subtree { .. }), _) => Step::Open {
                left: true,
                right: false,
            },
            (_, Some(Elem::Subtree { .. })) => Step::Open {
                left: false,
                right: true,
            },
            (Some(Elem::Entry(a)), Some(Elem::Entry(b))) => Step::Take(a.key.cmp(&b.key)),
            (Some(Elem::Entry(_)), None) => Step::Take(Ordering::Less),
            (None, Some(Elem::Entry(_))) => Step::Take(Ordering::Greater),
        };
        match step {
            Step::Done => return Ok(differences),
            Step::SkipBoth => {
                lhs.stack.pop();
                rhs.stack.pop();
            }
            Step::Open { left, right } => {
                if left {
                    lhs.open()?;
                }
                if right {
                    rhs.open()?;
                }
            }
            Step::Take(Ordering::Less) => differences.extend(lhs.take().map(|item| Difference {
                key: item.key,
                left: Some(item.value),
                right: None,
            })),
            Step::Take(Ordering::Greater) => {
                differences.extend(rhs.take().map(|item| Difference {
                    key: item.key,
                    left: None,
                    right: Some(item.value),
                }))
            }
            Step::Take(Ordering::Equal) => {
                if let (Some(a), Some(b)) = (lhs.take(), rhs.take()) {
                    if a.value != b.value {
                        differences.push(Difference {
                            key: a.key,
                            left: Some(a.value),
                            right: Some(b.value),
                        });
                    }
                }
            }
        }
    }
}
