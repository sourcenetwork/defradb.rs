use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "identity";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The DID-binding half (a distinct identity cannot impersonate another)
        // IS observed behaviorally by the ACP test (ungranted bob denied). The
        // signer-binding half — forging a malformed / alg-confused token, or one
        // whose iss/signer DID mismatches its signature — is unreachable through
        // the public CLI (the client always signs correctly), so it is Boundary.
        family: "JWT issuer / algorithm binding",
        name: "INV_TokenBindsGenuineDid — alg/iss/sig bound to did(pubkey)",
        axis: Tla,
        anchor: "crates/building-blocks/identity token verification",
        model_ref: "MC_Jwt_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
