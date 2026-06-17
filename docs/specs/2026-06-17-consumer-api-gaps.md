# Spec: consumer-facing API gaps surfaced by the `tcv` example

**Status:** open — for a future session.
**Origin:** building `examples/tcv.rs` (a faithful re-port of the 1993 Turbo
Pascal program *Tobi's Catalog Vision*) as an outside *consumer* of the public
API surfaced three places where tvision-rs can't do what C++ Turbo Vision did
trivially. Internal widgets never hit these — they aren't `pub(crate)`-restricted
and each got a bespoke deferred variant — so the gaps only show when you build a
real app from the published surface.

Two were also live layout bugs in the example, already fixed in commit
`f47eaa3` (search-overlay column accumulation; button-row inset). This spec
covers the three *framework* gaps that remain, each its own change. Do them as
small, two-stage-reviewed tasks (CLAUDE.md methodology). Verify on the integrated
tree with `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`, `-j4`.

When each lands: update `examples/tcv.rs` to drop the corresponding workaround
(documented in its header) and tighten its tests, then add an IMPLEMENTATION-LOG
entry.

---

## 1. Window decoration flags are not publicly settable  *(low risk; do first)*

**Gap.** A consumer cannot configure a window's decoration/behavior flags or its
drag/grow modes. So every `Dialog` is movable and wears a close box; you cannot
build a fixed, icon-less, full-desktop panel.

**Evidence.**
- `src/window/window.rs:237` — `pub(crate) fn set_flags(&mut self, flags: WindowFlags)`
- `src/window/window.rs:267` — `pub(crate) fn set_grow_mode(...)`
- `src/dialog/dialog.rs:117` — `pub(crate) fn set_flags(...)`
- `Window::new(bounds, title, number)` / `Dialog::new(bounds, title)` take **no**
  flags argument; `Dialog::new` hardcodes `move | close`.
- `WindowFlags` (fields `r#move`/`grow`/`close`/`zoom`) and `Window::flags()`
  getter **are** public — only the setters/constructors are closed.

**C++ baseline.** `TWindow.flags`, `TView.dragMode`, `TView.growMode` are public
fields, freely assigned. TCV.PAS:
`Window^.Flags := $00; Window^.DragMode := $00; Window^.GrowMode := $00;`
→ a fixed, iconless full-desktop panel (still framed + titled).

**Proposed fix.** Expose a public way to set decoration after construction (and/or
a builder):
- `pub fn Window::set_flags(&mut self, WindowFlags)` (un-`pub(crate)`) plus a
  builder `with_flags(self, WindowFlags) -> Self`.
- Public `set_grow_mode`/`with_grow_mode` and a drag-mode equivalent.
- Mirror on `Dialog` (`set_flags`/`with_flags`).
- Keep all current defaults unchanged (only add the knobs).

**Verify.** A new headless test builds a desktop-filling window with all flags
off and confirms the frame draws no close/zoom icon and the window doesn't move
on a frame drag. Then update `tcv.rs` to make the catalog window the fixed panel
the original was.

---

## 2. No generic deferred modal — a view can't `ExecView` an arbitrary dialog  *(medium risk; the bigger one)*

**Gap.** A leaf view (which only borrows `&mut Context` downward, per the D9
single-loop model) cannot pop up a *custom* modal dialog. The deferred
modal-from-a-view seam only offers a fixed catalog of built-in popups; there is
no "exec this `Box<dyn View>` I just built."

**Evidence.** `src/view/context.rs` request-modal surface is all specific:
`request_message_box` (1356), `request_save_as_dialog`, `open_color_dialog_for_role`,
`open_find_dialog`, `open_replace_dialog`, `request_open_history`,
`request_open_menu_box`. No `request_exec_view`. Design context:
`docs/design/async-modal-from-a-view.md`, `docs/design/deferred-effects.md`
("a new deferred capability ADDS A VARIANT").

**C++ baseline.** Inside `TDirBox.HandleEvent`, TCV builds a `TDialog` with six
`TStaticText` fields and an OK button and calls `Desktop^.ExecView(Pinfo)` —
spinning a nested modal loop inline, from within the view.

**Proposed approach.** Add a generic deferred variant, e.g.
`Deferred::ExecView(Box<dyn View>)` + `Context::request_exec_view(view)`, that the
pump executes as a modal at deferred-apply (same machinery as the existing
`Open*Dialog` → `pending_modal` → `ModalCompletion::*` flow used by C1/C8).
Key design question to settle first: **how the result returns to the requester**
— reuse the established `answer_to` + `then_command` pattern (deliver the modal's
end command to the requesting view), and/or a `ModalCompletion::ExecView` carrying
the end command + the boxed view back for the caller to read state off via
`as_any`. Follow the C1 reuse note in HANDOVER (don't invent a new seam shape).

**Verify.** `tcv.rs`'s Info box becomes a real custom `Dialog` (six labelled
fields) launched from the list via `request_exec_view`; headless test opens it,
asserts the fields render, closes it. Snapshot.

---

## 3. `get_help_ctx` does not bubble to the focused child  *(low–medium risk)*

**Gap.** The status line shows the wrong help context for nested focus — e.g. in
`tcv` it stays on "BROWSE MODE" while the list is actively searching, because the
list's `help_ctx` never reaches the status line.

**Evidence.**
- `src/app/program.rs:1757` — the idle arm reads
  `captures.top_modal_view()` then `v.get_help_ctx()`.
- `src/view/view.rs:965` — the default `View::get_help_ctx` returns the view's
  **own** `state().get_help_ctx()`; `Group`/`Window`/`Dialog` do **not** override
  it to delegate to the focused (`current`) child.
- Net effect: a leaf's help context can't propagate up to the top modal the
  status line reads. (The C7 work, HEAD history, wired the *read*; the *bubble*
  was never added — see HANDOVER "Standing deferrals: idle→statusLine->update".)
- `tcv.rs` works around it by caching the list's `help_ctx` into the `DataWindow`
  in `handle_event` — a bandaid that still can't reach the status line because the
  window isn't the top modal.

**C++ baseline.** `TGroup::getHelpCtx()` returns `current->getHelpCtx()`
(recursively to the focused leaf), falling back to the group's own when there's
no current / while dragging.

**Proposed fix.** Override `get_help_ctx` on `Group` (so `Window`/`Dialog` inherit
via their embedded group) to return the focused child's `get_help_ctx`
recursively, falling back to own state when there is no focused child. Preserve
the existing dragging-flag behavior (`view.rs:452`, test at `view.rs:1136`). This
is a `View` trait method, so check whether a `tvision-rs-macros/src/specs.rs`
forwarder / `delegate_view` spy update is needed (per the HANDOVER process note).

**Verify.** Re-enable a status-line assertion in `tcv.rs`
(`search_does_not_corrupt_focused_row` or a new test) confirming the line reads
"SEARCH MODE" while searching and "BROWSE MODE" otherwise; drop the `DataWindow`
caching hack. Confirm `hello`/`tvedit`/other examples' status lines are
unaffected. Snapshot.

---

## Suggested order

1 (flags) → 3 (help-ctx bubble) → 2 (generic deferred modal). 1 and 3 are small,
high-value, and make `tcv` faithful on the cheap; 2 is the larger unlock (custom
modals from views) and benefits from settling the result-delivery design first.
