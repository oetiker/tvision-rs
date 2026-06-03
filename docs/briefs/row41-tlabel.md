# Brief — Row 41 `TLabel` (FOUNDATION-ish)

Port `TLabel` from magiblot-tvision: a caption that **links** to a control,
**focuses** it on click / hotkey, and **highlights** ("lights") while its linked
control is focused. This row also builds one new FOUNDATION substrate seam: a
**focus-by-`ViewId` deferred tree-op**.

Crate is `tvision`, house style `tv::`. Port FROM
`/home/oetiker/scratch/tvision-spec/magiblot-tvision/`. Read
`docs/PORTING-GUIDE.md` D-rules as needed.

## The C++ source (`source/tvision/tlabel.cpp`) — port verbatim behavior

```cpp
#define cpLabel "\x07\x08\x09\x09"

TLabel::TLabel( const TRect& bounds, TStringView aText, TView* aLink) noexcept :
    TStaticText( bounds, aText ),
    link( aLink ),
    light( False )
{
    options |= ofPreProcess | ofPostProcess;
    eventMask |= evBroadcast;
}

void TLabel::draw()
{
    TAttrPair color;
    TDrawBuffer b;
    uchar scOff;
    if( light ) { color = getColor(0x0402); scOff = 0; }
    else        { color = getColor(0x0301); scOff = 4; }
    b.moveChar( 0, ' ', color, size.x );
    if( text != 0 )
        b.moveCStr( 1, text, color );        // NOTE: text drawn at column 1
    if( showMarkers )
        b.putChar( 0, specialChars[scOff] );
    writeLine( 0, 0, size.x, 1, b );          // single row only
}

void TLabel::focusLink(TEvent& event)
{
    if (link && (link->options & ofSelectable))
        link->focus();
    clearEvent(event);                         // cleared UNCONDITIONALLY
}

void TLabel::handleEvent( TEvent& event )
{
    TStaticText::handleEvent(event);           // StaticText has none → no-op
    if( event.what == evMouseDown )
        focusLink(event);
    else if( event.what == evKeyDown )
        {
        char c = hotKey( text );
        if( event.keyDown.keyCode != 0 &&
            ( getAltCode(c) == event.keyDown.keyCode ||
                ( c != 0 && owner->phase == TGroup::phPostProcess &&
                  c == (char) toupper(event.keyDown.charScan.charCode) )
            )
          )
            focusLink(event);
        }
    else if( event.what == evBroadcast && link &&
            ( event.message.command == cmReceivedFocus ||
              event.message.command == cmReleasedFocus )
           )
            {
            light = Boolean( (link->state & sfFocused) != 0 );
            drawView();
            }
}
```

## Part 1 — the FOUNDATION seam: focus-by-`ViewId` deferred tree-op

`focusLink` does `link->focus()`. We have no `TView*`; `link` is an
`Option<ViewId>` (D3). A view deep in the tree (the label) cannot reach the
loop-owned focus machinery during dispatch, so it **requests** a focus and the
pump **applies** it after dispatch — exactly like `request_close` /
`request_set_state` / `request_bounds`. Build this NEW seam, mirroring the
existing `remove_descendant` tree-op family (do NOT invent a different shape):

1. **`src/view/context.rs`** — add a `Deferred` variant and a `Context` request
   method:
   - `Deferred::FocusById(ViewId)` (document it next to `Close(ViewId)`: it
     touches loop-owned **view-tree** focus state; insertion-order drain stays
     order-equivalent — same reasoning as the other tree-family variants).
   - `Context::request_focus(&mut self, id: ViewId)` → pushes
     `Deferred::FocusById(id)`. Doc-comment it like `request_close`.

