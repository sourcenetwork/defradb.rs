use identity::Did;

/// The owner of a known live document visible to this transaction's actor.
/// `None` cannot establish ownership, absence or inventory completeness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOwnerObservation {
    pub doc_id: String,
    /// The normal read path returned this live document for the chosen actor.
    /// False does not distinguish absence from read denial. True does not
    /// establish registered ownership or authorize any related document.
    pub visible: bool,
    pub owner: Option<Did>,
}

/// Bound one transaction-owner lookup independently of document contents.
pub const MAX_OWNER_OBSERVATIONS: usize = 64;
