//! The Postgres wire protocol on nuclei sockets: pgwire's messages and
//! handlers, driven by a connection of our own.
//!
//! pgwire's own connection loop runs only on tokio sockets; its handlers
//! take any client type, so our connection carries the framing and the
//! client state they need, and [`serve`] follows pgwire's state machine
//! (`pgwire::tokio::server::process_message` and `process_error`, 0.38).

mod connection;
mod process;

pub use process::serve;
