use rustc_hir::def::{DefKind, Res};
use rustc_hir::{FnDecl, ImplicitSelfKind, Item, ItemKind, QPath, TyKind};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_middle::ty::{self, TypeVisitableExt};
use rustc_session::config::CrateType;
use rustc_session::{declare_lint, impl_lint_pass};
use rustc_span::Symbol;
use rustc_span::def_id::{CRATE_DEF_ID, DefId, LocalDefId};

use crate::diagnostic::Message;

declare_lint! {
    /// Public fallible APIs use the owning crate's Result alias and error.
    pub CRATE_RESULT,
    Warn,
    "a public fallible API outside its crate's error boundary"
}

declare_lint! {
    /// Tests return unit or the crate's Result.
    pub TEST_RESULT,
    Warn,
    "a test returning a foreign result type"
}

#[derive(Default)]
pub(crate) struct CrateResults {
    tests: Vec<(u32, Symbol)>,
    package_library: Option<Symbol>,
}

impl_lint_pass!(CrateResults => [CRATE_RESULT, TEST_RESULT]);

fn root_result(context: &LateContext<'_>, allow_private: bool) -> Option<DefId> {
    context
        .tcx
        .module_children_local(CRATE_DEF_ID)
        .iter()
        .find_map(|child| {
            if child.ident.name == rustc_span::sym::Result
                && (allow_private || child.vis.is_public())
                && let Res::Def(DefKind::TyAlias, definition) = child.res
            {
                Some(definition)
            } else {
                None
            }
        })
}

fn result_error<'compiler>(
    context: &LateContext<'compiler>,
    value: ty::Ty<'compiler>,
) -> Option<ty::Ty<'compiler>> {
    let ty::Adt(definition, arguments) = value.kind() else {
        return None;
    };
    context
        .tcx
        .is_diagnostic_item(Symbol::intern("Result"), definition.did())
        .then(|| arguments.type_at(1))
}

fn library_name(manifest: &str) -> Option<String> {
    let manifest = manifest.parse::<toml::Table>().ok()?;
    let explicit = manifest.get("lib").and_then(|lib| lib.get("name"));
    let package = manifest
        .get("package")
        .and_then(|package| package.get("name"));
    Some(explicit.or(package)?.as_str()?.replace('-', "_"))
}

fn package_library_result(context: &LateContext<'_>, alias: DefId, name: Option<Symbol>) -> bool {
    let Some(name) = name else {
        return false;
    };
    !alias.is_local()
        && context.tcx.crate_name(alias.krate) == name
        && context
            .tcx
            .sess
            .opts
            .externs
            .get(name.as_str())
            .and_then(|entry| entry.files())
            .is_none_or(|mut files| {
                let loaded = context.tcx.crate_extern_paths(alias.krate);
                files.any(|file| loaded.contains(file.canonicalized()))
            })
        && context
            .tcx
            .module_children(alias.krate.as_def_id())
            .iter()
            .any(|child| {
                child.ident.name == rustc_span::sym::Result
                    && child.vis.is_public()
                    && child.res.opt_def_id() == Some(alias)
            })
}

fn expected_error<'compiler>(
    context: &LateContext<'compiler>,
    alias: DefId,
    test: bool,
    library: Option<Symbol>,
) -> Option<DefId> {
    if !alias.is_local() && !(test && package_library_result(context, alias, library)) {
        return None;
    }
    let value = context
        .tcx
        .type_of(alias)
        .instantiate_identity()
        .skip_norm_wip();
    let mut error = result_error(context, value)?;
    if let ty::Param(parameter) = error.kind() {
        let parameter = context
            .tcx
            .generics_of(alias)
            .param_at(parameter.index as usize, context.tcx);
        if matches!(
            parameter.kind,
            ty::GenericParamDefKind::Type {
                has_default: true,
                ..
            }
        ) {
            error = context
                .tcx
                .type_of(parameter.def_id)
                .instantiate_identity()
                .skip_norm_wip();
        }
    }
    let ty::Adt(definition, _) = error.kind() else {
        return None;
    };
    if !definition.is_enum() || definition.did().krate != alias.krate {
        return None;
    }
    // Integration tests compile separately from their package's library.
    if !alias.is_local() {
        return Some(definition.did());
    }
    let error_trait = context.tcx.get_diagnostic_item(Symbol::intern("Error"))?;
    let derived = context.tcx.all_local_trait_impls(()).get(&error_trait)
        .is_some_and(|implementations| implementations.iter().any(|&implementation| {
            let self_type = context.tcx.type_of(implementation).instantiate_identity().skip_norm_wip();
            matches!(self_type.kind(), ty::Adt(implemented, _) if implemented.did() == definition.did())
                && crate::errors::from_thiserror(context, context.tcx.def_span(implementation))
        }));
    derived.then_some(definition.did())
}

