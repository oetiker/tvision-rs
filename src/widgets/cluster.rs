//! `TCluster` + `TCheckBoxes` / `TRadioButtons` / `TMultiCheckBoxes` — faithful
//! Rust port of `tcluster.cpp` / `tcheckbo.cpp` / `tradiobu.cpp` / `tmulchkb.cpp`
//! (rows 38 / 42 / 43 / 44).
//!
//! # The seam (D1 + D2)
//!
//! C++ has an abstract `TCluster` with virtuals (`mark`/`multiMark`/`press`/
//! `movedTo`/`draw`) overridden by three concrete subclasses that differ **only**
//! in: the box icon string, the marker characters, and how `value` is
//! interpreted. With no vtable inheritance (D2), the polymorphism is modeled as
//! **data** via the closed [`ClusterKind`] enum (D1: closed sets → enum):
//!
//! * [`Cluster`] is the full engine — state + layout + nav + draw + events — and
//!   **branches on `kind`** for the per-subclass behavior. It is the only type
//!   that `impl`s [`View`] with real bodies.
//! * [`CheckBoxes`] / [`RadioButtons`] / [`MultiCheckBoxes`] are thin D2
//!   embed-and-delegate wrappers (`{ cluster: Cluster }`) whose `impl View`
//!   forwards every method to `self.cluster`. Their constructors build a
//!   `Cluster` with the right `ClusterKind`. This keeps the named types a C++
//!   veteran recognizes while the logic lives once.
//!
//! # Layout (verbatim `column`/`row`/`findSel`)
//!
//! `size.y` (the bounds height) is the **column-break period**: items fill
//! top-to-bottom within a column, then wrap to the next column
//! (`cur = j*size.y + i`). [`Cluster::column`] ports the `-6`/`+6` width-walk
//! verbatim; a column is `6 + max-label-width` cells wide (` [ ] ` icon is 5
//! cells + a 1-cell gap). Label widths use a local `cstrlen` that **strips `~`**
//! (C++ `cstrlen`), since the `~`-toggle hotkey marker is not a printed column.
//!
//! # D-rules applied
//!
//! * **D3** `makeLocal` is gone — the `Group` delivers mouse positions already in
//!   view-local coords; `getExtent()` → `self.state.get_extent()`.
//! * **D4** `enum Event` match; `TView::handleEvent` (the mouse-down auto-select)
//!   is relocated to `Group`, so this `handle_event` starts at the `ofSelectable`
//!   guard.
//! * **D7** colors come from `ctx.style(Role::Cluster*)`. The C++ `getColor`
//!   AttrPairs (`0x0301`/`0x0402`/`0x0505`) are `(lo, hi)` pairs consumed by
//!   `moveCStr`'s `~`-toggle → the icon and label go through
//!   [`DrawCtx::put_cstr`]`(x, y, s, lo, hi)`; the marker glyph is a plain
//!   `put_char` in the row's `lo` style (faithful to `putChar` preserving the
//!   icon attribute).
//! * **D8** draw into the back buffer through `DrawCtx`; `drawView`/`writeBuf`/
//!   occlusion dropped. The C++ `setState` override only called `drawView` (a D8
//!   no-op), so there is **no** `set_state` override — the base flips the flag and
//!   fires the focus broadcast.
//! * **D10** `getData`/`setData`/`dataSize` (the typed value protocol) are
//!   **deferred to row 39** (`TInputLine`). `value`/`sel` are the eventual
//!   backing fields but no `get_data`/`set_data` is added here. **Breadcrumb for
//!   row 39:** `TRadioButtons::setData` additionally does `sel = value` after
//!   the base `setData` — fold that in when the data protocol lands.
//! * **D12** streamable dropped.
//!
//! # Marker / icon data
//!
//! The box icons (` [ ] `, ` ( ) `) and marker chars (`X`, the CP437 0x07 bullet
//! → U+2022) live inline as per-`kind` data (the simplest realization of the
//! row-9 "glyphs fill in per-widget" convention — no `theme.rs` edit needed).
//!
//! # Deferrals (documented TODOs, not built)
//!
//! 1. **Mouse drag-cursor tracking loop** (`do { … } while(mouseEvent(…))`): the
//!    synchronous inner pump the scrollbar also deferred. `// TODO(row 31, D9)`.
//!    Single-shot fallback only (see [`Cluster::handle_event`]).
//! 2. **`ctrlToArrow` WordStar Ctrl-letter nav aliases**: not ported (shared
//!    helper, port centrally later). Only literal arrow keys + Space are handled.
//! 3. **Alt-hotkey / focused-letter accelerator scan** (`hotKey`/`getAltCode`):
//!    `// TODO(row 41, accelerators)` — accelerators land with `TLabel`/menus.
//!    Only the focused-Space → press survives.
//! 4. **`getData`/`setData`/`dataSize`** (D10) → row 39.
//! 5. **`showMarkers` specialChars block** in `drawMultiBox`: **dropped**
//!    (`showMarkers` removed at row 23).
//! 6. **`getHelpCtx`'s `helpCtx + sel`** integer offset does not map onto the
//!    string-identity [`HelpCtx`](crate::help::HelpCtx) newtype; it is dropped
//!    here (consistent with the project's `HelpCtx` treatment).

use crate::event::{Event, Key};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Options, Point, Rect, View, ViewState};

// ---------------------------------------------------------------------------
// ClusterKind — the data-driven polymorphism (D1 closed enum)
// ---------------------------------------------------------------------------

