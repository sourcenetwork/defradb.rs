pub(super) struct ClientWaves {
    children: Vec<Vec<u8>>,
    child_signatures: Vec<Vec<u8>>,
    composite: Vec<u8>,
    composite_signature: Vec<u8>,
    phase: Phase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Children,
    Composite,
    Complete,
}

impl ClientWaves {
    pub(super) fn new(
        children: Vec<Vec<u8>>,
        child_signatures: Vec<Vec<u8>>,
        composite: Vec<u8>,
        composite_signature: Vec<u8>,
    ) -> Self {
        let phase = if children.is_empty() {
            Phase::Composite
        } else {
            Phase::Children
        };
        Self {
            children,
            child_signatures,
            composite,
            composite_signature,
            phase,
        }
    }

    pub(super) fn complete() -> Self {
        Self {
            children: Vec::new(),
            child_signatures: Vec::new(),
            composite: Vec::new(),
            composite_signature: Vec::new(),
            phase: Phase::Complete,
        }
    }

    pub(super) fn is_complete(&self) -> bool {
        self.phase == Phase::Complete
    }
    pub(super) fn children(&self) -> &[Vec<u8>] {
        &self.children
    }
    pub(super) fn composite(&self) -> &[u8] {
        &self.composite
    }
    pub(super) fn child_signatures(&self) -> &[Vec<u8>] {
        &self.child_signatures
    }
    pub(super) fn composite_signature(&self) -> &[u8] {
        &self.composite_signature
    }

    pub(super) fn respond(&mut self, wave: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
        if self.phase == Phase::Composite {
            if wave.len() != 1 || wave[0] != self.composite {
                return None;
            }
            self.phase = Phase::Complete;
            return Some(vec![self.composite_signature.clone()]);
        }
        if self.phase != Phase::Children || wave.len() != self.children.len() {
            return None;
        }
        let mut used = vec![false; self.children.len()];
        let mut result = Vec::with_capacity(wave.len());
        for requested in wave {
            let index = self
                .children
                .iter()
                .enumerate()
                .find(|(index, expected)| !used[*index] && *expected == requested)
                .map(|(index, _)| index)?;
            used[index] = true;
            result.push(self.child_signatures[index].clone());
        }
        self.phase = Phase::Composite;
        Some(result)
    }
}
