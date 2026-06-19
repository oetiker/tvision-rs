# Globals / Procedures / Constants (guide pp. 347–362)

Rust module(s): `src/event/key.rs`, `src/event/mod.rs`, `src/dialog/msgbox.rs`,
`src/app/program.rs`, `src/screen/draw_buffer.rs`, `src/view/view.rs`,
`src/widgets/outline.rs`, `src/widgets/history.rs`, `src/menu/mod.rs`,
`src/status/mod.rs`, `src/window/window.rs`  |  magiblot: `tkeys.h`, `system.h`,
`msgbox.cpp`, `tprogram.cpp`, `drivers.cpp`, `views.h`, `menus.h`, `toutline.cpp`

> **Scope:** keyboard key-code constants (kbXXXX families, Tables 19.19–19.26 +
> shift masks), `LoadHistory`, `LoadIndexes`, `LongDiv`, `LongMul`, `LongRec`,
> `LowMemory`, `LowMemSize`, `MaxBufMem`, `MaxCollectionSize`, `MaxHeapSize`,
> `MaxLineLength`, `MaxViewWidth`, `mbXXXX` constants, `MemAlloc`, `MemAllocSeg`,
> `MenuBar` variable, `MenuColorItems`, `Message`, `MessageBox`, `MessageBoxRect`,
> `mfXXXX` constants, `MinWinSize`, `MouseButtons`, `MouseEvents`, `MouseIntFlag`,
> `MouseReverse`, `MouseWhere`, `MoveBuf`, `MoveChar`, `MoveCStr`, `MoveStr`,
> `NewBuffer`, `NewCache`, `NewItem`, `NewLine`, `NewMenu`, `NewNode`, `NewSItem`,
> `NewStr`, `NewStatusDef`, `NewStatusKey`, `NewSubMenu`, `ofXXXX` constants,
> `ovXXXX` constants, `PositionalEvents`, `PrintStr`, `PString`, `PtrRec`.

---

## kbXXXX constants — keyboard key codes (Tables 19.19–19.26 + shift masks)

