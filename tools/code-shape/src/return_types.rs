//! The value delivered by a function, including future carriers and aliases.

use rustc_hir::{FnDecl, FnRetTy, GenericArg, QPath, Term, Ty, TyKind};
use rustc_infer::infer::TyCtxtInferExt;
use rustc_lint::LateContext;
use rustc_middle::ty::{self, TypeVisitableExt};
use rustc_span::{Span, def_id::LocalDefId, symbol::sym};
use rustc_trait_selection::infer::InferCtxtExt;

pub(crate) struct Output<'compiler> {
    pub(crate) value: ty::Ty<'compiler>,
    pub(crate) written: &'compiler Ty<'compiler>,
    pub(crate) spells_result: bool,
}

impl Output<'_> {
    pub(crate) fn span(&self) -> Span {
        self.written.span
    }
}

fn future_binding<'compiler>(
    context: &LateContext<'compiler>,
    reference: &rustc_hir::TraitRef<'compiler>,
) -> Option<&'compiler Ty<'compiler>> {
    if reference.trait_def_id() != context.tcx.lang_items().future_trait() {
        return None;
    }
    reference
        .path
        .segments
        .iter()
        .flat_map(|segment| segment.args().constraints)
        .find_map(|constraint| match constraint.kind {
            rustc_hir::AssocItemConstraintKind::Equality {
                term: Term::Ty(output),
            } if constraint.ident.name == sym::Output => Some(output),
            _ => None,
        })
}

fn written_future_output<'compiler>(
    context: &LateContext<'compiler>,
    value: &'compiler Ty<'compiler>,
) -> Option<&'compiler Ty<'compiler>> {
    match value.kind {
        TyKind::OpaqueDef(opaque) => opaque
            .bounds
            .iter()
            .filter_map(|bound| bound.trait_ref())
            .find_map(|reference| future_binding(context, reference)),
        TyKind::TraitObject(bounds, _) => bounds
            .iter()
            .find_map(|reference| future_binding(context, &reference.trait_ref)),
        TyKind::Path(QPath::Resolved(_, path))
            if path.res.opt_def_id().is_some_and(|definition| {
                Some(definition) == context.tcx.lang_items().owned_box()
                    || Some(definition) == context.tcx.lang_items().pin_type()
            }) =>
        {
            path.segments
                .iter()
                .flat_map(|segment| segment.args().args)
                .find_map(|argument| match argument {
                    GenericArg::Type(value) => {
                        written_future_output(context, value.as_unambig_ty())
                    }
                    _ => None,
                })
        }
        TyKind::Ref(_, value) | TyKind::Ptr(value) => written_future_output(context, value.ty),
        _ => None,
    }
}

pub(crate) fn output<'compiler>(
    context: &LateContext<'compiler>,
    definition: LocalDefId,
    declaration: &FnDecl<'compiler>,
) -> Option<Output<'compiler>> {
    let FnRetTy::Return(written) = declaration.output else {
        return None;
    };
    let leaf = written_future_output(context, written);
    // Erasing the lowered type alone leaves a borrowed alias's regions bound
    // outside that type. Instantiate the function's own binder first.
    let signature = context
        .tcx
        .fn_sig(definition.to_def_id())
        .instantiate_identity()
        .skip_norm_wip();
    let signature = context.tcx.instantiate_bound_regions_with_erased(signature);
    let value = context.tcx.erase_and_anonymize_regions(signature.output());
    assert!(
        !value.has_escaping_bound_vars(),
        "code-shape: function regions escaped their signature binder; check the pinned compiler adapter"
    );
    let candidate = context
        .tcx
        .try_normalize_erasing_regions(context.typing_env(), ty::Unnormalized::new_wip(value))
        .unwrap_or(value);
    // Result cannot implement Future under std's orphan rules.
    if matches!(candidate.kind(), ty::Adt(definition, _)
        if context.tcx.is_diagnostic_item(sym::Result, definition.did()))
    {
        return Some(Output {
            value: candidate,
            written,
            spells_result: true,
        });
    }
    let Some(delivered) = future_value(context, candidate, context.typing_env()) else {
        return Some(Output {
            value,
            written,
            spells_result: true,
        });
    };
    Some(Output {
        value: delivered,
        written: leaf.unwrap_or(written),
        spells_result: leaf.is_some(),
    })
}

pub(crate) const FUTURE_PROJECTION_LIMIT: usize = 64;

