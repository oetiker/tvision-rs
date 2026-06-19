# TEditWindow  (guide pp. 430–432)

Rust module(s): src/widgets/editor.rs (`struct EditWindow`)   |   magiblot: include/tvision/editors.h / source/tvision/teditwnd.cpp

> TEditWindow is a TWindow subclass that owns a TFileEditor wired to two hidden scroll bars and a
> hidden TIndicator. It owns the `editor` field (a raw pointer to the inserted TFileEditor) and an
> `indicator` field. It overrides `close`, `getTitle`, `handleEvent`, and `sizeLimits`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `editor` (field) | 430 | EQUIVALENT | OK | `tv::EditWindow::editor_id: ViewId` | 3 | Raised: doc now explains what ViewId is, how to resolve via `window.child_mut(editor_id)` + downcast to `&mut FileEditor`. |
| `indicator` (field) | 430 | EQUIVALENT | OK | `Indicator` child inserted by `EditWindow::new`, wired to the editor as `Option<ViewId>` | N/A | The guide lists `indicator` as a TEditWindow field (Borland TV 2.0). The capability is present — `new` inserts a hidden `Indicator` and wires it into the `FileEditor` as an `Option<ViewId>` handle (`editor.rs:445/1452`), exactly as the `Init` row describes. Structural placement differs from the printed guide (held by id via the editor rather than as a direct owned EditWindow sub-object, per D3 `ViewId` handles); magiblot's modern header likewise routes it through the editor. EQUIVALENT, not a field gap. Internal → doc N/A. |
| `Init` (constructor) | 430 | PORTED | OK | `tv::EditWindow::new(bounds, file_name, number)` | 3 | Raised: doc now explains bounds/number, tileability, the insert-bars-first sequence, missing-file behavior, and how to embed into the desktop. |
| `close` (method) | 431 | PORTED | OK | `tv::EditWindow::handle_event` `cmClose` arm | 3 | Maps to `handle_event`; raised together with `handleEvent` below. |
| `getTitle` (method) | 431 | EQUIVALENT | OK | `tv::EditWindow::new` sets the window title; title updated via `handle_event` `cmUpdateTitle` broadcast | 3 | Maps to `handle_event`; raised together with `handleEvent` below. The push-vs-pull divergence and clipboard-title absence are documented in the `handle_event` heritage section. |
| `handleEvent` (method) | 431 | PORTED | OK | `tv::EditWindow::handle_event` (impl `View::handle_event` in `#[delegate]` block) | 3 | Raised: doc now leads with what the method does, explains the title-refresh and clipboard-close-guard behaviors, and includes a `# Turbo Vision heritage` section documenting the consume-vs-leave-live divergence. |
| `sizeLimits` (method) | 432 | PORTED | OK | `tv::EditWindow::size_limits(&self, owner_size) -> (Point, Point)` | 3 | Raised: doc now explains the 24×6 minimum, why that number (frame + bars + content), and the `calc_bounds` skip rationale. |
| `palette` (7 entries: CEditWindow) | 432 | EQUIVALENT | OK | `tv::theme::Role::Frame*`, `Role::ScrollerNormal`, `Role::ScrollerSelected` (via `Window` + `Editor` delegation) | N/A | C++: `CEditWindow` palette has 7 slots: frame passive/active/icon (3), scrollbar page/controls (2), editor normal/selected text (2). Rust: `EditWindow` has no `getPalette` override; `Window` and then `Editor` provide their respective role lookups through the delegation chain. Known idiomatic mapping: class Palette → `tv::Theme`. Private concern; no public Rust symbol. |
| `Load` / `Store` (stream) | 432 | NOT-PORTED | — | — | — | `TStreamable` dropped project-wide. Known idiomatic mapping. |

## Summary

- PORTED: 4   EQUIVALENT: 4   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All 5 previously below-bar public symbols raised to score 3 in this sweep. The `handleEvent`/`cmUpdateTitle` push-vs-pull divergence (C++ consumed the broadcast; Rust leaves it live for all windows) is now documented in a `# Turbo Vision heritage` section on `handle_event`. The `indicator` and `palette` rows remain N/A (no public Rust symbol).
