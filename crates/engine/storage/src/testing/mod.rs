//! Fault injection for tests that drive a real store: an [`Env`](regolith::Env)
//! whose write-ahead log fails on command. Behind the non-default `testing`
//! feature, so no default build links it.

mod fault_env;

pub use fault_env::{Fault, FaultEnv};
