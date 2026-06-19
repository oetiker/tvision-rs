//! Clusters of related toggles: [`CheckBoxes`], [`RadioButtons`], and
//! [`MultiCheckBoxes`].
//!
//! # The seam
//!
//! The three cluster kinds differ **only** in their box icon, marker characters,
//! and how `value` is interpreted, so the polymorphism is modeled as **data**
//! rather than separate behavior:
//!
//! * [`Cluster`] is the full engine — state + layout + nav + draw + events — and
//!   **branches on its [`ClusterKind`]** for the per-kind behavior. It is the only
//!   type that `impl`s [`View`] with real bodies.
//! * [`CheckBoxes`] / [`RadioButtons`] / [`MultiCheckBoxes`] are thin
//!   embed-and-delegate wrappers (`{ cluster: Cluster }`) whose `impl View`
//!   forwards every method to `self.cluster`. Their constructors build a
//!   `Cluster` with the right `ClusterKind`. This keeps named, familiar types
//!   while the logic lives once.
//!
//! # Layout
//!
//! `size.y` (the bounds height) is the **column-break period**: items fill
//! top-to-bottom within a column, then wrap to the next column. A column is
//! `6 + max-label-width` cells wide (the ` [ ] ` icon is 5 cells plus a 1-cell
//! gap). Label widths strip the `~`-toggle hotkey marker, which is not a printed
//! column.
//!
//! # Marker / icon data
//!
//! The box icons (` [ ] `, ` ( ) `) and marker chars (`X`, the bullet `•`) live
//! inline as per-kind data, with no theme edit needed.
//!
//! # Mouse hold-tracking
//!
//! Mouse-down begins a modal hold:
//! * **MouseDown:** select the item under the cursor (if enabled), set
//!   `tracking = true`, and call [`Context::start_mouse_track`]. Do NOT press yet.
//! * **MouseMove arm** (loop body): a no-op — the original only toggled cursor
//!   visibility while dragging, which has no TUI equivalent. Guarded by
//!   `tracking`.
//! * **MouseUp arm** (post-loop): press only if the item under the release
//!   position is still the selected one — the same-item release-confirm. Clear
//!   `tracking`. Guarded by `tracking`.
//!
//! The `abs_origin` field caches the view-local `(0, 0)` in absolute screen
//! coords from the last `draw`, used by the capture to localize events.
//!
//! # Value and help context
//!
//! The typed value protocol (`value`/`set_value`) is not implemented for clusters:
//! their `value`/`sel` fields are read and written directly by their owning dialog
//! rather than through the gather/scatter protocol. The original per-item
//! help-context offset (help id plus selected index) does not map onto tvision-rs's
//! string-identity [`HelpCtx`](crate::help::HelpCtx) and is not modeled.
//!
//! # Turbo Vision heritage
//!
//! Ports `TCluster` and its subclasses `TCheckBoxes` / `TRadioButtons` /
//! `TMultiCheckBoxes` (`tcluster.cpp`, `tcheckbo.cpp`, `tradiobu.cpp`,
//! `tmulchkb.cpp`). The abstract base with its per-kind virtual overrides becomes
//! one engine branching on a closed [`ClusterKind`] enum, with the named
//! subclasses as embed-and-delegate wrappers (deviations D1, D2). The palette
//! AttrPairs become `(lo, hi)` [`Role`] pairs, and coordinates arrive view-local
//! from the group so no manual localization is needed.

use crate::capture::TrackMask;
use crate::event::{Event, Key, ctrl_to_arrow, hot_key, is_alt_hotkey, is_plain_hotkey};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Options, Phase, Point, Rect, View, ViewState};

// ---------------------------------------------------------------------------
// ClusterKind — the data-driven polymorphism
// ---------------------------------------------------------------------------

/// Which concrete cluster behavior the engine runs. Replaces per-subclass virtual
/// dispatch with a closed set of data variants the engine branches on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClusterKind {
    /// Check boxes — `value` is a bitmask (bit `item` set ⇔ item checked).
    /// Box ` [ ] `, markers `" X"` (idx 0 / 1).
    CheckBoxes,
    /// Radio buttons — `value` is the selected item index. Box ` ( ) `, markers
    /// `" •"` (idx 0 / 1; CP437 0x07 bullet → U+2022). Moving the selection sets
    /// `value = item`.
    RadioButtons,
    /// Multi-state check boxes — `value` packs an n-bit state per item. Box
    /// ` [ ] `, markers from `states` (idx = the item's current state).
    MultiCheckBoxes {
        /// Number of distinct states an item cycles through.
        sel_range: u8,
        /// Packed item layout: `lo = flags & 0xff` is the per-item mask; `hi =
        /// flags >> 8` is the per-item bit-shift multiplier.
        flags: u16,
        /// The marker glyph for each state value (idx = state).
        states: String,
    },
}

