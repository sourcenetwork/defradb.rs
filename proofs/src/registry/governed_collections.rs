use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A node with a merge validator judges every collection that declares
        // a governance root and carries no policy, as well as the ones it
        // claimed by name, so a collection defined after it started is judged
        // too and two such nodes never split. A node with no validator holds
        // what it claimed and merges the rest, as before roots governed
        // anything. A rooted collection with a policy is left to ACP.
        family: "Governed collections",
        name: "INV_JudgedWhereValidating / INV_ValidatorNeverHoldsRooted / INV_NoSplitAmongValidating / INV_NoValidatorAsBefore / INV_UngovernedUntouched / INV_PolicyLeftToAcp",
        axis: Tla,
        anchor: "crates/database/db/src/merge/governance/validator.rs MergeGovernance::governs; crates/database/db/src/merge/governance/judge.rs DbMergeHandler::is_governed, DbMergeHandler::judge_governed",
        model_ref: "MC_GovernedByRoot_{Green,Red_DropRootArm,Red_DropRootArmSplit,Red_DropNameArm,Red_RootWithoutValidator,Red_ValidatorOnlyForNamed,Red_ValidatorWithoutInstall,Red_RootOverPolicy,Red_ClaimAll}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/governance/rooted.rs a_rooted_collection_defined_after_start_is_judged_with_no_claim, a_rooted_collection_merges_what_the_validator_accepts, a_rooted_collection_on_a_node_with_no_validator_merges_as_before, a_rooted_collection_on_a_node_without_governance_merges_as_before, a_claimed_rooted_collection_with_no_validator_is_held_as_before, a_collection_with_no_root_and_no_claim_merges_unjudged, a_rooted_collection_with_a_policy_is_not_judged_unless_claimed, a_claimed_collection_with_a_policy_is_judged_as_before; tools/integration-test/tests/testo/net/governed.rs a_node_with_no_merge_validator_merges_a_governed_collection_as_before, a_merge_validator_judges_a_governed_collection_defined_after_it_started",
    },
    Property {
        // The decision as a function of the definition and of what a node
        // installed: with a validator a declared root governs whatever was
        // claimed, without one (or with no governance) the path is the one
        // from before, and the change only ever adds judgement.
        family: "Governed collections",
        name: "GovernedByRoot.no_governance_is_ungoverned / no_governance_as_before / no_validator_as_before / rooted_with_a_validator_is_judged / validating_nodes_agree_on_rooted / validator_never_holds / claimed_is_governed / claimed_without_a_validator_is_held / rootless_and_unclaimed_is_ungoverned / policied_as_before / judged_before_is_judged_now",
        axis: Lean,
        anchor: "crates/database/db/src/merge/governance/validator.rs MergeGovernance::governs; crates/database/db/src/merge/governance/judge.rs DbMergeHandler::is_governed, DbMergeHandler::judge_governed",
        model_ref: "GovernedByRoot (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/governance/rooted.rs a_rooted_collection_defined_after_start_is_judged_with_no_claim, a_rooted_collection_on_a_node_with_no_validator_merges_as_before, a_rooted_collection_on_a_node_without_governance_merges_as_before, a_claimed_rooted_collection_with_no_validator_is_held_as_before, a_collection_with_no_root_and_no_claim_merges_unjudged, a_rooted_collection_with_a_policy_is_not_judged_unless_claimed, a_claimed_collection_with_a_policy_is_judged_as_before",
    },
];
