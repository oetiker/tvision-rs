# Changelog

All notable changes to tvision-rs will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The `Unreleased` section accumulates changes on `main`; the release workflow
moves it into a dated, versioned section when a release is cut.

## Unreleased

### New

- Hierarchical Tab focus traversal. Tab / Shift-Tab now walk the focusable-leaf
  tree across nested groups — descending into sub-groups (and splitter panes) at
  their first/last leaf and ascending when a sub-tree is exhausted — instead of
  only cycling a window's direct children. `Group::handle_event` advances focus
  one level after the focused child has had the key (so a leaf at its sub-tree's
  edge lets Tab bubble to the parent), and the window wraps at the top. Two new
  `View` hooks support it (forwarded by `#[delegate]`): `focus_to_edge` (enter a
  sub-tree at its first/last leaf) and `has_focusable_leaf` (skip empty
  sub-trees). Composes through arbitrarily nested groups/splitters, and lets a
  multi-widget pane (e.g. a form) be Tab-navigated. Widgets that own Tab (a
  multi-line editor) still consume it; divider resize is unaffected
  (`Command::RESIZE`).

### Changed

- `tvision-rs-macros` now ships a `README.md` (and a `readme` manifest field), so
  its crates.io page renders documentation instead of a blank body. Takes effect
  on the next version publish — crates.io captures the README at publish time and
  cannot backfill it onto the already-published 0.1.2.

### Fixed

## 0.1.2 - 2026-06-23

### Changed

- Release: publish to crates.io via **Trusted Publishing** (OIDC,
  `rust-lang/crates-io-auth-action`) instead of a long-lived `CRATES_IO_TOKEN`
  secret. The `publish` job now requests `id-token: write` and no token secret
  is required. Both crates must have a Trusted Publisher configured on crates.io
  (repo `oetiker/tvision-rs`, workflow `release.yml`).
### Fixed

- Release: the `tvision-rs` crate now requires Trusted Publishing on crates.io,
  which 403'd the token-based publish (run 28026618214) after `tvision-rs-macros`
  0.1.1 had already gone out — leaving a half-published v0.1.1. Added a
  `republish` workflow that finishes a half-published tag idempotently (skips
  crate versions already indexed) via Trusted Publishing.

## 0.1.1 - 2026-06-23

### New

- `examples/external_state.rs` — demonstrates the canonical pattern for feeding
  data from a background thread (or any external source) into the TUI: a zero-
  area `PumpView` drains an `mpsc::Receiver` on each `Event::Timer` tick,
  updates shared `Rc<RefCell<AppState>>`, and broadcasts `REFRESH` so a
  `ListBox`-backed `ListPane` repopulates without holding a `RefCell` borrow
  across the `new_list` / `broadcast` calls.
- `tvision_rs::Deferred` is now re-exported at the crate root (alongside
  `Context`, `DrawCtx`, etc.), so consumers writing headless tests no longer
  need the two-segment path `tvision_rs::view::Deferred`.
- `Outline` now implements `View::value()`, returning
  `Some(FieldValue::Int(foc))` — the focused node's DFS index — consistent
  with `ListBox`. Previously it inherited the default `None`.
### Changed

- `Outline` now auto-seeds its scrollbar limits and focus on first
  display/interaction (the first context-bearing lifecycle call —
  `handle_event`, `set_state`, or `on_bounds_changed`), so `ov_update` no
  longer needs to be called manually just to populate it after construction.
  An explicit `ov_update` is still required after mutating the tree (swapping
  `root`, or expanding/collapsing nodes programmatically).
- CI: bump `actions/checkout` to v5 and pass the crates.io token via the
  `CARGO_REGISTRY_TOKEN` env var instead of the deprecated `cargo publish
  --token` flag, clearing the Node-20 and cargo deprecation warnings in the
  release workflow.

### Docs

- `Outline::new` / `OutlineViewerState::new`: documented that `ov_update` only
  needs to be called after **mutating** the tree (swapping `root`, or
  expanding/collapsing nodes programmatically); the initial population is
  auto-seeded on first display (see _Changed_), so no manual call is required
  after construction.
- `Program::set_on_idle`: added a **"Driving the UI from external / async data
  sources"** note with a code sketch of the `Rc<RefCell<AppState>>` +
  `broadcast` pattern.
- `Context::set_timer`: added a **note** describing the view-owned periodic-
  drain variant (zero-area pump view + `Event::Timer` + `broadcast`), with
  cross-references to `set_on_idle`.
- `Context::broadcast`: added a brief note cross-referencing the external-state
  pattern documented on `set_on_idle` and `set_timer`.
- `InputLine::new`: added a **headless / unit-test** note: a standalone
  `InputLine` constructed outside a `Program` must have
  `state.state.selected = true` set manually to receive key events — useful for
  consumers writing view-level unit tests with a hand-built `Context`.

### Fixed

## 0.1.0 - 2026-06-22

### New

- Initial public release of `tvision-rs` — an idiomatic Rust port of Turbo Vision
  (magiblot/tvision): the `View` trait + `ViewState` composition, the single
  event loop and deferred-effects channel, the core widget set (windows,
  dialogs, menus, buttons, input lines, list/scroll views, validators, color
  picker, …), the `Theme` palette system, and the `crossterm`-backed terminal
  `Backend` with a `HeadlessBackend` for snapshot testing.
### Changed

### Fixed
