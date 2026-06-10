//! `TStaticText` — faithful Rust port of `tstatict.cpp` (row 36, MECHANICAL).
//!
//! Displays a read-only multi-line text block, word-wrapping to fill its bounds.
//!
//! ## Algorithm (word-wrap)
//!
//! For each output row `y` in `0..size.y`: fill with `StaticText` color; if text
//! remains, handle a leading `\x03` (ETX) byte as a *center-this-line* flag
//! (persists across wrapped continuations until `\n` resets it); compute `last`
//! = the byte offset reached by scrolling `size.x` display columns
//! (`text::scroll`, relative to `i`); pack whole words (advance word-by-word
//! within the `last` limit); handle the back-off (if we overshot `last`, step
//! back to the last word-boundary `j`, or hard-break at `last` if the first word
//! alone overflows); draw `[i,p)` at column `draw_col` (`(size.x − width) / 2`
//! when centered, else 0); consume trailing spaces; on `\n` reset `center` and
//! advance past the newline.
//!
//! ## D-rules applied
//!
//! - **D1**: drop `T` prefix → `StaticText`; `snake_case` methods.
//! - **D2/D5**: `View` trait + `ViewState` composition. `growMode |= gfFixed`
//!   sets `state.grow_mode.fixed = true` (faithful to the C++ bit).
//! - **D7**: no palette chain — `ctx.style(Role::StaticText)` instead of
//!   `getColor(1)`.
//! - **D8**: draw via `DrawCtx`; no `writeLine`/`writeBuf`/`TDrawBuffer`.
//! - **D12**: no `TStreamable` (`read`/`write`/`build` dropped).
//! - **D13**: grapheme-based; byte-offset word-scan safe because space/newline/
//!   ETX (0x03) are single-byte ASCII — only `text::next` is used to advance
//!   through non-ASCII runs.

use crate::command::Command;
use crate::event::{Event, hot_key, is_alt_hotkey, is_plain_hotkey};
use crate::text;
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Options, Phase, Rect, View, ViewId, ViewState};

// ---------------------------------------------------------------------------
// StaticText
// ---------------------------------------------------------------------------

/// `TStaticText` — a read-only, word-wrapped text block (D2 View trait +
/// ViewState).
///
/// Embed pattern: `state: ViewState`, `impl View`, draw through `DrawCtx`.
/// No events (no `handle_event` override); not selectable.
pub struct StaticText {
    /// View state (geometry, flags, etc.) — the D2 composition target.
    pub state: ViewState,
    /// The text content. Held as a full `String`; no 255-byte truncation.
    text: String,
}

impl StaticText {
    /// Construct a static text view from `bounds` and `text`.
    ///
    /// Faithful to `TStaticText::TStaticText`:
    /// - `growMode |= gfFixed` → `state.grow_mode.fixed = true`.
    /// - Not selectable (static text ignores focus).
    pub fn new(bounds: Rect, text: impl Into<String>) -> Self {
        let mut state = ViewState::new(bounds);
        // gfFixed: the view keeps its size regardless of the owner's resize.
        state.grow_mode = GrowMode {
            fixed: true,
            ..Default::default()
        };
        // Not selectable — `TStaticText` has no `handleEvent` and is not
        // interactive (`options.selectable` is already false by default).
        StaticText {
            state,
            text: text.into(),
        }
    }

    /// The current text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replace the text content. Caller is responsible for triggering a redraw
    /// (the whole-tree repaint under D8 handles it on the next pump pass).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl View for StaticText {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// `TStaticText::draw` — word-wrap and paint.
    ///
    /// Faithful port of `tstatict.cpp`:
    ///
    /// ```text
    /// color = getColor(1);
    /// getText(buf); s = buf;  // we hold the full String, no 255-byte truncation
    /// l = s.size(); p = 0; y = 0; center = False;
    /// while (y < size.y) {
    ///     b.moveChar(0, ' ', color, size.x);
    ///     if (p < l) {
    ///         if (s[p] == 3) { center = True; ++p; }
    ///         i = p;
    ///         last = i + TText::scroll(s.substr(i), size.x, False).bytes;
    ///         do {
    ///             j = p;
    ///             while (p<l && s[p]==' ') p++;
    ///             while (p<l && s[p]!=' ' && s[p]!='\n') p += TText::next(…).bytes;
    ///         } while (p<l && p<last && s[p]!='\n');
    ///         if (p > last) { p = (j > i) ? j : last; }
    ///         width = strwidth(s[i..p]);
    ///         draw_col = center ? (size.x - width) / 2 : 0;
    ///         b.moveStr(draw_col, s[i..p], color, width);
    ///         while (p<l && s[p]==' ') p++;
    ///         if (p<l && s[p]=='\n') { center = False; p++; }
    ///     }
    ///     writeLine(0, y++, size.x, 1, b);
    /// }
    /// ```
    ///
    /// ## Byte-offset discipline (D13)
    ///
    /// The word-scan advances through the text as **byte offsets** (`p`, `i`,
    /// `j`/`word_start`, `last`). ASCII guards (`' '`, `'\n'`, `\x03`) are
    /// safe as single-byte comparisons. Non-ASCII word characters advance by
    /// `text::next(&self.text[p..]).0` (grapheme cluster byte length); this
    /// naturally aligns `p` to grapheme boundaries, so `&self.text[i..p]` is
    /// always a valid UTF-8 slice.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::StaticText);
        let size_x = self.state.size.x;
        let size_y = self.state.size.y;

        // Fill the entire view with the StaticText color first (faithful: C++
        // calls moveChar for every row, even after text is exhausted).
        ctx.fill(self.state.get_extent(), ' ', color);

        let s = self.text.as_bytes(); // byte slice for ASCII comparisons
        let l = s.len();
        let mut p: usize = 0; // current byte position in text
        // `center` persists across wrapped continuations; only `\n` resets it.
        let mut center = false;

