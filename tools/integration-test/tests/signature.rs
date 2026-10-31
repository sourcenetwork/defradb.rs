#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod signature {
    mod acp;
    mod acp_branchable;
    mod branchable;
    mod commit;
    mod fallback_signer;
    mod peer;
    mod query;
    mod verify;
}
