use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Cursor tokens (#2481): a token is {d, k}, and an older token with an
        // empty `o` still decodes. Tokens are pinned byte for byte by
        // crates/foundation/cursor/tests/go_fixtures.rs; no conformance-binary test
        // reaches them. Boundary.
        family: "Cursor tokens",
        name: "encode_has_no_o / decode_encode / decode_legacy_o: a token is {d, k}",
        axis: Lean,
        anchor: "crates/foundation/cursor/src/lib.rs Cursor::encode, Cursor::decode",
        model_ref: "CursorToken (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