        for y in 0..size_y {
            if p < l {
                // Check for ETX (0x03) center-toggle prefix.
                if s[p] == 3 {
                    center = true;
                    p += 1;
                }

                let i = p; // start of this line's text slice

                // `last` = byte offset (absolute) of the last byte that fits in
                // `size_x` display columns. `text::scroll` returns relative bytes.
                let last = i + text::scroll(&self.text[i..], size_x, false).0;

                // Word-pack loop (do-while: body runs at least once).
                //
                // `word_start` is the C++ `j`: the byte offset at the start of
                // the last word's leading spaces, set at the top of every body
                // iteration. After the loop, if `p > last`, we back off to
                // `word_start` (or hard-break at `last` if the first word
                // overflowed). Declared uninitialized: the `loop` body
                // unconditionally assigns `word_start = p` before any read, so
                // definite-assignment is satisfied without a dummy initializer.
                let mut word_start;
                loop {
                    word_start = p; // j = p (C++)
                    // Skip leading spaces.
                    while p < l && s[p] == b' ' {
                        p += 1;
                    }
                    // Skip a word (non-space, non-newline), advancing by grapheme.
                    // The `map_or(1, …)` fallback is unreachable under the `p < l`
                    // guard (a non-empty slice always yields a grapheme); 1 is a
                    // safe defensive default.
                    while p < l && s[p] != b' ' && s[p] != b'\n' {
                        p += text::next(&self.text[p..]).map_or(1, |(len, _)| len);
                    }
                    // Continue packing words while still within `last` and not at
                    // a hard newline.
                    if !(p < l && p < last && s[p] != b'\n') {
                        break;
                    }
                }

                // Back-off: if we overshot `last`, retreat to the last word
                // boundary (`word_start`), or hard-break at `last` if the very
                // first word overflowed.
                if p > last {
                    p = if word_start > i { word_start } else { last };
                }

                // Compute display width of the slice [i, p) and draw column.
                let slice = &self.text[i..p];
                let width = text::width(slice);
                let draw_col = if center {
                    (size_x - width as i32) / 2
                } else {
                    0
                };

                // Draw the slice. `put_str` clips to the view bounds; the slice
                // is exactly `width` columns wide (never wider than `size_x`
                // since `last` was computed from `scroll(size_x, false)`).
                ctx.put_str(draw_col, y, slice, color);

                // Consume trailing spaces.
                while p < l && s[p] == b' ' {
                    p += 1;
                }
                // On newline: reset centering and advance past it.
                if p < l && s[p] == b'\n' {
                    center = false;
                    p += 1;
                }
            }
            // Rows beyond text are already filled with spaces (done above).
        }
    }
}

// ---------------------------------------------------------------------------
// ParamText
// ---------------------------------------------------------------------------

/// `TParamText` — a dynamic-text variant of [`StaticText`] whose content is
/// replaced at run time (D2 embed-and-delegate).
///
/// ## C++ lineage
///
/// `TParamText` subclasses `TStaticText`, overriding only `getText` /
/// `getTextLen` / `setText` to supply a 256-byte mutable `char*` buffer filled
/// by `vsnprintf`. Everything else — `draw`, event routing, grow mode — is
/// unchanged from `TStaticText`.
///
/// ## D-rules applied
///
/// - **D2**: embed `StaticText` as `inner`; delegate ALL `View` trait methods.
///   A single `ViewState` lives inside `inner`; no second `ViewState` on
///   `ParamText` (that would silently break id-stamping at `Group::insert` and
///   the focus-broadcast `source` lookup).
/// - **D12**: no `TStreamable` (`read`/`write`/`build` dropped).
///
/// ## `printf` → `format!` deviation
///
/// C++ `setText(const char* fmt, ...)` formats via `vsnprintf` into a 256-byte
/// stack buffer. Rust has no C varargs. The idiomatic translation moves
/// formatting to the call site (`format!("{}", x)` / `format!("...")`) and lets
/// `set_text` accept the already-formatted `String`. The 256-byte cap does
/// **not** carry over — `ParamText` holds a `String` with no length limit.
/// Document call sites with `format!` where the C++ would have used `setText`
/// with format arguments.
///
/// ## `text_len` byte semantics
///
/// `getTextLen` returns `strlen(str)`, which is a **byte count**. `text_len`
/// mirrors this with `.len()` on the inner `String`. For all-ASCII content (the
/// common case in dialog labels) this is identical; for multi-byte UTF-8
/// graphemes the byte count diverges from the display-column count — but that
/// matches the C++ `strlen` behaviour.
pub struct ParamText {
    /// The delegated `StaticText` — its `state: ViewState` is the one true home
    /// for all view metadata. All `View` methods forward here.
    inner: StaticText,
}

impl ParamText {
    /// `TParamText::TParamText(bounds)` — construct with empty text.
    ///
    /// Faithful to the C++ ctor: `TStaticText(bounds, 0)` with `str[0] = EOS`.
    /// The fixed grow mode and non-selectable options come from `StaticText::new`.
    pub fn new(bounds: Rect) -> Self {
        ParamText {
            inner: StaticText::new(bounds, ""),
        }
    }

    /// Set (or replace) the displayed text — the Rust equivalent of C++
    /// `setText(const char* fmt, ...)`.
    ///
    /// The C++ uses `vsnprintf` into a 256-byte buffer; here, formatting is the
    /// caller's responsibility via `format!(…)`. The view will pick up the new
    /// text on the next render pass (D8 whole-tree repaint).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.inner.set_text(text);
    }

    /// `TParamText::getTextLen` — byte length of the current text (`strlen`
    /// equivalent). See the struct-level note on byte vs. display-column
    /// semantics.
    pub fn text_len(&self) -> usize {
        self.inner.text().len()
    }
}

