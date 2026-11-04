//! Remote inputs, made fast: the one mechanism behind every input a write or
//! a read fetches over the network before its transaction runs (KMS keys,
//! remote signatures, remote ACP decisions, embeddings).
//!
//! A [`Service`] wraps one input kind's remote side, its [`Source`], with:
//! - single-flight: one round trip per key in flight; every other task that
//!   asks for the key suspends on the flight's `regolith::sync::Event`;
//! - batching: keys of one group that arrive while a round trip is in flight
//!   ride the next one, up to [`Config::max_batch`];
//! - a cache bounded by bytes ([`Cache`]), where the source allows one, with a
//!   time to live and invalidation that a flight in progress cannot undo;
//! - a deadline on every caller and every round trip, bounded retries with
//!   jittered exponential backoff timed on the given clock, and a cap on round
//!   trips in flight;
//! - metrics ([`Stats`]): requests, cache hits, joined flights, batch sizes,
//!   round trips, retries, latency per round trip and per caller, and round
//!   trips per call.
//!
//! Round trips run on the [`Network`] the service is given, apart from the
//! tasks that wait on them: a waiter that gives up never cancels a round trip
//! other waiters share, and a waiter on any thread or executor suspends on an
//! `Event`, holding no thread.
//!
//! The embedding providers' requests are here too ([`embeddings`]), so the
//! data path that asks for an embedding makes no network call of its own.
//!
//! What must never happen: two round trips in flight for one key; an answer
//! cached after an invalidation that began while it was being fetched; a
//! caller waiting past its deadline; a cache above its byte budget for longer
//! than the insert that crossed it.

mod cache;
mod config;
pub mod embeddings;
mod error;
mod flight;
mod lane;
mod network;
mod retry;
mod service;
mod source;
mod stats;

pub use cache::Cache;
pub use config::Config;
pub use error::Error;
pub use network::{Network, Runtime, RuntimeClock, Stopwatch};
pub use service::Service;
pub use source::{Answers, Source};
pub use stats::{Histogram, Stats};

#[cfg(test)]
mod tests;
