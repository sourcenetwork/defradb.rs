//! Written Result aliases paired with their delivered type positions.

use rustc_hir::def::DefKind;
use rustc_hir::intravisit::{self, Visitor, VisitorExt};
use rustc_hir::{GenericArg, QPath, Ty, TyKind};
use rustc_lint::LateContext;
use rustc_middle::ty::{self, TypeVisitableExt};
use rustc_span::symbol::sym;

use crate::return_types::{
    FUTURE_PROJECTION_LIMIT, collect_results, collection_values, future_value, is_result,
};

fn written_value<'compiler>(
    context: &LateContext<'compiler>,
    syntax: &'compiler Ty<'compiler>,
) -> ty::Ty<'compiler> {
    let value = rustc_hir_analysis::lower_ty(context.tcx, syntax);
    // Nested HIR types can sit below trait binders outside this node.
    // Erase those free regions while preserving binders inside the type.
    let value = ty::fold_regions(context.tcx, value, |_, _| context.tcx.lifetimes.re_erased);
    let value = context.tcx.erase_and_anonymize_regions(value);
    let value = context
        .tcx
        .try_normalize_erasing_regions(context.typing_env(), ty::Unnormalized::new_wip(value))
        .unwrap_or(value);
    assert!(
        !value.has_escaping_bound_vars(),
        "code-shape written type has escaping regions at {:?}: {value:?}",
        syntax.span
    );
    value
}

fn delivers_parameter<'compiler>(
    context: &LateContext<'compiler>,
    environment: ty::TypingEnv<'compiler>,
    value: ty::Ty<'compiler>,
    index: u32,
    remaining_futures: &mut usize,
) -> bool {
    let mut delivers =
        |nested| delivers_parameter(context, environment, nested, index, remaining_futures);
    match value.kind() {
        ty::Param(parameter) => return parameter.index == index,
        ty::Adt(definition, arguments) => {
            let limit = if is_result(context, value) {
                1
            } else {
                collection_values(context, definition.did())
            };
            if limit != 0 {
                return arguments.types().take(limit).any(delivers);
            }
        }
        ty::Tuple(values) => return values.iter().any(delivers),
        ty::Ref(_, nested, _)
        | ty::RawPtr(nested, _)
        | ty::Array(nested, _)
        | ty::Slice(nested) => return delivers(*nested),
        _ => {}
    }
    if *remaining_futures == 0 {
        return false;
    }
    let Some(delivered) = future_value(context, value, environment) else {
        return false;
    };
    *remaining_futures -= 1;
    delivers_parameter(context, environment, delivered, index, remaining_futures)
}

struct WrittenResults<'context, 'compiler> {
    context: &'context LateContext<'compiler>,
    values: Vec<&'compiler Ty<'compiler>>,
}

impl<'compiler> WrittenResults<'_, 'compiler> {
    fn visit_arguments(&mut self, path: &'compiler rustc_hir::Path<'compiler>) -> bool {
        let Some(definition) = path.res.opt_def_id() else {
            return false;
        };
        if !matches!(
            self.context.tcx.def_kind(definition),
            DefKind::Struct | DefKind::Enum | DefKind::Union | DefKind::TyAlias
        ) {
            return false;
        }
        let Some(segment) = path.segments.last() else {
            return false;
        };
        // Use the declared carrier shape. Substituting a concrete marker can
        // select a different Future implementation rather than reveal its output.
        let declared = self
            .context
            .tcx
            .type_of(definition)
            .instantiate_identity()
            .skip_norm_wip();
        let environment = ty::TypingEnv::non_body_analysis(self.context.tcx, definition);
        let parameters = self
            .context
            .tcx
            .generics_of(definition)
            .own_params
            .iter()
            .filter(|parameter| matches!(parameter.kind, ty::GenericParamDefKind::Type { .. }));
        let written = segment
            .args()
            .args
            .iter()
            .filter_map(|argument| match argument {
                GenericArg::Type(value) => Some(value.as_unambig_ty()),
                _ => None,
            });
        for (parameter, syntax) in parameters.zip(written) {
            let mut remaining_futures = FUTURE_PROJECTION_LIMIT;
            if delivers_parameter(
                self.context,
                environment,
                declared,
                parameter.index,
                &mut remaining_futures,
            ) {
                self.visit_ty_unambig(syntax);
            }
        }
        true
    }
}

impl<'compiler> Visitor<'compiler> for WrittenResults<'_, 'compiler> {
    fn visit_ty(&mut self, value: &'compiler Ty<'compiler, rustc_hir::AmbigArg>) {
        if collect_results(
            self.context,
            written_value(self.context, value.as_unambig_ty()),
        )
        .is_empty()
        {
            return;
        }
        if let TyKind::Path(QPath::Resolved(_, path)) = value.kind {
            if let Some(definition) = path.res.opt_def_id() {
                let result = self.context.tcx.is_diagnostic_item(sym::Result, definition)
                    || (matches!(self.context.tcx.def_kind(definition), DefKind::TyAlias)
                        && is_result(
                            self.context,
                            self.context
                                .tcx
                                .type_of(definition)
                                .instantiate_identity()
                                .skip_norm_wip(),
                        ));
                if result {
                    self.values.push(value.as_unambig_ty());
                }
            }
            if self.visit_arguments(path) {
                return;
            }
        }
        if !matches!(value.kind, TyKind::FnPtr(..)) {
            intravisit::walk_ty(self, value);
        }
    }
}

pub(crate) fn collect<'compiler>(
    context: &LateContext<'compiler>,
    syntax: &'compiler Ty<'compiler>,
) -> Vec<(&'compiler Ty<'compiler>, ty::Ty<'compiler>)> {
    let mut written = WrittenResults {
        context,
        values: Vec::new(),
    };
    written.visit_ty_unambig(syntax);
    written
        .values
        .into_iter()
        .map(|syntax| (syntax, written_value(context, syntax)))
        .collect()
}
