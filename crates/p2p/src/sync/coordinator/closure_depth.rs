use crate::sync::car::CAR_MAX_BLOCKS;

/// A new head, the block it links, and their field blocks: a steady-state head
/// hint is complete within this cut.
const INITIAL_CLOSURE_DEPTH: u32 = 2;

/// The responder stops at [`CAR_MAX_BLOCKS`] blocks, so a deeper cut serves no more.
const MAX_CLOSURE_DEPTH: u32 = CAR_MAX_BLOCKS as u32;

/// How far below the missing frontier a DAG fetch asks a provider to walk.
/// It doubles each round that leaves a frontier, so a receiver with nothing
/// reaches any depth in logarithmically many rounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClosureDepth(u32);

impl ClosureDepth {
    pub(crate) fn initial() -> Self {
        Self(INITIAL_CLOSURE_DEPTH)
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }

    pub(crate) fn widen(&mut self) {
        self.0 = self.0.saturating_mul(2).min(MAX_CLOSURE_DEPTH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening_doubles_up_to_the_response_cap() {
        let mut depth = ClosureDepth::initial();
        let mut seen = vec![depth.get()];
        while depth.get() < MAX_CLOSURE_DEPTH {
            depth.widen();
            seen.push(depth.get());
        }
        depth.widen();
        assert_eq!(depth.get(), MAX_CLOSURE_DEPTH);
        assert_eq!(seen[..3], [INITIAL_CLOSURE_DEPTH, 4, 8]);
        assert!(seen
            .windows(2)
            .all(|w| w[1] == (w[0] * 2).min(MAX_CLOSURE_DEPTH)));
    }
}