/// Which concrete cluster behavior the engine runs. Replaces the C++ virtual
/// overrides of `mark`/`multiMark`/`press`/`movedTo`/`draw` with data (D2: no
/// vtable inheritance; D1: a closed set → enum).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClusterKind {
    /// `TCheckBoxes` — `value` is a bitmask (bit `item` set ⇔ item checked).
    /// Box ` [ ] `, markers `" X"` (idx 0 / 1).
    CheckBoxes,
    /// `TRadioButtons` — `value` is the selected item index. Box ` ( ) `,
    /// markers `" •"` (idx 0 / 1; CP437 0x07 bullet → U+2022). `movedTo` sets
    /// `value = item`.
    RadioButtons,
    /// `TMultiCheckBoxes` — `value` packs an n-bit state per item. Box ` [ ] `,
    /// markers from `states` (idx = `multi_mark`).
    MultiCheckBoxes {
        /// `selRange` — number of distinct states an item cycles through.
        sel_range: u8,
        /// `flags` — `lo = flags & 0xff` is the per-item mask; `hi = flags >> 8`
        /// is the per-item bit-shift multiplier.
        flags: u16,
        /// `states` — the marker glyph for each state value (idx = state).
        states: String,
    },
}

impl ClusterKind {
    /// The box icon string (with the embedded marker slot at col+2). Faithful to
    /// each subclass's `drawMultiBox` first arg / `button` member.
    fn icon(&self) -> &'static str {
        match self {
            ClusterKind::CheckBoxes => " [ ] ",
            ClusterKind::RadioButtons => " ( ) ",
            ClusterKind::MultiCheckBoxes { .. } => " [ ] ",
        }
    }
}

// ---------------------------------------------------------------------------
// Cluster — the engine
// ---------------------------------------------------------------------------

/// `TCluster` — the cluster engine (state + layout + nav + draw + events).
///
/// Embeds [`ViewState`] (D2), `impl`s [`View`] fully, and branches on
/// [`kind`](Cluster::kind) for per-subclass behavior. The concrete
/// [`CheckBoxes`] / [`RadioButtons`] / [`MultiCheckBoxes`] wrappers delegate to
/// an instance of this.
pub struct Cluster {
    /// View state (geometry, flags, cursor) — the D2 composition target.
    pub state: ViewState,
    /// `value` — interpreted per [`kind`](Cluster::kind) (bitmask / index /
    /// packed states). Widened to `u32` (C++ `value` is a `long`/`int32_t`;
    /// `TMultiCheckBoxes` uses the full 32 bits).
    pub value: u32,
    /// `sel` — the currently-highlighted item index.
    pub sel: i32,
    /// `enableMask` — bit `item` set ⇔ item is enabled (`buttonState`). Ctor
    /// default `0xFFFF_FFFF` (all enabled).
    pub enable_mask: u32,
    /// `strings` — the item labels, in `cur = j*size.y + i` fill order.
    pub strings: Vec<String>,
    /// The per-subclass behavior selector (D1/D2).
    pub kind: ClusterKind,
}

