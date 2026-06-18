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
| `Cascade` (method) | 380 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3287–3300`) | 2 | Guide: calls `GetTileRect`, then `Desktop.Cascade`. Rust: `Command::CASCADE` is caught in `program_handle_event` after group dispatch; gets desktop extent via `group.find_mut(id).map(v.state().get_extent())` then calls `dt.cascade(r)` — faithful. `Application::get_tile_rect` is a public helper for the same rectangle. Inline comment documents the C++ original. |
| `DosShell` (method) | 380 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3302–3323`) | 2 | Guide: suspend → writeShellMsg → `system(COMSPEC)` / `raise(SIGTSTP)` → resume → redraw. Rust: `Command::DOS_SHELL` caught in `program_handle_event`; calls `backend.suspend()`, prints the shell message (the `set_shell_msg_hook` closure if registered, else the platform default), raises `SIGTSTP` on unix (gated `#[cfg(all(unix, not(test)))]`), calls `backend.resume()`, `renderer.invalidate_all()`. The `writeShellMsg` override point is preserved as the `set_shell_msg_hook` closure — see the `WriteShellMsg` row. |
| `GetTileRect` (virtual method) | 380 | PORTED | OK | `tv::Application::get_tile_rect` / `tv::Program::get_tile_rect` | 2 | Guide: returns `Desktop.getExtent()` (the full desktop rectangle); virtual so subclasses can exclude areas. Rust: `Program::get_tile_rect` returns `group.find_mut(desktop_id).map(v.state().get_extent())` — same semantics. `Application::get_tile_rect` forwards to it. It is an inherent method rather than a trait method, so user code adjusts the tile/cascade area by sizing the desktop rather than by overriding. Doc score 2: explains what it returns; missing "how to influence tile/cascade area." |
| `HandleEvent` (virtual method) | 381 | PORTED | OK | `program_handle_event` free fn (`src/app/program.rs:3218`) | 2 | Guide: calls `TProgram.HandleEvent` first, then dispatches `cmTile`, `cmCascade`, `cmDosShell`. Rust: the single `program_handle_event` function runs after group dispatch and handles all three commands (`TILE`, `CASCADE`, `DOS_SHELL`). Faithful ordering; comments cite C++ source. |
| `Tile` (method) | 381 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3287–3300`) | 2 | Guide: calls `GetTileRect`, then `Desktop.Tile`. Rust: `Command::TILE` caught alongside `CASCADE` in `program_handle_event`; calls `dt.tile(r)`. Faithful. |
| `WriteShellMsg` (virtual method) | 381 | EQUIVALENT | OK | `Program::set_shell_msg_hook` / `Application::set_shell_msg_hook`; default via `default_shell_msg()` (`src/app/program.rs`) | 2 | Guide: virtual procedure; default prints "Type EXIT to return..." (DOS/Windows) or the SIGTSTP return instruction (unix). Rust: the shell-suspend message is produced by a closure hook registered via `set_shell_msg_hook`; when no hook is set, `default_shell_msg()` returns the platform-correct text matching both C++ branches (Windows: "Type EXIT to return..."; Unix: "The application has been stopped. You can return by entering 'fg'."). The hook is called in `program_handle_event` at the `Command::DOS_SHELL` site, preserving the C++ virtual override point as a runtime-settable closure. |
| `suspend` (virtual method) | (app.h) | EQUIVALENT | OK | `Backend::suspend` (`backend` trait method) | 2 | C++: calls `TSystemError::suspend`, `TEventQueue::suspend`, `TScreen::suspend`. Rust: `Backend::suspend()` encapsulates all terminal subsystem suspension behind the backend abstraction. Known idiomatic mapping: subsystem dispatch → backend trait method. |
| `resume` (virtual method) | (app.h) | EQUIVALENT | OK | `Backend::resume` (`backend` trait method) | 2 | Symmetric with `suspend`. `Backend::resume()` restores the terminal. |

## Summary

- PORTED: 7   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 7   |   → concept: 0
- No SUSPECT entries remain. The main doc gap is that the tile/cascade/dos-shell handlers live in `program.rs` rather than `application.rs`, which the module-level comments explain but the public API rustdoc does not guide callers toward.
