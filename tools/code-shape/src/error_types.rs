use rustc_hir::{ItemKind, Node, OwnerId, OwnerNode, Ty};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_middle::ty::{self, TypeVisitableExt};
use rustc_session::{declare_lint, impl_lint_pass};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, symbol::sym};

use crate::diagnostic::Message;

declare_lint! {
    /// Function signatures retain concrete errors rather than boxed error trait objects.
    pub ERASED_ERROR_SIGNATURE,
    Warn,
    "a boxed error trait object in a signature"
}

#[derive(Default)]
pub(crate) struct ErrorTypes {
    reported: Vec<(OwnerId, Span)>,
}

impl_lint_pass!(ErrorTypes => [ERASED_ERROR_SIGNATURE]);

fn signature(context: &LateContext<'_>, value: &Ty<'_, rustc_hir::AmbigArg>) -> bool {
    // A conversion bound accepts concrete ecosystem errors; it does not erase
    // a parameter or return value. In particular Box<dyn Error> itself does
    // not implement Error and must remain accepted by Into<Box<dyn Error>>.
    if context.tcx.hir_parent_iter(value.hir_id).any(|(_, node)| {
        matches!(node, Node::TraitRef(reference) if reference.trait_def_id()
            .is_some_and(|definition|
                context.tcx.is_diagnostic_item(sym::Into, definition)
                || context.tcx.is_diagnostic_item(sym::From, definition)))
    }) {
        return false;
    }
    let owner = context.tcx.hir_owner_node(value.hir_id.owner);
    let function = match owner {
        OwnerNode::Item(item) => match item.kind {
            ItemKind::Fn { .. } => true,
            ItemKind::TyAlias(..) => {
                return context.tcx.visibility(item.owner_id).is_public();
            }
            ItemKind::Struct(..) | ItemKind::Enum(..) => {
                return context
                    .tcx
                    .hir_parent_iter(value.hir_id)
                    .any(|(_, node)| matches!(node, Node::Field(_)))
                    && error_owner(context, item.owner_id.to_def_id());
            }
            ItemKind::Trait { .. } | ItemKind::TraitAlias(..) => true,
            _ => false,
        },
        OwnerNode::TraitItem(item) => matches!(
            item.kind,
            rustc_hir::TraitItemKind::Fn(..) | rustc_hir::TraitItemKind::Type(..)
        ),
        OwnerNode::ImplItem(item) => match item.kind {
            rustc_hir::ImplItemKind::Type(..) => return true,
            rustc_hir::ImplItemKind::Fn(..) => true,
            _ => false,
        },
        _ => false,
    };
    if !function {
        return false;
    }
    // An external trait's fixed method signature cannot be changed by an adapter.
    if matches!(owner, OwnerNode::ImplItem(_))
        && context.tcx.hir_parent_iter(value.hir_id).any(|(_, node)| {
            matches!(node, Node::Item(item) if matches!(&item.kind, ItemKind::Impl(implementation)
                if implementation.of_trait.and_then(|header| header.trait_ref.trait_def_id())
                    .is_some_and(|definition| !definition.is_local())))
        })
    {
        return false;
    }
    context
        .tcx
        .hir_parent_iter(value.hir_id)
        .take_while(|(parent, _)| parent.owner == value.hir_id.owner)
        .all(|(_, node)| !matches!(node, Node::Expr(_) | Node::LetStmt(_) | Node::Block(_)))
}

fn error_owner(context: &LateContext<'_>, owner: rustc_span::def_id::DefId) -> bool {
    let Some(error_trait) = context.tcx.get_diagnostic_item(Symbol::intern("Error")) else {
        return false;
    };
    context
        .tcx
        .all_local_trait_impls(())
        .get(&error_trait)
        .is_some_and(|implementations| {
            implementations.iter().any(|&implementation| {
                let value = context
                    .tcx
                    .type_of(implementation)
                    .instantiate_identity()
                    .skip_norm_wip();
                matches!(value.kind(), ty::Adt(definition, _) if definition.did() == owner)
            })
        })
}

fn erased(context: &LateContext<'_>, value: ty::Ty<'_>) -> bool {
    value
        .walk()
        .filter_map(|argument| argument.as_type())
        .any(|value| {
            let ty::Adt(definition, arguments) = value.kind() else {
                return false;
            };
            if !definition.is_box() {
                return false;
            }
            let ty::Dynamic(predicates, _) = arguments.type_at(0).kind() else {
                return false;
            };
            predicates.iter().any(|predicate| {
                matches!(predicate.skip_binder(), ty::ExistentialPredicate::Trait(reference)
                if error_interface(context, reference.def_id))
            })
        })
}

fn error_interface(context: &LateContext<'_>, definition: DefId) -> bool {
    let mut pending = vec![definition];
    let mut visited = Vec::new();
    while let Some(definition) = pending.pop() {
        if context
            .tcx
            .is_diagnostic_item(Symbol::intern("Error"), definition)
        {
            return true;
        }
        if visited.contains(&definition) {
            continue;
        }
        visited.push(definition);
        for (predicate, _) in context
            .tcx
            .explicit_super_clauses_of(definition)
            .skip_binder()
        {
            if let ty::ClauseKind::Trait(reference) = predicate.kind().skip_binder() {
                if matches!(reference.trait_ref.self_ty().kind(), ty::Param(parameter) if parameter.index == 0)
                {
                    pending.push(reference.def_id());
                }
            }
        }
    }
    false
}

impl<'compiler> LateLintPass<'compiler> for ErrorTypes {
    fn check_ty(
        &mut self,
        context: &LateContext<'compiler>,
        value: &'compiler Ty<'compiler, rustc_hir::AmbigArg>,
    ) {
        if crate::source::generated(context, value.span)
            || !signature(context, value)
            || self
                .reported
                .iter()
                .any(|(owner, span)| *owner == value.hir_id.owner && span.contains(value.span))
        {
            return;
        }
        let resolved = context
            .typeck_results
            .filter(|results| results.hir_owner == value.hir_id.owner)
            .and_then(|results| results.node_type_opt(value.hir_id))
            .unwrap_or_else(|| rustc_hir_analysis::lower_ty(context.tcx, value.as_unambig_ty()));
        let resolved = if resolved.has_escaping_bound_vars() {
            resolved
        } else {
            context
                .tcx
                .try_normalize_erasing_regions(
                    context.typing_env(),
                    ty::Unnormalized::new_wip(resolved),
                )
                .unwrap_or(resolved)
        };
        if erased(context, resolved) {
            self.reported.push((value.hir_id.owner, value.span));
            context.emit_span_lint(
                ERASED_ERROR_SIGNATURE,
                value.span,
                Message {
                    text: "keep a concrete error type in this signature".into(),
                    help: "return a typed `thiserror` error and retain the original failure as its source",
                },
            );
        }
    }
}