pub(crate) fn future_value<'compiler>(
    context: &LateContext<'compiler>,
    candidate: ty::Ty<'compiler>,
    environment: ty::TypingEnv<'compiler>,
) -> Option<ty::Ty<'compiler>> {
    let tcx = context.tcx;
    let future = tcx.lang_items().future_trait()?;
    let (inference, param_env) = tcx.infer_ctxt().build_with_typing_env(environment);
    if !inference
        .type_implements_trait(future, [candidate], param_env)
        .must_apply_modulo_regions()
    {
        return None;
    }
    let output = tcx.associated_items(future).find_by_ident_and_kind(
        tcx,
        rustc_span::Ident::with_dummy_span(sym::Output),
        ty::AssocTag::Type,
        future,
    )?;
    let projection = ty::Ty::new_projection(tcx, ty::IsRigid::No, output.def_id, [candidate]);
    tcx.try_normalize_erasing_regions(environment, ty::Unnormalized::new_wip(projection))
        .ok()
        .filter(|&value| value != projection)
}

pub(crate) fn is_result(context: &LateContext<'_>, value: ty::Ty<'_>) -> bool {
    matches!(value.kind(), ty::Adt(definition, _) if context.tcx.is_diagnostic_item(sym::Result, definition.did()))
}

pub(crate) fn collection_values(
    context: &LateContext<'_>,
    definition: rustc_span::def_id::DefId,
) -> usize {
    if !matches!(
        context.tcx.crate_name(definition.krate).as_str(),
        "core" | "alloc" | "std"
    ) {
        return 0;
    }
    match context.tcx.item_name(definition).as_str() {
        "Option" | "Poll" | "Pin" | "Vec" | "Box" | "Rc" | "Arc" | "BTreeSet" | "HashSet"
        | "VecDeque" | "LinkedList" | "BinaryHeap" | "Ready" => 1,
        "BTreeMap" | "HashMap" => 2,
        _ => 0,
    }
}

pub(crate) fn collect_results<'compiler>(
    context: &LateContext<'compiler>,
    value: ty::Ty<'compiler>,
) -> Vec<ty::Ty<'compiler>> {
    let mut results = Vec::new();
    let mut remaining_futures = FUTURE_PROJECTION_LIMIT;
    walk(context, value, &mut remaining_futures, &mut results);
    results
}

fn walk<'compiler>(
    context: &LateContext<'compiler>,
    value: ty::Ty<'compiler>,
    remaining_futures: &mut usize,
    results: &mut Vec<ty::Ty<'compiler>>,
) {
    let value = context
        .tcx
        .try_normalize_erasing_regions(context.typing_env(), ty::Unnormalized::new_wip(value))
        .unwrap_or(value);
    assert!(
        !value.has_escaping_bound_vars(),
        "code-shape carrier has escaping regions: {value:?}"
    );
    if is_result(context, value) {
        results.push(value);
        let ty::Adt(_, arguments) = value.kind() else {
            unreachable!()
        };
        // The success can deliver another result; the error is its cause.
        walk(context, arguments.type_at(0), remaining_futures, results);
        return;
    }
    if let ty::Adt(definition, arguments) = value.kind() {
        let count = collection_values(context, definition.did());
        if count != 0 {
            for nested in arguments.types().take(count) {
                walk(context, nested, remaining_futures, results);
            }
            return;
        }
    }
    match value.kind() {
        ty::Tuple(values) => {
            for nested in values.iter() {
                walk(context, nested, remaining_futures, results);
            }
        }
        ty::Ref(_, nested, _)
        | ty::RawPtr(nested, _)
        | ty::Array(nested, _)
        | ty::Slice(nested) => walk(context, *nested, remaining_futures, results),
        _ if *remaining_futures > 0 => {
            if let Some(delivered) = future_value(context, value, context.typing_env()) {
                // Count across sibling branches: one projection can grow and fork.
                *remaining_futures -= 1;
                walk(context, delivered, remaining_futures, results);
            }
        }
        _ => {}
    }
}

/// The Result leaves delivered by a return value, including contained futures.
pub(crate) fn results<'compiler>(
    context: &LateContext<'compiler>,
    output: Output<'compiler>,
) -> Vec<Output<'compiler>> {
    let delivered = collect_results(context, output.value);
    if delivered.is_empty() {
        return Vec::new();
    }
    let mut written: Vec<_> = crate::written_results::collect(context, output.written)
        .into_iter()
        .map(|(syntax, value)| (syntax, value, false))
        .collect();
    delivered
        .into_iter()
        .map(|value| {
            if value == output.value && output.spells_result {
                Output {
                    value,
                    written: output.written,
                    spells_result: output.spells_result,
                }
            } else if let Some((syntax, _, used)) = written
                .iter_mut()
                .find(|(_, candidate, used)| !*used && *candidate == value)
            {
                *used = true;
                Output {
                    value,
                    written: syntax,
                    spells_result: true,
                }
            } else {
                // An opaque carrier alias exposes its delivered error's owner;
                // its hidden declaration is not a written Result at this call site.
                Output {
                    value,
                    written: output.written,
                    spells_result: false,
                }
            }
        })
        .collect()
}
