# TApplication  (guide pp. 379–382)

Rust module(s): `src/app/application.rs`, `src/app/program.rs`   |   magiblot: `include/tvision/app.h` / `source/tvision/tapplica.cpp`

> The guide describes `TApplication` as a thin subclass of `TProgram` that adds
> subsystem init/teardown, suspend/resume, tile/cascade window management, a
> DOS-shell command, and a `HandleEvent` override that dispatches those commands.
> The Rust port collapses `TApplication` + `TProgram` into two separate types:
> `Application` (embed-and-delegate wrapper, `src/app/application.rs`) and
> `Program` (the event loop itself, `src/app/program.rs`). Inherited `TProgram`
> methods are audited separately in `TProgram.md`; this file covers only the
> entries the guide documents under the `TApplication` heading.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 380 | PORTED | OK | `tv::Application::new(backend, clock, theme, create_desktop, create_status_line, create_menu_bar)` | 2 | Guide: constructs by initializing all TV subsystems (memory, video, event, sysError, history) then calls `TProgram.Init`. Rust: subsystem init is handled by the `Backend`/`Renderer` construction path (crossterm setup); history is a `thread_local! Vec` (auto-initializes). The factory-function trio is forwarded verbatim to `Program::new`. Functionally equivalent; the doc explains what to pass but not the why-of-the-difference from C++. |
| `Done` (destructor) | 380 | PORTED | OK | `Drop` for `Application` / `Program` (implicit) | 1 | Guide: calls `TProgram.Done` then shuts down all TV subsystems. Rust: `Backend` drop handles terminal teardown; history `thread_local` drops automatically. No explicit `Done` override documented — matched by RAII. Doc score 1: the heritage section mentions this but there is no explicit rustdoc on the drop behavior. |
| `Cascade` (method) | 380 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3290–3310`) | 2 | Guide: calls `GetTileRect`, then `Desktop.Cascade`. Rust: `Command::CASCADE` is caught in `program_handle_event` after group dispatch; gets desktop extent via `group.find_mut(id).map(v.state().get_extent())` then calls `dt.cascade(r)` — faithful. `Application::get_tile_rect` is a public helper for the same rectangle. Inline comment documents the C++ original. |
| `DosShell` (method) | 380 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3313–3334`) | 2 | Guide: suspend → writeShellMsg → `system(COMSPEC)` / `raise(SIGTSTP)` → resume → redraw. Rust: `Command::DOS_SHELL` caught in `program_handle_event`; calls `backend.suspend()`, prints the shell message inline, raises `SIGTSTP` on unix (gated `#[cfg(all(unix, not(test)))]`), calls `backend.resume()`, `renderer.invalidate_all()`. `writeShellMsg` is inlined rather than virtual-overridable — see SUSPECT note. |
| `GetTileRect` (virtual method) | 380 | PORTED | OK | `tv::Application::get_tile_rect` / `tv::Program::get_tile_rect` | 2 | Guide: returns `Desktop.getExtent()` (the full desktop rectangle); virtual so subclasses can exclude areas. Rust: `Program::get_tile_rect` returns `group.find_mut(desktop_id).map(v.state().get_extent())` — same semantics. `Application::get_tile_rect` forwards to it. Not a trait method so cannot be overridden by user code — see SUSPECT note. Doc score 2: explains what it returns; missing "how to influence tile/cascade area." |
| `HandleEvent` (virtual method) | 381 | PORTED | OK | `program_handle_event` free fn (`src/app/program.rs:3221+`) | 2 | Guide: calls `TProgram.HandleEvent` first, then dispatches `cmTile`, `cmCascade`, `cmDosShell`. Rust: the single `program_handle_event` function runs after group dispatch and handles all three commands (`TILE`, `CASCADE`, `DOS_SHELL`). Faithful ordering; comments cite C++ source. |
| `Tile` (method) | 381 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3290–3310`) | 2 | Guide: calls `GetTileRect`, then `Desktop.Tile`. Rust: `Command::TILE` caught alongside `CASCADE` in `program_handle_event`; calls `dt.tile(r)`. Faithful. |
| `WriteShellMsg` (virtual method) | 381 | EQUIVALENT | SUSPECT | `println!` inline in `program_handle_event` (`src/app/program.rs:3319`) | N/A | Guide: virtual procedure; default prints "Type EXIT to return..." (DOS) or the SIGTSTP message (unix). Rust inlines the print statement directly in `program_handle_event` rather than exposing a virtual/overridable hook. The printed text matches the magiblot unix branch. However, the virtual override point is **not preserved** — user code cannot customize the shell message without forking the crate. This is an undocumented loss of extensibility. Not a behavior-correctness bug (message text is correct) but a deliberate API reduction that is not called out in any D-rule or comment. SUSPECT on the "intentional deviation not documented" axis. |
| `suspend` (virtual method) | (app.h) | EQUIVALENT | OK | `Backend::suspend` (`backend` trait method) | 2 | C++: calls `TSystemError::suspend`, `TEventQueue::suspend`, `TScreen::suspend`. Rust: `Backend::suspend()` encapsulates all terminal subsystem suspension behind the backend abstraction. Known idiomatic mapping: subsystem dispatch → backend trait method. |
| `resume` (virtual method) | (app.h) | EQUIVALENT | OK | `Backend::resume` (`backend` trait method) | 2 | Symmetric with `suspend`. `Backend::resume()` restores the terminal. |

## Summary

- PORTED: 7   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 1   |   doc<3 (public): 7   |   → concept: 0
- Notable finding: `WriteShellMsg` is the one SUSPECT entry — it is inlined as a non-overridable `println!` in `program_handle_event`, silently dropping the virtual override point the guide documents. All other methods are faithfully ported; the main doc gap is that the tile/cascade/dos-shell handlers live in `program.rs` rather than `application.rs`, which the module-level comments explain but the public API rustdoc does not guide callers toward.