fn trait_contract(context: &LateContext<'_>, definition: LocalDefId) -> bool {
    let parent = context.tcx.local_parent(definition);
    matches!(context.tcx.hir_node_by_def_id(parent), rustc_hir::Node::Item(item)
        if matches!(&item.kind, ItemKind::Impl(implementation)
            if implementation.of_trait.is_some()))
}

fn returns_owned_receiver<'compiler>(
    context: &LateContext<'compiler>,
    definition: LocalDefId,
    declaration: &FnDecl<'compiler>,
    error: ty::Ty<'compiler>,
) -> bool {
    if !matches!(
        declaration.implicit_self(),
        ImplicitSelfKind::Imm | ImplicitSelfKind::Mut
    ) {
        return false;
    }
    let signature = context
        .tcx
        .fn_sig(definition.to_def_id())
        .instantiate_identity()
        .skip_norm_wip();
    let signature = context.tcx.instantiate_bound_regions_with_erased(signature);
    signature.inputs().first().is_some_and(|&receiver| {
        let receiver = context.tcx.erase_and_anonymize_regions(receiver);
        context
            .tcx
            .try_normalize_erasing_regions(
                context.typing_env(),
                ty::Unnormalized::new_wip(receiver),
            )
            .unwrap_or(receiver)
            == error
    })
}

impl CrateResults {
    fn boundary<'compiler>(
        &mut self,
        context: &LateContext<'compiler>,
        definition: LocalDefId,
        declaration: &FnDecl<'compiler>,
    ) {
        let test = self
            .tests
            .binary_search(&(
                context
                    .tcx
                    .local_parent(definition)
                    .local_def_index
                    .as_u32(),
                context.tcx.item_name(definition.to_def_id()),
            ))
            .is_ok();
        let library = context
            .tcx
            .crate_types()
            .iter()
            .any(|kind| *kind != CrateType::Executable);
        let entry = context
            .tcx
            .entry_fn(())
            .is_some_and(|(entry, _)| entry == definition.to_def_id());
        if (!test
            && (context.sess().opts.test
                || !(entry
                    || (library && context.effective_visibilities.is_reachable(definition)))))
            || trait_contract(context, definition)
        {
            return;
        }
        let Some(output) = crate::return_types::output(context, definition, declaration) else {
            return;
        };
        if crate::source::generated(context, output.span()) {
            return;
        }
        if test {
            self.returned_result(context, definition, declaration, output, test, library);
        } else {
            for result in crate::return_types::results(context, output) {
                self.returned_result(context, definition, declaration, result, test, library);
            }
        }
    }

    fn returned_result<'compiler>(
        &self,
        context: &LateContext<'compiler>,
        definition: LocalDefId,
        declaration: &FnDecl<'compiler>,
        output: crate::return_types::Output<'compiler>,
        test: bool,
        library: bool,
    ) {
        let resolved = output.value;
        if test && resolved.is_unit() {
            return;
        }
        let error = result_error(context, resolved).map(|error| {
            context
                .tcx
                .try_normalize_erasing_regions(
                    context.typing_env(),
                    ty::Unnormalized::new_wip(error),
                )
                .unwrap_or(error)
        });
        if !test && error.is_none() {
            return;
        }
        // Extension contracts carry an implementor's error or rejection value.
        // Requiring a concrete lower-crate error here would erase that payload.
        if !test
            && error.is_some_and(|error| {
                crate::forwarded_errors::parameter(context, definition, error)
                    || (matches!(
                        error.kind(),
                        ty::Alias(
                            _,
                            ty::AliasTy {
                                kind: ty::Projection { .. },
                                ..
                            }
                        )
                    ) && error.has_non_region_param())
                    || returns_owned_receiver(context, definition, declaration, error)
            })
        {
            return;
        }
        let spelled_alias = match output.written.kind {
            TyKind::Path(QPath::Resolved(_, path)) => path.res.opt_def_id(),
            _ => None,
        };
        let alias = spelled_alias
            .filter(|&alias| test && package_library_result(context, alias, self.package_library))
            .or_else(|| root_result(context, test || !library));
        let correct_error = alias
            .and_then(|alias| expected_error(context, alias, test, self.package_library))
            .is_some_and(|expected| {
                error.is_some_and(|error| {
                matches!(error.kind(), ty::Adt(definition, _) if definition.did() == expected)
            })
            });
        if alias.is_some() && correct_error && (!output.spells_result || alias == spelled_alias) {
            return;
        }
        context.emit_span_lint(
            if test { TEST_RESULT } else { CRATE_RESULT },
            output.span(),
            Message {
                text: if test {
                    "return `()` or the crate's `Result` from this test".into()
                } else {
                    "return the owning crate's `Result` at this public boundary".into()
                },
                help: "keep specific errors internally; wrap typed causes at this crate's boundary, and wrap them again only at a facade boundary",
            },
        );
    }
}