The C++ `kb*` family is a flat namespace of `Word` (2-byte scan code) constants.
Tvision-rs replaces the entire family with a closed **`Key` enum** + separate
**`KeyModifiers`** struct-of-bools (deviation D5). Every `kbXXXX` constant maps
to a `Key` variant optionally combined with a `KeyModifiers` field — see the
"Rust symbol / mapping" column below.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `kbAltA`…`kbAltZ` (Table 19.19, Alt-letter keys) | 347 | EQUIVALENT | OK | `Key::Char('a')`…`Key::Char('z')` + `KeyModifiers { alt: true, .. }` | 3 | Each Alt-letter is expressed as `Key::Char(letter)` + the `alt` modifier. The whole table is a single `Key::Char` variant — no 26 separate named constants. Known mapping: closed key set → enum (deviation D5). Module doc explains the decomposition. |
| `kbAlt0`…`kbAlt9` (Table 19.20, Alt-number keys) | 348 | EQUIVALENT | OK | `Key::Char('0')`…`Key::Char('9')` + `KeyModifiers { alt: true, .. }` | 3 | Same pattern as Alt-letter: `Key::Char(digit)` + `alt` modifier. No separate Alt-digit constants. |
| `kbF1`…`kbF12` (Table 19.21, function keys) | 348 | EQUIVALENT | OK | `Key::F(1)`…`Key::F(12)` | 3 | `Key::F(n)` covers F1–F12. `key.rs` test explicitly covers F11/F12. |
| `kbShiftF1`…`kbShiftF12` (Table 19.22, Shift+function) | 348 | EQUIVALENT | OK | `Key::F(n)` + `KeyModifiers { shift: true, .. }` | 3 | Shift+F key = `Key::F(n)` + `shift` modifier; no separate Shift-Fn constants. |
| `kbCtrlF1`…`kbCtrlF12` (Table 19.23, Ctrl+function) | 348 | EQUIVALENT | OK | `Key::F(n)` + `KeyModifiers { ctrl: true, .. }` | 3 | Same decomposed pattern. |
| `kbAltF1`…`kbAltF12` (Table 19.24, Alt+function) | 348 | EQUIVALENT | OK | `Key::F(n)` + `KeyModifiers { alt: true, .. }` | 3 | Same decomposed pattern. Module doc example shows Alt+F3. |
| `kbHome` | 347 | EQUIVALENT | OK | `Key::Home` | 3 | Named nav key variant. |
| `kbEnd` | 347 | EQUIVALENT | OK | `Key::End` | 3 | Named nav key variant. |
| `kbUp` | 347 | EQUIVALENT | OK | `Key::Up` | 3 | Named nav key variant. |
| `kbDown` | 347 | EQUIVALENT | OK | `Key::Down` | 3 | Named nav key variant. |
| `kbLeft` | 347 | EQUIVALENT | OK | `Key::Left` | 3 | Named nav key variant. |
| `kbRight` | 347 | EQUIVALENT | OK | `Key::Right` | 3 | Named nav key variant. |
| `kbPgUp` | 347 | EQUIVALENT | OK | `Key::PageUp` | 3 | Named nav key variant. |
| `kbPgDn` | 347 | EQUIVALENT | OK | `Key::PageDown` | 3 | Named nav key variant. |
| `kbIns` | 347 | EQUIVALENT | OK | `Key::Insert` | 3 | Named nav key variant. |
| `kbDel` | 347 | EQUIVALENT | OK | `Key::Delete` | 3 | Named nav key variant. |
| `kbEnter` | 347 | EQUIVALENT | OK | `Key::Enter` | 3 | Named variant. |
| `kbEsc` | 347 | EQUIVALENT | OK | `Key::Esc` | 3 | Named variant. |
| `kbBack` | 347 | EQUIVALENT | OK | `Key::Backspace` | 3 | Named variant. |
| `kbTab` | 347 | EQUIVALENT | OK | `Key::Tab` | 3 | Named variant. Note: Shift+Tab = `Key::Tab` + `shift`; no BackTab variant — documented in `key.rs` module doc and test. |
| `kbShiftTab` | 347 | EQUIVALENT | OK | `Key::Tab` + `KeyModifiers { shift: true, .. }` | 3 | Explicitly documented in `key.rs` module doc and test `shift_tab_is_tab_plus_shift_not_a_backtab_variant`. |
| `kbCtrlA`…`kbCtrlZ` (Table 19.25, Ctrl-letter mapping table) | 348 | EQUIVALENT | OK | `Key::Char(c)` + `KeyModifiers { ctrl: true, .. }`; the nav mapping is `tv::event::ctrl_to_arrow` | 3 | The C++ Table 19.9/19.25 WordStar Ctrl→arrow mapping is `ctrl_to_arrow` in `src/event/key.rs`. Ctrl-letter otherwise = `Key::Char(c)` + ctrl. |
| `kbShiftIns`, `kbShiftDel`, `kbCtrlIns`, `kbCtrlDel` (Table 19.26, clipboard shift keys) | 348 | EQUIVALENT | OK | `Key::Insert`/`Key::Delete` + `KeyModifiers { shift/ctrl: true, .. }` | 3 | Clipboard accelerators use the decomposed modifier model. |
| Shift masks (`kbShift`, `kbCtrlShift`, `kbAltShift`, `kbLeftShift`, `kbRightShift`, `kbLeftCtrl`, `kbRightCtrl`, `kbLeftAlt`, `kbRightAlt`) | 349 | EQUIVALENT | OK | `KeyModifiers { shift/ctrl/alt: bool }` struct (deviation D5) | 3 | Left/right distinctions are deliberately folded into one flag each (`alt`, `ctrl`, `shift`). Documented in `KeyModifiers` rustdoc: "the platform left/right-Ctrl, left/right-Alt and left/right-Shift distinctions collapse into a single flag each." |

---

## LoadHistory procedure

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `LoadHistory` (procedure, `HistList` unit) | 349 | NOT-PORTED | — | — | N/A | DOS-era I/O: reads the history block from a `TStream`. No stream serialization in tvision-rs (TStreamable dropped — known mapping). The history store is `thread_local!` and resets each run; no serialization surface. |

