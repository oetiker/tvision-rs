# Design note — dialog layout guide

> Status: **LANDED** (constants in `src/dialog/layout.rs`; `Dialog::button_row`
> in `src/dialog/dialog.rs`). This document is the authoritative reference for
> the recovered classic Turbo Vision dialog conventions.

Dialogs in rstv are gray-palette modal containers. The conventions below are
confirmed against `msgbox.cpp`, `tfildlg.cpp`, and `tdialog.cpp` in
magiblot/tvision. Using the named constants from `crate::dialog::layout` keeps
all dialogs consistent and makes coordinate bugs obvious.

---

## 1. Dialog construction

```rust
let d = Dialog::new(Rect::new(col, row, col + w, row + h), Some("Title".into()));
```

[`Dialog::new`](crate::dialog::Dialog::new) hard-wires:

- **Palette** — gray (`WindowPalette::Gray`), applied immediately and pushed
  down into the embedded frame child. This is what makes a dialog look different
  from a window: do not call `set_palette` again unless you need something unusual.
- **Decoration flags** — `move | close` only: no grow handle, no zoom icon.
  A resizable dialog must call `set_flags` explicitly after construction
  (see `FileDialog`).
- **Grow mode** — all-false (`GrowMode::default()`): the dialog does not track
  its owner's resize.
- **Window number** — zero, so no number is drawn in the frame.

Run the constructed dialog modally:

```rust
let result = program.exec_view(Box::new(d));  // returns Command::OK / CANCEL / …
```

---

## 2. Interior margins

Classic TV keeps content off the frame edges. The named constants in
`crate::dialog::layout` (re-exported flat under `crate::dialog`) encode the
recovered values:

| Constant | Value | Meaning |
|---|---|---|
| [`MARGIN_LEFT`](crate::dialog::MARGIN_LEFT) | 3 | First content column |
| [`MARGIN_RIGHT`](crate::dialog::MARGIN_RIGHT) | 2 | Last content column = `width - 1 - MARGIN_RIGHT` |
| [`MARGIN_TOP`](crate::dialog::MARGIN_TOP) | 2 | First content row |
| [`BUTTON_ROW_FROM_BOTTOM`](crate::dialog::BUTTON_ROW_FROM_BOTTOM) | 3 | Button-row top = `height - 3` |

A `40 × 12` dialog therefore has usable interior `[3, 37] × [2, 8]`  
(columns 3–37, rows 2–8) with the button row starting at row 9.

---

## 3. The button row

Standard buttons are [`STD_BUTTON`](crate::dialog::STD_BUTTON) = 10 columns × 2 rows.
Row 2 is the drop shadow, so the visual label occupies only row 1. Adjacent
buttons are [`BUTTON_GAP`](crate::dialog::BUTTON_GAP) = 2 columns apart.

Use [`Dialog::button_row`](crate::dialog::Dialog::button_row) to add a row with
consistent metrics:

```rust
// Centered row (message-box convention):
d.button_row(
    &[
        ("~O~K",     Command::OK,     ButtonFlags { default: true, ..ButtonFlags::new() }),
        ("~C~ancel", Command::CANCEL, ButtonFlags::new()),
    ],
    ButtonRowAlign::Center,
);

// Right-grouped row (action dialogs):
d.button_row(
    &[
        ("~O~K",     Command::OK,     ButtonFlags::new()),
        ("~C~ancel", Command::CANCEL, ButtonFlags::new()),
    ],
    ButtonRowAlign::Right,
);
```

[`ButtonRowAlign::Center`](crate::dialog::ButtonRowAlign::Center) places the
row's centre at the dialog's centre column —  
`left = (width - span) / 2`.

[`ButtonRowAlign::Right`](crate::dialog::ButtonRowAlign::Right) ends the last
button `MARGIN_RIGHT` from the right frame —  
`left = width - MARGIN_RIGHT - span`.

Both alignments put the top edge at `height - BUTTON_ROW_FROM_BOTTOM` (= `height - 3`).
The method returns a `Vec<ViewId>` in declaration order so you can reach the
inserted buttons by id if needed.

---

## 4. Labels and input lines

Place a [`Label`](crate::widgets::Label) above or to the left of the associated
control. The label links to its target via a command:

```rust
let input_id = d.insert_child(Box::new(InputLine::new(
    Rect::new(MARGIN_LEFT + 8, 2, 36, 3), 40,
)));
// Label fires Command::GOTO_VIEW with the input's id as context — wired
// by Label::new via its target ViewId.
d.insert_child(Box::new(Label::new(
    Rect::new(MARGIN_LEFT, 2, MARGIN_LEFT + 7, 3),
    "~N~ame:", input_id,
)));
```

