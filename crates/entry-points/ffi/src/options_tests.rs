//! The size-prefix rule: a caller's struct is read up to its size, the rest
//! is zero, and bytes past the library's struct must be zero.

use std::mem::size_of;

use super::{read, ZeroDefault};

/// A struct as an older header defined it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct V1 {
    a: u64,
    b: u64,
}

/// The same struct after a field was appended.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct V2 {
    a: u64,
    b: u64,
    c: u64,
}

// SAFETY: integers only; all-zero is valid.
unsafe impl ZeroDefault for V1 {}
// SAFETY: integers only; all-zero is valid.
unsafe impl ZeroDefault for V2 {}

#[test]
fn null_options_are_every_default() {
    let read = unsafe { read::<V2>(std::ptr::null(), 0) }.unwrap();
    assert_eq!(read, V2 { a: 0, b: 0, c: 0 });
}

#[test]
fn an_older_callers_shorter_struct_gets_defaults_for_the_new_fields() {
    let old = V1 { a: 7, b: 9 };
    let size = size_of::<V1>() as u32;
    let read = unsafe { read::<V2>((&old as *const V1).cast(), size) }.unwrap();
    assert_eq!(read, V2 { a: 7, b: 9, c: 0 });
}

#[test]
fn a_struct_of_the_libraries_size_is_read_whole() {
    let given = V2 { a: 1, b: 2, c: 3 };
    let read = unsafe { read::<V2>(&given, size_of::<V2>() as u32) }.unwrap();
    assert_eq!(read, given);
}

#[test]
fn a_newer_callers_longer_struct_is_read_when_its_new_fields_are_unset() {
    let newer = V2 { a: 4, b: 5, c: 0 };
    let read = unsafe { read::<V1>((&newer as *const V2).cast(), size_of::<V2>() as u32) }.unwrap();
    assert_eq!(read, V1 { a: 4, b: 5 });
}

#[test]
fn a_set_field_the_library_does_not_know_is_refused() {
    let newer = V2 { a: 4, b: 5, c: 6 };
    let error =
        unsafe { read::<V1>((&newer as *const V2).cast(), size_of::<V2>() as u32) }.unwrap_err();
    assert!(
        error.contains("fields this library does not know"),
        "{error}"
    );
}

#[test]
fn a_size_that_stops_inside_a_field_is_refused() {
    let given = V2 {
        a: u64::MAX,
        b: u64::MAX,
        c: u64::MAX,
    };
    for size in [1, 4, 12, 20, 23] {
        let error = unsafe { read::<V2>(&given, size) }.unwrap_err();
        assert!(error.contains("ends inside a field"), "{size}: {error}");
    }
    for size in [0, 8, 16, 24] {
        assert!(unsafe { read::<V2>(&given, size) }.is_ok(), "{size}");
    }
}
