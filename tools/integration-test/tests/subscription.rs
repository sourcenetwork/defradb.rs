#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod subscription {
    mod subscription;
    mod with_commit;
}
