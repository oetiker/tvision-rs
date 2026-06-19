# TButton  (guide pp. 386–390)

Rust module(s): `src/widgets/button.rs`   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/tbutton.cpp`

> The guide covers: 4 fields (`Title`, `Command`, `Flags`, `AmDefault`), 9 methods
> (`Init`, `Load`, `Done`, `Draw`, `DrawState`, `GetPalette`, `HandleEvent`,
> `MakeDefault`, `Press`, `SetState`, `Store`), 5 bfXXXX flag constants, and the
> `CButton` palette (8 entries). `GetCmd` is listed in the class diagram (p. 386)
> but has **no body text** in the guide — it is a TView-inherited method stub with
> no documented override; see note below.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Title` (field) | 387 | PORTED | OK | `Button::title: String` | 3 | C++ is `const char *` (heap PString); Rust is owned `String`. Rustdoc now explains `~`-hotkey marking and print behavior. |
| `Command` (field) | 387 | PORTED | OK | `Button::command: Command` | 3 | C++ `ushort`; Rust `Command` newtype. Rustdoc now explains post vs. broadcast behavior. |
| `Flags` (field) | 387 | EQUIVALENT | OK | `Button::flags: ButtonFlags` | 3 | C++ `uchar` bitmask; Rust struct-of-bools. Rustdoc now explains how to construct and use. |
| `AmDefault` (field) | 387 | PORTED | OK | `Button::am_default: bool` | 3 | Rustdoc now explains the live/runtime nature, how it starts, how to read vs. change it. |
| `bfNormal` (constant) | 319 | EQUIVALENT | OK | `ButtonFlags::default()` (all-false) | 3 | `ButtonFlags::new()` rustdoc now notes $00 equivalence and gives a usage example. |
| `bfDefault` (constant) | 319 | EQUIVALENT | OK | `ButtonFlags::default` field | 3 | Guide: $01; Rust: `ButtonFlags.default: bool`. Fully documented in field doc. |
| `bfLeftJust` (constant) | 319 | EQUIVALENT | OK | `ButtonFlags::left_just` field | 3 | Guide: $02; Rust: `ButtonFlags.left_just: bool`. |
| `bfBroadcast` (constant) | 319 | EQUIVALENT | OK | `ButtonFlags::broadcast` field | 3 | Guide: $04; Rust: `ButtonFlags.broadcast: bool`. |
| `bfGrabFocus` (constant) | 319 | EQUIVALENT | OK | `ButtonFlags::grab_focus` field | 3 | Guide: $08; Rust: `ButtonFlags.grab_focus: bool`. |
| `Init` (constructor) | 387 | PORTED | OK | `Button::new(bounds, title, command, flags)` | 3 | Sets `Options` (`ofSelectable | ofFirstClick | ofPreProcess | ofPostProcess`), `am_default` from `flags.default`, timer unarmed. Guide says `EventMask |= evBroadcast`: Rust note explains broadcasts reach every child regardless of mask (module doc). Initial disabled state is lazy via `COMMAND_SET_CHANGED` broadcast rather than inline check — deviation documented in constructor doc. Matches C++ behavior on the first idle pass. |
| `Load` (stream constructor) | 387 | NOT-PORTED | — | — | — | `TStreamable` / stream persistence dropped per project decision (D-rule: `TStreamable` → dropped, serde-if-revived). |
| `Done` (destructor) | 388 | PORTED | OK | `Drop` for `Button` (implicit) | N/A | C++ `~TButton` frees `title` and kills the timer. Rust: `title` (String) drops automatically; `animation_timer` (Option<TimerId>) is a handle copy — no resource to release on drop. Functionally equivalent. Private/infrastructure, N/A doc score. |
| `Draw` (method) | 388 | PORTED | OK | `Button::draw` (impl `View::draw`) | 3 | Guide: draws button with correct palette for state (normal, default, disabled), label position per `bfLeftJust`. Rust: full geometry-faithful implementation with `state_roles()` → `Role` pair for the five states; shadow glyphs from `ctx.glyphs()`; title via `put_cstr` with lo/hi role. Palette → Theme is known idiomatic mapping (D7). Module doc explains geometry. |
| `DrawState` (method) | 388 | PORTED | OK | `Button::draw` with `self.down` field | 3 | C++ `DrawState(Boolean down)` is a separate method. Rust merges it into `draw()` reading `self.down`. The `draw` rustdoc now includes a `# Note: DrawState merge` subsection calling this out explicitly. |
| `GetPalette` (method) | 388 | EQUIVALENT | OK | `Button::state_roles() -> (Role, Role)` + `Theme` | N/A | `state_roles()` is private; internal doc is adequate. `Role::Button*` variants live in `theme.rs` (separate file, separate sweep). |
| `HandleEvent` (method) | 388 | PORTED | OK | `Button::handle_event` (impl `View::handle_event`) | 3 | Guide: mouse clicks, hotkey (Alt+letter, focused Space, plain letter at post-process), `cmDefault` broadcast (default button → flash + fire), `cmGrabDefault` / `cmReleaseDefault` broadcasts (toggle `am_default`), `cmCommandSetChanged` (gray/ungray). Rust handles all branches. `cmGrabDefault`/`cmReleaseDefault` are renamed to `Button::GRAB_DEFAULT`/`Button::RELEASE_DEFAULT` (namespaced `Command::custom`, deviation D4). Mouse hold uses `MouseTrackCapture` deferred mechanism instead of a blocking loop (deviation D9). Module doc covers all branches. |
| `MakeDefault` (method) | 389 | PORTED | OK | `Button::make_default(enable, ctx)` | 3 | Guide: does nothing if already `bfDefault`; else broadcasts `cmGrabDefault`/`cmReleaseDefault` and redraws. Rust: guards on `!self.flags.default`; broadcasts `GRAB_DEFAULT`/`RELEASE_DEFAULT`; sets `am_default`. No explicit `draw_view()` — whole-tree redraw per D9 covers it (noted in set_state doc). `pub(crate)` to allow pump broker access. Fully documented. |
| `Press` (method) | 389 | PORTED | OK | `Button::press(ctx)` (private) | 3 | Guide: broadcasts `cmRecordHistory`, then either broadcasts `command` (bfBroadcast set) or `PutEvent` (posts command event). Rust: `ctx.broadcast(RECORD_HISTORY, None)`, then `ctx.broadcast(command, id)` or `ctx.post(command)`. Source for bfBroadcast path is `self.id()` (D4 ViewId). Documented in method doc. |
| `SetState` (method) | 389 | PORTED | OK | `Button::set_state` (impl `View::set_state`) | 3 | Guide: calls `TView::setState`, then redraws if `sfSelected` or `sfActive`, and calls `MakeDefault` if `sfFocused`. Rust: sets the flag (base step), emits `RECEIVED_FOCUS`/`RELEASED_FOCUS` broadcast (base focus semantic), calls `make_default` on `Focused`. No explicit `draw_view()` for selected/active — whole-tree redraw (D9) covers it. Documented in method doc with the replication note. |
| `Store` (stream method) | 389 | NOT-PORTED | — | — | — | `TStreamable` / stream persistence dropped (same reason as `Load`). |
| `GetCmd` (method) | 386 | NOT-PORTED | — | — | — | Listed only in the p. 386 class diagram with **no reference body** (pp. 387–390), not in `dialogs.h`, and no definition in magiblot `tbutton.cpp`. Not a real TV 2.0 TButton API method — a diagram artifact / v1.0 leftover. Nothing to port. |
| `CButton` palette (8 entries) | 390 | EQUIVALENT | OK | `Role::ButtonNormal`, `ButtonDefault`, `ButtonSelected`, `ButtonDisabled`, `ButtonNormalShortcut`, `ButtonDefaultShortcut`, `ButtonSelectedShortcut`, `ButtonShadow` | 3 | Guide: `CButton` maps 8 entries (10–15 in the dialog palette) covering normal text, default text, selected text, disabled text, normal shortcut, default shortcut, selected shortcut, shadow. Rust: 8 `Role::Button*` variants documented in `src/theme.rs` (theme pass) with full chain, color, and widget context for each variant. Known idiomatic mapping (D7). |

## Summary

- PORTED: 11   EQUIVALENT: 8   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: No correctness gaps or SUSPECT items. All previously below-bar public symbols raised to score 3. `state_roles()` re-scored N/A (private). `Role::Button*` variants raised to score 3 in the theme.rs Role pass (documented in `src/theme.rs`). The `draw` rustdoc now includes a `# Note: DrawState merge` subsection documenting the C++ `DrawState(bool)` → `self.down` field consolidation.
