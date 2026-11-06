#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod acp;
mod backup;
mod collection;
mod collection_version;
mod encryption;
mod explain;
mod export;
mod index;
mod issues;
mod matchers;
mod mutation;
mod net;
mod node;
mod query;
mod searchable_encryption;
mod signature;
mod subscription;
mod txn;
mod view;