impl<'compiler> LateLintPass<'compiler> for CrateResults {
    fn check_crate(&mut self, context: &LateContext<'compiler>) {
        self.package_library = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .and_then(|directory| std::fs::read_to_string(directory.join("Cargo.toml")).ok())
            .and_then(|manifest| library_name(&manifest))
            .map(|name| Symbol::intern(&name));
        // The test macro marks a generated sibling constant, not the function.
        for item in context.tcx.hir_free_items() {
            let item = context.tcx.hir_item(item);
            if let ItemKind::Const(name, ..) = item.kind {
                if rustc_hir::find_attr!(context.tcx, item.owner_id.def_id, RustcTestMarker(_)) {
                    self.tests.push((
                        context
                            .tcx
                            .local_parent(item.owner_id.def_id)
                            .local_def_index
                            .as_u32(),
                        name.name,
                    ));
                }
            }
        }
        self.tests.sort_unstable();
    }

    fn check_item(&mut self, context: &LateContext<'compiler>, item: &'compiler Item<'compiler>) {
        if let ItemKind::Fn { sig, .. } = item.kind {
            self.boundary(context, item.owner_id.def_id, sig.decl);
        }
    }

    fn check_impl_item(
        &mut self,
        context: &LateContext<'compiler>,
        item: &'compiler rustc_hir::ImplItem<'compiler>,
    ) {
        if let rustc_hir::ImplItemKind::Fn(signature, _) = item.kind {
            self.boundary(context, item.owner_id.def_id, signature.decl);
        }
    }

    fn check_trait_item(
        &mut self,
        context: &LateContext<'compiler>,
        item: &'compiler rustc_hir::TraitItem<'compiler>,
    ) {
        if let rustc_hir::TraitItemKind::Fn(signature, _) = item.kind {
            self.boundary(context, item.owner_id.def_id, signature.decl);
        }
    }
}

#[test]
fn package_library_name_follows_cargo() {
    assert_eq!(
        library_name("[package]\nname = 'my-package'\n").as_deref(),
        Some("my_package")
    );
    assert_eq!(
        library_name("[package]\nname = 'my-package'\n[lib]\nname = 'custom_library'\n").as_deref(),
        Some("custom_library")
    );
    assert_eq!(library_name("[workspace]\n"), None);
}