// P  = {state, state_mut, draw, handle_event, set_state, valid, awaken,
//       size_limits, calc_bounds, change_bounds, cursor_request, find_mut,
//       remove_descendant, number, select_window_num}   — all verbatim forwards.
// DELETE = P (macro regenerates them all identically).
// SKIP   = 21 − P = {apply_list_scroll, as_any_mut, focus_descendant,
//          grabs_focus_on_click, set_value, value}.
#[crate::delegate(to = inner, skip(
    apply_list_scroll,
    as_any_mut,
    focus_descendant,
    grabs_focus_on_click,
    set_value,
    value
))]
impl View for ParamText {}

// ---------------------------------------------------------------------------
// Label
// ---------------------------------------------------------------------------

/// `TLabel` — a single-line caption that **links** to a control: clicking it (or
/// pressing its `~`-marked hotkey) focuses the linked control, and the label
/// **highlights** while that control is focused.
///
/// # Model (D2 / D3)
///
/// Embeds [`StaticText`] as `inner` (D2 embed-and-delegate, like [`ParamText`]):
/// a single [`ViewState`] lives in `inner`; all geometry / id / flags methods
/// forward there. The **link is an [`Option<ViewId>`]** (D3) — a label holds a
/// resolvable handle, never a `TView*`. The C++ `link->focus()` becomes the
/// deferred [`Context::request_focus`] tree-op (the loop walks to the link's
/// owning group and selects it); the C++ `link->state & sfFocused` re-read
/// becomes a **broadcast subscription** on the link's focus transitions.
///
/// # `light` from `Broadcast{source}` (D4 — first consumer)
///
/// `TLabel` is the first consumer of the Phase-A `Broadcast { source }` payload.
/// On `cmReceivedFocus`/`cmReleasedFocus` whose `source == link`, it sets
/// `light = (command == cmReceivedFocus)`. For focus *changes* this matches the
/// C++ `light = (link->state & sfFocused)` re-read: each focus transition of the
/// link emits a `source == link` focus broadcast (`View::set_state`), so the
/// broadcast carries the same bit the C++ re-read would. A broadcast about any
/// *other* view (or with no link set) leaves `light` unchanged.
///
/// **Known limitation (row-26 substrate).** Equivalence holds for focus *changes*,
/// not for link *removal*: C++ `TGroup::remove` calls `p->hide()` before
/// `removeView(p)`, so removing a current selectable link emits
/// `cmReleasedFocus(infoPtr == p)` and a lit label clears. Our `Group::remove`
/// removes the child and nulls `current` *without* first emitting
/// `set_state(Focused, false)` on the departing child, so no
/// `RELEASED_FOCUS { source == link }` fires — a label whose **selectable link is
/// removed at runtime** can keep a stale `light` highlight. This is a pre-existing
/// release-after-remove ordering gap in the row-26 substrate (faithful C++ is
/// `tgroup.cpp` `hide()`-before-`removeView`), to revisit there; no consumer
/// removes a bare link today.
///
/// # D-rules applied
///
/// * **D1** the `T` prefix is dropped; `snake_case`.
/// * **D2** embed `StaticText`; delegate all `View` methods except `draw` /
///   `handle_event` (a label has its own single-row draw and event logic).
/// * **D3** no owner / link pointer — `link: Option<ViewId>`; focusing is the
///   deferred [`Context::request_focus`].
/// * **D4** `enum Event` match; `light` driven by `Broadcast { source }`.
/// * **D7** colors via `ctx.style(Role::Label*)`; the C++ `getColor(0x0301)` /
///   `getColor(0x0402)` AttrPairs become the explicit (lo, hi) role pairs
///   `(LabelNormal, LabelNormalShortcut)` / `(LabelLight, LabelLightShortcut)`.
/// * **D8** draw into the back buffer through `DrawCtx`; `writeLine`/`TDrawBuffer`
///   dropped.
/// * **D12** `TStreamable` (`read`/`write`/`build`/`shutDown`) dropped.
///
/// # `eventMask |= evBroadcast` is a no-op here
///
/// As with [`Button`](crate::widgets::Button), the broadcast phase under our
/// [`Group`](crate::view::Group) delivers to **every** child regardless of
/// `event_mask`, so the C++ ctor's opt-in is automatic.
///
/// # Deferrals (documented, not built)
///
/// 1. **`showMarkers` / `specialChars` markers** dropped (always the no-markers
///    branch), as in `TStaticText`/`TButton`/cluster.
///
/// (The plain-letter postProcess accelerator — formerly deferred here — landed
/// with the A5 phase signal: see `handle_event`.)
pub struct Label {
    /// The delegated [`StaticText`] — its `state: ViewState` is the one true home
    /// for all view metadata.
    inner: StaticText,
    /// `link` — the control this label focuses on click/hotkey and tracks for
    /// highlighting. `None` if the label links nothing (a bare caption).
    link: Option<ViewId>,
    /// `light` — whether the linked control currently holds focus (drives the
    /// lit/normal color pair). Set from the link's focus broadcasts (D4).
    light: bool,
}

impl Label {
    /// `TLabel::TLabel(bounds, text, link)` — build a label over `bounds` with
    /// `text` (a `~`-marked hotkey title) optionally linking `link`.
    ///
    /// Faithful to the C++ ctor: `light = False`, `options |= ofPreProcess |
    /// ofPostProcess` (both load-bearing — a non-selectable label only ever sees
    /// its hotkey via the pre/post sweeps), `eventMask |= evBroadcast` (a no-op
    /// under our group, see the type docs). The inherited `gfFixed` + non-selectable
    /// come from [`StaticText::new`].
    pub fn new(bounds: Rect, text: impl Into<String>, link: Option<ViewId>) -> Self {
        let mut inner = StaticText::new(bounds, text);
        // ofPreProcess | ofPostProcess — keep StaticText's gfFixed (in grow_mode,
        // untouched) and its non-selectable default; only add the two phase opt-ins.
        inner.state.options = Options {
            pre_process: true,
            post_process: true,
            ..inner.state.options
        };
        Label {
            inner,
            link,
            light: false,
        }
    }