impl Cluster {
    /// `TCluster::TCluster` — build a cluster from `bounds`, `strings`, `kind`.
    ///
    /// Faithful to the C++ ctor: `value = 0`, `sel = 0`,
    /// `options |= ofSelectable | ofFirstClick | ofPreProcess | ofPostProcess`,
    /// `setCursor(2, 0)`, `showCursor()`, `enableMask = 0xFFFFFFFF`.
    pub fn new(bounds: Rect, strings: Vec<String>, kind: ClusterKind) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            first_click: true,
            pre_process: true,
            post_process: true,
            ..Default::default()
        };
        state.set_cursor(2, 0);
        state.show_cursor();

        Cluster {
            state,
            value: 0,
            sel: 0,
            enable_mask: 0xFFFF_FFFF,
            strings,
            kind,
        }
    }

    /// Number of items (`strings->getCount()`).
    fn count(&self) -> i32 {
        self.strings.len() as i32
    }

    /// `size.y` — the column-break period (rows per column).
    fn size_y(&self) -> i32 {
        self.state.size.y
    }

    /// `size.x` — the view width.
    fn size_x(&self) -> i32 {
        self.state.size.x
    }

    // -----------------------------------------------------------------------
    // Per-kind behavior (mark / multiMark / press / movedTo) — branches on kind
    // -----------------------------------------------------------------------

    /// `TCluster::mark` — whether item is "on" (boolean view of the state).
    /// CheckBoxes: bit set; RadioButtons: `item == value`. The MultiCheckBoxes
    /// arm returns `false` to match the C++ base `TCluster::mark` (it is never
    /// overridden by `TMultiCheckBoxes`, which computes its marker through
    /// `multiMark` directly) — this arm is in practice unreachable for multi
    /// (`multi_mark` short-circuits it; only the check/radio arms have callers).
    fn mark(&self, item: i32) -> bool {
        match self.kind {
            // `item >= 32` → false (mirrors `button_state`'s 32-bit cap; the
            // shift would overflow). Reachable via `draw`→`marker_char`→
            // `multi_mark` for a >32-item CheckBoxes.
            ClusterKind::CheckBoxes => item < 32 && (self.value & (1u32 << item)) != 0,
            ClusterKind::RadioButtons => item == self.value as i32,
            ClusterKind::MultiCheckBoxes { .. } => false,
        }
    }

    /// `TCluster::multiMark` / `TMultiCheckBoxes::multiMark` — the marker index
    /// for `item` (indexes the 2-char marker for check/radio, the `states`
    /// string for multi).
    ///
    /// For CheckBoxes/RadioButtons this is `mark(item) as usize` (0 or 1). For
    /// MultiCheckBoxes it is the packed-state read:
    /// `(value & (flo << fhi)) >> fhi`, `flo = flags & 0xff`,
    /// `fhi = (flags >> 8) * item` (verbatim `tmulchkb.cpp`).
    fn multi_mark(&self, item: i32) -> usize {
        match &self.kind {
            ClusterKind::CheckBoxes | ClusterKind::RadioButtons => self.mark(item) as usize,
            ClusterKind::MultiCheckBoxes { flags, .. } => {
                let flo = (flags & 0xff) as u32;
                let fhi = (flags >> 8) as u32 * item as u32;
                // `fhi >= 32` → state 0 (mirrors `button_state`'s cap; the shift
                // would overflow). `checked_shl` yields None past the width.
                match flo.checked_shl(fhi) {
                    Some(mask) => ((self.value & mask) >> fhi) as usize,
                    None => 0,
                }
            }
        }
    }

    /// `TCluster::press` — act on `item` (the subclass override).
    ///
    /// CheckBoxes: `value ^= 1 << item`. RadioButtons: `value = item`.
    /// MultiCheckBoxes: cycle the packed state `0 → 1 → … → selRange-1 → 0`
    /// (verbatim `tmulchkb.cpp::press`).
    fn press(&mut self, item: i32) {
        match &self.kind {
            ClusterKind::CheckBoxes => {
                // `item >= 32` → no-op (mirrors `button_state`'s 32-bit cap;
                // the shift would overflow).
                if item < 32 {
                    self.value ^= 1u32 << item;
                }
            }
            ClusterKind::RadioButtons => self.value = item as u32,
            ClusterKind::MultiCheckBoxes {
                sel_range, flags, ..
            } => {
                let sel_range = *sel_range as i32;
                let flo = (flags & 0xff) as u32;
                let fhi = (flags >> 8) as u32 * item as u32;
                // `fhi >= 32` → no-op (the read mask `flo << fhi` and the write
                // would overflow; mirrors `button_state`'s cap).
                let Some(mask) = flo.checked_shl(fhi) else {
                    return;
                };
                let mut cur_state = ((self.value & mask) >> fhi) as i32;
                cur_state += 1;
                if cur_state >= sel_range {
                    cur_state = 0;
                }
                self.value = (self.value & !mask) | ((cur_state as u32) << fhi);
            }
        }
    }

    /// `TCluster::movedTo` — the subclass hook fired when `sel` moves.
    /// RadioButtons set `value = item`; the others do nothing.
    fn moved_to(&mut self, item: i32) {
        if let ClusterKind::RadioButtons = self.kind {
            self.value = item as u32;
        }
    }

    /// `TCluster::buttonState` — whether `item` is enabled. `item >= 32 → false`
    /// (faithful to the C++ 32-bit mask cap).
    fn button_state(&self, item: i32) -> bool {
        item < 32 && (self.enable_mask & (1u32 << item)) != 0
    }

    /// `TCluster::setButtonState` — enable/disable the items in `a_mask` and
    /// recompute `ofSelectable` (clear it iff *all* items are disabled).
    ///
    /// Faithful port: with `n = count < 32`, `testMask = (1 << n) - 1`;
    /// `ofSelectable` is set iff any enabled bit lies in `testMask`.
    pub fn set_button_state(&mut self, a_mask: u32, enable: bool) {
        if !enable {
            self.enable_mask &= !a_mask;
        } else {
            self.enable_mask |= a_mask;
        }
        let n = self.count();
        if n < 32 {
            let test_mask = (1u32 << n) - 1;
            self.state.options.selectable = (self.enable_mask & test_mask) != 0;
        }
    }

    /// `TCluster::moveSel(i, s)` — move the selection to item `s`, where `i` is
    /// the **loop step-counter** (number of items scanned), not an item index.
    ///
    /// Faithful guard: `if (i <= count)` aborts the move when the nav loop
    /// scanned `count` items without finding an enabled one (the all-disabled
    /// case). On success: `sel = s; movedTo(sel)`. The C++ `drawView()` is a D8
    /// no-op.
    fn move_sel(&mut self, i: i32, s: i32) {
        if i <= self.count() {
            self.sel = s;
            self.moved_to(self.sel);
        }
    }

    // -----------------------------------------------------------------------
    // Layout math (column / row / findSel) — verbatim ports
    // -----------------------------------------------------------------------

    /// `TCluster::column` — the left column (cell x) at which `item`'s box is
    /// drawn. Verbatim port of the `-6`/`+6` width-walk: each column is
    /// `6 + max-label-width` cells wide, `width` reset at each column break
    /// (`i % size.y == 0`). Label widths use [`cstrlen`] (strips `~`).
    ///
    /// `l` deliberately persists across iterations (matches the C++ `int l = 0;`
    /// declared outside the loop — defensive when `i >= count`).
    fn column(&self, item: i32) -> i32 {
        let size_y = self.size_y();
        // `size_y <= 0` guards the `% size_y` below (a zero-height cluster has a
        // single column at 0); `item < size_y` is the C++ early-out.
        if size_y <= 0 || item < size_y {
            0
        } else {
            let mut width = 0i32;
            let mut col = -6i32;
            let mut l = 0i32;
            let count = self.count();
            for i in 0..=item {
                if i % size_y == 0 {
                    col += width + 6;
                    width = 0;
                }
                if i < count {
                    l = cstrlen(&self.strings[i as usize]);
                }
                if l > width {
                    width = l;
                }
            }
            col
        }
    }

    /// `TCluster::row` — the row (cell y) at which `item` is drawn:
    /// `item % size.y`.
    fn row(&self, item: i32) -> i32 {
        let size_y = self.size_y();
        // Guard `% 0`: a zero-height cluster has no rows.
        if size_y <= 0 { -1 } else { item % size_y }
    }

    /// `TCluster::findSel` — the item index at view-local point `p`, or `-1` if
    /// none. Walks columns (`while p.x >= column(i + size.y)`), then adds `p.y`;
    /// returns `-1` if the result is out of range. Verbatim port.
    fn find_sel(&self, p: Point) -> i32 {
        let size_y = self.size_y();
        // Guard the `i += size_y` walk + the `column` calls below against a
        // zero-height cluster (no item at any point).
        if size_y <= 0 {
            return -1;
        }
        let r = self.state.get_extent();
        if !r.contains(p) {
            -1
        } else {
            let mut i = 0i32;
            while p.x >= self.column(i + size_y) {
                i += size_y;
            }
            let s = i + p.y;
            if s >= self.count() { -1 } else { s }
        }
    }
}