2. **`src/view/view.rs`** — add a `View` trait tree-op, default no-op, next to
   `remove_descendant`:
   ```rust
   /// Tree-op: ask this subtree to focus (select) the view named `id`, running
   /// it through the owning group's select/validate machinery (NOT a raw
   /// set_state — that bypasses `current`/select). Mirrors `remove_descendant`:
   /// a view cannot focus an arbitrary view itself (D3); the live loop holds the
   /// subtree only by id and asks it to act. Returns whether `id` was found.
   /// Base: `false` (a leaf owns nothing).
   fn focus_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
       let _ = (id, ctx);
       false
   }
   ```

3. **`src/view/group.rs`** — `Group::focus_descendant`, mirroring
   `Group::remove_descendant` exactly, **plus an `ofSelectable` gate** at the
   resolution point (faithful to C++ `focusLink`'s `link->options & ofSelectable`
   check — but in the port the LABEL holds only a `ViewId`, so the selectable
   check must happen here, where the view is resolved):
   ```rust
   fn focus_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
       if let Some(i) = self.index_of(id) {
           // direct child: this is its owning group. Faithful to focusLink's
           // `link->options & ofSelectable` gate — focus only a selectable target,
           // but ALWAYS report it found (so the search stops here).
           if self.children[i].view.state().options.selectable {
               self.focus_child(id, ctx); // == C++ select() (see focus_by_number doc)
           }
           return true;
       }
       for child in self.children.iter_mut() {
           if child.view.focus_descendant(id, ctx) {
               return true;
           }
       }
       false
   }
   ```
   `focus_child` is the faithful realization of C++ `select()` here (same
   reasoning as `focus_by_number`'s doc-comment — windows raise via
   `make_first`, the outgoing `valid(cmReleasedFocus)` re-check is benign).
   **Breadcrumb (do NOT build):** C++ `TView::focus()` also focuses the OWNER
   chain up to the top before `select()`. We focus only the owning group. This is
   correct for the current reality (flat, modal dialogs where the dialog is
   already `current`); the owner-chain walk lands if/when nested non-current
   groups need it. State this in the doc-comment.

4. **Delegations** (mirror each type's existing `remove_descendant`):
   - `src/window/window.rs` → `self.group.focus_descendant(id, ctx)`
   - `src/desktop/desktop.rs` → `self.group.focus_descendant(id, ctx)`
   - `src/dialog/dialog.rs` → `self.window.focus_descendant(id, ctx)`
   - `src/widgets/static_text.rs` `ParamText` → `self.inner.focus_descendant(id, ctx)`
   - `Label` (new, part 2) → `self.inner.focus_descendant(id, ctx)`
   - `Background`/`Frame` are leaves — they do NOT override `remove_descendant`,
     so likewise do NOT override `focus_descendant` (default `false` is correct).

5. **`src/app/program.rs`** — in the pump's `Deferred` drain match (next to
   `Deferred::Close(id) => { group.remove_descendant(id, &mut ctx); }`):
   ```rust
   Deferred::FocusById(id) => {
       group.focus_descendant(id, &mut ctx);
   }
   ```
   (Exhaustive match — the compiler forces you to add this arm.)

## Part 2 — the `Label` widget (`src/widgets/static_text.rs`, append; or sibling)

Add `Label` next to `StaticText`/`ParamText` in `src/widgets/static_text.rs`
(it embeds `StaticText`, same file is natural; re-export from
`src/widgets/mod.rs` alongside `StaticText`/`ParamText`).

**Model (D2 embed-delegate, like `ParamText`):** `Label` embeds `StaticText` as
`inner` (the ONE `ViewState` lives in `inner`; do NOT add a second `ViewState`),
plus `link: Option<ViewId>` and `light: bool`. Delegate all `View` methods to
`inner` EXCEPT `draw`, `handle_event`, and `focus_descendant` (delegated) — see
the `ParamText` delegation block as the template (it forwards every trait method;
copy it, then override `draw`/`handle_event`).

