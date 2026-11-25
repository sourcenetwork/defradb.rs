//! Compiler lints for the workspace's code-shape agreements.

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_hir_analysis;
extern crate rustc_infer;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_trait_selection;

mod bounds;
mod callbacks;
mod diagnostic;
mod error_types;
mod errors;
mod forwarded_errors;
mod results;
mod return_types;
mod source;
mod written_results;

use rustc_lint::LintStore;
use rustc_session::Session;
use rustc_session::lint::LintId;

dylint_linting::dylint_library!();

/// Register the code-shape diagnostics with Dylint's compiler driver.
#[unsafe(no_mangle)]
pub fn register_lints(_session: &Session, store: &mut LintStore) {
    store.register_lints(&[
        bounds::INLINE_GENERIC_BOUNDS,
        callbacks::ANONYMOUS_CALLBACK,
        callbacks::CALLBACK_LIFETIME_NAME,
        errors::MANUAL_ERROR_IMPL,
        errors::UNTYPED_ERROR,
        error_types::ERASED_ERROR_SIGNATURE,
        results::CRATE_RESULT,
        results::TEST_RESULT,
    ]);
    store.register_group(
        true,
        "defra_code_shape",
        None,
        vec![
            LintId::of(bounds::INLINE_GENERIC_BOUNDS),
            LintId::of(callbacks::ANONYMOUS_CALLBACK),
            LintId::of(callbacks::CALLBACK_LIFETIME_NAME),
            LintId::of(errors::MANUAL_ERROR_IMPL),
            LintId::of(errors::UNTYPED_ERROR),
            LintId::of(error_types::ERASED_ERROR_SIGNATURE),
            LintId::of(results::CRATE_RESULT),
            LintId::of(results::TEST_RESULT),
        ],
    );
    store.register_early_lint_pass(Box::new(|| Box::new(bounds::Bounds)));
    store.register_late_lint_pass(Box::new(|_| Box::new(callbacks::Callbacks)));
    store.register_late_lint_pass(Box::new(|_| Box::new(errors::Errors)));
    store.register_late_lint_pass(Box::new(|_| Box::new(error_types::ErrorTypes::default())));
    store.register_late_lint_pass(Box::new(|_| Box::new(results::CrateResults::default())));
}

#[test]
fn ui() {
    dylint_testing::ui::Test::src_base(env!("CARGO_PKG_NAME"), "ui")
        .rustc_flags(["--edition=2021"])
        .run();
    dylint_testing::ui_test_example(env!("CARGO_PKG_NAME"), "derived");
    dylint_testing::ui::Test::example(env!("CARGO_PKG_NAME"), "review")
        .rustc_flags(["--test", "--cfg=code_shape_ui"])
        .run();
    dylint_testing::ui::Test::example(env!("CARGO_PKG_NAME"), "async_results")
        .rustc_flags(["--crate-type=lib", "--cfg=code_shape_ui"])
        .run();
    dylint_testing::ui::Test::example(env!("CARGO_PKG_NAME"), "results")
        .rustc_flags([
            "--crate-type=lib",
            "-Aasync_fn_in_trait",
            "--cfg=code_shape_ui",
        ])
        .run();
    dylint_testing::ui::Test::example(env!("CARGO_PKG_NAME"), "results")
        .rustc_flags(["--test", "-Aasync_fn_in_trait", "--cfg=code_shape_ui"])
        .run();
    dylint_testing::ui::Test::example(env!("CARGO_PKG_NAME"), "reordered_results")
        .rustc_flags(["--crate-type=lib", "--cfg=code_shape_ui"])
        .run();
    source::assert_generated_scope();
}
