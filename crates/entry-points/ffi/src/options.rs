//! Reading a caller's options struct, whatever version of the header the
//! caller was built against.
//!
//! Every call that takes options is exported as `<name>_sized(..., opts,
//! opts_size)`: a pointer to the caller's struct and its size as the
//! caller's header defines it. `defra.h` wraps each in a `static inline
//! <name>(..., opts)` that takes the struct by value and passes `&opts` and
//! `sizeof opts`, so the size is always the caller's.
//!
//! A struct only ever grows at its end, and zero is every field's default.
//! So the library reads the bytes the caller has, and treats the rest as
//! zero: an older caller's shorter struct gets defaults for the fields it
//! does not know. A newer caller's longer struct is accepted when the bytes
//! past the library's struct are all zero; a set field this library does not
//! know is refused, never ignored (`proofs/lean/FfiOptions.lean`).
//!
//! That needs every byte of a struct to belong to a field. An options struct
//! has no implicit padding, and its size is a multiple of 8: a gap is filled
//! with a reserved field. Otherwise an appended field could sit in an older
//! struct's tail padding, ignored instead of refused, and the bytes past the
//! library's struct could be padding nobody set.
//!
//! A reserved field never becomes a field. The library reads reserved fields
//! and drops them, so an older library would ignore a caller who set a
//! reserved field that a later version had turned into an option, where the
//! size rule refuses a field it does not know. New fields only ever go after
//! a struct's last field.

use std::mem::{size_of, MaybeUninit};

/// An options struct whose every field is valid, and means its default,
/// when all its bytes are zero.
///
/// # Safety
///
/// The type must be `#[repr(C)]`, and the all-zero bit pattern must be a
/// valid value of every field: raw pointers (null), integers and floats
/// (zero), and `Option` of a function pointer (`None`). No reference,
/// `NonNull`, `bool` with niche assumptions beyond zero, or enum without a
/// zero discriminant.
pub unsafe trait ZeroDefault: Sized + Copy {}

/// The caller's options, as this library's struct: the caller's first
/// `size` bytes, then zero. Null `opts` is every option at its default. The
/// error is the message to answer the call with.
///
/// # Safety
///
/// `opts` must be null, or point to at least `size` readable bytes.
pub unsafe fn read<T>(opts: *const T, size: u32) -> Result<T, String>
where
    T: ZeroDefault,
{
    let mut value = MaybeUninit::<T>::zeroed();
    if opts.is_null() {
        // SAFETY: `ZeroDefault` makes the all-zero bytes a valid `T`.
        return Ok(unsafe { value.assume_init() });
    }
    let given = size as usize;
    let known = size_of::<T>();
    let source = opts.cast::<u8>();
    if given > known {
        // SAFETY: the caller promises `size` readable bytes at `opts`.
        let extra = unsafe { std::slice::from_raw_parts(source.add(known), given - known) };
        if extra.iter().any(|byte| *byte != 0) {
            return Err(format!(
                "the options set fields this library does not know (it reads {known} of \
                 {given} bytes); use a newer library or leave \
                 those fields unset"
            ));
        }
    }
    // SAFETY: at most `size` bytes are read from `opts`, which the caller
    // promises are readable, and at most `size_of::<T>()` are written into
    // `value`; the rest stays zero, which `ZeroDefault` makes valid.
    unsafe {
        std::ptr::copy_nonoverlapping(source, value.as_mut_ptr().cast::<u8>(), given.min(known));
        Ok(value.assume_init())
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
