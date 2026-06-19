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
| `Init` (constructor) | 380 | PORTED | OK | `tv::Application::new(backend, clock, theme, create_desktop, create_status_line, create_menu_bar)` | 3 | Guide: constructs by initializing all TV subsystems (memory, video, event, sysError, history) then calls `TProgram.Init`. Rust: subsystem init is handled by the `Backend`/`Renderer` construction path (crossterm setup); history is a `thread_local! Vec` (auto-initializes). The factory-function trio is forwarded verbatim to `Program::new`. Rustdoc now adds Rust-first "construct and drive" guidance, factory-closure role explanation, heritage note naming C++ subsystems. |
| `Done` (destructor) | 380 | PORTED | OK | `Drop` for `Application` / `Program` (implicit) | N/A | Guide: calls `TProgram.Done` then shuts down all TV subsystems. Rust: `Backend` drop handles terminal teardown; history `thread_local` drops automatically. No standalone public method — RAII replaces it. Not a public doc target. |
| `Cascade` (method) | 380 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3287–3300`) | N/A | Guide: calls `GetTileRect`, then `Desktop.Cascade`. Rust: `Command::CASCADE` is caught in the private `program_handle_event` after group dispatch; gets desktop extent via `get_tile_rect()` then calls `dt.cascade(r)` — faithful. The private function is not a public doc target; behavior is documented via inline comments citing C++. |
| `DosShell` (method) | 380 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3302–3323`) | N/A | Guide: suspend → writeShellMsg → `system(COMSPEC)` / `raise(SIGTSTP)` → resume → redraw. Rust: `Command::DOS_SHELL` caught in the private `program_handle_event`. Not a public doc target; the user-facing override point is `set_shell_msg_hook` (see that row). |
| `GetTileRect` (virtual method) | 380 | PORTED | OK | `tv::Application::get_tile_rect` / `tv::Program::get_tile_rect` | 3 | Guide: returns `Desktop.getExtent()` (the full desktop rectangle); virtual so subclasses can exclude areas. Rust: `Program::get_tile_rect` returns `group.find_mut(desktop_id).map(v.state().get_extent())` — same semantics. `Application::get_tile_rect` forwards to it. It is an inherent method rather than a trait method, so user code adjusts the tile/cascade area by sizing the desktop rather than by overriding. Rustdoc now adds "how to use" context (call directly for window positioning; size desktop to restrict tiling). |
| `HandleEvent` (virtual method) | 381 | PORTED | OK | `program_handle_event` free fn (`src/app/program.rs:3218`) | N/A | Guide: calls `TProgram.HandleEvent` first, then dispatches `cmTile`, `cmCascade`, `cmDosShell`. Rust: the single private `program_handle_event` function handles all three commands plus group dispatch. Not a public doc target. |
| `Tile` (method) | 381 | PORTED | OK | handled in `program_handle_event` (`src/app/program.rs:3287–3300`) | N/A | Guide: calls `GetTileRect`, then `Desktop.Tile`. Rust: `Command::TILE` caught alongside `CASCADE` in the private `program_handle_event`; calls `dt.tile(r)`. Not a public doc target. |
| `WriteShellMsg` (virtual method) | 381 | EQUIVALENT | OK | `Program::set_shell_msg_hook` / `Application::set_shell_msg_hook`; default via private `default_shell_msg()` (`src/app/program.rs`) | 3 | Guide: virtual procedure; default prints "Type EXIT to return..." (DOS/Windows) or the SIGTSTP return instruction (unix). Rust: the shell-suspend message is produced by a closure hook registered via `set_shell_msg_hook`; when no hook is set, the built-in platform default is used (Windows: "Type EXIT to return..."; Unix: the `fg` instruction). Rustdoc now adds "when to call" guidance, a code example of the hook closure, and the heritage note. |
| `suspend` (virtual method) | (app.h) | EQUIVALENT | OK | `Backend::suspend` (`backend` trait method) | 3 | C++: calls `TSystemError::suspend`, `TEventQueue::suspend`, `TScreen::suspend`. Rust: `Backend::suspend()` encapsulates all terminal subsystem suspension behind the backend abstraction. Rustdoc now adds: when it fires (before SIGTSTP / DOS-shell command), what the production backend does (LeaveAlternateScreen + DisableMouseCapture + disable_raw_mode), the headless no-op contract, and the paired `resume` contract for custom implementors. |
| `resume` (virtual method) | (app.h) | EQUIVALENT | OK | `Backend::resume` (`backend` trait method) | 3 | Symmetric with `suspend`. Rustdoc now adds: when it fires (after process returns to foreground), what the production backend does (enable_raw_mode + EnterAlternateScreen + EnableMouseCapture + full redraw), and the headless no-op contract. |

## Summary

- PORTED: 7   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- No SUSPECT entries remain. `Init`/`GetTileRect`/`WriteShellMsg` raised to score 3 in an earlier pass. `Done`, `Cascade`, `DosShell`, `HandleEvent`, `Tile` are private (RAII/private fn) — N/A for public doc. `suspend`/`resume` raised to score 3: `Backend::suspend` now documents when it fires, what the production backend does (LeaveAlternateScreen + DisableMouseCapture + disable_raw_mode), the paired `resume` contract, and the headless no-op; `Backend::resume` documents the symmetric restore sequence.
