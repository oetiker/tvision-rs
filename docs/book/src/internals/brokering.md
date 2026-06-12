# Cross-view brokering & `ViewId`

A scroller needs to know how far its scrollbars have moved; a list box needs to
push its position back into them. In C++ Turbo Vision the two views simply hold
raw `TView*` pointers at each other and call across. Rust forbids that aliased
mutable access, so tvision splits the old pointer web into two halves: ownership
is a downward tree of `Box<dyn View>`, and every up- or sideways link becomes a
lightweight handle — a [`ViewId`](../api/tvision/view/struct.ViewId.html). This
is deviation **D3** (see [Pointers & `infoPtr` → handles](../port/handles.md)).

## `ViewId`: identity, not a pointer

A `ViewId` is a `Copy`, globally-unique identity — internally one
`NonZeroU64`, minted from a single monotonic process counter when a view is
inserted into its group (`Group::insert`), and stamped into the view's own
`ViewState`. It is **not** an index into any store. You can stash it freely (in
a sibling link, a focus stack, a capture handler) because it borrows nothing.
Use `Option<ViewId>` for "no link" — the `NonZeroU64` gives that a free niche,
so it costs no extra size.

Resolving a handle back to the live view is a tree-walk:
`View::find_mut(id)` recurses through the groups and hands back a
`&mut dyn View`, or `None`. A handle whose view has since been removed simply
resolves to `None` — there is no dangling slot to alias, so no generational
validation is ever needed.

## Why a leaf can't reach its sibling

During event handling a leaf view holds only `&mut Context` (the downward
borrow). It cannot reach back up to its parent group, let alone sideways to a
sibling — the borrow checker would be holding two `&mut` into the same tree. So
a scroller that wants to read its scrollbars, or write new parameters into them,
has no inline way to do it.

## The pump is the broker

The resolution: the leaf does not act directly. It *requests* the cross-view
read or write as a [deferred effect](deferred.md), and the
[event loop](event-loop.md) — which owns the whole tree — performs it at
deferred-apply time, when the entire tree is reachable through the root group.
The pump is the broker.

The pattern (established by the scroller, reused by the list viewer and outline
viewer) is always the same: resolve each participant in its **own**
`find_mut`, so only one `&mut` is live at a time, then downcast to the concrete
type via `as_any_mut()` to call its real method:

```rust,ignore
// At deferred-apply, inside the pump, `group` is the whole tree:
let dx = h_bar_id
    .and_then(|id| group.find_mut(id))   // resolve one bar
    .and_then(|view| view.value())       // read its value (D10)
    .and_then(field_int)
    .unwrap_or(0);
// ...read the v-bar the same way, then write the scroller:
if let Some(s) = group
    .find_mut(scroller)
    .and_then(|view| view.as_any_mut())  // dyn View -> Any
    .and_then(|a| a.downcast_mut::<Scroller>())
{
    s.apply_delta(Point::new(dx, dy));
}
```

Reads cross the seam through the value protocol (`View::value()` →
`FieldValue::Int`), so the broker never needs to know the sibling's concrete
type just to read a number. Writes that *do* need the concrete type use
`as_any_mut()` + `downcast_mut`. When the base is a trait rather than a struct
(the list viewer family), the broker instead calls back through a defaulted
`View` trait method — `apply_list_scroll` — since `dyn View` cannot be
downcast to a trait.

Each cross-view interaction is its own deferred variant — `SyncScrollerDelta`,
`ScrollBarSetParams`, `SyncListViewer`, and so on — so adding a new brokered
relationship means [adding a variant](deferred.md), not threading a new pointer.

## Avoiding feedback loops

A read-sync that writes back (the list viewer pushes its new position into the
v-bar) could re-enter forever. It does not, because the scrollbar's parameter
setter — `ScrollBar::set_params` — is change-guarded: it re-broadcasts
`cmScrollBarChanged` only when the value actually changes. Writing back the value
the bar already holds is a silent no-op, and the cycle goes quiet on the next
pump.
