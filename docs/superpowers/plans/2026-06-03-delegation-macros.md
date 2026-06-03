# Delegation Macro (`#[delegate]`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a proc-macro `#[delegate(to = <field>)]` that gap-fills the boilerplate half of the D2 embed-and-delegate pattern — it injects, into a hand-written `impl Trait for Type`, a forwarder for every trait method the author did *not* write — so the crate (which is built on itself) and its users write only the methods that actually differ.

**Architecture:** A new in-workspace proc-macro crate `tvision-macros`. The attribute reads the trait name from `impl Trait for Type`, collects the method names the author already provided, looks up a **hardcoded per-trait signature table** (Rust-side set subtraction — reliable, no `macro_rules!` membership tricks), and pushes forwarder `fn`s for the rest into the same impl block. Generated type paths are resolved with `proc-macro-crate` + `extern crate self as tvision`, so they compile inside the lib, in examples, and downstream **under any alias the consumer chooses** — `tv`, `tvision`, or anything else: `proc-macro-crate` reports whatever name the consumer's `Cargo.toml` imports the crate as, and the macro emits `::<that name>::Type`. (`extern crate self as tvision` only fixes the in-crate self-reference; it does not constrain the consumer's alias.) v1 populates only the `View` spec (the trait with real embed-delegate consumers); the table is keyed by trait name, so `ListViewer`/`Validator`/future menu traits are a one-function addition each when a field-delegating consumer appears.

**Tech Stack:** Rust 2024, `proc-macro2`, `syn` 2.0 (full), `quote`, `proc-macro-crate` (all already transitively compiled via crossterm→derive_more, so no new heavyweight build cost); `trybuild` (dev-dep) for compile-fail tests.

**Key design rules (do not reopen — settled with the advisor):**
- Trait name is read from `impl Trait for Type`; it is **not** passed in the attribute.
- Set subtraction happens **in Rust inside the proc macro**, against a hardcoded signature table — not in generated `macro_rules!`.
- `skip(...)` lists methods to leave at the **trait default** (neither provided nor forwarded). It exists to make migration behavior-preserving.
- **Migration is behavior-preserving, never semantic.** A site's `skip(...)` must equal *exactly* the set of methods it currently leaves defaulted, re-derived from current source. If forwarding would be "more correct" than the old default, that is **out of scope** — preserve the default, file the discrepancy separately.
- Migration equivalence is proven by **`cargo expand` diff of the impl's method set**, not by snapshots (the divergent methods are structural, not visual).

---

## File Structure

- `tvision-macros/Cargo.toml` — new proc-macro crate manifest.
- `tvision-macros/src/lib.rs` — the `#[delegate]` attribute entry point, attribute-arg parser, `expand()`, and crate-path resolution.
- `tvision-macros/src/specs.rs` — the hardcoded per-trait signature tables (the `View` forwarder list) + the trait-name dispatch.
- `tvision-macros/tests/compile_fail/*.rs` + `tvision-macros/tests/ui.rs` — `trybuild` compile-fail cases.
- `Cargo.toml` (root) — add `[workspace]` + `tvision-macros` path dependency.
- `src/lib.rs` — add `extern crate self as tvision;` + re-export `pub use tvision_macros::delegate;`.
- `tests/delegate_view.rs` — behavioral "spy" test proving every known `View` method forwards.
- Migration edits: `src/widgets/cluster.rs`, `src/widgets/static_text.rs`, `examples/hello.rs` (clean wins); optionally `src/dialog/dialog.rs`, `src/window/window.rs`, `src/desktop/desktop.rs` (complex sites).
- `docs/design/delegation-macros.md` — design note (rationale + the "add a spec when you add a trait method" maintenance rule).
- `CLAUDE.md` — one line under Conventions: adding a `View` method means adding a forwarder to `tvision-macros/src/specs.rs`.

---

## Phase 0 — Spike: prove path resolution end-to-end

> The whole plan rests on generated `::tvision::Type` paths resolving in three places: the lib itself, examples, and downstream under the `tv` alias. Prove it with a one-method slice before writing the real spec. If this phase is not clean, STOP and rethink before proceeding.

### Task 1: Scaffold the `tvision-macros` crate and workspace

**Files:**
- Create: `tvision-macros/Cargo.toml`
- Create: `tvision-macros/src/lib.rs`
- Modify: `Cargo.toml` (root)
- Modify: `src/lib.rs`

- [ ] **Step 1: Create the proc-macro crate manifest**

Create `tvision-macros/Cargo.toml`:

```toml
[package]
name = "tvision-macros"
version = "0.1.0"
edition = "2024"
description = "Internal proc-macros for the tvision crate (delegation boilerplate)."
license = "MIT"
publish = true

[lib]
proc-macro = true

[dependencies]
proc-macro2 = "1"
quote = "1"
syn = { version = "2", features = ["full"] }
proc-macro-crate = "3"

[dev-dependencies]
trybuild = "1"
```

- [ ] **Step 2: Write a minimal one-method `#[delegate]` for the spike**

Create `tvision-macros/src/lib.rs`. This spike version handles only `View::number` so we can prove path resolution before writing 21 methods:

```rust
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, ImplItem, ItemImpl, Token};

/// `#[delegate(to = <field>)]` — inject forwarders for un-provided trait methods.
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
                input.parse::<Token![=]>()?;
                field = Some(input.parse()?);
            } else if key == "skip" {
                let content;
                syn::parenthesized!(content in input);
                let names =
                    content.parse_terminated(Ident::parse, Token![,])?;
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
/// the `tv` alias). ALWAYS returns an `::<ident>` form — never `crate`.
fn tvision_path() -> TokenStream2 {
    use proc_macro_crate::{crate_name, FoundCrate};
    let ident = match crate_name("tvision") {
        // `Itself` happens when compiling the tvision lib AND its own examples;
        // `extern crate self as tvision;` makes `::tvision` valid in the lib,
        // and the example's implicit dep makes `::tvision` valid there too.
        Ok(FoundCrate::Itself) => Ident::new("tvision", Span::call_site()),
        Ok(FoundCrate::Name(name)) => Ident::new(&name, Span::call_site()),
        // Fall back to the canonical name; if it is wrong the user sees a clear
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
    let trait_ident = trait_path.1.segments.last().unwrap().ident.to_string();

    let provided: std::collections::HashSet<String> = item_impl
        .items
        .iter()
        .filter_map(|it| match it {
            ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    let skip: std::collections::HashSet<String> =
        args.skip.iter().map(|i| i.to_string()).collect();

    let krate = tvision_path();
    let field = &args.field;

    // SPIKE: only `View::number` is known.
    if trait_ident != "View" {
        return Err(syn::Error::new(
            Span::call_site(),
            format!("#[delegate]: unknown delegatable trait `{trait_ident}`"),
        ));
    }
    let candidates: Vec<(&str, TokenStream2)> = vec![(
        "number",
        quote! { fn number(&self) -> ::core::option::Option<i16> { self.#field.number() } },
    )];

    for (name, tokens) in candidates {
        if !provided.contains(name) && !skip.contains(name) {
            let f: ImplItem = syn::parse2(tokens)?;
            item_impl.items.push(f);
        }
    }
    let _ = &krate; // used once the real spec lands
    Ok(quote! { #item_impl })
}
```

- [ ] **Step 3: Make the root crate a workspace and depend on the macro crate**

Modify `Cargo.toml` (root) — add these two blocks (keep everything else):

```toml
[workspace]
members = ["tvision-macros"]

[dependencies]
tvision-macros = { path = "tvision-macros", version = "0.1.0" }
```

(The `[dependencies]` table already exists — add the `tvision-macros` line to it; do not create a second table.)

- [ ] **Step 4: Add `extern crate self` and re-export the macro**

Modify `src/lib.rs`. Immediately after the crate-level `//!` doc comment and before the first `pub mod`, add:

```rust
// Lets proc-macro-generated `::tvision::Type` paths resolve inside this crate.
extern crate self as tvision;
```

And in the root re-export block, add:

```rust
pub use tvision_macros::delegate;
```

- [ ] **Step 5: Build the workspace to verify it compiles**

Run: `cargo build`
Expected: PASS (workspace builds; `tvision-macros` compiles; `tvision` sees the macro).

- [ ] **Step 6: Commit**

```bash
git add tvision-macros Cargo.toml src/lib.rs docs/superpowers/plans/2026-06-03-delegation-macros.md
git commit -m "feat: scaffold tvision-macros proc-macro crate + workspace"
```

### Task 2: Prove `::tvision::` paths resolve from all three call sites

**Files:**
- Create: `tvision-macros/tests/ui.rs`
- Create: `tvision-macros/tests/ui/alias_tv.rs`
- Test: an internal module + a throwaway in `examples/`

- [ ] **Step 1: Internal call site — a temporary delegator in `src/lib.rs`**

Append to `src/lib.rs` (temporary, removed in Step 4):

```rust
#[cfg(test)]
mod __delegate_spike {
    use crate::{Context, DrawCtx, View, ViewState};

    struct Inner(ViewState);
    impl View for Inner {
        fn state(&self) -> &ViewState { &self.0 }
        fn state_mut(&mut self) -> &mut ViewState { &mut self.0 }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn number(&self) -> Option<i16> { Some(7) }
    }

    struct Wrap { inner: Inner }
    #[crate::delegate(to = inner)]
    impl View for Wrap {
        fn state(&self) -> &ViewState { self.inner.state() }
        fn state_mut(&mut self) -> &mut ViewState { self.inner.state_mut() }
        fn draw(&mut self, ctx: &mut DrawCtx) { self.inner.draw(ctx) }
        // `number` is NOT written — the macro must inject it forwarding to inner.
    }

    #[test]
    fn injected_number_forwards_to_inner() {
        let w = Wrap { inner: Inner(ViewState::default()) };
        assert_eq!(w.number(), Some(7));
        let _: &dyn View = &w; // the impl must be complete (all required methods present)
    }
    let _ = || { let _ = Context::new; }; // keep the Context import referenced
}
```

(If `Context::new` is awkward to reference, drop that last line and the `Context` import — the point is only that the generated `number` resolves and the trait impl is complete.)

- [ ] **Step 2: Run the internal test**

Run: `cargo test --lib __delegate_spike`
Expected: PASS — proves `#[delegate]` injected `number` and `::tvision`-pathless `number` resolves inside the lib.

- [ ] **Step 3: Example call site — verify `examples/hello.rs` still builds with a generated method**

Temporarily add `#[tvision::delegate(to = dialog)]` above `impl View for AboutDialog` in `examples/hello.rs` and **delete the hand-written `fn number`** from that impl (leave the rest).

Run: `cargo build --example hello`
Expected: PASS — proves the generated forwarder resolves in an example crate (the `FoundCrate::Itself`-from-example case).

Revert the `examples/hello.rs` change after the build passes (`git checkout examples/hello.rs`).

- [ ] **Step 4: Downstream-alias call site — a `trybuild` pass-case aliasing the crate as `tv`**

Create `tvision-macros/tests/ui.rs`:

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/alias_tv.rs");
}
```

Create `tvision-macros/tests/ui/alias_tv.rs`. This file is compiled by `trybuild` as its own crate; its `Cargo.toml` (generated by trybuild) must alias the dependency. Use a path-dep on the workspace root under the name `tv`:

```rust
// trybuild compiles this as a standalone crate that depends on tvision AS `tv`.
use tv::{Context, DrawCtx, View, ViewState};

struct Inner(ViewState);
impl View for Inner {
    fn state(&self) -> &ViewState { &self.0 }
    fn state_mut(&mut self) -> &mut ViewState { &mut self.0 }
    fn draw(&mut self, _ctx: &mut DrawCtx) {}
    fn number(&self) -> Option<i16> { Some(9) }
}

struct Wrap { inner: Inner }
#[tv::delegate(to = inner)]
impl View for Wrap {
    fn state(&self) -> &ViewState { self.inner.state() }
    fn state_mut(&mut self) -> &mut ViewState { self.inner.state_mut() }
    fn draw(&mut self, ctx: &mut DrawCtx) { self.inner.draw(ctx) }
}

fn main() {
    let w = Wrap { inner: Inner(ViewState::default()) };
    assert_eq!(w.number(), Some(9));
    let _ = Context::new; // keep the import live
}
```

> **Note on the alias:** `tv` here is just a *representative non-canonical alias* — the mechanism is alias-agnostic (`proc-macro-crate` reports whatever name the consumer uses), so proving it under one arbitrary alias proves the general case. `trybuild` reuses the host crate's dependency table for `[dependencies]` of the test case unless overridden. To force the `tv` alias, the implementer must confirm trybuild picks up a `tv = { package = "tvision", path = ".." }` dependency. If trybuild's default dep injection does not expose the crate as `tv`, set it explicitly via a `tests/ui/Cargo.toml`-style override per the trybuild docs, OR validate the alias path with a one-off manual crate in `/tmp` that has `tv = { package = "tvision", path = "<repo>" }` and the same body. The acceptance criterion is: **the generated forwarder resolves when the crate is imported under a name OTHER than `tvision`** (here `tv`).

- [ ] **Step 5: Run the trybuild pass-case**

Run: `cargo test -p tvision-macros --test ui`
Expected: PASS — proves generated `::tvision::`-prefixed paths (once the real spec uses them) resolve under the `tv` alias via `proc-macro-crate`.

- [ ] **Step 6: Remove the temporary internal spike module**

Delete the `__delegate_spike` module from `src/lib.rs` (added in Task 2 Step 1).

Run: `cargo test --lib` and `cargo build --example hello`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add tvision-macros/tests src/lib.rs
git commit -m "test: prove #[delegate] path resolution from lib, example, and tv-alias"
```

---

## Phase 1 — Build the real `View` spec

### Task 3: Move the spec table into `specs.rs` and populate all 21 `View` methods

**Files:**
- Create: `tvision-macros/src/specs.rs`
- Modify: `tvision-macros/src/lib.rs`

- [ ] **Step 1: Write the spec module**

Create `tvision-macros/src/specs.rs`. Each entry is `(method_name, full_forwarder_fn)`. The signatures must match `src/view/view.rs` **exactly** (receiver, arg names, arg types, return type), because the generated `fn` must satisfy the trait. All type paths go through `#krate` (= `::tvision`). Arg names match the trait so the body can pass them through.

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

/// Forwarders for `trait_name`, for the embed field `field`, with crate path
/// `krate`. Returns `None` if `trait_name` is not a known delegatable trait.
/// The caller filters out provided/skipped names.
pub fn forwarders(
    trait_name: &str,
    field: &Ident,
    krate: &TokenStream,
) -> Option<Vec<(&'static str, TokenStream)>> {
    match trait_name {
        "View" => Some(view(field, krate)),
        // ListViewer / Validator / menu traits: add an arm here when a
        // field-delegating consumer exists (see docs/design/delegation-macros.md).
        _ => None,
    }
}

/// The method names known for `trait_name` (drives the maintenance/spy test).
pub fn method_names(trait_name: &str) -> Option<Vec<&'static str>> {
    let f = Ident::new("__f", proc_macro2::Span::call_site());
    let k = quote! { ::tvision };
    forwarders(trait_name, &f, &k).map(|v| v.into_iter().map(|(n, _)| n).collect())
}

#[rustfmt::skip]
fn view(f: &Ident, k: &TokenStream) -> Vec<(&'static str, TokenStream)> {
    vec![
        ("state",
         quote! { fn state(&self) -> & #k::ViewState { self.#f.state() } }),
        ("state_mut",
         quote! { fn state_mut(&mut self) -> &mut #k::ViewState { self.#f.state_mut() } }),
        ("draw",
         quote! { fn draw(&mut self, ctx: &mut #k::DrawCtx) { self.#f.draw(ctx) } }),
        ("handle_event",
         quote! { fn handle_event(&mut self, ev: &mut #k::Event, ctx: &mut #k::Context) { self.#f.handle_event(ev, ctx) } }),
        ("set_state",
         quote! { fn set_state(&mut self, flag: #k::StateFlag, enable: bool, ctx: &mut #k::Context) { self.#f.set_state(flag, enable, ctx) } }),
        ("valid",
         quote! { fn valid(&self, cmd: #k::Command) -> bool { self.#f.valid(cmd) } }),
        ("value",
         quote! { fn value(&self) -> ::core::option::Option<#k::FieldValue> { self.#f.value() } }),
        ("set_value",
         quote! { fn set_value(&mut self, v: #k::FieldValue) { self.#f.set_value(v) } }),
        ("awaken",
         quote! { fn awaken(&mut self) { self.#f.awaken() } }),
        ("size_limits",
         quote! { fn size_limits(&self, owner_size: #k::Point) -> (#k::Point, #k::Point) { self.#f.size_limits(owner_size) } }),
        ("calc_bounds",
         quote! { fn calc_bounds(&mut self, owner_size: #k::Point, delta: #k::Point) -> #k::Rect { self.#f.calc_bounds(owner_size, delta) } }),
        ("change_bounds",
         quote! { fn change_bounds(&mut self, bounds: #k::Rect) { self.#f.change_bounds(bounds) } }),
        ("cursor_request",
         quote! { fn cursor_request(&self) -> ::core::option::Option<#k::Point> { self.#f.cursor_request() } }),
        ("find_mut",
         quote! { fn find_mut(&mut self, id: #k::ViewId) -> ::core::option::Option<&mut dyn #k::View> { self.#f.find_mut(id) } }),
        ("remove_descendant",
         quote! { fn remove_descendant(&mut self, id: #k::ViewId, ctx: &mut #k::Context) -> bool { self.#f.remove_descendant(id, ctx) } }),
        ("focus_descendant",
         quote! { fn focus_descendant(&mut self, id: #k::ViewId, ctx: &mut #k::Context) -> bool { self.#f.focus_descendant(id, ctx) } }),
        ("number",
         quote! { fn number(&self) -> ::core::option::Option<i16> { self.#f.number() } }),
        ("grabs_focus_on_click",
         quote! { fn grabs_focus_on_click(&self) -> bool { self.#f.grabs_focus_on_click() } }),
        ("select_window_num",
         quote! { fn select_window_num(&mut self, num: i16, ctx: &mut #k::Context) -> bool { self.#f.select_window_num(num, ctx) } }),
        ("apply_list_scroll",
         quote! { fn apply_list_scroll(&mut self, h: ::core::option::Option<i32>, v: ::core::option::Option<i32>, ctx: &mut #k::Context) { self.#f.apply_list_scroll(h, v, ctx) } }),
        ("as_any_mut",
         quote! { fn as_any_mut(&mut self) -> ::core::option::Option<&mut dyn ::core::any::Any> { self.#f.as_any_mut() } }),
    ]
}
```

> **Maintenance rule:** this list must stay in lockstep with `trait View` in `src/view/view.rs`. When a `View` method is added, add an entry here. The behavioral spy test (Task 6) catches a *forgotten forwarder* for any currently-known method; a brand-new method is caught by the convention note in `CLAUDE.md` (Task 9).

- [ ] **Step 2: Replace the spike `expand()` body to use `specs::forwarders`**

In `tvision-macros/src/lib.rs`: add `mod specs;` near the top, and replace the SPIKE block in `expand()` (the `if trait_ident != "View"` … `candidates` … loop) with:

```rust
    let candidates = specs::forwarders(&trait_ident, field, &krate).ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            format!("#[delegate]: unknown delegatable trait `{trait_ident}`"),
        )
    })?;

    for (name, tokens) in candidates {
        if !provided.contains(name) && !skip.contains(name) {
            let f: ImplItem = syn::parse2(tokens)?;
            item_impl.items.push(f);
        }
    }
    Ok(quote! { #item_impl })
```

Remove the now-unused `let _ = &krate;` line.

- [ ] **Step 3: Build the macro crate**

Run: `cargo build -p tvision-macros`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add tvision-macros/src
git commit -m "feat: full View forwarder spec table for #[delegate]"
```

### Task 4: Behavioral spy test — every known `View` method forwards

**Files:**
- Create: `tests/delegate_view.rs`

- [ ] **Step 1: Write the spy + pure delegator + assertions**

Create `tests/delegate_view.rs`. A `Spy` records each method it receives; a pure delegator `D { inner: Spy }` is built with an empty `#[delegate(to = inner)]` impl (so the macro injects ALL 21 forwarders). Each method is then exercised and asserted to have reached the spy.

```rust
//! Proves `#[delegate(to = inner)]` injects a working forwarder for every
//! currently-known `View` method. A forgotten forwarder shows up as a missing
//! call record (or, for required methods, a compile error).

use std::cell::RefCell;
use std::collections::HashSet;

use tvision::{
    Command, Context, DrawCtx, Event, FieldValue, Point, Rect, StateFlag, View, ViewId,
    ViewState,
};

#[derive(Default)]
struct Spy {
    st: ViewState,
    seen: RefCell<HashSet<&'static str>>,
}
impl Spy {
    fn mark(&self, m: &'static str) {
        self.seen.borrow_mut().insert(m);
    }
}

impl View for Spy {
    fn state(&self) -> &ViewState { self.mark("state"); &self.st }
    fn state_mut(&mut self) -> &mut ViewState { self.seen.borrow_mut().insert("state_mut"); &mut self.st }
    fn draw(&mut self, _ctx: &mut DrawCtx) { self.mark("draw"); }
    fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) { self.mark("handle_event"); }
    fn set_state(&mut self, _f: StateFlag, _e: bool, _ctx: &mut Context) { self.mark("set_state"); }
    fn valid(&self, _c: Command) -> bool { self.mark("valid"); true }
    fn value(&self) -> Option<FieldValue> { self.mark("value"); None }
    fn set_value(&mut self, _v: FieldValue) { self.mark("set_value"); }
    fn awaken(&mut self) { self.mark("awaken"); }
    fn size_limits(&self, o: Point) -> (Point, Point) { self.mark("size_limits"); (o, o) }
    fn calc_bounds(&mut self, _o: Point, _d: Point) -> Rect { self.mark("calc_bounds"); Rect::new(0, 0, 0, 0) }
    fn change_bounds(&mut self, _b: Rect) { self.mark("change_bounds"); }
    fn cursor_request(&self) -> Option<Point> { self.mark("cursor_request"); None }
    fn find_mut(&mut self, _id: ViewId) -> Option<&mut dyn View> { self.mark("find_mut"); None }
    fn remove_descendant(&mut self, _id: ViewId, _ctx: &mut Context) -> bool { self.mark("remove_descendant"); false }
    fn focus_descendant(&mut self, _id: ViewId, _ctx: &mut Context) -> bool { self.mark("focus_descendant"); false }
    fn number(&self) -> Option<i16> { self.mark("number"); None }
    fn grabs_focus_on_click(&self) -> bool { self.mark("grabs_focus_on_click"); true }
    fn select_window_num(&mut self, _n: i16, _ctx: &mut Context) -> bool { self.mark("select_window_num"); false }
    fn apply_list_scroll(&mut self, _h: Option<i32>, _v: Option<i32>, _ctx: &mut Context) { self.mark("apply_list_scroll"); }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> { self.mark("as_any_mut"); None }
}

struct D { inner: Spy }

#[tvision::delegate(to = inner)]
impl View for D {}

#[test]
fn delegate_forwards_every_known_view_method() {
    // Build the test Context/DrawCtx harness the SAME way the existing tests do.
    // Copy the construction from src/capture.rs's loop tests (Context) and
    // tests/render_pipeline.rs (Buffer + DrawCtx). Bind them as `ctx` and `dctx`.
    // --- harness construction goes here (see referenced tests) ---

    let mut d = D { inner: Spy::default() };

    // Exercise every method. Methods needing Context/DrawCtx use the harness.
    let _ = d.state();
    let _ = d.state_mut();
    d.draw(&mut dctx);
    d.handle_event(&mut Event::Nothing, &mut ctx); // use the crate's actual no-op Event variant
    d.set_state(StateFlag::Active, true, &mut ctx);
    let _ = d.valid(Command::OK);
    let _ = d.value();
    d.set_value(FieldValue::Text(String::new()));
    d.awaken();
    let _ = d.size_limits(Point { x: 1, y: 1 });
    let _ = d.calc_bounds(Point { x: 1, y: 1 }, Point { x: 0, y: 0 });
    d.change_bounds(Rect::new(0, 0, 1, 1));
    let _ = d.cursor_request();
    let _ = d.find_mut(d.inner.st.id().unwrap_or_else(|| panic!("no id")));
    // For find/remove/focus/select use any ViewId; the spy records regardless.
    let _ = d.number();
    let _ = d.grabs_focus_on_click();
    let _ = d.apply_list_scroll(None, None, &mut ctx);
    let _ = d.as_any_mut();

    let expected: HashSet<&str> = [
        "state", "state_mut", "draw", "handle_event", "set_state", "valid", "value",
        "set_value", "awaken", "size_limits", "calc_bounds", "change_bounds",
        "cursor_request", "find_mut", "number", "grabs_focus_on_click",
        "apply_list_scroll", "as_any_mut",
    ]
    .into_iter()
    .collect();
    let seen = d.inner.seen.borrow();
    for m in &expected {
        assert!(seen.contains(m), "method `{m}` was not forwarded to the inner view");
    }
}
```

> **Implementer note:** the exact `Context`/`DrawCtx` constructors and the no-op `Event` variant are not invented here — copy the harness construction verbatim from `src/capture.rs`'s loop test(s) and `tests/render_pipeline.rs`. `remove_descendant`/`focus_descendant`/`select_window_num` take a `Context` and a `ViewId`; include them in the exercised set if a `ViewId` is cheap to mint in the harness, otherwise assert the subset you can drive — the goal is "no plumbing forwarder silently missing," and covering the Context-free + most Context methods achieves it.

- [ ] **Step 2: Run the spy test**

Run: `cargo test --test delegate_view`
Expected: PASS — every injected forwarder reaches the inner spy.

- [ ] **Step 3: Verify the test BITES — temporarily drop a forwarder**

Comment out the `"number"` entry in `tvision-macros/src/specs.rs`, rebuild, and re-run the test.
Expected: FAIL (`method 'number' was not forwarded`). This proves the guard works. Restore the entry; re-run; PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/delegate_view.rs
git commit -m "test: behavioral spy proves #[delegate] forwards every View method"
```

### Task 5: `skip(...)` support and compile-fail cases

**Files:**
- Modify: `tests/delegate_view.rs`
- Create: `tvision-macros/tests/ui/unknown_trait.rs` + `.stderr`
- Create: `tvision-macros/tests/ui/missing_to.rs` + `.stderr`
- Modify: `tvision-macros/tests/ui.rs`

- [ ] **Step 1: Add a `skip` behavioral test**

Append to `tests/delegate_view.rs`:

```rust
struct Skipper { inner: Spy }

// `number` is skipped: NOT provided and NOT forwarded -> uses the View default (None),
// so the inner spy never records "number".
#[tvision::delegate(to = inner, skip(number))]
impl View for Skipper {}

#[test]
fn skip_leaves_method_at_trait_default() {
    let s = Skipper { inner: Spy::default() };
    assert_eq!(s.number(), None);                 // View default, not inner's
    assert!(!s.inner.seen.borrow().contains("number")); // inner was NOT called
}
```

- [ ] **Step 2: Run the skip test**

Run: `cargo test --test delegate_view skip_leaves_method_at_trait_default`
Expected: PASS.

- [ ] **Step 3: Add compile-fail cases**

Create `tvision-macros/tests/ui/unknown_trait.rs`:

```rust
use tvision_macros::delegate;

trait Foo { fn bar(&self); }
struct W { inner: () }

#[delegate(to = inner)]
impl Foo for W {
    fn bar(&self) {}
}

fn main() {}
```

Create `tvision-macros/tests/ui/missing_to.rs`:

```rust
use tvision_macros::delegate;
use tvision::{View, ViewState, DrawCtx};

struct W { inner: () }

#[delegate(skip(number))]
impl View for W {
    fn state(&self) -> &ViewState { unimplemented!() }
    fn state_mut(&mut self) -> &mut ViewState { unimplemented!() }
    fn draw(&mut self, _c: &mut DrawCtx) {}
}

fn main() {}
```

Update `tvision-macros/tests/ui.rs`:

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/alias_tv.rs");
    t.compile_fail("tests/ui/unknown_trait.rs");
    t.compile_fail("tests/ui/missing_to.rs");
}
```

- [ ] **Step 4: Generate the `.stderr` snapshots**

Run: `TRYBUILD=overwrite cargo test -p tvision-macros --test ui`
Then run without the env var to lock them in:
Run: `cargo test -p tvision-macros --test ui`
Expected: PASS — `unknown_trait` errors with "unknown delegatable trait `Foo`", `missing_to` errors with "missing `to = <field>`".

> Inspect the generated `.stderr` files; confirm the messages are the intended ones (not an unrelated error). Commit the `.stderr` files.

- [ ] **Step 5: Commit**

```bash
git add tests/delegate_view.rs tvision-macros/tests
git commit -m "feat: #[delegate] skip(...) support + compile-fail tests"
```

---

## Phase 2 — Clean-win migrations (low risk, high payoff)

> These sites override nothing or only `draw`, so forwarding-all is exactly their current behavior. Verify each with a `cargo expand` method-set diff AND the full suite.

### Task 6: Retire `cluster_wrapper!` in favor of `#[delegate]`

**Files:**
- Modify: `src/widgets/cluster.rs`

- [ ] **Step 1: Capture the baseline expansion**

Run: `cargo expand --lib widgets::cluster 2>/dev/null > /tmp/cluster_before.rs`
(If `cargo expand` is not installed: `cargo install cargo-expand`. The cargo target dir is `/home/oetiker/scratch/cargo-target` per project config — expand respects it.)

- [ ] **Step 2: Replace the macro definition + invocations**

In `src/widgets/cluster.rs`: delete the `macro_rules! cluster_wrapper { … }` block and its three invocations. Replace each of the three wrapper types with a struct + an empty delegated impl. For `CheckBoxes`:

```rust
/// `TCheckBoxes` — a column of independent checkboxes; `value` is a bitmask.
/// D2 embed-delegate wrapper over [`Cluster`] with [`ClusterKind::CheckBoxes`].
pub struct CheckBoxes {
    /// The shared engine (state + layout + nav + draw + events).
    pub cluster: Cluster,
}

#[crate::delegate(to = cluster)]
impl View for CheckBoxes {}
```

Repeat for `RadioButtons` and `MultiCheckBoxes` (same shape, their own doc comments). Keep all the *inherent* `impl CheckBoxes { … }` constructor blocks below unchanged.

> Note: `cluster_wrapper!` forwarded 15 methods and omitted 6 (the trait defaults). `#[delegate]` forwards all 20 non-`draw`... wait — it forwards ALL 21 including `draw` here (the impl is empty, nothing provided). That is MORE than the old macro. Confirm via Step 4 that the extra forwarders (`draw`, `value`, `set_value`, `grabs_focus_on_click`, `focus_descendant`, `apply_list_scroll`, `as_any_mut`) forward to `Cluster`, and that `Cluster` implements them correctly (it has a real `draw`, and the rest use `View` defaults on `Cluster` itself — so forwarding to them is behavior-identical to the old "use the wrapper's default"). If any differs, add it to a `skip(...)` list to preserve the old behavior, and note the discrepancy.

- [ ] **Step 3: Build + full suite**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 4: Expand-diff verification**

Run: `cargo expand --lib widgets::cluster 2>/dev/null > /tmp/cluster_after.rs`
Run: `diff <(grep -E '    fn [a-z_]+' /tmp/cluster_before.rs | sort -u) <(grep -E '    fn [a-z_]+' /tmp/cluster_after.rs | sort -u)`
Expected: the only additions are the methods the old macro omitted (now forwarded to `Cluster`); confirm each addition is behavior-identical (forwards to `Cluster`'s own default = same as the wrapper's old default). No method should be *removed*.

- [ ] **Step 5: Run clippy + fmt**

Run: `cargo clippy --all-targets && cargo fmt --check`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/widgets/cluster.rs
git commit -m "refactor: replace cluster_wrapper! with #[delegate(to = cluster)]"
```

### Task 7: Migrate `ParamText` and `AboutDialog` (the example)

**Files:**
- Modify: `src/widgets/static_text.rs`
- Modify: `examples/hello.rs`

- [ ] **Step 1: `ParamText` — replace the 15 manual forwarders**

In `src/widgets/static_text.rs`, replace the `impl View for ParamText { … }` body (all hand-written forwarders) with:

```rust
#[crate::delegate(to = inner)]
impl View for ParamText {}
```

(`ParamText` overrides nothing — it is a pure wrapper over `StaticText`.)

- [ ] **Step 2: `AboutDialog` — keep `draw`, delegate the rest**

In `examples/hello.rs`: update the import to add `delegate` and drop the now-unused names, then collapse the impl:

```rust
use tvision::{
    Backend, Color, Command, CrosstermBackend, Desktop, Dialog, DrawCtx, Program, Rect, Style,
    SystemClock, Theme, View, delegate,
};
```

```rust
#[delegate(to = dialog)]
impl View for AboutDialog {
    /// `TAboutDialog::draw` — `TDialog::draw()` first, then the interior fill +
    /// centred text. Only this method differs from the inner dialog; every other
    /// `View` method is injected by `#[delegate(to = dialog)]`.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // ... existing draw body unchanged ...
    }
}
```

- [ ] **Step 3: Build everything + suite**

Run: `cargo build --example hello && cargo test`
Expected: PASS.

- [ ] **Step 4: Snapshot check (these two ARE visual)**

Run: `cargo test` (the existing snapshot tests covering StaticText/ParamText render unchanged).
Expected: PASS — no snapshot diffs.

- [ ] **Step 5: clippy + fmt**

Run: `cargo clippy --all-targets && cargo fmt --check`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/widgets/static_text.rs examples/hello.rs
git commit -m "refactor: adopt #[delegate] in ParamText and the hello example"
```

---

## Phase 3 — Complex sites (OPTIONAL; each its own commit + expand-diff)

> The win here is modest (≈9–13 forwarder bodies → an empty impl + a `skip(...)` of the deliberately-defaulted names) against real behavior-change risk. Do these ONLY if they earn it. **For each site, re-derive the override/forward/default breakdown from current source — do not trust any earlier survey.**

### Task 8: Per-site migration procedure (apply to Dialog, then Window, then Desktop, then Label)

**Files (one per sub-task):** `src/dialog/dialog.rs`, `src/window/window.rs`, `src/desktop/desktop.rs`, `src/widgets/static_text.rs`

For EACH site, in its own commit:

- [ ] **Step 1: Capture baseline method set**

Run: `cargo expand --lib <module path of the site> 2>/dev/null > /tmp/site_before.rs`
List the method names present in the current `impl View for <Type>`:
Run: `grep -E '    fn [a-z_]+' /tmp/site_before.rs | sort -u`

- [ ] **Step 2: Classify each of the 21 `View` methods for this site**

From the current source, put each method in exactly one bucket:
- **Provided** — the impl writes a real body (override or verbatim-forward). Keep it hand-written.
- **Defaulted** — the impl does NOT mention it (relies on the trait default). → goes in `skip(...)`.

The macro forwards everything that is neither provided nor skipped. So: keep all current bodies; set `skip(...)` = exactly the **Defaulted** set.

> **Known case — Window `calc_bounds`:** Window omits `calc_bounds` so the trait default routes through `Window::size_limits` (the 16×6 floor). It MUST be in Window's `skip(...)`. Forwarding it to `group.calc_bounds` would bypass the floor. Verify the same way for every Defaulted method at every site.

- [ ] **Step 3: Apply the macro**

Add `#[crate::delegate(to = <field>, skip(<defaulted methods>))]` above the impl, and DELETE the now-redundant verbatim-forward methods from the impl body (keep only the genuine overrides — the ones with custom bodies / super-calls).

- [ ] **Step 4: Expand-diff — the method set MUST be unchanged**

Run: `cargo expand --lib <module path> 2>/dev/null > /tmp/site_after.rs`
Run: `diff <(grep -E '    fn [a-z_]+' /tmp/site_before.rs | sort -u) <(grep -E '    fn [a-z_]+' /tmp/site_after.rs | sort -u)`
Expected: **empty diff** (same set of methods, same signatures). If the diff is non-empty, your provided/skip classification is wrong — fix it. This diff is the proof of behavior preservation; do not rely on snapshots.

- [ ] **Step 5: Full suite + clippy + fmt**

Run: `cargo test && cargo clippy --all-targets && cargo fmt --check`
Expected: PASS / clean.

- [ ] **Step 6: Commit (one per site)**

```bash
git add <site file>
git commit -m "refactor: adopt #[delegate] in <Type> (behavior-preserving)"
```

---

## Phase 4 — Docs & maintenance guard

### Task 9: Design note + CLAUDE.md convention

**Files:**
- Create: `docs/design/delegation-macros.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Write the design note**

Create `docs/design/delegation-macros.md` covering: the D2 boilerplate problem; why proc-macro gap-fill over `macro_rules!` (can't see sibling items / no ident-membership); the architecture (trait read from impl, hardcoded specs, Rust subtraction, `skip(...)`, `proc-macro-crate` + `extern crate self`); the behavior-preserving migration rule (`skip` = current Defaulted set, expand-diff verified); and the extension recipe ("to add a delegatable trait: add an arm to `specs::forwarders` matching the trait's signatures exactly").

- [ ] **Step 2: Add the maintenance line to CLAUDE.md**

Under `## Conventions` in `CLAUDE.md`, add:

```markdown
- Delegation: a type embedding another view forwards `View` methods via
  `#[delegate(to = <field>)]` (proc-macro in `tvision-macros`). **When you add a
  `View` trait method, add a matching forwarder to `tvision-macros/src/specs.rs`**
  (the spy test `tests/delegate_view.rs` catches a forgotten forwarder for
  existing methods). Use `skip(<m>)` to leave a method at its trait default.
```

- [ ] **Step 3: Final full verification**

Run: `cargo test && cargo clippy --all-targets && cargo fmt --check`
Expected: all green / clean.

- [ ] **Step 4: Commit**

```bash
git add docs/design/delegation-macros.md CLAUDE.md
git commit -m "docs: delegation-macros design note + maintenance convention"
```

---

## Out of scope (explicitly deferred)
- **`ListViewer` / `Validator` / menu-trait specs** — the mechanism is trait-generic, but no field-delegating consumer exists yet (`ListBox` delegates `View` to free functions, not a field). Add a `specs::forwarders` arm when one appears.
- **The airtight `#[delegatable]` const-compare guard** — v1 ships the behavioral spy test + the CLAUDE.md convention. Add the const-compare guard only if spec drift actually bites in practice.
- **Semantic "improvements" during migration** — e.g. Window `valid` → `group.valid` (vs base `true`). Preserve current behavior; file any such discrepancy separately.
- **Tuple-field targets** (`to = 0`) — all current sites use named fields. Add index support if a tuple-struct consumer appears.

---

## Self-Review notes
- **Spec coverage:** spike (Phase 0) → build View spec (Phase 1) → clean wins (Phase 2) → optional complex sites (Phase 3) → docs/guard (Phase 4). The user's "make it general across base traits" is met by the trait-keyed dispatch + extension recipe; "use it in the library itself" is met by Phases 2–3.
- **Type consistency:** the 21 forwarder signatures in `specs.rs` are transcribed to match `src/view/view.rs`; Task 4 Step 3 forces a verification that the set is complete and correct (the test bites).
- **Known fragile spots flagged for the implementer:** trybuild's `tv`-alias dependency wiring (Phase 0 Task 2 Step 4) and the test-harness `Context`/`DrawCtx` construction (Task 4) point to existing code to copy rather than inventing constructor calls.