The label respects `LabelNormal` / `LabelLight` roles from the gray palette,
which are automatically correct inside a `Dialog`.

---

## 5. Separating regions

Use whitespace (empty rows) to separate logical sections. If a section needs a
visible header, use a [`StaticText`](crate::widgets::StaticText):

```rust
d.insert_child(Box::new(StaticText::new(
    Rect::new(MARGIN_LEFT, 4, 37, 5),
    "── Options ──",
)));
```

Do **not** draw separator lines by hand with box-drawing characters spread
across multiple cells; `StaticText` with a composed string is the right level
of abstraction.

---

## 6. Roles inside a gray dialog

A `Dialog` uses the **gray** palette; the `WindowPalette::Gray` family maps roles
differently from the blue window family. The rules:

- **Input lines, list boxes, scrollers**: use `FieldNormal` / `FieldSelected`
  (gray field colors). The default role resolution inside `WindowPalette::Gray`
  handles this automatically — you only need to ensure you do not override with
  a blue-window role.
- **Static text, labels, separators**: use `StaticText` / `LabelNormal` /
  `LabelLight` — these are explicitly gray-dialog roles.
- **Scroll bars paired with a scroller**: they already receive `ScrollBarNormal`
  within the gray palette. No manual role override is needed.
- **Do NOT use `FramePassive` / `ScrollerNormal` blue-window roles** inside a
  gray dialog. These were the source of the color-picker bug where panel borders
  rendered in window-blue instead of gray. The palette chain is authoritative:
  derive colors from it, do not hard-code.

If a custom widget inside a dialog renders incorrectly, check that its
`draw` implementation is calling `DrawCtx::style(role)` with a gray-dialog role
and not borrowing a blue role by mistake.

---

## 7. Shadows

- **Buttons** self-shadow: the second row of each button is its own drop shadow,
  drawn by `Button::draw`. No manual shadow call is needed.
- **Dialogs** cast a desktop shadow: the `Window` frame's draw routine
  automatically draws a one-cell drop shadow against the desktop background.
  No manual shadow call is needed.
- **Never call a shadow-drawing helper directly** on a `Dialog` child — the
  frame and button handles this at the right layer.

---

## 8. rstv-original extensions: `TabBar` and `PageStack`

Classic Turbo Vision exposes only `TGroup` with `sfVisible` toggled per page.
rstv adds two widgets for the common "tabbed dialog" idiom:

**`TabBar`** — a corner-capped tab strip rendered above the content area.
Each tab has a title and fires
[`Command::TAB_BAR_CHANGED`](crate::command::Command::TAB_BAR_CHANGED)
(a broadcast with the bar's own `ViewId` as `source`) whenever the selected
tab changes. The source-in-broadcast is the D3/D4 pattern (the same mechanism
as `SCROLL_BAR_CHANGED` / `ScrollBar` → `Scroller` brokering): the pump's
deferred-apply pass resolves the source id to the `PageStack` sibling and
calls `set_active(idx, ctx)` on it with the new tab index.

**`PageStack`** — a content multiplexer that holds one child per tab and shows
only the currently selected page. It consumes `TAB_BAR_CHANGED` via the pump
broker (cross-view sibling broker, D3) exactly as `Scroller` consumes
`SCROLL_BAR_CHANGED`: the pump calls `group.find_mut(bar_id)` to read the bar's
`value()`, then `page_stack.set_active(idx, ctx)` to show/hide the pages and
move focus to the newly active one (`set_active` is the show/hide+focus
operation, distinct from the `set_value` View data-transfer method).

Wiring a tabbed dialog:

```rust
let bar = TabBar::new(Rect::new(1, 1, 39, 2), &["General", "Advanced"]);
let bar_id = d.insert_child(Box::new(bar));

let mut pages = PageStack::new(Rect::new(1, 2, 39, 10));
pages.insert_page(Box::new(general_group));
pages.insert_page(Box::new(advanced_group));
pages.bind_tab_bar(bar_id);
let _pages_id = d.insert_child(Box::new(pages));
// The pump brokers TAB_BAR_CHANGED from bar_id to the PageStack automatically.
```

Note: `TabBar` and `PageStack` are rstv-original — they have no direct Turbo
Vision counterpart. Classic TV authors would achieve the same effect by toggling
`sfVisible` on groups manually; the rstv widgets encapsulate that pattern and
wire the broker automatically.

---

## 9. Conformance

`msgbox` and `inputbox` already use these conventions (they were the source
for recovering the exact metrics). `FileDialog` / `ChDirDialog` and the
theme editor are known non-conformers and will be migrated in a later pass — the
constants are already available; it is a matter of substituting hard-coded
coordinates.
