//! `select!`: waits on several futures at once and runs the handler of the
//! first one to finish whose output matches its pattern.
//!
//! Follows nuclei 0.5's `select!` (`src/macros/select.rs` and `mod.rs`) with
//! the starting branch of a fair select rotating per call instead of drawn
//! from nuclei's random source, and without its cooperative budget. The
//! futures sit pinned in a tuple and are polled from one `poll_fn`; a future
//! that finishes with an output its pattern rejects disables its branch and the
//! others go on. Handlers run after the futures are dropped, so `return`, `?`,
//! `break` and `continue` in a handler act on the code around the `select!`.
//!
//! The macro is exported at the crate root as `local_select`; `select` here
//! is the name `crate::rt` gives it on wasm32.

/// What the expansions name; not a public interface.
#[doc(hidden)]
pub mod support {
    use std::cell::Cell;

    pub use std::future::poll_fn;

    /// The output of one branch, nested once per branch before it: branch `k`
    /// is `Next` `k` times around `Taken`, and the value with every branch
    /// disabled is `Next` once per branch around [`AllDisabled`].
    pub enum Branch<T, R> {
        Taken(T),
        Next(R),
    }

    /// Every branch was disabled.
    pub struct AllDisabled;

    thread_local! {
        static ROTATION: Cell<u32> = const { Cell::new(0) };
    }

    /// The branch a fair select polls first: a counter that rotates.
    pub fn start(branches: u32) -> u32 {
        ROTATION.with(|rotation| {
            let turn = rotation.get();
            rotation.set(turn.wrapping_add(1));
            turn % branches
        })
    }
}

/// Waits on several branches at once and runs the handler of the first to
/// finish, dropping the others.
///
/// Each branch is `<pattern> = <future> (, if <precondition>)? => <handler>`,
/// and an `else => <expression>` branch may come last. A branch whose
/// precondition is false is disabled; a branch that finishes is disabled, and
/// its handler runs if the output matches the pattern. Polling starts at a
/// rotating branch, or in written order after `biased;`. With every branch
/// disabled the `else` expression is the value.
///
/// # Panics
///
/// When every branch is disabled and there is no `else` branch.
#[macro_export]
macro_rules! local_select {
    (biased;) => {
        ::core::compile_error!("`select!` needs at least one branch")
    };
    (biased; $($branches:tt)+) => {
        $crate::__local_select!(@branch [biased] [] [] $($branches)+)
    };
    () => {
        ::core::compile_error!("`select!` needs at least one branch")
    };
    ($($branches:tt)+) => {
        $crate::__local_select!(@branch [fair] [] [] $($branches)+)
    };
}

/// The branch muncher of [`local_select!`]. The state is
/// `[mode] [parsed branches] [one _ per parsed branch]`; a parsed branch is
/// `([its _ list] [its pattern] future, precondition => handler)`.
#[doc(hidden)]
#[macro_export]
macro_rules! __local_select {
    (@branch $mode:tt [$($done:tt)*] [$($all:tt)*] else => $else:expr $(,)?) => {
        $crate::__local_select!(@emit $mode [$($done)*] [$($all)*] [$else])
    };
    (@branch $mode:tt [$($done:tt)*] [$($all:tt)*]) => {
        $crate::__local_select!(
            @emit $mode [$($done)*] [$($all)*]
            [::core::panic!("all branches of `select!` are disabled and there is no `else` branch")]
        )
    };
    (@branch $mode:tt [$($done:tt)*] [$($all:tt)*] $($rest:tt)+) => {
        $crate::__local_select!(@pat $mode [$($done)*] [$($all)*] [] $($rest)+)
    };

    (@pat $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)+] = $future:expr, if $cond:expr => $($rest:tt)+) => {
        $crate::__local_select!(@handler $mode [$($done)*] [$($all)*] [$($pat)+] $future, $cond => $($rest)+)
    };
    (@pat $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)+] = $future:expr => $($rest:tt)+) => {
        $crate::__local_select!(@handler $mode [$($done)*] [$($all)*] [$($pat)+] $future, true => $($rest)+)
    };
    (@pat $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)*] $next:tt $($rest:tt)*) => {
        $crate::__local_select!(@pat $mode [$($done)*] [$($all)*] [$($pat)* $next] $($rest)*)
    };

    (@handler $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)+] $future:expr, $cond:expr => $handler:block, $($rest:tt)*) => {
        $crate::__local_select!(@branch $mode [$($done)* ([$($all)*] [$($pat)+] $future, $cond => $handler)] [$($all)* _] $($rest)*)
    };
    (@handler $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)+] $future:expr, $cond:expr => $handler:block $($rest:tt)*) => {
        $crate::__local_select!(@branch $mode [$($done)* ([$($all)*] [$($pat)+] $future, $cond => $handler)] [$($all)* _] $($rest)*)
    };
    (@handler $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)+] $future:expr, $cond:expr => $handler:expr, $($rest:tt)*) => {
        $crate::__local_select!(@branch $mode [$($done)* ([$($all)*] [$($pat)+] $future, $cond => { $handler })] [$($all)* _] $($rest)*)
    };
    (@handler $mode:tt [$($done:tt)*] [$($all:tt)*] [$($pat:tt)+] $future:expr, $cond:expr => $handler:expr) => {
        $crate::__local_select!(@branch $mode [$($done)* ([$($all)*] [$($pat)+] $future, $cond => { $handler })] [$($all)* _])
    };

    (@emit [$mode:ident]
        [$( ([$($skip:tt)*] [$($pat:tt)+] $future:expr, $cond:expr => $handler:tt) )+]
        [$($all:tt)*]
        [$else:expr]
    ) => {{
        const _: () = ::core::assert!(
            $crate::__local_count!($($all)*) <= 64,
            "`select!` takes at most 64 branches"
        );
        // The futures live in this block only: they are dropped before the
        // handler runs, so the handler may use what they borrowed.
        let __output = {
            let mut __disabled: u64 = 0;
            $(
                if !$cond {
                    __disabled |= 1u64 << $crate::__local_count!($($skip)*);
                }
            )+
            let mut __futures = ($(
                ::core::pin::pin!(::core::future::IntoFuture::into_future($future)),
            )+);
            $crate::rt::local::select::support::poll_fn(|__cx: &mut ::core::task::Context<'_>| {
                let __branches: u32 = $crate::__local_count!($($all)*);
                let __start: u32 = $crate::__local_select_start!($mode, __branches);
                let mut __pending = false;
                for __turn in 0..__branches {
                    let __branch = (__start + __turn) % __branches;
                    $(
                        if __branch == $crate::__local_count!($($skip)*) {
                            if __disabled & (1u64 << __branch) == 0 {
                                let ($($skip,)* __future, ..) = &mut __futures;
                                match ::core::future::Future::poll(::core::pin::Pin::as_mut(__future), __cx) {
                                    ::core::task::Poll::Ready(__value) => {
                                        __disabled |= 1u64 << __branch;
                                        let __matches = match &__value {
                                            #[allow(unused_variables)]
                                            $crate::__local_select_clean!($($pat)+) => true,
                                            #[allow(unreachable_patterns)]
                                            _ => false,
                                        };
                                        if __matches {
                                            return ::core::task::Poll::Ready($crate::__local_select_wrap!(
                                                [$($skip)*]
                                                $crate::rt::local::select::support::Branch::Taken(__value)
                                            ));
                                        }
                                    }
                                    ::core::task::Poll::Pending => __pending = true,
                                }
                            }
                            continue;
                        }
                    )+
                }
                if __pending {
                    ::core::task::Poll::Pending
                } else {
                    ::core::task::Poll::Ready($crate::__local_select_wrap!(
                        [$($all)*]
                        $crate::rt::local::select::support::AllDisabled
                    ))
                }
            })
            .await
        };
        match __output {
            $(
                $crate::__local_select_wrap!(
                    [$($skip)*]
                    $crate::rt::local::select::support::Branch::Taken($($pat)+)
                ) => $handler,
            )+
            $crate::__local_select_wrap!([$($all)*] $crate::rt::local::select::support::AllDisabled) => $else,
            // An output reaches here only through a pattern it matched.
            #[allow(unreachable_patterns)]
            _ => ::core::unreachable!("a `select!` output its pattern rejected"),
        }
    }};
}