    /// The current link, if any.
    pub fn link(&self) -> Option<ViewId> {
        self.link
    }

    /// Whether the label is currently highlighted (its link holds focus).
    pub fn is_light(&self) -> bool {
        self.light
    }

    /// The (lo, hi) [`Role`] pair the current `light` state selects — the D7 form
    /// of the C++ `getColor` AttrPairs. `lo` is the caption color, `hi` the
    /// hotkey-shortcut color (the `~`-toggled half).
    ///
    /// * lit (`light`) → `(LabelLight, LabelLightShortcut)` (`getColor(0x0402)`)
    /// * normal → `(LabelNormal, LabelNormalShortcut)` (`getColor(0x0301)`)
    fn state_roles(&self) -> (Role, Role) {
        if self.light {
            (Role::LabelLight, Role::LabelLightShortcut)
        } else {
            (Role::LabelNormal, Role::LabelNormalShortcut)
        }
    }

    /// `TLabel::focusLink` — focus the linked control (if any) and consume the
    /// event. The C++ `if (link && (link->options & ofSelectable)) link->focus()`
    /// → a deferred [`Context::request_focus`]; the `ofSelectable` gate is applied
    /// by the owning group during the tree-walk (a label holds only the id, D3).
    /// `clearEvent` is **unconditional** — the event is consumed whether or not a
    /// link is present (faithful to C++ `focusLink`).
    fn focus_link(&mut self, ev: &mut Event, ctx: &mut Context) {
        if let Some(id) = self.link {
            ctx.request_focus(id);
        }
        ev.clear();
    }
}