---

## LoadIndexes procedure

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `LoadIndexes` (procedure, `StdDlg` unit) | 349 | NOT-PORTED | — | — | N/A | Loads the file-collection index from a `TStream`. No stream serialization in tvision-rs. `TFileCollection` → idiomatic `Vec` with no serialization seam. |

---

## LongDiv, LongMul, LongRec

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `LongDiv` (function) | 349 | NOT-PORTED | — | — | N/A | 32-bit integer division helper for 16-bit Pascal (no 32-bit native divide). Rust's `i32::wrapping_div` / `/` supersedes it; no analog needed. |
| `LongMul` (function) | 349 | NOT-PORTED | — | — | N/A | Same reason: 32-bit multiply helper for 16-bit Pascal. Rust has native 32/64-bit arithmetic. |
| `LongRec` (type) | 349 | NOT-PORTED | — | — | N/A | Pascal `record` overlay for manual 32-bit hi/lo word split. No analog in Rust: shift/mask do the same. |

---

## LowMemory, LowMemSize, MaxBufMem, MaxCollectionSize, MaxHeapSize

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `LowMemory` (function, `Memory` unit) | 350 | NOT-PORTED | — | — | N/A | DOS memory manager: returns `True` if free heap < `LowMemSize`. No analog — Rust's allocator is OS-managed. |
| `LowMemSize` (variable, `Memory` unit) | 350 | NOT-PORTED | — | — | N/A | DOS memory manager: the low-water mark in paragraphs. No analog. |
| `MaxBufMem` (constant, `Memory` unit) | 350 | NOT-PORTED | — | — | N/A | DOS memory manager: max cache-buffer memory. No analog. |
| `MaxCollectionSize` (constant, `Objects` unit) | 350 | NOT-PORTED | — | — | N/A | `TCollection` max element count on 16-bit DOS (`0xFFFF / 4` or similar). Rust `Vec` is unbounded. Known mapping: `TCollection` → `Vec`. |
| `MaxHeapSize` (variable, `Memory` unit) | 350 | NOT-PORTED | — | — | N/A | DOS memory manager: maximum heap size. No analog in Rust. |

---