impl View for Cluster {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// `TCluster::drawMultiBox` — paint the cluster.
    ///
    /// Per row `i` (the outer loop runs `0..=size.y`; the extra `i == size.y`
    /// row writes at `y == size.y`, which `DrawCtx::put_char` clips away — a
    /// no-op, kept faithful to the C++ `<= size.y` bound): blank the full width
    /// in `cNorm.lo`, then for each item in the row (`cur = j*size.y + i`,
    /// `j` over the columns) whose `column(cur) < size.x`:
    /// pick the item color (`cDis` if disabled, `cSel` if selected+`sfSelected`,
    /// else `cNorm`), re-fill `col..size.x` in that color, draw the icon via
    /// `put_cstr`, the marker glyph via `put_char` at `col+2` in the row's `lo`,
    /// and the label via `put_cstr` at `col+5`.
    ///
    /// Ends with `setCursor(column(sel)+2, row(sel))` so the hardware cursor
    /// tracks the selection (surfaced via the base `cursor_request`).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // A zero-height cluster draws nothing. This early-return is also the
        // safety guard for the layout math below: `column`/`row`/the end-of-draw
        // `set_cursor` all do `% size_y`, which would divide-by-zero (panic in
        // every build profile) on a degenerate / resized-to-tiny cluster.
        if self.size_y() <= 0 {
            return;
        }

        // AttrPairs (lo, hi): cNorm = getColor(0x0301), cSel = getColor(0x0402),
        // cDis = getColor(0x0505). (lo = the 01/02/05 byte, hi = the 03/04/05.)
        let c_norm = (
            ctx.style(Role::ClusterNormal),
            ctx.style(Role::ClusterNormalShortcut),
        );
        let c_sel = (
            ctx.style(Role::ClusterSelected),
            ctx.style(Role::ClusterSelectedShortcut),
        );
        let c_dis = (
            ctx.style(Role::ClusterDisabled),
            ctx.style(Role::ClusterDisabled),
        );

        let size_x = self.size_x();
        let size_y = self.size_y();
        let count = self.count();
        let icon = self.kind.icon();
        let selected = self.state.state.selected;

        // (count - 1) / size.y + 1 — the number of columns. size_y > 0 is
        // guaranteed by the early-return at the top of draw.
        let j_max = (count - 1) / size_y + 1;

        for i in 0..=size_y {
            // Blank the whole row in cNorm.lo first.
            for x in 0..size_x {
                ctx.put_char(x, i, ' ', c_norm.0);
            }
            for j in 0..=j_max {
                let cur = j * size_y + i;
                if cur < count {
                    let col = self.column(cur);
                    if col < size_x {
                        let color = if !self.button_state(cur) {
                            c_dis
                        } else if cur == self.sel && selected {
                            c_sel
                        } else {
                            c_norm
                        };
                        // Re-fill col..size.x in the item color (lo) before the
                        // icon/marker/label.
                        for x in col..size_x {
                            ctx.put_char(x, i, ' ', color.0);
                        }
                        ctx.put_cstr(col, i, icon, color.0, color.1);
                        // The marker glyph at col+2, in the row's lo style
                        // (C++ putChar preserves the icon attribute = lo).
                        let marker = self.marker_char(cur);
                        ctx.put_char(col + 2, i, marker, color.0);
                        ctx.put_cstr(col + 5, i, &self.strings[cur as usize], color.0, color.1);
                    }
                }
            }
        }

