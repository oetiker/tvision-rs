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
/// # Errors
///
/// Emits a compile error if `to` is missing or duplicated, if the macro is on a
/// plain `impl` (not `impl Trait for Type`), if the trait is not a known
/// delegatable trait, or if a `skip(...)` name is not a method of that trait.
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

/// Resolve the path prefix for crate `tvision` that is valid at the call site
/// (inside the lib via `extern crate self`, in examples, and downstream under
/// ANY alias the consumer chooses). ALWAYS returns an `::<ident>` form — never `crate`.
fn tvision_path() -> TokenStream2 {
    use proc_macro_crate::{FoundCrate, crate_name};
    let ident = match crate_name("tvision") {
        // `Itself` happens when compiling the tvision lib AND its own examples;
        // `extern crate self as tvision;` makes `::tvision` valid in the lib,
        // and the example's implicit dep makes `::tvision` valid there too.
        Ok(FoundCrate::Itself) => Ident::new("tvision", Span::call_site()),
        Ok(FoundCrate::Name(name)) => Ident::new(&name, Span::call_site()),
        // Fall back to the canonical name; a wrong name yields a clear
        // unresolved-path error rather than a silently wrong expansion.
        Err(_) => Ident::new("tvision", Span::call_site()),
    };
    quote! { ::#ident }
}

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

    let provided: std::collections::HashSet<String> = item_impl
        .items
        .iter()
        .filter_map(|it| match it {
            ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    let skip: std::collections::HashSet<String> = args.skip.iter().map(|i| i.to_string()).collect();

    let krate = tvision_path();
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