/// The first branch a select polls.
#[doc(hidden)]
#[macro_export]
macro_rules! __local_select_start {
    (biased, $branches:expr) => {
        0
    };
    (fair, $branches:expr) => {
        $crate::rt::local::select::support::start($branches)
    };
}

/// Wraps a value or a pattern in one `Branch::Next` per `_`.
#[doc(hidden)]
#[macro_export]
macro_rules! __local_select_wrap {
    ([] $($inner:tt)+) => {
        $($inner)+
    };
    ([_ $($skip:tt)*] $($inner:tt)+) => {
        $crate::rt::local::select::support::Branch::Next($crate::__local_select_wrap!([$($skip)*] $($inner)+))
    };
}

/// The number of `_` tokens given, as a `u32` constant expression.
#[doc(hidden)]
#[macro_export]
macro_rules! __local_count {
    ($($skip:tt)*) => {
        (<[()]>::len(&[$($crate::__local_unit!($skip)),*]) as u32)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __local_unit {
    ($skip:tt) => {
        ()
    };
}

/// A branch's pattern as a test against a reference to the output: the same
/// pattern with its `ref`, `mut` and `&` taken out, so its bindings borrow
/// and match ergonomics apply in every edition. The state is
/// `@ [cleaned so far] [enclosing groups]`.
#[doc(hidden)]
#[macro_export]
macro_rules! __local_select_clean {
    (@ [$($out:tt)*] []) => {
        $($out)*
    };
    (@ [$($inner:tt)*] [(paren [$($out:tt)*] [$($rest:tt)*]) $($stack:tt)*]) => {
        $crate::__local_select_clean!(@ [$($out)* ($($inner)*)] [$($stack)*] $($rest)*)
    };
    (@ [$($inner:tt)*] [(bracket [$($out:tt)*] [$($rest:tt)*]) $($stack:tt)*]) => {
        $crate::__local_select_clean!(@ [$($out)* [$($inner)*]] [$($stack)*] $($rest)*)
    };
    (@ [$($inner:tt)*] [(brace [$($out:tt)*] [$($rest:tt)*]) $($stack:tt)*]) => {
        $crate::__local_select_clean!(@ [$($out)* {$($inner)*}] [$($stack)*] $($rest)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] ref $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [$($out)*] [$($stack)*] $($rest)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] mut $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [$($out)*] [$($stack)*] $($rest)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] & $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [$($out)*] [$($stack)*] $($rest)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] && $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [$($out)*] [$($stack)*] $($rest)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] ($($group:tt)*) $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [] [(paren [$($out)*] [$($rest)*]) $($stack)*] $($group)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] [$($group:tt)*] $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [] [(bracket [$($out)*] [$($rest)*]) $($stack)*] $($group)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] {$($group:tt)*} $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [] [(brace [$($out)*] [$($rest)*]) $($stack)*] $($group)*)
    };
    (@ [$($out:tt)*] [$($stack:tt)*] $next:tt $($rest:tt)*) => {
        $crate::__local_select_clean!(@ [$($out)* $next] [$($stack)*] $($rest)*)
    };
    ($($pat:tt)+) => {
        $crate::__local_select_clean!(@ [] [] $($pat)+)
    };
}
