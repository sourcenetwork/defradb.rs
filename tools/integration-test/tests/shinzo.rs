//! Go Shinzo's core data features (#2439). A test named `shinzo_` compares
//! with a Go Shinzo node and also runs under `DEFRA_CLIENT=go-shinzo`; the
//! others configure what only a Rust node can be given.

#[path = "shinzo/purge.rs"]
mod purge;