**Constructor** `Label::new(bounds: Rect, text, link: Option<ViewId>)`:
- build `inner = StaticText::new(bounds, text)`
- set `inner.state_mut().options.pre_process = true;` and `.post_process = true;`
  (C++ `options |= ofPreProcess | ofPostProcess`). THIS is what makes the label —
  a non-selectable, never-`current` view — receive `KeyDown` in the group's
  pre/post phases. (The `eventMask |= evBroadcast` is a no-op here: our `Group`
  delivers broadcasts to every child regardless — see the `TButton` module note.)
- `light = false`.

**`draw`** (override; single row, NOT StaticText's word-wrap):
- pick the lo/hi role pair by `light`:
  - lit → lo = `Role::LabelLight`, hi = `Role::LabelLightShortcut`
  - not lit → lo = `Role::LabelNormal`, hi = `Role::LabelNormalShortcut`
  (these 4 roles are ALREADY in `theme.rs`, committed — just use them.)
- fill row 0 across `size.x` with the **lo** style (C++ `moveChar(0,' ',color,size.x)`).
- draw the text with `ctx.put_cstr(1, 0, text, lo, hi)` — **column 1** (C++
  `moveCStr(1, …)`; col 0 is the marker slot). `put_cstr` does the `~`-toggle
  (lo↔hi) exactly like `TButton`'s title draw — reuse it.
- **Drop `showMarkers`/`specialChars`** (always-off global; `TButton` dropped it
  identically — breadcrumb a one-line comment).
- The label's text lives in `inner` — expose it via `inner.text()` (the
  `StaticText::text` getter exists). Width = `self.inner.state().size.x`.

**`handle_event`** (override):
- helper `focus_link(ctx, ev)`: `if let Some(id) = self.link { ctx.request_focus(id); }`
  then `ev.clear()` — **the clear is UNCONDITIONAL** (C++ clears even when link is
  None / non-selectable; the `ofSelectable` gate lives in `focus_descendant`).
- `Event::MouseDown(_)` → `focus_link`. (The label is non-selectable so the
  group's mouse-down auto-select never fires on it; positional routing still
  delivers the click. No `grabs_focus_on_click` override needed.)
- `Event::KeyDown(ke)` → compute `let c = hot_key(self.inner.text());`
  (`use crate::event::{hot_key, is_alt_hotkey};`). If `c` is `Some(hot)` and
  `is_alt_hotkey(ke, hot)` → `focus_link`.
  **Defer the plain-letter branch** (C++ `owner->phase == phPostProcess && c ==
  toupper(charCode)`): it needs a dispatch-phase signal on `Context` that does not
  exist yet — `TButton` deferred the identical `is_plain_hotkey` branch for the
  same reason. Add a `// TODO(label/cluster/button: plain-hotkey postProcess
  accelerator — needs a phase signal on Context)` matching button.rs's wording.
  (`is_plain_hotkey` exists in `event/key.rs` but stays UNused here.)
- `Event::Broadcast { command, source }` → if `self.link.is_some()` and `command`
  is `Command::RECEIVED_FOCUS | Command::RELEASED_FOCUS` **and `*source ==
  self.link`**: set `self.light = (*command == Command::RECEIVED_FOCUS);`.
  - This is the FIRST consumer of `Broadcast { source }` (Phase A `7efecb3`).
    The `source` gate is the faithful realization of C++ `(link->state &
    sfFocused)`: a focus transition always emits the linked view's own
    RECEIVED/RELEASED_FOCUS with `source = that view's id`, so matching `source ==
    link` captures exactly the cases the C++ pointer-deref would. (Do NOT clear
    the event — broadcasts are seen by all; C++ does not clear here.)
- everything else: ignore (no-op).

## D-rules
D1 (drop `T`, snake_case), D2 (embed `StaticText` + delegate), D3 (`link` =
`Option<ViewId>`; focus via the deferred tree-op, NOT an up-pointer), D4 (`enum
Event` match; `Broadcast { source }` consumer), D7 (4 `Label*` roles via
`ctx.style`/`put_cstr`, no palette chain), D8 (draw through `DrawCtx`, whole-tree
redraw — `drawView()` is implicit), D12 (no `TStreamable` — drop
`shutDown`/`read`/`write`/`build`), D13 (grapheme text already handled by
`put_cstr`/`StaticText`).

