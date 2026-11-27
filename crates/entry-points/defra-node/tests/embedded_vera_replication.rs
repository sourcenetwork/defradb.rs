//! Replicated documents between DefraDB nodes that run embedded Vera
//! deployments, over iroh on localhost: the receiving node registers a
//! foreign creator's document, parks the changes its provider refuses, and
//! applies each of them once when a grant lets it; a revocation on one node
//! of a shared deployment stops the revoked writer on another; a node of
//! another account decides the owner's policy against the owner's
//! deployment.

#[path = "embedded_vera_replication/actor.rs"]
mod actor;
#[path = "embedded_vera_replication/assembly.rs"]
mod assembly;
#[path = "embedded_vera_replication/documents.rs"]
mod documents;
#[path = "embedded_vera_replication/evidence.rs"]
mod evidence;
#[path = "embedded_vera_replication/network.rs"]
mod network;
#[path = "embedded_vera_replication/peer.rs"]
mod peer;
#[path = "embedded_vera_replication/wait.rs"]
mod wait;

#[path = "embedded_vera_replication/registration.rs"]
mod registration;

#[path = "embedded_vera_replication/parked.rs"]
mod parked;

#[path = "embedded_vera_replication/refusals.rs"]
mod refusals;

#[path = "embedded_vera_replication/burst.rs"]
mod burst;

#[path = "embedded_vera_replication/revocation.rs"]
mod revocation;

#[path = "embedded_vera_replication/addressed.rs"]
mod addressed;

#[path = "embedded_vera_replication/foreign.rs"]
mod foreign;

#[path = "embedded_vera_replication/foreign_reads.rs"]
mod foreign_reads;
