#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

#[path = "backup_scenarios"]
mod backup {
    mod one_to_many {
        mod export;
        mod import;
    }
    mod one_to_one {
        mod export;
        mod import;
    }
    mod self_reference {
        mod export;
        mod import;
    }
    mod simple {
        mod export;
        mod import;
    }
}