// P      = {state, state_mut, draw, handle_event, set_state, valid, awaken,
//            size_limits, calc_bounds, change_bounds, cursor_request, find_mut,
//            remove_descendant, focus_descendant, number, select_window_num}
// KEEP   = {draw, handle_event}  — custom bodies.
// DELETE = P \ KEEP = {state, state_mut, set_state, valid, awaken, size_limits,
//            calc_bounds, change_bounds, cursor_request, find_mut,
//            remove_descendant, focus_descendant, number, select_window_num}
// SKIP   = 21 − P = {apply_list_scroll, as_any_mut, grabs_focus_on_click,
//            set_value, value}.
#[crate::delegate(to = inner, skip(
    apply_list_scroll,
    as_any_mut,
    grabs_focus_on_click,
    set_value,
    value
))]
impl View for Label {
    /// `TLabel::draw` — a single row: fill with the caption color, then draw the
    /// `~`-marked text at column 1 through `put_cstr`'s lo/hi toggle.
    ///
    /// Faithful port of `tlabel.cpp` (markers branch dropped):
    ///
    /// ```text
    /// color = light ? getColor(0x0402) : getColor(0x0301);
    /// b.moveChar(0, ' ', color, size.x);
    /// if (text != 0) b.moveCStr(1, text, color);
    /// writeLine(0, 0, size.x, 1, b);
    /// ```
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let (lo_role, hi_role) = self.state_roles();
        let lo = ctx.style(lo_role);
        let hi = ctx.style(hi_role);
        let size_x = self.state().size.x;
        // moveChar(0, ' ', color, size.x): fill row 0 in the caption color.
        ctx.fill(Rect::new(0, 0, size_x, 1), ' ', lo);
        // moveCStr(1, text, color): the ~-marked title at column 1, lo/hi toggle.
        let text = self.inner.text();
        if !text.is_empty() {
            ctx.put_cstr(1, 0, text, lo, hi);
        }
    }

    /// `TLabel::handleEvent` — see the per-branch mapping inline.
    ///
    /// The C++ leading `TStaticText::handleEvent(event)` is a no-op (static text
    /// has no `handleEvent`), so it is omitted. Branches:
    /// * **MouseDown** → `focusLink` (focus the link, consume).
    /// * **KeyDown** → if it is the Alt+hotkey accelerator → `focusLink`. (The C++
    ///   plain-letter branch fires only at `Phase::PostProcess`.)
    /// * **Broadcast** `cmReceivedFocus`/`cmReleasedFocus` whose `source` is our
    ///   link → update `light`; **not consumed** (other views may also react).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::MouseDown(_) => {
                self.focus_link(ev, ctx);
            }

            Event::KeyDown(ke) => {
                // Alt+hotkey accelerator (the C++ `getAltCode(c) == keyCode` arm),
                // OR — on the post-process walk only (`owner->phase ==
                // TGroup::phPostProcess` → `ctx.phase()`, the A5 phase signal) —
                // the plain hotkey letter (`tlabel.cpp:91-99`).
                if let Some(c) = hot_key(self.inner.text())
                    && (is_alt_hotkey(ke, c)
                        || (ctx.phase() == Phase::PostProcess && is_plain_hotkey(ke, c)))
                {
                    self.focus_link(ev, ctx);
                }
            }

            // Track the link's focus transitions to drive `light`. For focus
            // *changes* this matches the C++ `light = (link->state & sfFocused)`
            // re-read: the link emits a `source == link` focus broadcast on each
            // transition (D4). (Not for link *removal* — see the type-doc "Known
            // limitation": `Group::remove` does not emit RELEASED_FOCUS, so a label
            // whose link is removed at runtime can keep a stale highlight.) A
            // broadcast about any other view (or with no link) is ignored. The
            // `is_some()` guard rejects the `link == None && source == None`
            // coincidence. Not consumed — faithful to C++ (no clearEvent here).
            Event::Broadcast {
                command: Command::RECEIVED_FOCUS,
                source,
            } if self.link.is_some() && *source == self.link => {
                self.light = true;
            }
            Event::Broadcast {
                command: Command::RELEASED_FOCUS,
                source,
            } if self.link.is_some() && *source == self.link => {
                self.light = false;
            }

            _ => {}
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
    use crate::screen::Buffer;
    use crate::theme::Theme;

    // -- Snapshot helper (mirrors scrollbar tests) ---------------------------

    fn render_static_text(bounds: Rect, text: &str, buf_w: u16, buf_h: u16) -> String {
        let theme = Theme::classic_blue();
        let mut st = StaticText::new(bounds, text);
        let (backend, screen) = HeadlessBackend::new(buf_w, buf_h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = st.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            st.draw(&mut dc);
        });
        screen.snapshot()
    }

    // -- Snapshot section parsers -------------------------------------------

    /// Extract text rows from the `text:` section of a snapshot string.
    /// Returns the inner content of each `|...|` line, without the pipes.
    fn text_rows(snap: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut in_section = false;
        for line in snap.lines() {
            match line.trim() {
                "text:" => {
                    in_section = true;
                }
                "attr:" | "legend:" => {
                    in_section = false;
                }
                _ if in_section && line.starts_with('|') => {
                    // Strip leading and trailing '|'.
                    result.push(line[1..line.len() - 1].to_string());
                }
                _ => {}
            }
        }
        result
    }

    /// Extract attr rows from the `attr:` section of a snapshot string.
    fn attr_rows(snap: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut in_section = false;
        for line in snap.lines() {
            match line.trim() {
                "attr:" => {
                    in_section = true;
                }
                "legend:" => {
                    in_section = false;
                }
                _ if in_section && line.starts_with('|') => {
                    result.push(line[1..line.len() - 1].to_string());
                }
                _ => {}
            }
        }
        result
    }

    // -- Algorithm unit tests ------------------------------------------------

    /// Simple wrapping: "hello world" in a 6-wide, 2-high box wraps after
    /// "hello" (5 chars fit, space consumed, "world" on row 1).
    #[test]
    fn wraps_at_word_boundary() {
        let snap = render_static_text(Rect::new(0, 0, 6, 2), "hello world", 6, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 2, "should have 2 text rows");
        assert_eq!(rows[0], "hello ", "row 0: 'hello' + trailing space fill");
        assert_eq!(rows[1], "world ", "row 1: 'world' + trailing space fill");
    }

    /// A word longer than the view is hard-broken at `last` (no word boundary).
    #[test]
    fn hard_break_when_first_word_overflows() {
        // Width 4, text "abcdefgh" — each row fits 4 chars.
        let snap = render_static_text(Rect::new(0, 0, 4, 2), "abcdefgh", 4, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows[0], "abcd", "row 0: first 4 chars");
        assert_eq!(rows[1], "efgh", "row 1: next 4 chars");
    }

    /// `\x03` (ETX) at the start of a paragraph centers that line (and
    /// continuations) horizontally within the view.
    #[test]
    fn etx_centers_the_line() {
        // Width 10, "\x03hi" → "hi" (width 2) centered: (10-2)/2 = 4 spaces of indent.
        let snap = render_static_text(Rect::new(0, 0, 10, 1), "\x03hi", 10, 1);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 1);
        // "    hi    " — 4 leading spaces (centered), 4 trailing fill.
        assert_eq!(&rows[0][..4], "    ", "should have 4 leading spaces");
        assert_eq!(&rows[0][4..6], "hi", "text should be 'hi'");
    }

    /// `\n` forces a line break and resets the centering flag.
    #[test]
    fn newline_forces_break_and_resets_centering() {
        // "\x03A\nB" — row 0: "A" centered; row 1: "B" left-aligned.
        let snap = render_static_text(Rect::new(0, 0, 6, 2), "\x03A\nB", 6, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 2);
        // Row 0: "A" centered in 6 → (6-1)/2 = 2 spaces before "A".
        assert_eq!(
            &rows[0][..2],
            "  ",
            "row 0 should have 2 leading spaces (centered)"
        );
        assert_eq!(&rows[0][2..3], "A", "row 0 text should be 'A'");
        // Row 1: "B" left-aligned (centering was reset by \n).
        assert_eq!(&rows[1][..1], "B", "row 1 text should start with 'B'");
        assert_ne!(&rows[1][..2], "  ", "row 1 should not be centered");
    }

    /// Centering persists across wrapped rows until a `\n`.
    ///
    /// Discriminating width: the CONTINUATION row's centered indent must be
    /// non-zero, so the test bites if carry-over is removed. Width 4, text
    /// "\x03aa bb": ETX sets center; "aa" (width 2) fits row 0, "bb" (width 2)
    /// wraps to row 1. Centered in 4: (4-2)/2 = 1 leading space on BOTH rows.
    #[test]
    fn centering_persists_across_wrapped_rows() {
        let snap = render_static_text(Rect::new(0, 0, 4, 2), "\x03aa bb", 4, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 2);
        // Row 0: " aa " (1 leading space, centered).
        assert_eq!(&rows[0][..1], " ", "row 0 centered: 1 leading space");
        assert_eq!(&rows[0][1..3], "aa", "row 0 text 'aa'");
        // Row 1: the continuation must STILL be centered (carry-over). 1 leading
        // space, "bb" at col 1 — would be col 0 ("bb..") if centering were lost.
        assert_eq!(
            &rows[1][..1],
            " ",
            "row 1 (continuation) stays centered: 1 leading space"
        );
        assert_eq!(&rows[1][1..3], "bb", "row 1 text 'bb' at the centered col");
        assert_ne!(
            &rows[1][..2],
            "bb",
            "row 1 must NOT be left-aligned (bites if carry-over breaks)"
        );
    }

    /// Rows beyond the text are filled with StaticText-colored spaces (not
    /// default-style blanks).
    #[test]
    fn trailing_rows_use_static_text_color() {
        // 1 row of text in a 2-row view: row 1 should appear in the attr map.
        let snap = render_static_text(Rect::new(0, 0, 3, 2), "hi", 3, 2);
        assert!(snap.contains("attr:"), "snapshot should have attr section");
        let a_rows = attr_rows(&snap);
        assert_eq!(a_rows.len(), 2, "should have 2 attr rows");
        // Row 1 should not be all '.' (default style).
        assert!(
            !a_rows[1].chars().all(|c| c == '.'),
            "trailing row should carry StaticText color, not default style"
        );
    }

    // -- Snapshot test -------------------------------------------------------

    #[test]
    fn snapshot_static_text_word_wrap() {
        let theme = Theme::classic_blue();
        let mut st = StaticText::new(Rect::new(0, 0, 12, 3), "hello world foo");
        let (backend, screen) = HeadlessBackend::new(12, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = st.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            st.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn snapshot_static_text_centered() {
        let theme = Theme::classic_blue();
        let mut st = StaticText::new(Rect::new(0, 0, 12, 2), "\x03centered\nplain");
        let (backend, screen) = HeadlessBackend::new(12, 2);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = st.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            st.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- ParamText unit tests -------------------------------------------------

    /// Helper: render a `ParamText` into a headless backend and return the
    /// snapshot string. Mirrors `render_static_text` for the inherited draw.
    fn render_param_text(bounds: Rect, text: &str, buf_w: u16, buf_h: u16) -> String {
        let theme = Theme::classic_blue();
        let mut pt = ParamText::new(bounds);
        pt.set_text(text);
        let (backend, screen) = HeadlessBackend::new(buf_w, buf_h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = pt.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            pt.draw(&mut dc);
        });
        screen.snapshot()
    }

    /// `new` starts with empty text — `text_len` is 0 and the rendered view
    /// shows only fill spaces.
    #[test]
    fn param_text_new_starts_empty() {
        let pt = ParamText::new(Rect::new(0, 0, 6, 1));
        assert_eq!(pt.text_len(), 0, "new ParamText must be empty");
    }

    /// `set_text("Hello")` replaces the content; the rendered output shows it.
    #[test]
    fn param_text_set_text_shows_in_render() {
        let snap = render_param_text(Rect::new(0, 0, 10, 1), "Hello", 10, 1);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].starts_with("Hello"),
            "rendered text must start with 'Hello', got: {:?}",
            rows[0]
        );
    }

    /// `set_text` called twice: the second call replaces the first.
    #[test]
    fn param_text_set_text_replaces_previous() {
        let mut pt = ParamText::new(Rect::new(0, 0, 10, 1));
        pt.set_text("First");
        assert_eq!(pt.text_len(), 5);
        pt.set_text("Second");
        // "Second" has 6 bytes; "First" must be gone.
        assert_eq!(pt.text_len(), 6, "second set_text must replace first");
        assert_eq!(pt.inner.text(), "Second");
    }

    /// `text_len` reflects byte length of the current text (faithful to C++
    /// `strlen`). ASCII strings: byte count == char count.
    #[test]
    fn param_text_text_len_reflects_current_text() {
        let mut pt = ParamText::new(Rect::new(0, 0, 20, 1));
        assert_eq!(pt.text_len(), 0, "empty after new");
        pt.set_text("Hello");
        assert_eq!(pt.text_len(), 5, "5 bytes for 'Hello'");
        pt.set_text("");
        assert_eq!(pt.text_len(), 0, "0 bytes after clearing");
        // Verify format!-at-call-site pattern (the printf→format! deviation).
        let n = 42;
        pt.set_text(format!("Item {n}"));
        assert_eq!(pt.text_len(), 7, "7 bytes for 'Item 42'");
    }

    /// Snapshot: inherited word-wrap on a set string. Demonstrates that
    /// `ParamText` reuses `StaticText::draw` (the text wraps exactly as if it
    /// were constructed with `StaticText::new`).
    #[test]
    fn snapshot_param_text_word_wrap() {
        let theme = Theme::classic_blue();
        let mut pt = ParamText::new(Rect::new(0, 0, 12, 3));
        pt.set_text("hello world foo");
        let (backend, screen) = HeadlessBackend::new(12, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = pt.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            pt.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- Label ---------------------------------------------------------------

    use crate::event::{Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent};
    use crate::timer::TimerQueue;
    use crate::view::{Deferred, Point, ViewId};

    /// Render a `Label` to a snapshot string.
    fn render_label(label: &mut Label) -> String {
        let theme = Theme::classic_blue();
        let size = label.state().size;
        let (backend, screen) = HeadlessBackend::new(size.x as u16, size.y as u16);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = label.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            label.draw(&mut dc);
        });
        screen.snapshot()
    }

    /// Run a closure with a fresh `Context` over loop-owned locals, returning the
    /// drained out-events, the deferred queue, and the closure's value.
    fn with_label_ctx<R>(
        timers: &mut TimerQueue,
        f: impl FnOnce(&mut Context) -> R,
    ) -> (Vec<Event>, Vec<Deferred>, R) {
        let mut out: std::collections::VecDeque<Event> = std::collections::VecDeque::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, timers, 0, &mut deferred);
            f(&mut ctx)
        };
        (out.into_iter().collect(), deferred, r)
    }

    fn label_mouse_down(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn alt_key(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Char(c),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        ))
    }

    fn focus_broadcast(received: bool, source: Option<ViewId>) -> Event {
        Event::Broadcast {
            command: if received {
                Command::RECEIVED_FOCUS
            } else {
                Command::RELEASED_FOCUS
            },
            source,
        }
    }

    // -- 1. draw -------------------------------------------------------------

    /// The caption is drawn at column 1 (column 0 is the fill / marker slot in
    /// C++), `~`-markers stripped.
    #[test]
    fn label_draw_text_at_column_one() {
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        let rows = text_rows(&render_label(&mut lbl));
        assert_eq!(rows.len(), 1);
        // Column 0 is a fill space; the caption "Name" (tildes stripped) at col 1.
        assert_eq!(&rows[0][..1], " ", "column 0 is the fill space");
        assert_eq!(
            &rows[0][1..5],
            "Name",
            "caption drawn at column 1, ~ stripped"
        );
    }

    /// Empty text: nothing past the fill (no panic, all spaces).
    #[test]
    fn label_draw_empty_text_is_all_fill() {
        let mut lbl = Label::new(Rect::new(0, 0, 6, 1), "", None);
        let rows = text_rows(&render_label(&mut lbl));
        assert_eq!(rows[0], "      ", "empty label is all fill spaces");
    }

    /// Lit vs normal must differ in the rendered colors: lit uses LabelLight
    /// (white-on-gray, BIOS 15), normal uses LabelNormal (black-on-gray, BIOS 0).
    /// The attr *pattern* is identical (same cells, same role layout); the
    /// difference is in the legend's fg color. The bite compares the FULL snapshot
    /// (which carries the legend), and asserts the caption fg actually changes.
    #[test]
    fn label_draw_lit_attr_differs_from_normal() {
        let mut normal = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        let normal_snap = render_label(&mut normal);

        let mut lit = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        lit.light = true;
        let lit_snap = render_label(&mut lit);

        assert_ne!(
            normal_snap, lit_snap,
            "the rendered snapshot must differ between normal and lit"
        );
        // Concrete bite on the caption fg: normal is black (BIOS 0), lit is white
        // (BIOS 15) — both over the gray-7 background.
        assert!(
            normal_snap.contains("fg=BIOS(0) bg=BIOS(7)"),
            "normal caption is black-on-gray"
        );
        assert!(
            lit_snap.contains("fg=BIOS(15) bg=BIOS(7)"),
            "lit caption is white-on-gray"
        );
    }

    /// `state_roles` returns the lit pair iff `light`.
    #[test]
    fn label_state_roles_track_light() {
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "X", None);
        assert_eq!(
            lbl.state_roles(),
            (Role::LabelNormal, Role::LabelNormalShortcut)
        );
        lbl.light = true;
        assert_eq!(
            lbl.state_roles(),
            (Role::LabelLight, Role::LabelLightShortcut)
        );
    }

    #[test]
    fn snapshot_label_normal() {
        let theme = Theme::classic_blue();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        let (backend, screen) = HeadlessBackend::new(12, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = lbl.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            lbl.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn snapshot_label_lit() {
        let theme = Theme::classic_blue();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        lbl.light = true;
        let (backend, screen) = HeadlessBackend::new(12, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = lbl.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            lbl.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- 2. focus_descendant tree-op -----------------------------------------

    /// A test leaf with a configurable `selectable` flag + a focus-record probe so
    /// we can observe whether `focus_child` selected it.
    struct Probe {
        state: ViewState,
    }
    impl Probe {
        fn new(selectable: bool) -> Self {
            let mut state = ViewState::new(Rect::new(0, 0, 4, 1));
            state.options = Options {
                selectable,
                ..Default::default()
            };
            Probe { state }
        }
    }
    impl View for Probe {
        fn state(&self) -> &ViewState {
            &self.state
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.state
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
    }

    /// `Group::focus_descendant` focuses a selectable direct child (sets it
    /// `current`) and returns true.
    #[test]
    fn focus_descendant_focuses_selectable_child() {
        use crate::view::Group;
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let mut timers = TimerQueue::new();
        let id = group.insert(Box::new(Probe::new(true)));
        let (_out, _def, found) =
            with_label_ctx(&mut timers, |ctx| group.focus_descendant(id, ctx));
        assert!(found, "selectable child is found");
        assert_eq!(
            group.current(),
            Some(id),
            "selectable child becomes current"
        );
    }

    /// A non-selectable child is *found* (stops the walk) but **not focused**.
    #[test]
    fn focus_descendant_finds_but_skips_non_selectable() {
        use crate::view::Group;
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let mut timers = TimerQueue::new();
        // Insert a selectable child first so `current` has a non-None baseline to
        // prove the non-selectable target does NOT steal it.
        let sel_id = group.insert(Box::new(Probe::new(true)));
        let target_id = group.insert(Box::new(Probe::new(false)));
        // Focus the selectable one to set current.
        with_label_ctx(&mut timers, |ctx| group.focus_child(sel_id, ctx));
        assert_eq!(group.current(), Some(sel_id));
        // focus_descendant on the non-selectable: found, but current unchanged.
        let (_o, _d, found) =
            with_label_ctx(&mut timers, |ctx| group.focus_descendant(target_id, ctx));
        assert!(
            found,
            "non-selectable child is still FOUND (stops the walk)"
        );
        assert_eq!(
            group.current(),
            Some(sel_id),
            "non-selectable target must NOT be focused"
        );
    }

    /// An unknown id misses (returns false, current unchanged).
    #[test]
    fn focus_descendant_misses_unknown_id() {
        use crate::view::Group;
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let mut timers = TimerQueue::new();
        group.insert(Box::new(Probe::new(true)));
        let stranger = ViewId::next();
        let (_o, _d, found) =
            with_label_ctx(&mut timers, |ctx| group.focus_descendant(stranger, ctx));
        assert!(!found, "an unknown id is not found");
    }

    /// Recurses through an embedder: a child group's grandchild is found+focused.
    #[test]
    fn focus_descendant_recurses_through_child_group() {
        use crate::view::Group;
        let mut root = Group::new(Rect::new(0, 0, 30, 20));
        let mut child = Group::new(Rect::new(0, 0, 20, 10));
        let mut timers = TimerQueue::new();
        // Insert a selectable grandchild into the child group.
        let gid = child.insert(Box::new(Probe::new(true)));
        root.insert(Box::new(child));
        // focus_descendant from the root must recurse into the child group.
        let (_o, _d, found) = with_label_ctx(&mut timers, |ctx| root.focus_descendant(gid, ctx));
        assert!(found, "grandchild is found via recursion");
    }

    // -- 3. handle_event: focusLink ------------------------------------------

    /// MouseDown → request_focus(link) deferred + event cleared.
    #[test]
    fn label_mouse_down_requests_focus_and_clears() {
        let link = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        let mut ev = label_mouse_down(3, 0);
        let (out, deferred, ()) = with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down on a label is consumed");
        assert!(out.is_empty(), "no out-events");
        assert_eq!(deferred.len(), 1, "one deferred focus request");
        assert!(matches!(deferred[0], Deferred::FocusById(id) if id == link));
    }

    /// MouseDown with NO link: still cleared, but no focus request (focusLink
    /// clears unconditionally, requests only when a link is present).
    #[test]
    fn label_mouse_down_no_link_clears_without_request() {
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        let mut timers = TimerQueue::new();
        let mut ev = label_mouse_down(3, 0);
        let (_out, deferred, ()) =
            with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "consumed even without a link");
        assert!(deferred.is_empty(), "no focus request when link is None");
    }

    /// Alt+hotkey → request_focus + clear.
    #[test]
    fn label_alt_hotkey_requests_focus_and_clears() {
        let link = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        // Hotkey is 'N' (first char after ~).
        let mut ev = alt_key('n');
        let (_out, deferred, ()) =
            with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "the Alt+hotkey is consumed");
        assert_eq!(deferred.len(), 1);
        assert!(matches!(deferred[0], Deferred::FocusById(id) if id == link));
    }

    /// A non-matching Alt key passes through (not consumed, no request).
    #[test]
    fn label_non_matching_alt_key_passes_through() {
        let link = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        let mut ev = alt_key('z'); // not the 'N' hotkey
        let (_out, deferred, ()) =
            with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "a non-matching key is left live");
        assert!(deferred.is_empty(), "no focus request");
    }

    /// A plain (no-Alt) hotkey letter IS honored on the post-process walk —
    /// the `owner->phase == phPostProcess` plain-letter arm (`tlabel.cpp:94`),
    /// via the A5 phase signal on Context.
    #[test]
    fn label_plain_hotkey_focuses_link_at_post_process() {
        let link = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        let mut ev = Event::KeyDown(KeyEvent::new(Key::Char('n'), KeyModifiers::default()));
        let (_out, deferred, ()) = with_label_ctx(&mut timers, |ctx| {
            ctx.set_phase(Phase::PostProcess);
            lbl.handle_event(&mut ev, ctx)
        });
        assert!(ev.is_nothing(), "the postProcess plain letter is consumed");
        assert_eq!(deferred.len(), 1);
        assert!(matches!(deferred[0], Deferred::FocusById(id) if id == link));
    }

    /// The same plain letter at the default (Focused) phase is NOT honored —
    /// the plain-letter arm is gated on phPostProcess (`tlabel.cpp:94`).
    #[test]
    fn label_plain_hotkey_ignored_outside_post_process() {
        let link = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        let mut ev = Event::KeyDown(KeyEvent::new(Key::Char('n'), KeyModifiers::default()));
        let (_out, deferred, ()) =
            with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(
            !ev.is_nothing(),
            "a plain letter outside phPostProcess is left live"
        );
        assert!(
            deferred.is_empty(),
            "no focus request outside phPostProcess"
        );
    }

    // -- 4. highlight (Broadcast{source}) ------------------------------------

    /// A focus broadcast whose source IS the link toggles `light`; the event is
    /// not consumed.
    #[test]
    fn label_light_tracks_link_focus_broadcast() {
        let link = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        assert!(!lbl.is_light(), "starts not lit");

        // RECEIVED_FOCUS from the link → lit.
        let mut ev = focus_broadcast(true, Some(link));
        with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(lbl.is_light(), "lit after the link receives focus");
        assert!(!ev.is_nothing(), "broadcast is NOT consumed");

        // RELEASED_FOCUS from the link → unlit.
        let mut ev = focus_broadcast(false, Some(link));
        with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(!lbl.is_light(), "unlit after the link releases focus");
    }

    /// A focus broadcast about ANOTHER view (source != link) must NOT change
    /// `light` — the discriminating bite for the `source == link` guard.
    #[test]
    fn label_light_ignores_other_source() {
        let link = ViewId::next();
        let other = ViewId::next();
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", Some(link));
        let mut timers = TimerQueue::new();
        let mut ev = focus_broadcast(true, Some(other));
        with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(
            !lbl.is_light(),
            "a broadcast about another view must not light the label"
        );
        // A None source likewise must not light it.
        let mut ev = focus_broadcast(true, None);
        with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(!lbl.is_light(), "a source-less broadcast must not light it");
    }

    /// A label with NO link ignores all focus broadcasts (even a None-source one).
    #[test]
    fn label_no_link_ignores_focus_broadcasts() {
        let mut lbl = Label::new(Rect::new(0, 0, 12, 1), "~N~ame", None);
        let mut timers = TimerQueue::new();
        let mut ev = focus_broadcast(true, None);
        with_label_ctx(&mut timers, |ctx| lbl.handle_event(&mut ev, ctx));
        assert!(!lbl.is_light(), "a link-less label never lights");
    }

    // -- ctor invariants -----------------------------------------------------

    /// The ctor sets ofPreProcess + ofPostProcess (load-bearing for hotkey
    /// delivery to a non-selectable view) and keeps StaticText's gfFixed +
    /// non-selectable.
    #[test]
    fn label_ctor_sets_pre_post_process_keeps_fixed_unselectable() {
        let lbl = Label::new(Rect::new(0, 0, 12, 1), "X", None);
        let opts = lbl.state().options;
        assert!(opts.pre_process, "ofPreProcess set");
        assert!(opts.post_process, "ofPostProcess set");
        assert!(!opts.selectable, "a label is not selectable");
        assert!(
            lbl.state().grow_mode.fixed,
            "gfFixed inherited from StaticText"
        );
    }
}
