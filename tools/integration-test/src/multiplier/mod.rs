mod secondary_index;

use testo::multiplier::{get_matching_multipliers, SkipReason};
use testo::prelude::*;

use crate::actions::Actions;
use crate::test_case::State;

pub use secondary_index::{add_indexes, has_index_directive, SecondaryIndex};

/// A rewrite of a test's actions that `DEFRA_MULTIPLIERS` turns on, as Go's.
/// Names this crate does not know, such as `signed-docs`, are read elsewhere.
#[derive(Clone, Copy, Debug)]
pub enum TestMultiplier {
    SecondaryIndex(SecondaryIndex),
}

impl TestMultiplier {
    pub const ALL: [TestMultiplier; 1] = [Self::SecondaryIndex(SecondaryIndex)];

    /// The multipliers `DEFRA_MULTIPLIERS` names, in the order of [`Self::ALL`].
    pub fn active() -> Vec<TestMultiplier> {
        Self::named(&std::env::var("DEFRA_MULTIPLIERS").unwrap_or_default())
    }

    /// The multipliers a comma-separated list names.
    pub fn named(list: &str) -> Vec<TestMultiplier> {
        let names: Vec<&str> = list.split(',').map(str::trim).collect();
        let all = Self::ALL;
        let possible: Vec<&TestMultiplier> = all.iter().collect();
        get_matching_multipliers::<State, Actions, Actions, TestMultiplier>(&names, &possible)
            .into_iter()
            .copied()
            .collect()
    }
}

impl Multiplier<Actions> for TestMultiplier {
    const NAME: &str = "defra-multiplier";

    fn name(&self) -> &str {
        match self {
            Self::SecondaryIndex(m) => m.name(),
        }
    }

    fn apply(&self, actions: &[ActionInstance<Actions>]) -> Vec<ActionInstance<Actions>> {
        match self {
            Self::SecondaryIndex(m) => m.apply(actions),
        }
    }

    fn should_skip(&self, actions: &[ActionInstance<Actions>]) -> SkipReason {
        match self {
            Self::SecondaryIndex(m) => m.should_skip(actions),
        }
    }
}
