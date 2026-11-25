//! Generic error parameters supported by caller inputs and their bounds.

use rustc_hir::{ItemKind, Node};
use rustc_lint::LateContext;
use rustc_middle::ty::{self, TypeVisitableExt};
use rustc_span::def_id::LocalDefId;

fn parameters(value: ty::Ty<'_>) -> Vec<u32> {
    value
        .walk()
        .filter_map(|argument| argument.as_type())
        .filter_map(|value| match value.kind() {
            ty::Param(parameter) => Some(parameter.index),
            _ => None,
        })
        .collect()
}

fn extend(supported: &mut Vec<u32>, value: ty::Ty<'_>) {
    for parameter in parameters(value) {
        if !supported.contains(&parameter) {
            supported.push(parameter);
        }
    }
}

fn input_values<'compiler>(
    context: &LateContext<'compiler>,
    value: ty::Ty<'compiler>,
    depth: usize,
    values: &mut Vec<ty::Ty<'compiler>>,
) {
    if depth == 128 || values.contains(&value) {
        return;
    }
    if matches!(value.kind(), ty::Adt(definition, _) if context.tcx.lang_items().phantom_data() == Some(definition.did()))
    {
        return;
    }
    values.push(value);
    match value.kind() {
        ty::Adt(definition, arguments) if definition.did().is_local() => {
            for field in definition.all_fields() {
                input_values(
                    context,
                    field.ty(context.tcx, arguments).skip_norm_wip(),
                    depth + 1,
                    values,
                );
            }
        }
        ty::Adt(_, arguments)
        | ty::Alias(
            _,
            ty::AliasTy {
                args: arguments, ..
            },
        ) => {
            for nested in arguments.types() {
                input_values(context, nested, depth + 1, values);
            }
        }
        ty::Tuple(elements) => {
            for nested in elements.iter() {
                input_values(context, nested, depth + 1, values);
            }
        }
        ty::Ref(_, nested, _)
        | ty::RawPtr(nested, _)
        | ty::Array(nested, _)
        | ty::Slice(nested) => input_values(context, *nested, depth + 1, values),
        ty::FnPtr(signature, _) => {
            let signature = context
                .tcx
                .instantiate_bound_regions_with_erased(*signature);
            input_values(context, signature.output(), depth + 1, values);
        }
        ty::Dynamic(predicates, _) => {
            for predicate in predicates.iter() {
                match context.tcx.instantiate_bound_regions_with_erased(predicate) {
                    ty::ExistentialPredicate::Projection(projection) => {
                        let owner = context.tcx.parent(projection.def_id);
                        if !crate::callbacks::callback_trait(context, owner)
                            || context.tcx.item_name(projection.def_id) == rustc_span::sym::Output
                        {
                            if let ty::TermKind::Ty(output) = projection.term.kind() {
                                input_values(context, output, depth + 1, values);
                            }
                        }
                    }
                    ty::ExistentialPredicate::Trait(reference)
                        if !crate::callbacks::callback_trait(context, reference.def_id) =>
                    {
                        for argument in reference.args.types() {
                            input_values(context, argument, depth + 1, values);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn parameter<'compiler>(
    context: &LateContext<'compiler>,
    definition: LocalDefId,
    error: ty::Ty<'compiler>,
) -> bool {
    let ty::Param(error) = error.kind() else {
        return false;
    };
    let parent = context.tcx.local_parent(definition);
    let trait_contract = matches!(context.tcx.hir_node_by_def_id(parent), Node::Item(item)
        if matches!(item.kind, ItemKind::Trait { .. }));
    if trait_contract && error.index < context.tcx.generics_of(definition).parent_count as u32 {
        return true;
    }
    let signature = context
        .tcx
        .fn_sig(definition)
        .instantiate_identity()
        .skip_norm_wip();
    let signature = context.tcx.instantiate_bound_regions_with_erased(signature);
    let mut inputs = Vec::new();
    for &input in signature.inputs() {
        input_values(context, input, 0, &mut inputs);
    }
    let mut supported = Vec::new();
    for &input in &inputs {
        if let ty::Param(parameter) = input.kind() {
            if !supported.contains(&parameter.index) {
                supported.push(parameter.index);
            }
        }
    }
    let mut clauses = Vec::new();
    let mut owner = Some(definition.to_def_id());
    while let Some(definition) = owner {
        let bounds = context.tcx.clauses_of(definition);
        clauses.extend(bounds.clauses.iter().map(|(clause, _)| *clause));
        owner = bounds.parent;
    }
    loop {
        if supported.contains(&error.index) {
            return true;
        }
        let before = supported.len();
        for clause in &clauses {
            let (source, destinations) = match clause.kind().skip_binder() {
                // Callable parameters consume arguments. Only their output
                // projection can supply an error returned by the caller.
                ty::ClauseKind::Trait(bound)
                    if crate::callbacks::callback_trait(context, bound.def_id()) =>
                {
                    continue;
                }
                ty::ClauseKind::Trait(bound) => (
                    bound.self_ty(),
                    bound.trait_ref.args.types().collect::<Vec<_>>(),
                ),
                ty::ClauseKind::Projection(bound) => (
                    bound.projection_term.self_ty(),
                    match bound.term.kind() {
                        ty::TermKind::Ty(value) => vec![value],
                        _ => Vec::new(),
                    },
                ),
                _ => continue,
            };
            let dependencies = parameters(source);
            let implementation =
                matches!(clause.kind().skip_binder(), ty::ClauseKind::Projection(_))
                    && source.has_non_region_param()
                    && !dependencies.contains(&error.index);
            let known = implementation
                || inputs.contains(&source)
                || (!dependencies.is_empty()
                    && dependencies.iter().all(|index| supported.contains(index)));
            if known {
                for destination in destinations {
                    extend(&mut supported, destination);
                }
            }
        }
        if supported.len() == before {
            return false;
        }
    }
}