impl ClusterKind {
    /// The box icon string (with the embedded marker slot at col+2).
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

/// The cluster engine (state + layout + nav + draw + events).
///
/// Embeds [`ViewState`], `impl`s [`View`] fully, and branches on
/// [`kind`](Cluster::kind) for per-kind behavior. The concrete
/// [`CheckBoxes`] / [`RadioButtons`] / [`MultiCheckBoxes`] wrappers delegate to
/// an instance of this.
///
/// # Turbo Vision heritage
///
/// Ports `TCluster` (`tcluster.cpp`).
pub struct Cluster {
    /// View state (geometry, flags, cursor) — the composition target.
    pub state: ViewState,
    /// The cluster value, interpreted per [`kind`](Cluster::kind) (bitmask /
    /// index / packed states). A `u32`; the multi-state kind uses all 32 bits.
    pub value: u32,
    /// The currently-highlighted item index.
    pub sel: i32,
    /// Enable mask: bit `item` set ⇔ item is enabled. Constructor default
    /// `0xFFFF_FFFF` (all enabled).
    pub enable_mask: u32,
    /// The item labels, in `cur = j*size.y + i` (column-major) fill order.
    pub strings: Vec<String>,
    /// The per-kind behavior selector.
    pub kind: ClusterKind,
    /// Absolute screen position of view-local `(0, 0)`, cached each `draw` for
    /// the mouse-tracking capture (the same pattern as [`Button`](crate::widgets::Button)).
    abs_origin: Point,
    /// Whether a mouse hold-track is in flight (between the arming `MouseDown`
    /// and the terminating `MouseUp`). Guards the `MouseMove`/`MouseUp` tracking
    /// arms against stray (untracked) events.
    tracking: bool,
}

impl Cluster {
    /// Build a cluster from `bounds`, `strings`, `kind`.
    ///
    /// Starts with `value = 0`, `sel = 0`, all items enabled, and the cursor at
    /// `(2, 0)` (inside the first box) and shown. The view is selectable,
    /// first-click-aware, and takes part in pre- and post-processing.
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
            abs_origin: Point::new(0, 0),
            tracking: false,
        }
    }

    /// Number of items.
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
    // Per-kind behavior (mark / multi_mark / press / moved_to) — branches on kind
    // -----------------------------------------------------------------------

    /// Whether `item` is "on" (a boolean view of the state). CheckBoxes: bit set;
    /// RadioButtons: `item == value`. The MultiCheckBoxes arm returns `false`
    /// because multi-state markers are computed through
    /// [`multi_mark`](Self::multi_mark) directly — this arm is in practice
    /// unreachable for the multi kind (only the check/radio arms have callers).
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

    /// The marker index for `item` (indexes the 2-char marker for check/radio, the
    /// `states` string for multi).
    ///
    /// For CheckBoxes/RadioButtons this is `mark(item) as usize` (0 or 1). For
    /// MultiCheckBoxes it is the packed-state read:
    /// `(value & (flo << fhi)) >> fhi`, where `flo = flags & 0xff` and
    /// `fhi = (flags >> 8) * item`.
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

    /// Act on `item` (the per-kind toggle).
    ///
    /// CheckBoxes: `value ^= 1 << item`. RadioButtons: `value = item`.
    /// MultiCheckBoxes: cycle the packed state `0 → 1 → … → sel_range-1 → 0`.
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

    /// The hook fired when `sel` moves. RadioButtons set `value = item`; the
    /// others do nothing.
    fn moved_to(&mut self, item: i32) {
        if let ClusterKind::RadioButtons = self.kind {
            self.value = item as u32;
        }
    }

    /// Whether `item` is enabled. `item >= 32 → false` (the enable mask is
    /// 32 bits wide).
    fn button_state(&self, item: i32) -> bool {
        item < 32 && (self.enable_mask & (1u32 << item)) != 0
    }

    /// Enable/disable the items in `a_mask` and recompute selectability (the
    /// cluster is selectable iff at least one item is enabled).
    ///
    /// With `n = count < 32`, `test_mask = (1 << n) - 1`; the cluster's
    /// `selectable` option is set iff any enabled bit lies in `test_mask`.
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

    /// Move the selection to item `s`, where `i` is the **loop step-counter**
    /// (number of items scanned), not an item index.
    ///
    /// Guard: `i <= count` aborts the move when the nav loop scanned `count` items
    /// without finding an enabled one (the all-disabled case). On success, sets
    /// `sel = s` and runs the [`moved_to`](Self::moved_to) hook.
    fn move_sel(&mut self, i: i32, s: i32) {
        if i <= self.count() {
            self.sel = s;
            self.moved_to(self.sel);
        }
    }

    // -----------------------------------------------------------------------
    // Layout math (column / row / find_sel)
    // -----------------------------------------------------------------------

    /// The left column (cell x) at which `item`'s box is drawn. The `-6`/`+6`
    /// width-walk: each column is `6 + max-label-width` cells wide, with `width`
    /// reset at each column break (`i % size.y == 0`). Label widths use
    /// [`cstrlen`] (which strips the `~` hotkey marker).
    ///
    /// `l` deliberately persists across iterations (declared outside the loop —
    /// defensive when `i >= count`).
    fn column(&self, item: i32) -> i32 {
        let size_y = self.size_y();
        // `size_y <= 0` guards the `% size_y` below (a zero-height cluster has a
        // single column at 0); `item < size_y` is the early-out.
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

    /// The row (cell y) at which `item` is drawn: `item % size.y`.
    fn row(&self, item: i32) -> i32 {
        let size_y = self.size_y();
        // Guard `% 0`: a zero-height cluster has no rows.
        if size_y <= 0 { -1 } else { item % size_y }
    }

    /// The item index at view-local point `p`, or `-1` if none. Walks columns
    /// (`while p.x >= column(i + size.y)`), then adds `p.y`; returns `-1` if the
    /// result is out of range.
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

    /// Paint the cluster.
    ///
    /// Per row `i` (the outer loop runs `0..=size.y`; the extra `i == size.y` row
    /// writes at `y == size.y`, which [`put_char`](DrawCtx::put_char) clips away —
    /// a harmless no-op kept for layout symmetry): blank the full width in the
    /// normal `lo` color, then for each item in the row (`cur = j*size.y + i`,
    /// `j` over the columns) whose `column(cur) < size.x`: pick the item color
    /// (disabled if disabled, selected if it is the selected item in a focused
    /// cluster, else normal), re-fill `col..size.x` in that color, draw the icon,
    /// the marker glyph at `col+2` in the row's `lo` color, and the label at
    /// `col+5`.
    ///
    /// Ends by parking the cursor at `(column(sel)+2, row(sel))` so the hardware
    /// cursor tracks the selection (surfaced via the base cursor request).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache the absolute origin for the mouse-tracking capture: the
        // MouseTrackCapture converts absolute mouse coords to view-local via
        // this value, matching the Button::abs_origin pattern.
        self.abs_origin = ctx.origin();

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

    /// Handle keyboard and mouse events.
    ///
    /// The mouse-down auto-select lives in `Group`, so this body starts at the
    /// selectable guard. On mouse-down: select the item and arm the mouse-track
    /// capture; press fires on mouse-up at a release inside the same item. On
    /// key-down: ctrl-to-arrow aliasing, the four arrow navigators (focused
    /// only), the Alt-hotkey / plain-letter accelerator scan, then focused-Space
    /// → press.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        if !self.state.options.selectable {
            return;
        }

        match *ev {
            // ---------------------------------------------------------------
            // evMouseDown — select the item under the cursor, arm the tracking
            // capture; do NOT press yet.
            //
            // MouseDown is the first loop iteration (select + arm). The press
            // decision happens in the MouseUp arm (same-item release-confirm).
            // ---------------------------------------------------------------
            Event::MouseDown(me) => {
                let mouse = me.position; // already view-local
                let i = self.find_sel(mouse);
                if i != -1 && self.button_state(i) {
                    self.sel = i;
                }
                // Arm the mouse-track capture if we have a ViewId. Without a
                // ViewId (test-only / uninserted cluster — ids are stamped at
                // Group::insert) we fall through to the single-shot press ON
                // DOWN, diverging from the C++ release-confirm semantics.
                if let Some(id) = self.state.id() {
                    self.tracking = true;
                    ctx.start_mouse_track(
                        id,
                        self.abs_origin,
                        TrackMask {
                            mouse_move: true,
                            ..Default::default()
                        },
                    );
                } else {
                    // Degenerate fallback (no ViewId): single-shot press.
                    if i != -1 && i == self.sel {
                        self.press(self.sel);
                    }
                }
                ev.clear();
            }

            // ---------------------------------------------------------------
            // MouseMove arm — the C++ loop body (`tcluster.cpp:174-178`):
            // `showCursor` / `hideCursor` toggled on item containment.
            // tvision-rs has no TUI cursor-visibility equivalent (the hardware cursor
            // position is tracked via `set_cursor` / `cursor_request`, not a
            // per-item toggle), so this is a faithful no-op. Guarded by
            // `tracking` against stray moves.
            // ---------------------------------------------------------------
            Event::MouseMove(_) if self.tracking => {
                // C++ tcluster.cpp:175-178: showCursor/hideCursor only — no
                // state or value change. tvision-rs drops the cue (no equivalent).
                ev.clear();
            }

            // ---------------------------------------------------------------
            // MouseUp arm — post-loop press confirm (`tcluster.cpp:181-184`):
            // press iff `findSel(up_pos) == sel`. Guarded by `tracking`.
            // ---------------------------------------------------------------
            Event::MouseUp(me) if self.tracking => {
                self.tracking = false;
                // C++ tcluster.cpp:181-184:
                //   mouse = makeLocal(event.mouse.where);
                //   if (findSel(mouse) == sel) { press(sel); drawView(); }
                let up = me.position; // already view-local (localized by capture)
                if self.find_sel(up) == self.sel {
                    self.press(self.sel);
                }
                ev.clear();
            }

            // ---------------------------------------------------------------
            // evKeyDown — ctrlToArrow + arrow nav (focused) + the accelerator
            // scan + focused-Space → press (`tcluster.cpp:190-291`).
            // ---------------------------------------------------------------
            Event::KeyDown(ke) => {
                // `switch (ctrlToArrow(event.keyDown.keyCode))` (tcluster.cpp:192)
                // — WordStar Ctrl-letter aliases (Ctrl+E→Up, Ctrl+X→Down, …).
                let ke = ctrl_to_arrow(ke);
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
                    // The C++ `default:` — the hotkey accelerator scan, then
                    // the focused-Space press (the scan runs FIRST, matching
                    // the C++ ordering).
                    _ => {
                        // Accelerator scan: Alt+hotkey from anywhere, or the
                        // plain letter when focused or on the post-process walk
                        // (`ctx.phase() == Phase::PostProcess`).
                        for i in 0..count {
                            let Some(c) = hot_key(&self.strings[i as usize]) else {
                                continue;
                            };
                            if is_alt_hotkey(&ke, c)
                                || ((ctx.phase() == Phase::PostProcess || focused)
                                    && is_plain_hotkey(&ke, c))
                            {
                                if self.button_state(i) {
                                    // KNOWN DEVIATION: the C++ gates the press on
                                    // `focus()` succeeding synchronously
                                    // (`tcluster.cpp:283`); tvision-rs's `request_focus`
                                    // is deferred with no success return (same
                                    // class as the deferred-focus notes at
                                    // group.rs `focus_child`), so we press
                                    // immediately and queue the focus.
                                    if let Some(id) = self.state.id() {
                                        ctx.request_focus(id);
                                    }
                                    self.sel = i;
                                    self.moved_to(self.sel);
                                    self.press(self.sel);
                                    ev.clear();
                                }
                                // C++ `return`s after ANY hotkey match — even a
                                // disabled item stops the scan (and skips the
                                // Space arm); only an enabled match acts/clears.
                                return;
                            }
                        }
                        // Focused-Space → press (after the scan, as in C++).
                        if ke.key == Key::Char(' ') && focused {
                            self.press(self.sel);
                            ev.clear();
                        }
                    }
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
    /// degrades to a space (defensive — the marker table is expected to be long
    /// enough to cover every reachable index).
    fn marker_char(&self, item: i32) -> char {
        let idx = self.multi_mark(item);
        match &self.kind {
            ClusterKind::CheckBoxes => [' ', 'X'].get(idx).copied().unwrap_or(' '),
            ClusterKind::RadioButtons => [' ', '\u{2022}'].get(idx).copied().unwrap_or(' '),
            ClusterKind::MultiCheckBoxes { states, .. } => states.chars().nth(idx).unwrap_or(' '),
        }
    }
}

/// Display width of a `~`-marked control string, **ignoring** the `~` toggle
/// characters (they are not printed columns). Used by [`Cluster::column`] for the
/// column-width walk.
fn cstrlen(s: &str) -> i32 {
    crate::text::width(&s.replace('~', "")) as i32
}

// ---------------------------------------------------------------------------
// Concrete subclasses — thin embed-and-delegate wrappers
// ---------------------------------------------------------------------------

/// A column of independent checkboxes; `value` is a bitmask. An
/// embed-and-delegate wrapper over [`Cluster`] with [`ClusterKind::CheckBoxes`].
///
/// # Turbo Vision heritage
///
/// Ports `TCheckBoxes` (`tcheckbo.cpp`).
pub struct CheckBoxes {
    /// The shared engine (state + layout + nav + draw + events).
    pub cluster: Cluster,
}

#[crate::delegate(to = cluster, skip(apply_scroll_sync, focus_descendant, grabs_focus_on_click, set_value, value))]
impl View for CheckBoxes {
    /// Downcast hook: allows `apply_modal_completion` (FindPick/ReplacePick) to
    /// read `cluster.value` directly from the in-tree CheckBoxes widget.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// This cluster's packed bit word as [`FieldValue::Bits`] (a bitmask). Ports
    /// `TCluster::getData` (copies `value`).
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Bits(self.cluster.value))
    }

    /// Load a [`FieldValue::Bits`] bit word; other variants are ignored. Ports
    /// `TCluster::setData`.
    fn set_value(&mut self, v: crate::data::FieldValue) {
        if let crate::data::FieldValue::Bits(bits) = v {
            self.cluster.value = bits;
        }
    }
}

