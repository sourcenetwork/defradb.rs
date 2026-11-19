//! The write path: transactional mutations.
#[path = "../common/mod.rs"]
mod common;

mod batch;
mod batch_retry;
mod batch_signing;
mod conflict_reasons;
mod counter_merges;
mod disjoint_parts;
mod doc_suite;
mod embedding;
mod embedding_reload;
mod empty_arrays;
mod identical_create;
mod identical_delta;
mod kms;
mod kms_authorization;
mod kms_request;
mod large_batch;
mod prepare;
mod request_context;
mod same_part_reruns;
mod same_request;
mod size_constraint;
mod unchanged_fields;
mod update;
mod version_key;
