# Design note — `#[delegate]`, the embed-and-delegate boilerplate macro

> Status: **LANDED** (branch `feat/delegate-macro`). v1 ships the `View` spec and
> the `hello` example migration. The broader internal migrations (retire
> `cluster_wrapper!`, `ParamText`, and the behaviour-sensitive complex sites
> Dialog/Window/Desktop/Label) and the `ListViewer`/`Validator` specs are deferred
> to their own commits — see *Migration discipline* and *Extending to another
> trait* below.

## The problem D2 leaves behind

D2 ports C++ inheritance to a `View` **trait** + `ViewState` **composition**: a
widget that would `: public TDialog` in C++ instead *embeds* a `Dialog`
(`struct AboutDialog { dialog: Dialog }`) and `impl View for AboutDialog`. C++
inheritance gives every un-overridden virtual *for free*; Rust composition does
not — each of the ~21 `View` methods the author doesn't customise must be
hand-forwarded to the inner field:

```rust
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    self.dialog.handle_event(ev, ctx)   // x20, by hand
}
```

`AboutDialog` overrode exactly one method (`draw`) and hand-wrote ~12 forwarders;
the cluster wrappers hand-wrote 15 via a bespoke `cluster_wrapper!`. This is the
boilerplate `#[delegate]` removes: **write only the methods that differ; the macro
injects a forwarder for the rest.**

## Why a proc-macro (and not `macro_rules!`)

The requirement is *gap-fill*: emit a forwarder for every trait method the author
did **not** write. That needs two facts at one site — the trait's full method set,
and the set the author already provided — and then their difference.

- A `macro_rules!` expanded **inside** an `impl` cannot see sibling items, so it
  can't know what the author wrote.
- A `macro_rules!` that **owns** the `impl` and takes the overrides as a token
  blob would have to *subtract* the override names from its known set — ident
  set-membership, which `macro_rules!` has no clean primitive for (you'd hand-roll
  ~20 equality arms or pull in `paste`/`tt-call`; all worse than the problem).

The clean `macro_rules!` ceiling is *fixed* omission — "delegate all" or "delegate
all-but-`draw`". True arbitrary gap-fill wants a proc-macro that parses the `impl`
block. So `#[delegate]` is a `proc_macro_attribute`.

**No new build cost.** `syn`/`quote`/`proc-macro2` are already compiled in the
graph (`crossterm → derive_more → derive_more-impl`), so the macro crate reuses
them; it adds only its own small compile. Proc-macros vanish after expansion —
zero runtime/binary cost.

## Architecture

`rstv-macros` is a workspace member (`proc-macro = true`). `rstv` depends on
it and re-exports `pub use rstv_macros::delegate;`.

`#[delegate(to = <field>, skip(m1, m2, …))]` on an `impl Trait for Type` block:

1. **Reads the trait name** from `impl Trait for Type` — it is *not* passed in the
   attribute. (`ListViewer: View` ⇒ a type impls two traits ⇒ two `#[delegate]`
   calls, one per `impl`.)
2. **Collects the author-provided methods** from the impl's `ImplItem::Fn` idents.
3. **Looks up a hardcoded per-trait signature table** (`specs.rs`) and emits, for
   each method that is **neither provided nor in `skip(...)`**, a forwarder
   `fn m(args) { self.<field>.m(args) }`, pushed into the same impl block.

The subtraction happens **in Rust, inside the macro** — reliable, no generated
`macro_rules!` membership tricks.

```rust
let candidates = specs::forwarders(&trait_ident, field, &krate)?; // (name, fn tokens)
for (name, tokens) in candidates {
    if !provided.contains(name) && !skip.contains(name) {
        item_impl.items.push(syn::parse2(tokens)?);
    }
}
```

### `skip(...)` — leave a method at the trait default

A `skip`-ped method is neither provided nor forwarded; it falls back to the
trait's own default body. This exists to make migration **behaviour-preserving**
(see below). A `skip(name)` whose `name` is not a method of the trait is a hard
error — a typo can't silently turn into a forward.

