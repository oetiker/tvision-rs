use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, ImplItem, ItemImpl, Token};

mod specs;

/// `#[delegate(to = <field>, skip(method1, method2, ...))]`
///
/// Injects forwarders for every method of the trait that the `impl` block does
/// not already provide, each forwarding to `self.<field>.<method>(<args>)`.
/// This is the boilerplate half of the embed-and-delegate pattern (the port's
/// stand-in for C++ implementation inheritance). Only the `View` trait is
/// currently supported.
///
/// - `to = <field>` (required): the field to forward un-provided methods to.
/// - `skip(...)` (optional): methods to leave at the trait's own default rather
///   than forwarding — use when a delegating type intentionally inherits the
///   default instead of the inner field's behavior.
///
/// # Pitfall: one ViewState
///
/// The delegate field owns the view's `ViewState`. Every forwarded method
/// (`set_state`, `change_bounds`, `calc_bounds`, `focus_to_edge`, ...) works on
/// `self.<field>.state()`. A `state`/`state_mut` override that returns a field
/// of the outer type gives the view a second `ViewState` that those methods
/// never touch: `StateFlag::Focused` lands on the inner state, so the outer
/// `state().state.focused` stays `false` while the view receives keys. The
/// macro therefore rejects such an override, and `skip(state)` /
/// `skip(state_mut)`. Keep the state in the delegate field, or implement the
/// trait by hand without `#[delegate]`.
///
/// # Errors
///
/// Emits a compile error if `to` is missing or duplicated, if the macro is on a
/// plain `impl` (not `impl Trait for Type`), if the trait is not a known
/// delegatable trait, if a `skip(...)` name is not a method of that trait, or if
/// a `View` impl provides or skips `state`/`state_mut`.
#[proc_macro_attribute]
pub fn delegate(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(attr as DelegateArgs);
    let item_impl = syn::parse_macro_input!(item as ItemImpl);
    match expand(args, item_impl) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct DelegateArgs {
    field: Ident,
    skip: Vec<Ident>,
}

impl syn::parse::Parse for DelegateArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut field = None;
        let mut skip = Vec::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if key == "to" {
                if field.is_some() {
                    return Err(syn::Error::new(key.span(), "#[delegate]: duplicate `to`"));
                }
                input.parse::<Token![=]>()?;
                field = Some(input.parse()?);
            } else if key == "skip" {
                let content;
                syn::parenthesized!(content in input);
                let names = content.parse_terminated(Ident::parse, Token![,])?;
                skip.extend(names);
            } else {
                return Err(syn::Error::new(key.span(), "expected `to` or `skip`"));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        let field = field.ok_or_else(|| {
            syn::Error::new(Span::call_site(), "#[delegate]: missing `to = <field>`")
        })?;
        Ok(DelegateArgs { field, skip })
    }
}

/// Resolve the path prefix for crate `tvision-rs` that is valid at the call site
/// (inside the lib via `extern crate self`, in examples, and downstream under
/// ANY alias the consumer chooses). ALWAYS returns an `::<ident>` form — never `crate`.
fn tvision_rs_path() -> TokenStream2 {
    use proc_macro_crate::{FoundCrate, crate_name};
    let ident = match crate_name("tvision-rs") {
        // `Itself` happens when compiling the tvision-rs lib AND its own examples;
        // `extern crate self as tvision_rs;` makes `::tvision_rs` valid in the lib,
        // and the example's implicit dep makes `::tvision_rs` valid there too.
        Ok(FoundCrate::Itself) => Ident::new("tvision_rs", Span::call_site()),
        Ok(FoundCrate::Name(name)) => Ident::new(&name, Span::call_site()),
        // Fall back to the canonical name; a wrong name yields a clear
        // unresolved-path error rather than a silently wrong expansion.
        Err(_) => Ident::new("tvision_rs", Span::call_site()),
    };
    quote! { ::#ident }
}

/// `View` accessors a `#[delegate]` impl must never provide or skip: the
/// delegate field owns the one ViewState.
const STATE_ACCESSORS: [&str; 2] = ["state", "state_mut"];

fn expand(args: DelegateArgs, mut item_impl: ItemImpl) -> syn::Result<TokenStream2> {
    let trait_path = item_impl.trait_.as_ref().ok_or_else(|| {
        syn::Error::new_spanned(
            &item_impl.self_ty,
            "#[delegate] must be placed on an `impl Trait for Type` block",
        )
    })?;
    let trait_ident = trait_path
        .1
        .segments
        .last()
        .expect("an impl trait path has at least one segment")
        .ident
        .to_string();

    // One ViewState: the delegate field owns it. A local `state`/`state_mut`
    // would give the outer view a second ViewState that every forwarded method
    // (set_state, change_bounds, calc_bounds, ...) ignores, so focus and geometry
    // silently land on the inner view. Reject it rather than expand into that.
    if trait_ident == "View" {
        let field = &args.field;
        let mut err: Option<syn::Error> = None;
        for it in &item_impl.items {
            if let ImplItem::Fn(f) = it
                && STATE_ACCESSORS.contains(&f.sig.ident.to_string().as_str())
            {
                let e = syn::Error::new_spanned(
                    &f.sig.ident,
                    format!(
                        "#[delegate(to = {field})]: the delegate field owns the ViewState; \
                         overriding `{name}` here gives the view a second ViewState that every \
                         forwarded method (set_state, change_bounds, calc_bounds, ...) ignores. \
                         Either drop the override and keep the state in `{field}`, or implement \
                         View by hand without #[delegate].",
                        name = f.sig.ident
                    ),
                );
                match &mut err {
                    Some(acc) => acc.combine(e),
                    None => err = Some(e),
                }
            }
        }
        for s in &args.skip {
            if STATE_ACCESSORS.contains(&s.to_string().as_str()) {
                let e = syn::Error::new(
                    s.span(),
                    format!(
                        "#[delegate(to = {field})]: `skip({s})` is not allowed; `{s}` is a \
                         required View method and must forward to the delegate field, which \
                         owns the ViewState"
                    ),
                );
                match &mut err {
                    Some(acc) => acc.combine(e),
                    None => err = Some(e),
                }
            }
        }
        if let Some(e) = err {
            return Err(e);
        }
    }

    let provided: std::collections::HashSet<String> = item_impl
        .items
        .iter()
        .filter_map(|it| match it {
            ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    let skip: std::collections::HashSet<String> = args.skip.iter().map(|i| i.to_string()).collect();

    let krate = tvision_rs_path();
    let field = &args.field;

    let candidates = specs::forwarders(&trait_ident, field, &krate).ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            format!("#[delegate]: unknown delegatable trait `{trait_ident}`"),
        )
    })?;

    // A `skip(...)` name that is not a method of the trait is almost certainly a
    // typo, and would silently forward instead of skipping — make it a hard error.
    let known: std::collections::HashSet<&str> = candidates.iter().map(|(n, _)| *n).collect();
    for s in &args.skip {
        if !known.contains(s.to_string().as_str()) {
            return Err(syn::Error::new(
                s.span(),
                format!("#[delegate]: `skip({s})` is not a method of trait `{trait_ident}`"),
            ));
        }
    }

    for (name, tokens) in candidates {
        if !provided.contains(name) && !skip.contains(name) {
            let f: ImplItem = syn::parse2(tokens)?;
            item_impl.items.push(f);
        }
    }
    Ok(quote! { #item_impl })
}