        self.state
            .set_cursor(self.column(self.sel) + 2, self.row(self.sel));
    }

    /// `TCluster::handleEvent` — keyboard + mouse (with the documented deferrals).
    ///
    /// The C++ `TView::handleEvent` first line (mouse-down auto-select) is
    /// relocated to `Group` (D4), so this body starts at the `ofSelectable`
    /// guard. On `evMouseDown`: single-shot select+press (the drag-cursor
    /// `do/while` loop is **deferred** — `// TODO(row 31, D9)`). On `evKeyDown`:
    /// the four arrow navigators (focused-only) + focused-Space → press
    /// (`ctrlToArrow` aliases and the Alt-hotkey/letter accelerator scan are
    /// **deferred**).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let _ = ctx;
        if !self.state.options.selectable {
            return;
        }

        match *ev {
            // ---------------------------------------------------------------
            // evMouseDown — single-shot select + press.
            //
            // TODO(row 31, D9): the C++ runs a `do { … } while(mouseEvent(event,
            // evMouseMove))` loop that shows/hides the cursor as the held mouse
            // tracks across items, then presses iff the cursor is still on `sel`
            // at release. That synchronous inner pump needs the live event loop
            // (a capture handler on Event::MouseMove / MouseUp). Until then we
            // do exactly one select + one press per mouse-down.
            // ---------------------------------------------------------------
            Event::MouseDown(me) => {
                let mouse = me.position; // already view-local (D3)
                let i = self.find_sel(mouse);
                if i != -1 && self.button_state(i) {
                    self.sel = i;
                }
                // Single-shot: the cursor is necessarily still on `sel`, so the
                // C++ `if(findSel(mouse) == sel) press(sel)` reduces to: press if
                // the click resolved to the (now-)selected item. `find_sel` is
                // a pure function of `mouse`, so reuse the value computed above
                // rather than calling it a second time.
                if i != -1 && i == self.sel {
                    self.press(self.sel);
                }
                ev.clear();
            }

            // ---------------------------------------------------------------
            // evKeyDown — arrow nav (focused) + focused-Space → press.
            //
            // TODO(ctrlToArrow): the C++ wraps keyCode through ctrlToArrow()
            // (WordStar Ctrl-letter aliases). Not ported — only literal arrows.
            // TODO(row 41, accelerators): the `default:` Alt-hotkey / focused-
            // letter accelerator scan (hotKey/getAltCode) is dropped; only the
            // focused-Space press survives.
            // ---------------------------------------------------------------
            Event::KeyDown(ke) => {
                let focused = self.state.state.focused;
                let count = self.count();
                let size_y = self.size_y();

                match ke.key {
                    Key::Up if focused => {
                        let mut s = self.sel;
                        let mut i = 0;
                        loop {
                            i += 1;
                            s -= 1;
                            if s < 0 {
                                s = count - 1;
                            }
                            if self.button_state(s) || i > count {
                                break;
                            }
                        }
                        self.move_sel(i, s);
                        ev.clear();
                    }
                    Key::Down if focused => {
                        let mut s = self.sel;
                        let mut i = 0;
                        loop {
                            i += 1;
                            s += 1;
                            if s >= count {
                                s = 0;
                            }
                            if self.button_state(s) || i > count {
                                break;
                            }
                        }
                        self.move_sel(i, s);
                        ev.clear();
                    }
                    Key::Right if focused => {
                        let mut s = self.sel;
                        let mut i = 0;
                        loop {
                            i += 1;
                            s += size_y;
                            if s >= count {
                                s = 0;
                            }
                            if self.button_state(s) || i > count {
                                break;
                            }
                        }
                        self.move_sel(i, s);
                        ev.clear();
                    }
                    Key::Left if focused => {
                        let mut s = self.sel;
                        let mut i = 0;
                        loop {
                            i += 1;
                            if s > 0 {
                                s -= size_y;
                                if s < 0 {
                                    s = ((count + size_y - 1) / size_y) * size_y + s - 1;
                                    if s >= count {
                                        s = count - 1;
                                    }
                                }
                            } else {
                                s = count - 1;
                            }
                            if self.button_state(s) || i > count {
                                break;
                            }
                        }
                        self.move_sel(i, s);
                        ev.clear();
                    }
                    Key::Char(' ') if focused => {
                        self.press(self.sel);
                        ev.clear();
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }
}

impl Cluster {
    /// The marker glyph drawn at `col+2` for `item` — the per-kind marker table
    /// indexed by [`multi_mark`](Self::multi_mark).
    ///
    /// CheckBoxes: `" X"`. RadioButtons: `" •"` (CP437 0x07 bullet → U+2022).
    /// MultiCheckBoxes: the `states` string (idx = state). An out-of-range index
    /// degrades to a space (defensive; matches the C++ relying on the marker
    /// string being long enough).
    fn marker_char(&self, item: i32) -> char {
        let idx = self.multi_mark(item);
        match &self.kind {
            ClusterKind::CheckBoxes => [' ', 'X'].get(idx).copied().unwrap_or(' '),
            ClusterKind::RadioButtons => [' ', '\u{2022}'].get(idx).copied().unwrap_or(' '),
            ClusterKind::MultiCheckBoxes { states, .. } => states.chars().nth(idx).unwrap_or(' '),
        }
    }
}

/// C++ `cstrlen` — display width of a `~`-marked control string, **ignoring**
/// the `~` toggle characters (they are not printed columns). Used by
/// [`Cluster::column`] for the column-width walk.
fn cstrlen(s: &str) -> i32 {
    crate::text::width(&s.replace('~', "")) as i32
}

// ---------------------------------------------------------------------------
// Concrete subclasses — thin D2 embed-and-delegate wrappers
// ---------------------------------------------------------------------------

/// Generate a concrete cluster type that embeds [`Cluster`] and delegates every
/// [`View`] method to it (the D2 embed-and-delegate pattern). The constructor
/// and `kind` differ per type; the `View` impl is identical, so it is macro-ed.
macro_rules! cluster_wrapper {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        pub struct $name {
            /// The shared engine (state + layout + nav + draw + events).
            pub cluster: Cluster,
        }

        impl View for $name {
            fn state(&self) -> &ViewState {
                self.cluster.state()
            }
            fn state_mut(&mut self) -> &mut ViewState {
                self.cluster.state_mut()
            }
            fn draw(&mut self, ctx: &mut DrawCtx) {
                self.cluster.draw(ctx)
            }
            fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
                self.cluster.handle_event(ev, ctx)
            }
            fn set_state(
                &mut self,
                flag: crate::view::StateFlag,
                enable: bool,
                ctx: &mut Context,
            ) {
                self.cluster.set_state(flag, enable, ctx)
            }
            fn valid(&self, cmd: crate::command::Command) -> bool {
                self.cluster.valid(cmd)
            }
            fn awaken(&mut self) {
                self.cluster.awaken()
            }
            fn size_limits(&self, owner_size: Point) -> (Point, Point) {
                self.cluster.size_limits(owner_size)
            }
            fn calc_bounds(&mut self, owner_size: Point, delta: Point) -> Rect {
                self.cluster.calc_bounds(owner_size, delta)
            }
            fn change_bounds(&mut self, bounds: Rect) {
                self.cluster.change_bounds(bounds)
            }
            fn cursor_request(&self) -> Option<Point> {
                self.cluster.cursor_request()
            }
            fn find_mut(&mut self, id: crate::view::ViewId) -> Option<&mut dyn View> {
                self.cluster.find_mut(id)
            }
            fn remove_descendant(
                &mut self,
                id: crate::view::ViewId,
                ctx: &mut Context,
            ) -> bool {
                self.cluster.remove_descendant(id, ctx)
            }
            fn number(&self) -> Option<i16> {
                self.cluster.number()
            }
            fn select_window_num(&mut self, num: i16, ctx: &mut Context) -> bool {
                self.cluster.select_window_num(num, ctx)
            }
        }
    };
}

