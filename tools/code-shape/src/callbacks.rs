use rustc_hir::{GenericParam, ItemKind, Node, OwnerNode, ParamName, PolyTraitRef, Ty, TyKind};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_middle::ty;
use rustc_session::{declare_lint, declare_lint_pass};
use rustc_span::def_id::DefId;

use crate::diagnostic::Message;

declare_lint! {
    /// Callback interfaces use a named trait or a type alias.
    pub ANONYMOUS_CALLBACK,
    Warn,
    "an unnamed function pointer or closure interface"
}

declare_lint! {
    /// Explicit lifetimes on callback declarations describe what they borrow.
    pub CALLBACK_LIFETIME_NAME,
    Warn,
    "an opaque lifetime name on a callback declaration"
}

declare_lint_pass!(Callbacks => [ANONYMOUS_CALLBACK, CALLBACK_LIFETIME_NAME]);

pub(crate) fn callback_trait(context: &LateContext<'_>, definition: DefId) -> bool {
    let items = context.tcx.lang_items();
    [
        items.fn_trait(),
        items.fn_mut_trait(),
        items.fn_once_trait(),
        items.async_fn_trait(),
        items.async_fn_mut_trait(),
        items.async_fn_once_trait(),
    ]
    .contains(&Some(definition))
}

fn callback_interface(context: &LateContext<'_>, definition: DefId) -> bool {
    let mut pending = vec![definition];
    let mut visited = Vec::new();
    while let Some(definition) = pending.pop() {
        if callback_trait(context, definition) {
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

fn named_context(context: &LateContext<'_>, owner: OwnerNode<'_>) -> bool {
    match owner {
        OwnerNode::Item(item) => match &item.kind {
            ItemKind::TyAlias(..) | ItemKind::TraitAlias(..) => true,
            ItemKind::Trait { .. } => callback_interface(context, item.owner_id.to_def_id()),
            ItemKind::Impl(implementation) => implementation
                .of_trait
                .and_then(|header| header.trait_ref.trait_def_id())
                .is_some_and(|definition| callback_interface(context, definition)),
            _ => false,
        },
        OwnerNode::ImplItem(item) => matches!(item.kind, rustc_hir::ImplItemKind::Type(..)),
        OwnerNode::TraitItem(item) => matches!(item.kind, rustc_hir::TraitItemKind::Type(..)),
        _ => false,
    }
}

fn named_bound(context: &LateContext<'_>, reference: &PolyTraitRef<'_>) -> bool {
    let id = reference.trait_ref.hir_ref_id;
    let owner = context.tcx.hir_owner_node(id.owner);
    if let OwnerNode::Item(item) = owner
        && matches!(item.kind, ItemKind::Trait { .. })
    {
        return context
            .tcx
            .hir_parent_iter(id)
            .take_while(|(parent, _)| parent.owner == id.owner)
            .find_map(|(_, node)| match node {
                Node::AssocItemConstraint(_) | Node::Ty(_) => Some(false),
                Node::WherePredicate(predicate) => Some(matches!(
                    predicate.kind,
                    rustc_hir::WherePredicateKind::BoundPredicate(bound)
                        if bound.is_param_bound(item.owner_id.to_def_id())
                )),
                _ => None,
            })
            .unwrap_or(true);
    }
    named_context(context, owner)
}

fn callback_alias(context: &LateContext<'_>, definition: rustc_hir::def_id::LocalDefId) -> bool {
    let mut types = context
        .tcx
        .type_of(definition)
        .instantiate_identity()
        .skip_norm_wip()
        .walk();
    while let Some(argument) = types.next() {
        let Some(kind) = argument.as_type() else {
            continue;
        };
        let callback = match kind.kind() {
            ty::Adt(definition, _)
                if Some(definition.did()) == context.tcx.lang_items().phantom_data() =>
            {
                types.skip_current_subtree();
                false
            }
            ty::FnPtr(..) => true,
            ty::Dynamic(predicates, _) => predicates.iter().any(|predicate| {
                matches!(
                    predicate.skip_binder(),
                    ty::ExistentialPredicate::Trait(reference)
                        if callback_interface(context, reference.def_id)
                )
            }),
            _ => false,
        };
        if callback {
            return true;
        }
    }
    false
}

impl<'compiler> LateLintPass<'compiler> for Callbacks {
    fn check_poly_trait_ref(
        &mut self,
        context: &LateContext<'compiler>,
        reference: &'compiler PolyTraitRef<'compiler>,
    ) {
        if crate::source::generated(context, reference.span)
            || named_bound(context, reference)
            || !reference
                .trait_ref
                .trait_def_id()
                .is_some_and(|definition| callback_trait(context, definition))
        {
            return;
        }
        context.emit_span_lint(
            ANONYMOUS_CALLBACK,
            reference.span,
            Message {
                text: "give this callback shape a named trait or type alias".into(),
                help: "a named trait with a blanket implementation preserves capturing closures and their bounds",
            },
        );
    }

    fn check_ty(
        &mut self,
        context: &LateContext<'compiler>,
        value: &'compiler Ty<'compiler, rustc_hir::AmbigArg>,
    ) {
        if crate::source::generated(context, value.span)
            || !matches!(value.kind, TyKind::FnPtr(..))
            || named_context(context, context.tcx.hir_owner_node(value.hir_id.owner))
        {
            return;
        }
        // A discarded type ascription pins a function's signature. It does
        // not store or pass a callback (for example an exported C ABI check).
        if context
            .tcx
            .hir_parent_iter(value.hir_id)
            .take_while(|(parent, _)| parent.owner == value.hir_id.owner)
            .any(|(_, node)| {
                matches!(node, Node::LetStmt(binding)
                if matches!(binding.pat.kind, rustc_hir::PatKind::Wild)
                    && binding.ty.is_some_and(|ty| ty.hir_id == value.hir_id))
            })
        {
            return;
        }
        // A function type in PhantomData specifies variance, not a callback.
        if context.tcx.hir_parent_iter(value.hir_id).any(|(_, node)| {
            matches!(node, Node::Ty(parent) if matches!(parent.kind, TyKind::Path(rustc_hir::QPath::Resolved(_, path))
                if path.res.opt_def_id() == context.tcx.lang_items().phantom_data()))
        }) {
            return;
        }
        context.emit_span_lint(
            ANONYMOUS_CALLBACK,
            value.span,
            Message {
                text: "give this function pointer a named type alias".into(),
                help: "declare a type alias that retains the pointer's ABI, safety, arguments and result",
            },
        );
    }

    fn check_generic_param(
        &mut self,
        context: &LateContext<'compiler>,
        parameter: &'compiler GenericParam<'compiler>,
    ) {
        if !parameter.is_lifetime()
            || parameter.is_elided_lifetime()
            || crate::source::generated(context, parameter.span)
        {
            return;
        }
        let ParamName::Plain(name) = parameter.name else {
            return;
        };
        let spelling = name.as_str();
        let spelling = spelling.trim_start_matches('\'');
        let mut characters = spelling.chars();
        if !characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
            || !characters.all(|character| character.is_ascii_digit())
        {
            return;
        }
        let owner = context.tcx.hir_owner_node(parameter.hir_id.owner);
        let callback = match owner {
            OwnerNode::Item(item) => match item.kind {
                ItemKind::TyAlias(..) => callback_alias(context, item.owner_id.def_id),
                ItemKind::Trait { .. } => callback_interface(context, item.owner_id.to_def_id()),
                _ => false,
            },
            OwnerNode::ImplItem(item) if matches!(item.kind, rustc_hir::ImplItemKind::Type(..)) => {
                callback_alias(context, item.owner_id.def_id)
            }
            OwnerNode::TraitItem(item) => match item.kind {
                rustc_hir::TraitItemKind::Type(bounds, _) => bounds.iter().any(|bound| {
                    bound
                        .trait_ref()
                        .and_then(|reference| reference.trait_def_id())
                        .is_some_and(|definition| callback_interface(context, definition))
                }),
                _ => false,
            },
            _ => false,
        };
        if callback {
            context.emit_span_lint(
                CALLBACK_LIFETIME_NAME,
                parameter.span,
                Message {
                    text: format!("name callback lifetime `{name}` for what it borrows"),
                    help: "use a descriptive name such as `'request`, `'store`, or `'callback`",
                },
            );
        }
    }
}
