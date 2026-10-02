//! `#[traced]` — a span attribute over `fastrace` that adds per-callsite
//! filtering and self-rooting, neither of which `fastrace` provides.
//!
//! Expansion keeps the body in one place by selecting a `Span::noop()` when the
//! callsite is disabled, rather than emitting the body twice. A noop span makes
//! both `set_local_parent` and `in_span` inert, so a disabled site costs the
//! callsite load and a branch.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, Ident, ItemFn, LitStr, Result, Stmt, Token};

struct Args {
    name: Option<LitStr>,
    level: Option<Ident>,
    root: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = Args {
            name: None,
            level: None,
            root: false,
        };
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                // `root` is a bare flag: the seam decides whether to begin a
                // trace, and that is not a value anyone passes conditionally.
                "root" => args.root = true,
                "name" => {
                    input.parse::<Token![=]>()?;
                    args.name = Some(input.parse()?);
                }
                "level" => {
                    input.parse::<Token![=]>()?;
                    let lit: LitStr = input.parse()?;
                    let value = lit.value();
                    let ident = match value.as_str() {
                        "error" => "Error",
                        "warn" => "Warn",
                        "info" => "Info",
                        "debug" => "Debug",
                        "trace" => "Trace",
                        other => {
                            return Err(syn::Error::new(
                                lit.span(),
                                format!(
                                    "unknown level `{other}`; expected error/warn/info/debug/trace"
                                ),
                            ))
                        }
                    };
                    args.level = Some(Ident::new(ident, lit.span()));
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown argument `{other}`; expected name, level or root"),
                    ))
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(args)
    }
}

/// `async_trait` rewrites `async fn f(..) -> T` into a plain fn returning
/// `Box::pin(async move { .. })`. The attribute therefore sees a *sync* fn, and
/// instrumenting that would time only the construction of the future — the span
/// would close in nanoseconds and the body's spans would attach to whatever
/// parent happened to be current. Reach inside and instrument the async block
/// instead.
fn async_trait_block(block: &syn::Block) -> Option<&syn::ExprAsync> {
    let Some(Stmt::Expr(Expr::Call(call), _)) = block.stmts.last() else {
        return None;
    };
    let Expr::Path(path) = call.func.as_ref() else {
        return None;
    };
    // `Box::pin(..)`, however it was spelled.
    if path.path.segments.last()?.ident != "pin" {
        return None;
    }
    match call.args.first()? {
        Expr::Async(inner) => Some(inner),
        _ => None,
    }
}

#[proc_macro_attribute]
pub fn traced(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as Args);
    let func = parse_macro_input!(item as ItemFn);

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = func;

    let name = args
        .name
        .map(|n| n.value())
        .unwrap_or_else(|| sig.ident.to_string());
    let level = args
        .level
        .unwrap_or_else(|| Ident::new("Info", sig.ident.span()));

    let span_expr = if args.root {
        quote!(::defra_trace::span_or_root(__TRACED_NAME))
    } else {
        quote!(::defra_trace::fastrace::Span::enter_with_local_parent(
            __TRACED_NAME
        ))
    };

    let preamble = quote! {
        const __TRACED_NAME: &str = #name;
        static __TRACED_CALLSITE: ::defra_trace::Callsite = ::defra_trace::Callsite::new(
            ::core::module_path!(),
            ::defra_trace::Level::#level,
        );
        let __traced_span = if __TRACED_CALLSITE.enabled() {
            #span_expr
        } else {
            ::defra_trace::fastrace::Span::noop()
        };
    };

    // An `async_trait` method is async in spirit even though its signature is
    // not, so it takes the future-carrying path.
    let async_trait_inner = async_trait_block(&block);

    let body = if let Some(inner) = async_trait_inner {
        let inner_block = &inner.block;
        let inner_attrs = &inner.attrs;
        quote! {
            #preamble
            use ::defra_trace::fastrace::future::FutureExt as _;
            Box::pin(#(#inner_attrs)* async move #inner_block.in_span(__traced_span))
        }
    } else if sig.asyncness.is_some() {
        // A thread-local parent guard cannot be held across an await, so the
        // future carries the span and re-enters it on each poll.
        quote! {
            #preamble
            use ::defra_trace::fastrace::future::FutureExt as _;
            async move #block.in_span(__traced_span).await
        }
    } else {
        quote! {
            #preamble
            let __traced_guard = __traced_span.set_local_parent();
            #block
        }
    };

    quote!(#(#attrs)* #vis #sig { #body }).into()
}