cluster_wrapper! {
    /// `TCheckBoxes` — a column of independent checkboxes; `value` is a bitmask.
    /// D2 embed-delegate wrapper over [`Cluster`] with [`ClusterKind::CheckBoxes`].
    CheckBoxes
}

cluster_wrapper! {
    /// `TRadioButtons` — a column of mutually-exclusive buttons; `value` is the
    /// selected index. D2 wrapper with [`ClusterKind::RadioButtons`].
    RadioButtons
}

cluster_wrapper! {
    /// `TMultiCheckBoxes` — checkboxes with multi-state items; `value` packs an
    /// n-bit state per item. D2 wrapper with [`ClusterKind::MultiCheckBoxes`].
    MultiCheckBoxes
}

impl CheckBoxes {
    /// `TCheckBoxes::TCheckBoxes` — build from `bounds` + `strings`.
    pub fn new(bounds: Rect, strings: Vec<String>) -> Self {
        CheckBoxes {
            cluster: Cluster::new(bounds, strings, ClusterKind::CheckBoxes),
        }
    }
}

impl RadioButtons {
    /// `TRadioButtons::TRadioButtons` — build from `bounds` + `strings`.
    pub fn new(bounds: Rect, strings: Vec<String>) -> Self {
        RadioButtons {
            cluster: Cluster::new(bounds, strings, ClusterKind::RadioButtons),
        }
    }
}

