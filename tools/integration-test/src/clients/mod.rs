//! An action's version for each client that is not in-process: [`Http`] and
//! [`Cli`] wrap the action and run the body that client has for it.

mod cli;
mod http;

pub use cli::{Cli, CliRun};
pub use http::{Http, HttpRun};