/// A column of mutually-exclusive buttons; `value` is the selected index. An
/// embed-and-delegate wrapper over [`Cluster`] with
/// [`ClusterKind::RadioButtons`].
///
/// # Turbo Vision heritage
///
/// Ports `TRadioButtons` (`tradiobu.cpp`).
pub struct RadioButtons {
    /// The shared engine (state + layout + nav + draw + events).
    pub cluster: Cluster,
}

#[crate::delegate(to = cluster, skip(apply_scroll_sync, as_any_mut, focus_descendant, grabs_focus_on_click, set_value, value))]
impl View for RadioButtons {
    /// This cluster's value as [`FieldValue::Bits`] (the selected index). Ports
    /// `TCluster::getData`.
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Bits(self.cluster.value))
    }

    /// Load a [`FieldValue::Bits`] (the selected index); other variants ignored.
    fn set_value(&mut self, v: crate::data::FieldValue) {
        if let crate::data::FieldValue::Bits(bits) = v {
            self.cluster.value = bits;
        }
    }
}

/// Checkboxes with multi-state items; `value` packs an n-bit state per item. An
/// embed-and-delegate wrapper over [`Cluster`] with
/// [`ClusterKind::MultiCheckBoxes`].
///
/// # Turbo Vision heritage
///
/// Ports `TMultiCheckBoxes` (`tmulchkb.cpp`).
pub struct MultiCheckBoxes {
    /// The shared engine (state + layout + nav + draw + events).
    pub cluster: Cluster,
}

