# TDialog  (guide pp. 415–418)

Rust module(s): `src/dialog/dialog.rs`, `src/dialog/mod.rs`   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/tdialog.cpp`

> TDialog has **no own fields** beyond those inherited from TWindow/TView. The
> guide documents 4 methods (Init, Load, HandleEvent, GetPalette, Valid), one
> `Palette` field (inherited from TWindow), and three full 32-entry dialog
> palettes (CGrayDialog, CBlueDialog, CCyanDialog with named semantic slots).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 416 | PORTED | OK | `tv::Dialog::new(bounds: Rect, title: Option<String>) -> Dialog` | 3 | Doc raised: added "when to use Dialog vs Window" guidance (use Dialog whenever running modally via exec_view), post-construction workflow (insert_child / button_row then exec_view), and the four field overrides. All four faithful mappings documented. |
| `Load` (stream constructor) | 416 | NOT-PORTED | — | — | N/A | DOS TStreamable/stream machinery is project-wide dropped (CLAUDE.md: "TStreamable → dropped (serde if revived)"). Version-sniffing logic (`ofVersion10` → `dpGrayDialog`) has no analog. |
| `HandleEvent` (method) | 416 | PORTED | OK | `tv::Dialog::handle_event` (impl `View::handle_event`) | 3 | Guide: calls TWindow.HandleEvent first; Esc generates `cmCancel` (posted as a command event); Enter broadcasts `cmDefault`; `cmOK`/`cmCancel`/`cmYes`/`cmNo` while modal call `endModal`. Rust: calls `self.window.handle_event(ev, ctx)` first; `Key::Esc` → `ctx.post(Command::CANCEL)` + clear; `Key::Enter` → `ctx.broadcast(Command::DEFAULT, None)` + clear; `Command::OK|CANCEL|YES|NO` while `state.modal` → `ctx.end_modal(c)` + clear. Modal `endModal` call → deferred `Deferred::EndModal` via `Context::end_modal` (D9, documented in module doc). All four cases faithful. `end_modal` deferred path documented explicitly in module doc and handle_event doc-comment. |
| `GetPalette` (method) | 416–417 | EQUIVALENT | OK | `tv::window::WindowPalette` (Gray/Cyan/Blue) selected in `Dialog::new` via `pub(crate) Window::set_palette` | 3 | Symbol documented via the TWindow sweep: `WindowPalette` + `Window::palette()` reached score 3 in `src/window/window.rs` (when-to-choose guidance + `Role`-family mapping), and `Dialog::new` documents the `Gray` override. No public Dialog-level palette accessor exists; the scheme selection is the consumer-facing surface and it is at score 3. |
| `Valid` (method) | 417 | PORTED | OK | `tv::Dialog::valid` (impl `View::valid`) | 3 | Guide: returns True if command is `cmCancel`, otherwise calls `TGroup::Valid`. Rust: `if cmd == Command::CANCEL { true } else { self.window.valid(cmd, ctx) }` — exact translation. Delegates to the embedded window (which delegates to its group) for all non-Cancel commands, so a validator child can veto OK. Doc-comment explains both arms and why Cancel cannot be vetoed. |
| `Palette` field (inherited, 3 values) | 416 | EQUIVALENT | OK | `tv::window::WindowPalette` (Gray / Blue / Cyan) stored in `Window::palette` private field; `pub(crate) Window::set_palette` used internally by `Dialog::new` | N/A | No public Dialog symbol to document: `Window::palette` is private; `Window::set_palette` is `pub(crate)`. The three C++ constants (`dpBlueDialog`/`dpCyanDialog`/`dpGrayDialog`) map directly to `WindowPalette::Blue/Cyan/Gray` enum variants; consumers pass variants directly. |
| `CGrayDialog` palette (32 entries, slots 1–32) | 417–418 | EQUIVALENT | OK | `tv::theme::Role::FrameGray*` + descendant widget `Role::*` entries in `tv::Theme` | 3 | Documented in `src/theme.rs` (theme pass): the `Role` enum doc now carries a cross-reference table mapping `CGrayDialog`/`CBlueDialog`/`CCyanDialog` to `WindowPalette::Gray/Blue/Cyan` and the corresponding `FrameGray*/Frame*/FrameCyan*` role families. Descendant widget roles (Button, Input, Cluster, etc.) are each documented with their full chain through `cpGrayDialog`. |
| `CBlueDialog` palette (32 entries) | 417–418 | EQUIVALENT | OK | `tv::theme::Role::Frame*` + widget `Role::*` entries under `WindowPalette::Blue` | 3 | Documented in `src/theme.rs` (theme pass) — see `CGrayDialog` row above. `Frame*` family documented in `Role::FrameActive/Passive/Dragging/Icon`. |
| `CCyanDialog` palette (32 entries) | 417–418 | EQUIVALENT | OK | `tv::theme::Role::FrameCyan*` + widget `Role::*` entries under `WindowPalette::Cyan` | 3 | Documented in `src/theme.rs` (theme pass) — see `CGrayDialog` row above. `FrameCyan*` family documented in `Role::FrameCyanActive/Passive/Dragging/Icon`. |
| `streamableName` / `build` (TStreamable) | — | NOT-PORTED | — | — | N/A | TStreamable registration machinery (`name`, `build()`, stream operators `>>` / `<<`) is project-wide dropped (see CLAUDE.md). No Rust counterpart exists or is planned unless serde is revived. |

## Summary

- PORTED: 3   EQUIVALENT: 5   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- `Dialog::new` raised to 3; `GetPalette` re-scored 3 (covered by the TWindow `WindowPalette`/`palette()` docs); `Palette` field re-scored N/A (`pub(crate)` only).
- `CGrayDialog`/`CBlueDialog`/`CCyanDialog` raised to 3 in the theme.rs Role pass: the `Role` enum doc carries the `WindowPalette` cross-reference table, and each descendant widget `Role::*` variant is documented with the full chain.
