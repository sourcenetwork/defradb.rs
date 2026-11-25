use rustc_hir::{Item, ItemKind};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_session::{declare_lint, declare_lint_pass};
use rustc_span::{Span, Symbol};

use crate::diagnostic::Message;

declare_lint! {
    /// Error implementations are derived with `thiserror`.
    pub MANUAL_ERROR_IMPL,
    Warn,
    "a handwritten implementation of the standard error trait"
}

declare_lint! {
    /// Application errors use the crate's typed errors, not anyhow or eyre.
    pub UNTYPED_ERROR,
    Warn,
    "use of anyhow or eyre in workspace source"
}

declare_lint_pass!(Errors => [MANUAL_ERROR_IMPL, UNTYPED_ERROR]);

pub(crate) fn from_thiserror(context: &LateContext<'_>, span: Span) -> bool {
    let mut syntax = span.ctxt();
    while !syntax.is_root() {
        let expansion = syntax.outer_expn_data();
        if expansion.macro_def_id.is_some_and(|definition| {
            context.tcx.crate_name(definition.krate).as_str() == "thiserror_impl"
        }) {
            return true;
        }
        syntax = expansion.call_site.ctxt();
    }
    false
}

impl<'compiler> LateLintPass<'compiler> for Errors {
    fn check_item(&mut self, context: &LateContext<'compiler>, item: &'compiler Item<'compiler>) {
        if crate::source::generated(context, item.span) {
            return;
        }
        let ItemKind::Impl(implementation) = &item.kind else {
            return;
        };
        if implementation
            .of_trait
            .and_then(|header| header.trait_ref.trait_def_id())
            .is_some_and(|definition| {
                context
                    .tcx
                    .is_diagnostic_item(Symbol::intern("Error"), definition)
            })
        {
            context.emit_span_lint(
                MANUAL_ERROR_IMPL,
                item.span.source_callsite(),
                Message {
                    text: "derive this error with `thiserror`".into(),
                    help: "keep its display messages and source chain in the derived error's variants",
                },
            );
        }
    }

    fn check_crate_post(&mut self, context: &LateContext<'compiler>) {
        for &dependency in context.tcx.crates(()) {
            let name = context.tcx.crate_name(dependency);
            if !matches!(name.as_str(), "anyhow" | "eyre" | "color_eyre") {
                continue;
            }
            let Some(imported) = context.tcx.extern_crate(dependency) else {
                continue;
            };
            let span = imported.span.source_callsite();
            if !imported.is_direct() || crate::source::generated(context, span) {
                continue;
            }
            context.emit_span_lint(
                UNTYPED_ERROR,
                span,
                Message {
                    text: format!("use the crate's typed errors instead of `{name}`"),
                    help: "derive the crate's error enum with `thiserror` and preserve concrete error sources",
                },
            );
        }
    }
}