## MaxLineLength constant

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MaxLineLength` (constant, `Editors` unit) | 350 | PORTED | OK | `src/widgets/editor.rs`: `const MAX_LINE_LENGTH: i32 = 256` | N/A | Module-private `const` (no `pub` or `pub(crate)`). Used as the editor line-limit internally. Not a public API symbol. |

---

## MaxViewWidth constant

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MaxViewWidth` (constant, `Views` unit) | 350 | NOT-PORTED | — | — | N/A | DOS fixed screen width ceiling (132 or 255, used to size `TDrawBuffer`'s fixed static array). In tvision-rs `DrawBuffer` is a `Vec<Cell>` sized dynamically at construction; no fixed-width ceiling is needed. |

---

## mbXXXX constants (mouse button flags)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `mbLeftButton` ($01) | 350 | EQUIVALENT | OK | `MouseButtons { left: true, .. }` | 3 | `mb*` bit-word → `MouseButtons` struct-of-bools (deviation D5). `left` field. Documented in `MouseButtons` rustdoc with a usage example. |
| `mbRightButton` ($02) | 350 | EQUIVALENT | OK | `MouseButtons { right: true, .. }` | 3 | `right` field of `MouseButtons`. Documented with a code example in the struct doc. |

---

## MemAlloc, MemAllocSeg

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MemAlloc` (procedure, `Memory` unit) | 350 | NOT-PORTED | — | — | N/A | DOS memory manager: allocate from the TV cache pool. Rust uses the standard allocator; no analog. |
| `MemAllocSeg` (procedure, `Memory` unit) | 351 | NOT-PORTED | — | — | N/A | DOS segment-based variant of `MemAlloc`. No analog. |

---

## MenuBar variable

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MenuBar` (global variable `PMenuBar`, `App` unit) | 351 | EQUIVALENT | OK | `Program::menu_bar() -> Option<ViewId>` (`src/app/program.rs`) | 3 | Rustdoc: "handle is stable for the application lifetime. Use it to resolve the menu bar view when you need to update its items at runtime… For command-enablement, prefer `enable_command` / `disable_command`." Heritage note cites `TProgram::menuBar` global. |

---

## MenuColorItems function

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MenuColorItems` (function, `ColorSel` unit) | 351 | NOT-PORTED | — | — | N/A | Returns a linked list of `TColorItem` records for the menu object's colour palette, used by the `TColorDialog`. The Rust port uses a `Theme`/`Role` system (deviation D7); color-item linked lists for the ColorSel subsystem are not ported. The `TColorDialog` class itself is ported as `src/dialog/colorpick/` but uses a different model. |

---

## Message function

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Message` (function, `Views` unit) | 351 | EQUIVALENT | OK | `Group::broadcast` / `View::handle_event` with `Event::Broadcast { command, source }` or `Event::Command` | N/A | The C++ `Message(view, evBroadcast, cmd, infoPtr)` free function finds a view and calls `handleEvent`. In tvision-rs the pump broadcasts via `Group::handle_event`. No single `message()` free function; the functionality is absorbed into event-loop routing. No single public symbol to document. |

---

## MessageBox, MessageBoxRect

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MessageBox` (function, `MsgBox` unit) | 351 | EQUIVALENT | OK | `Program::message_box(&mut self, msg, kind, buttons) -> Command` (`src/app/program.rs:964`) | 3 | Ports `messageBox` (`msgbox.cpp`). The `aOptions` packed word becomes typed `MessageBoxKind` + `MessageBoxButtons` (deviation D5); the builder is factored into `build_message_box` (deviation D9 — no nested modal loop). Full docs including sizing logic in `message_box` rustdoc. |
| `MessageBoxRect` (function, `MsgBox` unit) | 351 | EQUIVALENT | OK | `Program::message_box_rect(&mut self, r, msg, kind, buttons) -> Command` (`src/app/program.rs:936`) | 3 | Ports `messageBoxRect` (`msgbox.cpp`). Same shape as `message_box` but accepts an explicit `Rect`. Heritage section present in the rustdoc. |

---

## mfXXXX constants (message-box flags)

The C++ `mfXXXX` packed `aOptions` word is replaced by two typed enums: `MessageBoxKind` (title choice) and `MessageBoxButtons` (button selection). These are idiomatic deviations D5.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `mfWarning` ($0000) | 352 | EQUIVALENT | OK | `MessageBoxKind::Warning` | 3 | `MessageBoxKind` rustdoc now has a usage table (Warning/Error/Information/Confirmation → when to use each), a heritage note, and each variant has a one-line description. |
| `mfError` ($0001) | 352 | EQUIVALENT | OK | `MessageBoxKind::Error` | 3 | See `MessageBoxKind` rustdoc above. |
| `mfInformation` ($0002) | 352 | EQUIVALENT | OK | `MessageBoxKind::Information` | 3 | See `MessageBoxKind` rustdoc above. |
| `mfConfirmation` ($0003) | 352 | EQUIVALENT | OK | `MessageBoxKind::Confirmation` | 3 | See `MessageBoxKind` rustdoc above. |
| `mfInsertInApp` ($0004) | 352 | NOT-PORTED | — | — | N/A | Inserts the dialog into the application group rather than running it as a free-floating modal. tvision-rs always inserts the modal into the root group (deviation D9); no "insert into app" vs "insert into desktop" distinction exists. |
| `mfOKButton` ($0100) | 352 | EQUIVALENT | OK | `MessageBoxButtons { ok: true, .. }` | 3 | `MessageBoxButtons` rustdoc now has a combination table (ok/ok_cancel/yes_no/yes_no_cancel + when to use each), heritage note, and each field has docs explaining when to use it and combination advice. |
| `mfCancelButton` ($0200) | 352 | EQUIVALENT | OK | `MessageBoxButtons { cancel: true, .. }` | 3 | See `MessageBoxButtons` rustdoc above. |
| `mfYesButton` ($0400) | 352 | EQUIVALENT | OK | `MessageBoxButtons { yes: true, .. }` | 3 | See `MessageBoxButtons` rustdoc above. |
| `mfNoButton` ($0800) | 352 | EQUIVALENT | OK | `MessageBoxButtons { no: true, .. }` | 3 | See `MessageBoxButtons` rustdoc above. |
| `mfOKCancel` (shorthand) | 352 | EQUIVALENT | OK | `MessageBoxButtons::ok_cancel()` | 3 | Constructor documented in `MessageBoxButtons::ok_cancel`. Combination table in struct doc. |
| `mfYesNoCancel` (shorthand) | 352 | EQUIVALENT | OK | `MessageBoxButtons::yes_no_cancel()` | 3 | Constructor documented. Combination table in struct doc. |

---

## MinWinSize constant

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MinWinSize` (constant, `Dialogs` unit) | 352 | PORTED | OK | `Window::size_limits` returns `min = Point::new(16, 6)` (`src/window/window.rs:1234`) | N/A | C++ `MinWinSize = TPoint{16,6}`. Rust: the `size_limits` override hard-codes `Point::new(16, 6)` as the floor. There is no named `pub const MIN_WIN_SIZE`; the value lives inside an impl method override. Not a public constant symbol. The test `title_and_size_limits` asserts this value. |

---

## MouseButtons variable (global)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MouseButtons` (global variable `Byte`, `Drivers` unit) | 352 | EQUIVALENT | OK | `MouseEvent::buttons: MouseButtons` struct-of-bools delivered with each `Event::MouseDown/Up/Move/Auto` | 3 | `MouseButtons` struct has a code example (left/right click handler), heritage note (`mb*` bitmask → struct-of-bools). `MouseEvent::buttons` field doc explains event-local vs. global design. |

---

## MouseEvents variable

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MouseEvents` (global variable `Boolean`, `Drivers` unit) | 353 | NOT-PORTED | — | — | N/A | DOS: `True` if a mouse driver is present. In tvision-rs the crossterm backend provides mouse unconditionally; there is no "mouse absent" mode. Platform/detection concerns live in the backend. |

---

## MouseIntFlag variable

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MouseIntFlag` (global variable `Byte`, `Drivers` unit) | 353 | NOT-PORTED | — | — | N/A | DOS INT 33h status byte. No analog in crossterm-based tvision-rs. |

---

## MouseReverse variable

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MouseReverse` (global variable `Boolean`, `Drivers` unit) | 353 | NOT-PORTED | — | — | N/A | DOS: swap left/right buttons. No analog; crossterm delivers buttons directly. |

---

## MouseWhere variable

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MouseWhere` (global variable `TPoint`, `Drivers` unit) | 353 | EQUIVALENT | OK | `MouseEvent::position: Point` carried in each mouse event | 3 | `MouseEvent::position` rustdoc now explains: absolute screen coordinates; use `Context::make_local` to convert to view-local; always set (even on `MouseAuto`). Heritage note: ports `MouseWhere` global → event-local `position` (no mutable global). |

---

## MoveBuf, MoveChar, MoveCStr, MoveStr procedures

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `MoveBuf` (procedure, `Drivers` unit) | 353 | EQUIVALENT | OK | `DrawBuffer::move_buf(&mut self, indent, src: &[Cell])` (`src/screen/draw_buffer.rs:209`) | 3 | Rustdoc now explains when to use `move_buf` vs `move_char`/`move_str`/`move_cstr` (pre-built cells vs. single char vs. string vs. control string). Heritage note added. |
| `MoveChar` (procedure, `Drivers` unit) | 353 | EQUIVALENT | OK | `DrawBuffer::move_char(&mut self, indent, ch, style, count)` (`src/screen/draw_buffer.rs:68`) | 3 | Ports `MoveChar`. The `0 = retain` sentinel is dropped (deviation D6, documented in module note). Full docs including the sentinel-drop rationale. |
| `MoveCStr` (procedure, `Drivers` unit) | 353 | EQUIVALENT | OK | `DrawBuffer::move_cstr(&mut self, indent, text, lo, hi)` and `move_cstr_part` (`src/screen/draw_buffer.rs:199`) | 3 | Ports `MoveCStr` (tilde-toggle attribute). Extended to `move_cstr_part` for offset+max-width variants. Documented with the tilde-toggle semantics. |
| `MoveStr` (procedure, `Drivers` unit) | 353 | EQUIVALENT | OK | `DrawBuffer::move_str(&mut self, indent, text, style)` and `move_str_part` (`src/screen/draw_buffer.rs:117`) | 3 | Ports `MoveStr`. Extended to `move_str_part`. Documented. |

---

## NewBuffer, NewCache procedures

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewBuffer` (procedure, `Memory` unit) | 354 | NOT-PORTED | — | — | N/A | DOS TV memory manager: allocates a named cache buffer from a pool. No analog; Rust's allocator handles heap allocation. |
| `NewCache` (procedure, `Memory` unit) | 354 | NOT-PORTED | — | — | N/A | Same DOS memory manager subsystem. No analog. |

---

## NewItem, NewSubMenu functions (menu builders)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewItem` (function, `Menus` unit) | 354 | EQUIVALENT | OK | `MenuBuilder::command` / `MenuBuilder::command_key` / `MenuItem::Command { … }` (`src/menu/mod.rs`) | 3 | `NewItem` allocates a `TMenuItem` on the heap and chains it. The Rust `MenuBuilder` API (`Menu::builder().command(…).command_key(…)…build()`) is the idiomatic replacement. Fully documented in `MenuBuilder`. Known mapping: heap-linked-list builder → idiomatic fluent builder (deviation D1). |
| `NewSubMenu` (function, `Menus` unit) | 354 | EQUIVALENT | OK | `MenuBuilder::submenu(name, key_code, |m| m.…)` (`src/menu/mod.rs`) | 3 | `NewSubMenu` chains a submenu node. Replaced by `MenuBuilder::submenu` with a closure. Documented. |

---

## NewLine function (menu separator)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewLine` (function, `Menus` unit) | 354 | EQUIVALENT | OK | `MenuBuilder::separator()` (`src/menu/mod.rs:178`) | 3 | `NewLine()` allocates a separator `TMenuItem`. Replaced by `MenuBuilder::separator()`. Documented. |

---

## NewMenu function

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewMenu` (function, `Menus` unit) | 354 | EQUIVALENT | OK | `Menu::builder().….build()` → `Menu` (`src/menu/mod.rs`) | 3 | `NewMenu(item)` wraps the item list in a `TMenu`. `Menu::builder().build()` produces an owned `Menu` value. Documented. |

---

## NewNode function (outline)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewNode` (function, `Outline` unit) | 354 | EQUIVALENT | OK | `Node { text, children: Vec<Node> }` struct construction (`src/widgets/outline.rs`) | 3 | `Node` rustdoc has a complete code example (Animals/Cats/Dogs tree built with `Node::new().with_children().with_next()`), field-level docs explaining the linked-list structure, and a heritage note. Already at score 3. |

---

## NewSItem function (status-line single item)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewSItem` (function, `Menus` unit) | 354 | EQUIVALENT | OK | `StatusItem::new(text, key_code, command)` / `StatusItemsBuilder::item(…)` (`src/status/mod.rs`) | 3 | `NewSItem` heap-allocates a `TStatusItem`. Replaced by `StatusItem::new` (standalone) or `StatusItemsBuilder::item` (in the fluent builder). Fully documented. |

---

## NewStr function

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewStr` (function, `Objects` unit) | 355 | NOT-PORTED | — | — | N/A | Pascal heap-allocates a `PString` (length-prefixed Pascal string) from a C-string/Pascal string. Rust `String` / `&str` are the idiomatic string types; no `NewStr` analog needed. `PString` type is also NOT-PORTED (see below). |

---

## NewStatusDef function

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewStatusDef` (function, `Menus` unit) | 355 | EQUIVALENT | OK | `StatusDefListBuilder::def_all` / `StatusDefListBuilder::def_one_of` / `StatusDef { range, items }` (`src/status/mod.rs`) | 3 | `NewStatusDef` heap-allocates a `TStatusDef` node. Replaced by `StatusDef::list().def_all(…).build()` or `StatusDef { … }` direct construction. Fully documented. |

---

## NewStatusKey function

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `NewStatusKey` (function, `Menus` unit) | 355 | EQUIVALENT | OK | `StatusItemsBuilder::key_item(key_code, command)` / `StatusItem::key(…)` (`src/status/mod.rs`) | 3 | `NewStatusKey(text, key, cmd, next)` creates a hotkey-only status item. Replaced by `StatusItem::key` (hidden binding, no text) and `key_item` in the builder. Documented including the "hidden binding" design. |

---

## ofXXXX constants (view option flags)

All `of*` bit constants map to fields of the `Options` struct-of-bools (deviation D5), in `src/view/view.rs`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ofSelectable` ($001) | 356 | EQUIVALENT | OK | `Options { selectable: bool }` field | 3 | `Options` struct-level doc now lists common flag combinations with explanations. Each field has expanded docs: when to use it, typical widgets that set it. |
| `ofTopSelect` ($002) | 356 | EQUIVALENT | OK | `Options { top_select: bool }` field | 3 | Field doc explains: "used by `Window`: clicking a window selects it and brings it to the top of the z-order simultaneously." |
| `ofFirstClick` ($004) | 356 | EQUIVALENT | OK | `Options { first_click: bool }` field | 3 | Field doc explains: "without this flag, the first click on an unfocused view only focuses it; with it, the click is also delivered as a `MouseDown`." |
| `ofFramed` ($008) | 356 | EQUIVALENT | OK | `Options { framed: bool }` field | 3 | Field doc explains: "informs the owner that the view manages its own border; used by `Frame` so the owner can adjust layouts." |
| `ofPreProcess` ($010) | 356 | EQUIVALENT | OK | `Options { pre_process: bool }` field | 3 | Field doc explains: "views that must intercept events before the focused child sees them (e.g. a menu bar intercepting Alt+letter hotkeys)." |
| `ofPostProcess` ($020) | 356 | EQUIVALENT | OK | `Options { post_process: bool }` field | 3 | Field doc explains: "plain-letter accelerators (e.g. buttons and clusters), which fire only when no other view consumed the key first." |
| `ofBuffered` ($040) | 356 | NOT-PORTED | — | — | N/A | Per-view back buffer for damage tracking. tvision-rs uses whole-tree redraw + diff; no per-view back buffers (deviation D9, drop noted in `Options` rustdoc: "Dropped: the per-view back-buffer option"). |
| `ofTileable` ($080) | 356 | EQUIVALENT | OK | `Options { tileable: bool }` field | 3 | Field doc explains: "windows that should be included when the desktop tiles or cascades. Decorative or fixed-position windows leave this `false`." |
| `ofCenterX` ($100) | 357 | EQUIVALENT | OK | `Options { center_x: bool }` field | 3 | Field doc explains: "the owner adjusts the view's `x` position to center it. Combine with `center_y` (or use `Options::centered`) to center on both axes." |
| `ofCenterY` ($200) | 357 | EQUIVALENT | OK | `Options { center_y: bool }` field | 3 | Field doc explains: "the owner adjusts the view's `y` position to center it." |
| `ofCentered` (`ofCenterX | ofCenterY`) | 357 | EQUIVALENT | OK | `Options::centered() -> bool` helper | 3 | Method doc notes it is a read-only predicate; both fields must be set explicitly to enable centering. |
| `ofVersion` bits (streaming version bits) | 357 | NOT-PORTED | — | — | N/A | Stream-format version bits. Streaming dropped entirely (TStreamable dropped). Noted in `Options` rustdoc: "streaming-only `ofVersion*` bits" dropped. |
| `ofValidate` ($0400) | 357 | EQUIVALENT | OK | `Options { validate: bool }` field | 3 | Field doc explains: "when set, the group calls `view.valid(Command::RELEASED_FOCUS)` before allowing focus to move away. Return `false` from `valid` to keep focus locked." |

---

## ovXXXX constants (outline graph flags)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ovExpanded` ($01) | 357 | EQUIVALENT | OK | `const OV_EXPANDED: u16 = 0x01` (`src/widgets/outline.rs:66`) | N/A | Module-private `const` (no `pub`). Has an inline `///` doc comment: "Graph flag: the node is drawn as expanded (no children, or expanded)." Not a public API symbol. |
| `ovChildren` ($02) | 357 | EQUIVALENT | OK | `const OV_CHILDREN: u16 = 0x02` (`src/widgets/outline.rs:68`) | N/A | Module-private `const`. Inline doc comment: "Graph flag: the node has children AND is expanded (draw the child-link)." |
| `ovLast` ($04) | 357 | EQUIVALENT | OK | `const OV_LAST: u16 = 0x04` (`src/widgets/outline.rs:70`) | N/A | Module-private `const`. Inline doc comment: "Graph flag: the node is the last child of its parent (└ vs ├)." |
| `ovSelected` | 357 | NOT-PORTED | — | — | — | Not a distinct TV2 constant. The outline graph-flag family is exactly `ovExpanded`/`ovChildren`/`ovLast` (Table 19.29; rows above). Item selection is rendered via `Role::OutlineSelected` (a theme role), not a graph flag — so there is no `ovSelected` constant to port. Nothing to port. |

---

## PositionalEvents variable

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PositionalEvents` (global variable `Word`, `Views` unit) | 358 | NOT-PORTED | — | — | N/A | DOS TV global that holds the bitmask of event types treated as "positional" (sent to the view under the mouse, not the focused view). In tvision-rs, positional routing is hard-coded in `Group::handle_event`: `MouseDown`, `MouseMove`, `MouseAuto` are positional; `KeyDown`, `Command`, `Broadcast` are focused. There is no runtime-adjustable global; the policy is baked in. Not porting a settable global is intentional (deviation D9, collapsed routing). |

---

## PrintStr procedure

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PrintStr` (procedure, `Drivers` unit) | 358 | NOT-PORTED | — | — | N/A | DOS TV: writes a string directly to the BIOS video buffer at the current cursor position, bypassing the view system. No analog in tvision-rs; direct BIOS video access is not supported. Output goes through the crossterm backend + `DrawBuffer`. |

---

## PString type

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PString` (type, `Objects` unit) | 358 | NOT-PORTED | — | — | N/A | Pascal length-prefixed `^String` pointer. Replaced by Rust `String` / `&str` / `Option<String>`. No analog; the type is Pascal/DOS-specific. |

---

## PtrRec type

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PtrRec` (type, `Objects` unit) | 358 | NOT-PORTED | — | — | N/A | Pascal record that overlays a pointer into `Ofs`+`Seg` components (16-bit segmented memory). No analog in Rust; pointer arithmetic uses raw pointer casts when needed. |

---

## Summary

- PORTED: 2   EQUIVALENT: 68   NOT-PORTED: 28   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All genuinely `pub` symbols are at score 3. Private/module-private symbols (`OV_*`, `MAX_LINE_LENGTH`, `HISTORY` thread_local, `Message` free-function analog absorbed into routing) are marked N/A.
- Notable findings:
  - The **entire `kbXXXX` family** (Tables 19.19–19.26, ~60+ named C++ constants) collapses cleanly to the `Key` enum + `KeyModifiers` struct-of-bools (deviation D5). All entries at 3.
  - The **`mfXXXX` → `MessageBoxKind` + `MessageBoxButtons`** split is the most user-visible idiomatic substitution: both types now have combination tables and per-field "when to use" docs.
  - The **`ofXXXX` → `Options`** upgrade gives every field a "when to use" explanation and the struct-level doc lists common flag patterns.
  - The largest NOT-PORTED cluster is the DOS memory-manager family (`LowMemory`, `LowMemSize`, `MaxBufMem`, `MaxHeapSize`, `MemAlloc`, `MemAllocSeg`, `NewBuffer`, `NewCache`) — all correct and intentional (no DOS heap analog in Rust).
  - `ovSelected` is not a real TV2 constant (no graph flag to port; selection is a theme role).