#[crate::delegate(to = cluster, skip(apply_scroll_sync, as_any_mut, focus_descendant, grabs_focus_on_click, set_value, value))]
impl View for MultiCheckBoxes {}

impl CheckBoxes {
    /// Build from `bounds` + `strings`.
    pub fn new(bounds: Rect, strings: Vec<String>) -> Self {
        CheckBoxes {
            cluster: Cluster::new(bounds, strings, ClusterKind::CheckBoxes),
        }
    }
}

impl RadioButtons {
    /// Build from `bounds` + `strings`.
    pub fn new(bounds: Rect, strings: Vec<String>) -> Self {
        RadioButtons {
            cluster: Cluster::new(bounds, strings, ClusterKind::RadioButtons),
        }
    }
}

impl MultiCheckBoxes {
    /// Build from `bounds` + `strings` plus `sel_range`, `flags`, and `states`
    /// (the marker string).
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

    /// Like [`with_ctx`] but returns the deferred vec, for asserting on the
    /// accelerator scan's `Deferred::FocusById`.
    fn with_ctx_d<R>(f: impl FnOnce(&mut Context) -> R) -> (Vec<crate::view::Deferred>, R) {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        let r = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            f(&mut ctx)
        };
        (deferred, r)
    }

    fn alt_key_ev(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Char(c),
            crate::event::KeyModifiers {
                alt: true,
                ..Default::default()
            },
        ))
    }

    fn ctrl_key_ev(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Char(c),
            crate::event::KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ))
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

    fn mouse_move_at(x: i32, y: i32) -> Event {
        Event::MouseMove(MouseEvent {
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

    fn mouse_up_at(x: i32, y: i32) -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons::default(),
            flags: MouseEventFlags::default(),
            wheel: MouseWheel::None,
            modifiers: crate::event::KeyModifiers::default(),
        })
    }

    /// Stamp a `ViewId` onto a `CheckBoxes`' inner `Cluster` (as `Group::insert` would).
    fn stamp_id(c: &mut CheckBoxes) -> crate::view::ViewId {
        let id = crate::view::ViewId::next();
        c.cluster.state.id = Some(id);
        id
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

    // -- Accelerator scan -----------------------------------------------------

    /// Alt+hotkey selects + presses the item from anywhere (no focus/phase
    /// gate) and queues the deferred focus request.
    #[test]
    fn alt_hotkey_selects_and_presses() {
        let mut c = CheckBoxes::new(
            Rect::new(0, 0, 20, 3),
            strs(&["~A~one", "~B~two", "~C~tri"]),
        );
        let id = crate::view::ViewId::next();
        c.cluster.state.id = Some(id);
        c.cluster.sel = 0;
        let mut ev = alt_key_ev('b');
        let (deferred, ()) = with_ctx_d(|ctx| c.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "Alt+hotkey is consumed");
        assert_eq!(c.cluster.sel, 1, "sel moves to the hot item");
        assert_eq!(c.cluster.value, 0b10, "press toggled item 1's bit");
        assert_eq!(deferred.len(), 1);
        assert!(
            matches!(deferred[0], crate::view::Deferred::FocusById(d) if d == id),
            "the deferred focus request targets the cluster (deviation: queued, not synchronous focus())"
        );
    }

    /// The plain hotkey letter presses when the cluster is FOCUSED (the
    /// `(state & sfFocused)` leg of `tcluster.cpp:263-264`).
    #[test]
    fn plain_hotkey_presses_when_focused() {
        let mut c = CheckBoxes::new(
            Rect::new(0, 0, 20, 3),
            strs(&["~A~one", "~B~two", "~C~tri"]),
        );
        c.cluster.state.state.focused = true;
        c.cluster.sel = 0;
        let mut ev = key_ev(Key::Char('c'));
        with_ctx(|ctx| c.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "focused plain hotkey is consumed");
        assert_eq!(c.cluster.sel, 2);
        assert_eq!(c.cluster.value, 0b100, "press toggled item 2's bit");
    }

    /// The plain hotkey letter presses UNFOCUSED on the post-process walk (the
    /// post-process phase leg), and is ignored unfocused at the default (focused)
    /// phase.
    #[test]
    fn plain_hotkey_unfocused_post_process_only() {
        let mut c = CheckBoxes::new(
            Rect::new(0, 0, 20, 3),
            strs(&["~A~one", "~B~two", "~C~tri"]),
        );
        c.cluster.sel = 0;

        // Default phase, unfocused → ignored.
        let mut ev = key_ev(Key::Char('b'));
        with_ctx(|ctx| c.handle_event(&mut ev, ctx));
        assert!(
            !ev.is_nothing(),
            "unfocused plain letter at phFocused is left live"
        );
        assert_eq!(c.cluster.value, 0, "no press");

        // Post-process walk, unfocused → presses.
        let mut ev = key_ev(Key::Char('b'));
        with_ctx(|ctx| {
            ctx.set_phase(Phase::PostProcess);
            c.handle_event(&mut ev, ctx)
        });
        assert!(ev.is_nothing(), "postProcess plain letter is consumed");
        assert_eq!(c.cluster.sel, 1);
        assert_eq!(c.cluster.value, 0b10, "press toggled item 1's bit");
    }

    /// A hotkey match on a DISABLED item neither presses nor clears the event,
    /// and STOPS the scan — a later enabled item with the same hotkey letter
    /// must not press (the C++ `return` fires on any match, `tcluster.cpp:289`).
    #[test]
    fn disabled_hotkey_match_stops_scan_without_press() {
        let mut c = CheckBoxes::new(
            Rect::new(0, 0, 20, 3),
            strs(&["~X~one", "~X~two", "~C~tri"]),
        );
        // Disable item 0 (the first '~X~' match).
        c.cluster.set_button_state(0b001, false);
        c.cluster.sel = 2;
        let mut ev = alt_key_ev('x');
        let (deferred, ()) = with_ctx_d(|ctx| c.handle_event(&mut ev, ctx));
        assert!(
            !ev.is_nothing(),
            "a disabled-item match does NOT clear the event (clearEvent is inside buttonState)"
        );
        assert_eq!(c.cluster.value, 0, "no press — not even item 1 ('~X~two')");
        assert_eq!(c.cluster.sel, 2, "sel unchanged");
        assert!(deferred.is_empty(), "no focus request");
    }

    /// `ctrlToArrow`: Ctrl+E is the WordStar alias for Up (`tcluster.cpp:192`).
    #[test]
    fn ctrl_e_aliases_up() {
        let mut c = RadioButtons::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 1;
        let mut ev = ctrl_key_ev('e');
        with_ctx(|ctx| c.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "Ctrl+E is consumed like Up");
        assert_eq!(c.cluster.sel, 0, "moved up");
    }

    // -- Mouse hold-track: release-confirm ------------------------------------

    /// Degenerate fallback (no ViewId — uninserted cluster): single-shot press
    /// fires on mouse-down, preserving backwards compat for tests and inline use.
    #[test]
    fn mouse_down_selects_and_presses_item_no_id() {
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
            "fallback: mouse-down pressed item 2 immediately (no ViewId)"
        );
    }

    /// With a ViewId (inserted cluster): mouse-down selects + arms tracking;
    /// press fires only on release over the same item (`tcluster.cpp:181-184`).
    #[test]
    fn mouse_track_release_inside_presses() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 0;
        let _id = stamp_id(&mut c);

        // MouseDown at item 2 (col 0 row 2): select, arm tracking, do NOT press.
        let (deferred, ()) = with_ctx_d(|ctx| {
            let mut ev = mouse_down_at(0, 2);
            c.handle_event(&mut ev, ctx);
            assert!(ev.is_nothing(), "mouse-down consumed");
        });
        assert_eq!(c.cluster.sel, 2, "mouse-down selected item 2");
        assert_eq!(c.cluster.value, 0, "no press on mouse-down");
        assert!(c.cluster.tracking, "tracking armed");
        assert_eq!(deferred.len(), 1, "one PushCapture deferred");
        assert!(
            matches!(deferred[0], crate::view::Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        if let crate::view::Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), Some(_id), "capture routes to this cluster's id");
        }

        // MouseUp at the same item (item 2, col 0 row 2): press fires.
        // (Position is view-local — the capture localizes it.)
        with_ctx(|ctx| {
            let mut ev = mouse_up_at(0, 2);
            c.handle_event(&mut ev, ctx);
            assert!(ev.is_nothing(), "mouse-up consumed");
        });
        assert!(!c.cluster.tracking, "tracking cleared on MouseUp");
        assert_eq!(
            c.cluster.value, 0b100,
            "press fired on release-inside (tcluster.cpp:181-184)"
        );
    }

    /// Down on item A + release over item B = no press (tcluster.cpp:181-184:
    /// `if(findSel(mouse) == sel)` — the release must be on the SAME item).
    #[test]
    fn mouse_track_release_on_different_item_no_press() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        c.cluster.sel = 0;
        let _id = stamp_id(&mut c);

        // MouseDown at item 0 (col 0 row 0).
        with_ctx_d(|ctx| {
            let mut ev = mouse_down_at(0, 0);
            c.handle_event(&mut ev, ctx);
        });
        assert_eq!(c.cluster.sel, 0, "item 0 selected");
        assert!(c.cluster.tracking);

        // MouseUp at item 1 (col 0 row 1) — different item → no press.
        with_ctx(|ctx| {
            let mut ev = mouse_up_at(0, 1);
            c.handle_event(&mut ev, ctx);
        });
        assert!(!c.cluster.tracking);
        assert_eq!(
            c.cluster.value, 0,
            "no press: released on item 1, not item 0"
        );
    }

    /// Down on item + release outside all items = no press (tcluster.cpp:181-184:
    /// `findSel` returns -1 for an out-of-bounds position, which != sel).
    #[test]
    fn mouse_track_release_outside_all_items_no_press() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        c.cluster.state.state.focused = true;
        let _id = stamp_id(&mut c);

        with_ctx_d(|ctx| {
            let mut ev = mouse_down_at(0, 0);
            c.handle_event(&mut ev, ctx);
        });
        assert!(c.cluster.tracking);

        // Release outside the view extent (y = 5 is out of bounds for a 3-row cluster).
        with_ctx(|ctx| {
            let mut ev = mouse_up_at(0, 5);
            c.handle_event(&mut ev, ctx);
        });
        assert!(!c.cluster.tracking);
        assert_eq!(c.cluster.value, 0, "no press: released outside all items");
    }

    /// MouseMove during tracking is consumed but changes no state (faithful
    /// no-op: the C++ only toggled cursor show/hide which tvision-rs has no equivalent for).
    #[test]
    fn mouse_track_move_is_consumed_noop() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        let _id = stamp_id(&mut c);

        with_ctx_d(|ctx| {
            let mut ev = mouse_down_at(0, 0);
            c.handle_event(&mut ev, ctx);
        });
        assert!(c.cluster.tracking);
        let value_before = c.cluster.value;
        let sel_before = c.cluster.sel;

        with_ctx(|ctx| {
            let mut ev = mouse_move_at(0, 2);
            c.handle_event(&mut ev, ctx);
            assert!(ev.is_nothing(), "tracked move is consumed");
        });
        assert!(c.cluster.tracking, "still tracking after move");
        assert_eq!(c.cluster.value, value_before, "value unchanged");
        assert_eq!(c.cluster.sel, sel_before, "sel unchanged");
    }

    /// A stray MouseUp with no tracking in flight falls through untouched (the
    /// `tracking` guard — MouseUp is not mask-gated in `Group::wants`).
    #[test]
    fn stray_mouse_up_without_tracking_falls_through() {
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 3), strs(&["a", "b", "c"]));
        let _id = stamp_id(&mut c);

        with_ctx(|ctx| {
            let mut ev = mouse_up_at(0, 0);
            c.handle_event(&mut ev, ctx);
            assert!(!ev.is_nothing(), "stray up is NOT consumed");
        });
        assert_eq!(c.cluster.value, 0, "stray up fires nothing");
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

    // -- Value round-trips (Task 2: FieldValue::Bits) -----------------------

    #[test]
    fn checkboxes_value_round_trips_as_bits() {
        use crate::data::FieldValue;
        use crate::view::View;
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 4), strs(&["a", "b", "c"]));
        c.cluster.value = 0b101;
        assert_eq!(c.value(), Some(FieldValue::Bits(0b101)));
        c.set_value(FieldValue::Bits(0b010));
        assert_eq!(
            c.cluster.value, 0b010,
            "set_value(Bits) writes the bit word"
        );
        // A variant the control does not understand is ignored.
        c.set_value(FieldValue::Text("x".into()));
        assert_eq!(c.cluster.value, 0b010, "non-Bits value is ignored");
    }

    #[test]
    fn radiobuttons_value_round_trips_as_bits() {
        use crate::data::FieldValue;
        use crate::view::View;
        let mut r = RadioButtons::new(Rect::new(0, 0, 20, 4), strs(&["a", "b", "c"]));
        r.cluster.value = 2; // selected index
        assert_eq!(r.value(), Some(FieldValue::Bits(2)));
        r.set_value(FieldValue::Bits(1));
        assert_eq!(
            r.cluster.value, 1,
            "set_value(Bits) sets the selected index"
        );
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
