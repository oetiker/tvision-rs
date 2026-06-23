# Changelog

All notable changes to tvision-rs will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The `Unreleased` section accumulates changes on `main`; the release workflow
moves it into a dated, versioned section when a release is cut.

## Unreleased

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

- CI: bump `actions/checkout` to v5 and pass the crates.io token via the
  `CARGO_REGISTRY_TOKEN` env var instead of the deprecated `cargo publish
  --token` flag, clearing the Node-20 and cargo deprecation warnings in the
  release workflow.

### Docs

- `Outline::new`: added a **Warning** note explaining that `ov_update` must be
  called once after the widget is inserted (before any navigation), and what
  symptoms appear when it is skipped (`limit.y` stays 0, down-arrow appears
  broken).
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
