//! The C API's extensible entry points.
//!
//! Each call that takes options is exported as `defra_<name>_sized`, taking
//! its options struct by pointer and the struct's size as the caller's
//! header defines it. `defra.h` defines `defra_<name>` as a `static inline`
//! wrapper that takes the struct by value and passes both, so callers never
//! see a size. The options are read by [`crate::options::read`]; each entry
//! point then runs the call through the function that does the work.

use std::ffi::{c_char, c_int, CString};

use crate::types::{CollectionOptions, DefraResult};

pub mod acp;
pub mod collection;
pub mod document;
pub mod identity;
pub mod index;
pub mod node;
pub mod p2p;
pub mod query;
pub mod schema;

#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod sized_tests;

/// Declares that the all-zero bytes of each options struct are its
/// defaults, which [`crate::options::read`] relies on.
macro_rules! zero_default {
    ($($options:ty),+ $(,)?) => {
        $(
            // SAFETY: `#[repr(C)]`, with only raw pointers, integers, floats
            // and optional function pointers, all valid as zero.
            unsafe impl $crate::options::ZeroDefault for $options {}
        )+
    };
}
pub(crate) use zero_default;

/// Reads the caller's options, or answers the call with the error, as the
/// result type `$result` builds it from an error message. Used inside the
/// `unsafe` entry points, whose callers promise `$opts` is readable.
macro_rules! options {
    ($opts:expr, $size:expr) => {
        $crate::api::options!($opts, $size, $crate::types::DefraResult::error)
    };
    ($opts:expr, $size:expr, $error:path) => {
        match $crate::options::read($opts, $size) {
            Ok(options) => options,
            Err(message) => return $error(message),
        }
    };
}
pub(crate) use options;

/// Choosing a collection, as the flat fields of an options struct give it,
/// with the call's signing choice.
pub(crate) fn collection(
    name: *const c_char,
    version_id: *const c_char,
    collection_id: *const c_char,
    get_inactive: c_int,
    signing: c_int,
) -> CollectionOptions {
    CollectionOptions {
        version: version_id,
        collection_id,
        name,
        get_inactive,
        enable_signing: signing,
    }
}

/// A timeout in milliseconds as the Go duration string the sync calls
/// parse; zero is none, and a negative one is refused.
pub(crate) fn timeout(milliseconds: i64) -> Result<Option<CString>, DefraResult> {
    match milliseconds {
        0 => Ok(None),
        ms if ms < 0 => Err(DefraResult::error(format!(
            "timeout_ms must not be negative, got {ms}"
        ))),
        ms => Ok(Some(
            CString::new(format!("{ms}ms")).expect("a number has no NUL"),
        )),
    }
}

/// `value`'s C string, or null.
pub(crate) fn c_ptr(value: &Option<CString>) -> *const c_char {
    value
        .as_ref()
        .map_or(std::ptr::null(), |value| value.as_ptr())
}