## Existing types you build on (read these)
- `src/widgets/static_text.rs` — `StaticText` (embed target) + `ParamText` (the
  D2 delegation template to copy).
- `src/widgets/button.rs` — `handle_event` shape: `hot_key`/`is_alt_hotkey` use,
  `Event::Broadcast { command, .. }` match, `put_cstr` title draw, the
  `set_state` focus-broadcast `source = self.state.id()`.
- `src/view/context.rs` — `Deferred` enum, `Context::request_close`/`broadcast`,
  `DrawCtx::put_cstr`/`fill`/`style`.
- `src/view/group.rs` — `remove_descendant` (the recursion to mirror),
  `focus_by_number` + `focus_child` (the doc-comments to echo).
- `src/event/key.rs` — `hot_key`/`is_alt_hotkey`/`is_plain_hotkey`.
- `src/theme.rs` — the 4 `Label*` roles (already committed).

## Tests (make them DISCRIMINATING + bite-checked)
1. **Unit (handle_event):**
   - mouse-down → `Deferred::FocusById(link_id)` queued on the ctx + event
     cleared. (Inspect the deferred queue, as button/window tests do.)
   - Alt+hotkey keydown (title with `~`) → same; a non-matching Alt key → nothing
     queued, event NOT cleared.
   - `Broadcast { RECEIVED_FOCUS, source: Some(link_id) }` → `light` becomes true;
     `Broadcast { RELEASED_FOCUS, source: Some(link_id) }` → false; a broadcast
     with `source = Some(OTHER_id)` leaves `light` unchanged (this asserts the
     source gate BITES — verify by flipping the source and seeing the assert fail).
2. **`focus_descendant` (group.rs):** a group with a selectable child + a
   non-selectable child; `focus_descendant(selectable_id)` → focuses it (current
   changes) + returns true; `focus_descendant(nonselectable_id)` → returns true
   but current UNCHANGED; `focus_descendant(absent_id)` → false. Mirror
   `focus_by_number_*`'s test.
3. **Pump round-trip (`program.rs` or an integration test):** build a Program/
   Group with a `Label` linked to a selectable control; pump a `MouseDown` on the
   label (or directly queue the deferred), drain via a real `pump_once`, assert
   the linked control became `current`. THEN broadcast RECEIVED_FOCUS for it and
   assert the label `light`s. (This proves the whole seam end-to-end through the
   real loop — the row-33d-2 lesson: drive the drain through `pump_once`, not a
   scaffold.)
4. **Snapshot (lit vs not-lit):** render a `Label` with a `~`-hotkey title on a
   `HeadlessBackend` in both states; `assert_snapshot!` (or the manual
   text_rows/attr_rows helpers already in static_text.rs tests). Verify the text
   starts at column 1, the attr row shows the shortcut char in the hi role, and
   the lit snapshot differs from not-lit in the attr section.
   - snapshot gen: `cargo-insta` is NOT installed → if you use `assert_snapshot!`,
     run `INSTA_UPDATE=always cargo test <name>` once, hand-verify the `.snap`,
     re-run plain, and leave the `.snap` for commit. The static_text.rs tests use
     hand-written `text_rows`/`attr_rows` parsers instead — that style is fine too.

## Definitely-not (faithful scope — do NOT add)
- No owner-chain `focus()` walk (breadcrumb only — see Part 1.3).
- No plain-letter accelerator (breadcrumb — needs the deferred phase signal).
- No `showMarkers`/`specialChars` (dropped).
- No `getData`/`setData` (labels carry no data).
- No `TStreamable`.

## Done = 
`cargo test` (all green), `cargo clippy --all-targets -- -D warnings` clean,
`cargo fmt --check` clean. Report the exact test count delta and any deviation
you had to make from this brief (the brief can be wrong — if the C++ or the
existing types contradict it, follow them and say so).
