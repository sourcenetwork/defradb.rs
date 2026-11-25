use rustc_ast::{GenericBound, GenericParamKind, Generics};
use rustc_lint::{EarlyContext, EarlyLintPass, LintContext};
use rustc_session::{declare_lint, declare_lint_pass};

use crate::diagnostic::Message;

declare_lint! {
    /// Trait bounds on type parameters belong in a `where` clause.
    pub INLINE_GENERIC_BOUNDS,
    Warn,
    "trait bounds inside a generic parameter list"
}

declare_lint_pass!(Bounds => [INLINE_GENERIC_BOUNDS]);

impl EarlyLintPass for Bounds {
    fn check_generics(&mut self, context: &EarlyContext<'_>, generics: &Generics) {
        for parameter in &generics.params {
            if crate::source::generated(context, parameter.ident.span)
                || !matches!(parameter.kind, GenericParamKind::Type { .. })
                || !parameter
                    .bounds
                    .iter()
                    .any(|bound| matches!(bound, GenericBound::Trait(..)))
            {
                continue;
            }
            let span = parameter
                .ident
                .span
                .to(parameter.bounds.last().unwrap().span());
            context.emit_span_lint(
                INLINE_GENERIC_BOUNDS,
                span,
                Message {
                    text: format!("move the bounds on `{}` to a `where` clause", parameter.ident),
                    help: "keep type parameters in the generic list and put their trait bounds after `where`",
                },
            );
        }
    }
}
