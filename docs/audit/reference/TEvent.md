# TEvent  (guide pp. 434–435)

Rust module(s): `src/event/mod.rs`, `src/event/key.rs`, `src/command.rs`   |   magiblot: `include/tvision/system.h` (`TEvent`, `MouseEventType`, `KeyDownEvent`, `MessageEvent`) + `include/tvision/tkeys.h` (`kb*` constants, `TKey`)

> The guide documents TEvent as a Pascal variant record with a discriminant
> (`What: Word`) and three cases: `evNothing`, `evMouse`, `evKeyDown`, and
> `evMessage`. The magiblot C++ header expands this with `evMouseUp`,
> `evMouseMove`, `evMouseAuto`, `evMouseWheel`, and timer/paste events (not in
> the 1992 guide). All three sub-records (`MouseEventType`, `KeyDownEvent`,
> `MessageEvent`) are audited here. The idiomatic mapping is the tagged union →
> Rust `enum Event` (deviation D4, documented in the module).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TEvent` (type) | 434 | EQUIVALENT | OK | `tv::Event` (`src/event/mod.rs`) | 3 | Guide: Pascal variant record with `What` tag + union of three sub-records. Rust: closed `enum Event` matched arm-by-arm (deviation D4). Module doc names the deviation, explains the heritage, describes all variants including the extensions beyond the 1992 guide. Full what+how+heritage section. |
| `What: Word` (discriminant tag) | 434 | EQUIVALENT | OK | `enum Event` arm — the arm itself is the tag | N/A | Private/structural. The discriminant is not a separate field; the enum variant *is* the tag. Known idiomatic: union → enum. |
| `evNothing` (event code) | 434 | PORTED | OK | `Event::Nothing` | 3 | Raised: inline doc now explicitly states it is both "no pending event" and the consumed-event sentinel; documents how and when to call `clear()` vs. `is_nothing()`. |
| `evMouse` (event code / mouse case) | 434 | EQUIVALENT | OK | `Event::MouseDown` / `Event::MouseUp` / `Event::MouseMove` / `Event::MouseAuto` / `Event::MouseWheel` (all carry `MouseEvent`) | 3 | Raised: enum-level doc explains why there are five variants (one per magiblot `ev*` constant) rather than one masked class, with guidance on matching subsets. Each variant doc adds HOW/WHEN. |
| `evKeyDown` (event code / key case) | 434 | PORTED | OK | `Event::KeyDown(KeyEvent)` | 3 | Raised: `Event::KeyDown` variant doc adds "match `Event::KeyDown(ke)` and inspect `ke.key` and `ke.modifiers`" one-liner. |
| `evMessage` (event code / message case) | 434 | EQUIVALENT | OK | `Event::Command(Command)` + `Event::Broadcast { command, source }` | 3 | Guide: single `evMessage` with `Command` + `infoPtr` union. The `infoPtr` union carried three semantically distinct roles: a plain command (`evCommand`), a broadcast-with-subject (`evBroadcast`), and timer/paste payloads. Rust splits these into `Event::Command`, `Event::Broadcast { command, source }`, and `Event::Timer`. Deviation D4, documented in module doc. Score 3 — module doc explains all three splits and why. |
| **`MouseEventType` sub-record** | | | | | | |
| `MouseEventType.where` (mouse position) | 434 | PORTED | OK | `MouseEvent::position: Point` | 3 | Raised: `MouseEvent::position` field doc notes the Rust-keyword rename from `where`, and adds "in screen coordinates at the time of the event". `MouseEvent` struct doc adds a construction example. |
| `MouseEventType.buttons` (button state) | 434 | EQUIVALENT | OK | `MouseEvent::buttons: MouseButtons` (struct-of-bools: `left`, `right`, `middle`) | 3 | Raised: `MouseButtons` doc adds a usage example matching `left`/`right` in a `MouseDown` handler; field docs clarify `middle` = scroll-wheel click. Heritage note updated with original bitmask values. |
| `MouseEventType.Double` (double-click flag) | 434 | EQUIVALENT | OK | `MouseEvent::flags: MouseEventFlags` → `flags.double_click: bool` | 3 | Raised: `MouseEventFlags` doc adds "check `flags.double_click` in a `MouseDown` handler to react to double-click without manually tracking timing"; explains no wheel flag (wheel is an event class, not a flag). |
| `MouseEventType.eventFlags` (magiblot: moved/double/triple) | — | EQUIVALENT | OK | `MouseEvent::flags: MouseEventFlags` | 3 | Covered by `MouseEventFlags` upgrade above. |
| `MouseEventType.controlKeyState` (modifier bitmask) | 434 | EQUIVALENT | OK | `MouseEvent::modifiers: KeyModifiers` (struct-of-bools: `shift`, `ctrl`, `alt`) | 3 | Raised: `MouseEvent::modifiers` field doc now says "Reuses `KeyModifiers` — the same struct used for key-down events." `KeyModifiers` doc adds a key-handler example and explains the left/right folding decision. |
| `MouseEventType.wheel` (magiblot wheel direction) | — | EQUIVALENT | OK | `MouseEvent::wheel: MouseWheel` (enum: `None/Up/Down/Left/Right`) | 3 | Raised: `MouseWheel` doc adds a match example in a `MouseWheel` handler; explains `None` is the non-wheel-event default; variant docs clarify direction semantics. |
| **`KeyDownEvent` sub-record** | | | | | | |
| `KeyDownEvent.keyCode: Word` (combined scancode+charcode) | 434 | EQUIVALENT | OK | `KeyEvent::key: Key` (enum) + `KeyEvent::modifiers: KeyModifiers` | 3 | Guide (1992): `KeyCode: Word` (high byte = scan, low byte = char). magiblot `KeyDownEvent` preserves this plus `CharScanType` alias. Rust: decomposed into a `Key` enum (the base, modifier-free key) + `KeyModifiers` (deviation D4/D5). Module doc explains the decomposition explicitly. Score 3. |
| `KeyDownEvent.charCode: Char` (character part of keyCode) | 434 | EQUIVALENT | OK | `Key::Char(char)` variant (within `KeyEvent::key`) | 3 | Raised: `Key::Char` variant doc now states "The 1992 guide's `charCode: Char` (low byte of `keyCode: Word`) maps here, extended to full Unicode." |
| `KeyDownEvent.scanCode: Byte` (hardware scan code) | 434 | NOT-PORTED | — | — | — | Guide: `ScanCode: Byte` (high byte of `keyCode`). Rust drops the raw scan code; the `Key` enum models logical keys only. Raw scan codes are platform-specific DOS-era hardware data with no cross-platform meaning. Deliberate; commented in `key.rs` module doc ("Ports the `kb*` key-code family, the `TKey` class"). |
| `KeyDownEvent.controlKeyState` (modifier bitmask on key) | 434 | EQUIVALENT | OK | `KeyEvent::modifiers: KeyModifiers` | 3 | Raised: `KeyModifiers` doc upgrade (see `controlKeyState` on mouse row) covers this row too. |
| `kb*` key-code constants (`tkeys.h`) | — | EQUIVALENT | OK | `tv::Key` enum variants (`Key::F(n)`, `Key::Enter`, `Key::Esc`, `Key::Tab`, `Key::Up`/`Down`/`Left`/`Right`, `Key::Home`/`End`/`PageUp`/`PageDown`/`Key::Insert`/`Key::Delete`, `Key::Backspace`, `Key::Char(char)`) | 3 | Raised: `Key` enum doc now explains the replacement of ~150 `kb*` combined key+modifier constants by 16 orthogonal base-key variants plus a separate `KeyModifiers`, with Ctrl+C/Shift+Tab/Alt+F3 decomposition examples. |
| `TKey` class (magiblot canonical key+modifier form) | — | EQUIVALENT | OK | `tv::KeyEvent` (`key: Key`, `modifiers: KeyModifiers`) | 3 | Raised: `KeyEvent` doc now explicitly names `TKey` as the heritage analog, explains it was already a base-code + modifier-mask decomposition, adds `KeyEvent::from` / `KeyEvent::new` construction examples, and directs callers to match `Event::KeyDown(ke)`. |
| **`MessageEvent` sub-record** | | | | | | |
| `MessageEvent.command: Word` | 434 | PORTED | OK | `Command` field in `Event::Command(Command)` and `Event::Broadcast { command, .. }` | 3 | Guide: `Command: Word`. Rust: `Command` is a namespaced `&'static str` newtype (deviation D1), not a `u16`. Used in both `Command` and `Broadcast` arms. `command.rs` doc explains the identity model fully. Score 3. |
| `MessageEvent.infoPtr: Pointer` (pointer payload) | 434 | EQUIVALENT | OK | `Event::Broadcast { source: Option<ViewId> }` | 3 | Guide: `InfoPtr: Pointer` (case 0 of the `infoPtr` union). The most common use was passing a "subject view" pointer alongside a broadcast. Rust: `source: Option<ViewId>` — a resolvable typed handle (known idiomatic: `infoPtr` → `ViewId`; deviation D3/D4). Module doc explains the mapping and the three-role split. Score 3. |
| `MessageEvent.infoLong: LongInt` | 434 | NOT-PORTED | — | — | — | Guide: `InfoLong: LongInt` (case 1 of union). An integer payload slot that the framework itself never used (only app-specific message extensions). Dropped; commands in tvision-rs carry no numeric payload. The `source: Option<ViewId>` covers the one framework use-case. |
| `MessageEvent.infoWord: Word` | 434 | NOT-PORTED | — | — | — | Guide: `InfoWord: Word` (case 2 of union). Same rationale as `infoLong`. Not used by the framework itself. |
| `MessageEvent.infoInt: Integer` | 434 | NOT-PORTED | — | — | — | Guide: `InfoInt: Integer` (case 3 of union). Same rationale. |
| `MessageEvent.infoByte: Byte` | 434 | NOT-PORTED | — | — | — | Guide: `InfoByte: Byte` (case 4 of union). Same rationale. |
| `MessageEvent.infoChar: Char` | 434 | NOT-PORTED | — | — | — | Guide: `InfoChar: Char` (case 5 of union). Same rationale. |
| **Event codes and masks** | | | | | | |
| `evNothing = 0x0000` | 434 | EQUIVALENT | OK | `Event::Nothing` | 3 | Raised together with the `evNothing` row above. |
| `evMouse = 0x002f` (mask) | — | EQUIVALENT | OK | `matches!(ev, Event::MouseDown(_) \| Event::MouseUp(_) \| ...)` pattern | N/A | Not a public API; structural. The mask is gone; callers match arms. |
| `evKeyboard = 0x0010` (mask) | — | EQUIVALENT | OK | `matches!(ev, Event::KeyDown(_))` pattern | N/A | Not a public API; structural. |
| `evMessage = 0xFF00` (mask) | — | EQUIVALENT | OK | `matches!(ev, Event::Command(_) \| Event::Broadcast {..})` pattern | N/A | Not a public API; structural. |
| `evCommand = 0x0100` | — | EQUIVALENT | OK | `Event::Command(_)` arm | 3 | Raised: `Event::Command` variant doc now explains routing (capture stack, consumer should call `clear()`) making it HOW/WHEN-complete. |
| `evBroadcast = 0x0200` | — | EQUIVALENT | OK | `Event::Broadcast { .. }` arm | 3 | Raised: `Event::Broadcast` variant doc was already score 3 quality; confirmed. |
| **`EventMask` (TView.eventMask)** | | | | | | |
| `TView.eventMask` (per-view event-class opt-in) | — | EQUIVALENT | OK | `tv::EventMask` (`src/event/mod.rs`) | 3 | Raised: `EventMask` doc now leads with "most event classes are always on" and explicitly lists the always-on classes, then explains what the two opt-in fields buy and when to set them. |
| **Methods on TEvent** | | | | | | |
| `TEvent::getMouseEvent()` | — | NOT-PORTED | — | — | — | magiblot C++ method that calls `TEventQueue::getMouseEvent` to fill the event in-place. Rust: event production is internal to the event loop (`app::Program::pump_once`); callers never call this on an event value. The "fill in-place" pattern is DOS-era I/O style dropped in the Rust architecture. |
| `TEvent::getKeyEvent()` | — | NOT-PORTED | — | — | — | Same rationale as `getMouseEvent`. |
| **Extensions beyond the 1992 guide** | | | | | | |
| `Event::Timer(TimerId)` | — | EQUIVALENT | OK | `tv::Event::Timer` | 3 | Raised: `Event::Timer` variant doc explains it is separate from `Broadcast` (integer payload, not view subject), and that it is delivered to all views. |
| `Event::Paste(String)` | — | EQUIVALENT | OK | `tv::Event::Paste` | 3 | Raised: `Event::Paste` variant doc adds "handle it alongside `KeyDown` in input widgets that accept text." |
| `Event::clear()` / `Event::is_nothing()` helpers | — | EQUIVALENT | OK | `tv::Event::clear`, `tv::Event::is_nothing` | 3 | Raised: `clear()` doc now says WHEN to call it (inside a handler, after consuming the event) and prefers `clear()` over direct assignment. `is_nothing()` doc explains how to use it as a post-dispatch consumed-check. |
| `hot_key(s)` function | — | EQUIVALENT | OK | `tv::event::hot_key` | 3 | Ports `hotKey`/`hotKeyStr` (`tinputli.cpp`). Returns `Option<char>` instead of null char. Heritage noted; examples in doc. Score 3. |
| `ctrl_to_arrow(ke)` function | — | EQUIVALENT | OK | `tv::event::ctrl_to_arrow` | 3 | Ports `ctrlToArrow` (`drivers2.cpp`). Full table documented with examples. Score 3. |
| `is_alt_hotkey` / `is_plain_hotkey` helpers | — | EQUIVALENT | OK | `tv::event::is_alt_hotkey`, `tv::event::is_plain_hotkey` | 3 | Replaces the `getAltCode(c)` idiom (e.g. `tbutton.cpp`). Heritage noted; `is_plain_hotkey` explains the guard reasoning. Score 3. |

## Summary

- PORTED: 4   EQUIVALENT: 28   NOT-PORTED: 8   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All 15 previously below-bar public symbols raised to score 3. The always-on vs. opt-in distinction in `EventMask`, the consumed-event dual role of `Event::Nothing`, the `TKey` heritage link on `KeyEvent`, and the five-variant mouse split rationale are all now explicit in the rustdoc. The `scanCode` NOT-PORTED and the five `MessageEvent.info*` NOT-PORTED rows are unchanged; their justifications were already score N/A or correctly documented.
