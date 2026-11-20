mod sdl;
#[cfg(test)]
mod tests;

use testo::multiplier::SkipReason;
use testo::prelude::*;

use crate::actions::{Actions, AddCollection, Parallel};
use crate::scenario::Text;

pub use sdl::{add_indexes, has_index_directive};

/// Go's `secondary-index` multiplier: every collection gets an index on each
/// field it can, and every query must return what it did without them.
#[derive(Clone, Copy, Debug, Default)]
pub struct SecondaryIndex;

impl SecondaryIndex {
    pub const NAME: &str = "secondary-index";

    /// Why the test cannot run with the added indexes, if it cannot: it
    /// manages indexes itself, or explains a plan the indexes change.
    pub fn skip_reason(actions: &[Actions]) -> Option<&'static str> {
        actions.iter().find_map(|action| {
            let action = action.base().unwrap_or_else(|| action.clone());
            match &action {
                Actions::NewIndex(_) | Actions::DeleteIndex(_) | Actions::ListIndexes(_) => {
                    Some("the test manages indexes itself")
                }
                Actions::Request(request) if request.request.as_str().contains("@explain") => {
                    Some("the test explains a plan the indexes change")
                }
                Actions::AddCollection(add) if has_index_directive(&add.sdl.literal()) => {
                    Some("the test's SDL declares indexes")
                }
                Actions::Parallel(group) => Self::skip_reason(&group.actions),
                _ => None,
            }
        })
    }

    pub fn rewrite(action: &Actions) -> Actions {
        match action {
            Actions::AddCollection(add) => Actions::AddCollection(with_indexes(add)),
            Actions::Parallel(group) => Actions::Parallel(Parallel {
                actions: group.actions.iter().map(Self::rewrite).collect(),
                ..group.clone()
            }),
            other => other.clone(),
        }
    }
}

/// `add` with every field indexed. An SDL that names captures is marked, and
/// indexed once they resolve.
pub(crate) fn with_indexes(add: &AddCollection) -> AddCollection {
    let Text::Plain(sdl) = &add.sdl else {
        return AddCollection {
            index_every_field: true,
            ..add.clone()
        };
    };
    if has_index_directive(sdl) {
        return AddCollection {
            index_every_field: false,
            ..add.clone()
        };
    }
    let indexed = add_indexes(sdl);
    if indexed != *sdl {
        eprintln!("secondary-index multiplier SDL:\n{indexed}");
    }
    AddCollection {
        sdl: Text::Plain(indexed),
        index_every_field: false,
        ..add.clone()
    }
}

impl Multiplier<Actions> for SecondaryIndex {
    const NAME: &str = SecondaryIndex::NAME;

    fn apply(&self, actions: &[ActionInstance<Actions>]) -> Vec<ActionInstance<Actions>> {
        actions
            .iter()
            .map(|instance| match instance {
                ActionInstance::Sync(action) => ActionInstance::Sync(Self::rewrite(action)),
                ActionInstance::Async(action) => ActionInstance::Async(Self::rewrite(action)),
            })
            .collect()
    }

    fn should_skip(&self, actions: &[ActionInstance<Actions>]) -> SkipReason {
        let actions: Vec<Actions> = actions
            .iter()
            .map(|instance| match instance {
                ActionInstance::Sync(action) | ActionInstance::Async(action) => action.clone(),
            })
            .collect();
        match Self::skip_reason(&actions) {
            Some(reason) => SkipReason::Skip(format!("{}: {reason}", Self::NAME)),
            None => SkipReason::NoSkip,
        }
    }
}