### Path resolution works under any consumer alias

Generated forwarder signatures name crate types (`ViewState`, `DrawCtx`, `Point`,
…). They must resolve in three places: inside the `rstv` lib, in its examples,
and downstream — where the house style imports the crate under an arbitrary alias
(`tv = { package = "rstv" }`). The recipe:

- `src/lib.rs` declares `extern crate self as rstv;` (so `::rstv::T` resolves
  *inside* the lib).
- The macro asks `proc-macro-crate` for the crate's name *at the call site* and
  emits `::<that name>::T`. `crate_name("rstv")` returns `Itself`/`Name("rstv")`
  in-crate and in examples (both → `rstv`), and `Name("tv")` (or whatever the
  consumer chose) downstream. **Never `crate::`** — that would resolve to the
  *example/consumer* crate, not `rstv`.

This was proven by a spike before the spec was written: a delegated type compiled
and forwarded correctly from an internal module, from `examples/hello.rs`, and
from a throwaway crate importing the lib as `tv`.

## The drift hazard (and how it's contained)

`specs.rs` is a hardcoded mirror of `trait View`. The three coupled sites are
`trait View` (`src/view/view.rs`), the `view()` table (`specs.rs`), and the
`expected` list in the spy test (`tests/delegate_view.rs`).

- A missing **required** method (`state`/`state_mut`/`draw`) is caught at compile
  time — an empty `#[delegate] impl View for D {}` won't compile.
- A missing **defaulted** method is the silent hazard: a new `View` method with a
  default that nobody adds to `specs.rs` would leave every `#[delegate]` site on
  the trait default instead of forwarding. There is **no compile-time check** for
  this.

Containment (v1, pragmatic): `// MAINTENANCE:` signpost comments at all three
sites, plus the behavioural spy test that asserts every *currently known* method
forwards (and was confirmed to fail when a forwarder is removed). An airtight
const-compare guard (a `#[delegatable]` attribute on the trait emitting its method
names, compared against the macro's table) was considered and **deferred** — most
machinery for the least-likely failure, on a maintainer's own library with
two-stage review. Add it only if drift actually bites.

## Migration discipline (for the deferred internal sites)

Adopting `#[delegate]` at an existing site is **behaviour-preserving, never
semantic**. The macro forwards *every* non-provided, non-skipped method, but some
sites deliberately leave methods **defaulted**:

- **`skip(...)` must equal exactly the set the site currently leaves defaulted**,
  re-derived from current source (not from any survey).
- The flagged trap: **`Window` omits `calc_bounds`** so the trait default routes
  through `Window::size_limits`'s 16×6 floor; forwarding it to `group.calc_bounds`
  silently bypasses the floor. It must be in `Window`'s `skip(...)`.
- Where forwarding would be *more correct* than the old default (e.g. `Window::valid`
  → `group.valid` vs base `true`), that is **out of scope** — preserve the default,
  file the discrepancy separately.
- Verify equivalence with a **`cargo expand` method-set diff** of the impl
  before/after (the divergent methods are structural, not visual — snapshots prove
  nothing about them). An empty diff is the proof.

The `hello` example's `AboutDialog` is a *clean win*, not a complex site: it
overrides only `draw`, has no child controls, and *wants* to behave like its inner
`Dialog` for everything else — so forwarding all non-`draw` methods is the intended
shape (no `skip`, no semantic change).

## Extending to another trait

The mechanism is trait-generic; only the spec table is `View`-only today. To make
a trait delegatable, add an arm to `specs::forwarders` returning its forwarder list
(each entry: the method name + a `quote!` of the full `fn`, type paths through
`#k`, body forwarding to `self.#f.method(args)`), matching the trait's signatures
exactly. `ListViewer`/`Validator` are not yet added because no *field-delegating*
consumer exists — `TListBox` delegates `View` to free functions, not to an inner
field. Add the arm when one appears.