impl MultiCheckBoxes {
    /// `TMultiCheckBoxes::TMultiCheckBoxes` — build from `bounds` + `strings`
    /// plus `sel_range` (`selRange`), `flags`, and `states` (the marker string).
    pub fn new(
        bounds: Rect,
        strings: Vec<String>,
        sel_range: u8,
        flags: u16,
        states: String,
    ) -> Self {
        MultiCheckBoxes {
            cluster: Cluster::new(
                bounds,
                strings,
                ClusterKind::MultiCheckBoxes {
                    sel_range,
                    flags,
                    states,
                },
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{KeyEvent, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use std::collections::VecDeque;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn key_ev(key: Key) -> Event {
        Event::KeyDown(KeyEvent::from(key))
    }

    fn mouse_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: MouseEventFlags::default(),
            wheel: MouseWheel::None,
            modifiers: crate::event::KeyModifiers::default(),
        })
    }

    // -- Layout math: column / row / find_sel round-trip --------------------

    /// Discriminating layout test: **unequal-width labels across 3 columns with
    /// size.y = 2**. Equal widths would hide the `+6`/width-accumulation bugs.
    ///
    /// 5 items, size.y = 2 → columns:
    ///   col 0: items 0 ("ab", w2), 1 ("c", w1)        → max width 2
    ///   col 1: items 2 ("defg", w4), 3 ("hi", w2)     → max width 4
    ///   col 2: item 4 ("j", w1)                        → max width 1
    /// column(item): item<2 → 0. The walk: col starts -6, +width+6 at each break.
    ///   item 0,1 → col 0.
    ///   item 2,3 → 0 + (2 + 6) = 8.
    ///   item 4   → 8 + (4 + 6) = 18.
    #[test]
    fn column_row_layout_unequal_widths_three_columns() {
        let c = Cluster::new(
            Rect::new(0, 0, 30, 2),
            strs(&["ab", "c", "defg", "hi", "j"]),
            ClusterKind::CheckBoxes,
        );
        assert_eq!(c.size_y(), 2);
        assert_eq!(c.column(0), 0);
        assert_eq!(c.column(1), 0);
        assert_eq!(c.column(2), 8, "col1 = max('ab'..'c')=2, +6 = 8");
        assert_eq!(c.column(3), 8);
        assert_eq!(c.column(4), 18, "col2 = 8 + max('defg'..'hi')=4 +6 = 18");

        assert_eq!(c.row(0), 0);
        assert_eq!(c.row(1), 1);
        assert_eq!(c.row(2), 0);
        assert_eq!(c.row(3), 1);
        assert_eq!(c.row(4), 0);
    }

    /// find_sel round-trips against column/row: a point in each item's region
    /// must resolve back to that item.
    #[test]
    fn find_sel_round_trips_across_columns() {
        let c = Cluster::new(
            Rect::new(0, 0, 30, 2),
            strs(&["ab", "c", "defg", "hi", "j"]),
            ClusterKind::CheckBoxes,
        );
        // item 0 at col 0 row 0.
        assert_eq!(c.find_sel(Point::new(0, 0)), 0);
        // item 1 at col 0 row 1.
        assert_eq!(c.find_sel(Point::new(0, 1)), 1);
        // item 2 at col 8 row 0.
        assert_eq!(c.find_sel(Point::new(8, 0)), 2);
        // item 3 at col 8 row 1.
        assert_eq!(c.find_sel(Point::new(9, 1)), 3);
        // item 4 at col 18 row 0.
        assert_eq!(c.find_sel(Point::new(18, 0)), 4);
        // col 18 row 1 has no item (only item 4 in col 2) → -1.
        assert_eq!(c.find_sel(Point::new(18, 1)), -1);
        // outside extent → -1.
        assert_eq!(c.find_sel(Point::new(0, 5)), -1);
    }

    // -- CheckBoxes: Space toggles, arrows skip disabled --------------------

    #[test]
    fn checkboxes_space_toggles_sel_bit() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 1;
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Char(' '));
            c.handle_event(&mut ev, ctx);
            assert!(ev.is_nothing());
        });
        assert_eq!(c.cluster.value, 0b010, "space toggled bit 1 on");
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Char(' '));
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.value, 0, "space toggled bit 1 back off (XOR)");
    }

    #[test]
    fn checkboxes_arrow_down_skips_disabled() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 4), strs(&["a", "b", "c", "d"]));
        c.cluster.state.state.focused = true;
        // Disable item 1; from sel 0 Down should skip to 2.
        c.cluster.set_button_state(1 << 1, false);
        c.cluster.sel = 0;
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Down);
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.sel, 2, "Down skipped the disabled item 1");
    }

    #[test]
    fn checkboxes_set_button_state_disables_and_find_sel_respects_it() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        // Disable item 0; a mouse-down on it must not select it.
        c.cluster.set_button_state(1 << 0, false);
        c.cluster.sel = 1;
        with_ctx(|ctx| {
            // item 0 is at (0,0). find_sel finds it, but button_state is false →
            // sel stays at 1, and press operates on whatever find_sel == sel.
            let mut ev = mouse_down_at(0, 0);
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(
            c.cluster.sel, 1,
            "mouse-down on disabled item did not select"
        );
    }

    #[test]
    fn checkboxes_all_disabled_clears_selectable() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 2), strs(&["a", "b"]));
        assert!(c.cluster.state.options.selectable);
        c.cluster.set_button_state(0b11, false);
        assert!(
            !c.cluster.state.options.selectable,
            "all items disabled → ofSelectable cleared"
        );
        // Re-enable one → selectable again.
        c.cluster.set_button_state(0b01, true);
        assert!(c.cluster.state.options.selectable);
    }

    // -- RadioButtons: movedTo sets value, press sets value -----------------

    #[test]
    fn radio_arrow_down_movedto_sets_value() {
        let mut c = RadioButtons::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 0;
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Down);
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.sel, 1);
        assert_eq!(c.cluster.value, 1, "RadioButtons.movedTo sets value = sel");
    }

    #[test]
    fn radio_space_press_sets_value_only_one_marked() {
        let mut c = RadioButtons::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 2;
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Char(' '));
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.value, 2, "press sets value = item");
        // Exactly one marked: only item 2 returns mark()==true.
        assert!(!c.cluster.mark(0));
        assert!(!c.cluster.mark(1));
        assert!(c.cluster.mark(2));
    }

    // -- MultiCheckBoxes: press cycles state, multi_mark reads it back ------

    #[test]
    fn multi_press_cycles_state_and_multimark_reads_back() {
        // sel_range 3, flags 0x0301: flo = 0x01 (1-bit... but selRange 3 needs
        // 2 bits) → use flags lo = 0x03 (2-bit mask), hi = 0x02 (shift 2/item).
        // flo = 0x03, fhi = 2 * item.
        let mut c = MultiCheckBoxes::new(
            Rect::new(0, 0, 20, 3),
            strs(&["a", "b", "c"]),
            3,      // selRange: states 0,1,2
            0x0203, // hi=0x02 (shift 2 per item), lo=0x03 (2-bit mask)
            " XO".to_string(),
        );
        c.cluster.state.state.focused = true;
        c.cluster.sel = 1; // item 1 → fhi = 2

        // Initial state 0.
        assert_eq!(c.cluster.multi_mark(1), 0);
        // Press: 0 → 1.
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Char(' '));
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.multi_mark(1), 1);
        // Press: 1 → 2.
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Char(' '));
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.multi_mark(1), 2);
        // Press: 2 → wraps to 0 (>= selRange 3 → 0).
        with_ctx(|ctx| {
            let mut ev = key_ev(Key::Char(' '));
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.multi_mark(1), 0);
        // Item 0 (fhi 0) untouched throughout.
        assert_eq!(c.cluster.multi_mark(0), 0);
    }

    // -- Mouse single-shot select + press -----------------------------------

    #[test]
    fn mouse_down_selects_and_presses_item() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 0;
        // item 2 is at col 0 row 2.
        with_ctx(|ctx| {
            let mut ev = mouse_down_at(0, 2);
            c.handle_event(&mut ev, ctx);
            assert!(ev.is_nothing(), "mouse-down consumed");
        });
        assert_eq!(c.cluster.sel, 2, "mouse-down selected item 2");
        assert_eq!(
            c.cluster.value, 0b100,
            "mouse-down pressed item 2 (bit set)"
        );
    }

    // -- Cursor tracks the selection (drawMultiBox's end-of-draw setCursor) --

    #[test]
    fn draw_sets_cursor_to_column_plus_2_at_sel_and_cursor_request_surfaces_it() {
        // Multi-column layout so column(sel) is non-zero and discriminating.
        // 5 items, size.y = 2 → columns at x = 0 / 8 / 18 (see the layout test).
        let mut c = CheckBoxes::new(
            Rect::new(0, 0, 30, 2),
            strs(&["ab", "c", "defg", "hi", "j"]),
        );
        c.cluster.state.state.selected = true;
        c.cluster.state.state.focused = true; // so cursor_request surfaces it
        c.cluster.sel = 2; // col 8, row 0 → cursor (10, 0)

        let theme = Theme::classic_blue();
        let (backend, _screen) = HeadlessBackend::new(30, 2);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = c.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            c.draw(&mut dc);
        });

        // drawMultiBox ends with setCursor(column(sel)+2, row(sel)).
        assert_eq!(c.cluster.state.cursor, Point::new(10, 0));
        // The base cursor_request surfaces it (focused + cursor_vis from ctor).
        assert_eq!(c.cursor_request(), Some(Point::new(10, 0)));
    }

    // -- Regression: shift-overflow & zero-height guards --------------------

    /// M2 regression: a MultiCheckBoxes with > 16 items and 2-bit packing
    /// (`fhi = 2*item`) drives `fhi` to 32 at item 16. Without the `checked_shl`
    /// guard, `draw`'s marker pass (`flo << fhi` in `multi_mark`) overflows —
    /// debug panic, release silent corruption. Render a tall single-column
    /// cluster (height 17 so item 16 is drawn) and assert no panic.
    #[test]
    fn multi_draw_does_not_overflow_at_item_16() {
        let labels: Vec<String> = (0..17).map(|n| format!("i{n}")).collect();
        let mut c = MultiCheckBoxes::new(
            Rect::new(0, 0, 8, 17), // size.y = 17 → one column, all items drawn
            labels,
            3,      // selRange
            0x0203, // hi = 0x02 (shift 2 per item) → fhi = 2*16 = 32 at item 16
            " XO".to_string(),
        );
        c.cluster.state.state.selected = true;
        // Must not panic.
        let _ = snapshot_cluster(&mut c, 8, 17);
        // fhi >= 32 → the guard yields state 0 (no panic, no wrong-bit read).
        assert_eq!(
            c.cluster.multi_mark(16),
            0,
            "item 16 (fhi=32) reads back state 0 via the checked_shl guard"
        );
        // press at item 16 is a no-op (no panic, value unchanged).
        let before = c.cluster.value;
        c.cluster.press(16);
        assert_eq!(c.cluster.value, before, "press(16) is a no-op past the cap");
    }

    /// M1 regression: a zero-height cluster (degenerate / resized-to-tiny)
    /// must not divide-by-zero in `column`/`row`/`find_sel`/`draw`'s end-of-draw
    /// `set_cursor`. Without the guards every one of these `% 0`s panics in all
    /// build profiles.
    #[test]
    fn zero_height_cluster_does_not_panic() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 0), strs(&["a", "b", "c"]));
        // Layout math is robust.
        assert_eq!(c.cluster.column(0), 0);
        assert_eq!(c.cluster.column(5), 0);
        assert_eq!(c.cluster.row(0), -1);
        assert_eq!(c.cluster.find_sel(Point::new(0, 0)), -1);
        // draw on a zero-height buffer is a no-op (no panic).
        let _ = snapshot_cluster(&mut c, 20, 1);
    }

    // -- Snapshots: one per kind --------------------------------------------

    fn snapshot_cluster<V: View>(view: &mut V, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_checkboxes() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 14, 3), strs(&["~A~lpha", "Beta", "Gamma"]));
        c.cluster.state.state.selected = true;
        // Check item 0 and 2.
        c.cluster.value = 0b101;
        c.cluster.sel = 1;
        insta::assert_snapshot!(snapshot_cluster(&mut c, 14, 3));
    }

    #[test]
    fn snapshot_radiobuttons() {
        let mut c = RadioButtons::new(Rect::new(0, 0, 14, 3), strs(&["One", "Two", "Three"]));
        c.cluster.state.state.selected = true;
        c.cluster.value = 1; // Two selected.
        c.cluster.sel = 1;
        insta::assert_snapshot!(snapshot_cluster(&mut c, 14, 3));
    }

    #[test]
    fn snapshot_multicheckboxes() {
        let mut c = MultiCheckBoxes::new(
            Rect::new(0, 0, 14, 3),
            strs(&["Red", "Green", "Blue"]),
            3,
            0x0203,
            " XO".to_string(),
        );
        c.cluster.state.state.selected = true;
        // item 0 state 1 (X), item 1 state 2 (O), item 2 state 0 (space).
        // fhi = 2*item: item0 shift0 → 0b01; item1 shift2 → 0b10<<2 = 0b1000.
        c.cluster.value = 0b01 | (0b10 << 2);
        c.cluster.sel = 0;
        insta::assert_snapshot!(snapshot_cluster(&mut c, 14, 3));
    }
}
