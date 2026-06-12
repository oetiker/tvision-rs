//! Theming: a [`Role`] → [`Style`] map plus a [`Glyphs`] holder.
//!
//! Every widget asks the [`Theme`] for colours by **semantic role** rather than
//! a raw colour — a frame draws with `ctx.theme.style(Role::FrameActive)`, a
//! button with `Role::ButtonNormal`, and so on — and reaches drawing glyphs
//! (frame corners, scrollbar arrows, check/radio marks, shadows) through
//! [`Glyphs`]. Swapping themes (or editing a single role) recolours the whole
//! UI at once.
//!
//! [`Role`] is a **first-party closed enum**: third parties do not add roles.
//! It covers the frames (active/passive/dragging), the
//! normal/focused/disabled/pressed control quartet, the list-state matrix, and
//! the per-widget families (buttons, labels, menus, the status line, …).
//!
//! # Turbo Vision heritage
//!
//! The original framework resolved colours by walking an owner chain of
//! length-prefixed palette strings and scattered drawing glyphs as literals
//! through the widget source. rstv collapses both into one [`Theme`] keyed by a
//! semantic [`Role`] (deviation D7); each original colour lookup maps to one named
//! role here.

use crate::color::{Color, Style};

/// A semantic colour role — the key a widget uses to ask the [`Theme`] for a
/// [`Style`].
///
/// This enum is **closed and first-party** (not app-extensible).
///
/// # Turbo Vision heritage
///
/// Each colour lookup in the original framework maps to one named role here
/// (deviation D7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// The desktop background fill.
    Background,
    /// An active (focused) window frame.
    FrameActive,
    /// A passive (unfocused) window frame.
    FramePassive,
    /// A frame being dragged/resized.
    FrameDragging,
    /// A frame icon (close/zoom/resize glyphs).
    FrameIcon,
    /// An active (focused) **gray-scheme** frame (dialogs and gray windows). The
    /// frame selects the `FrameGray*` family when its owner's
    /// [`WindowPalette`](crate::window::WindowPalette) is `Gray`.
    FrameGrayActive,
    /// A passive (unfocused) gray-scheme frame.
    FrameGrayPassive,
    /// A gray-scheme frame being dragged/resized.
    FrameGrayDragging,
    /// A gray-scheme frame icon (close/zoom/resize glyphs).
    FrameGrayIcon,
    /// An active (focused) **cyan-scheme** frame (cyan windows). The frame
    /// selects the `FrameCyan*` family when its owner's
    /// [`WindowPalette`](crate::window::WindowPalette) is `Cyan`.
    FrameCyanActive,
    /// A passive (unfocused) cyan-scheme frame.
    FrameCyanPassive,
    /// A cyan-scheme frame being dragged/resized.
    FrameCyanDragging,
    /// A cyan-scheme frame icon (close/zoom/resize glyphs).
    FrameCyanIcon,
    /// The history dropdown's `↓` arrow glyph.
    HistoryArrow,
    /// The history dropdown's icon side blocks `▐` `▌`.
    HistorySides,
    /// A normal item in the history dropdown list. One role serves the
    /// active/inactive normals, the selected item, and the divider (they all share
    /// a colour; surfaced through
    /// [`ListViewer::list_roles`](crate::widgets::ListViewer::list_roles)).
    HistoryViewerNormal,
    /// The focused (cursor) item in the history dropdown list.
    HistoryViewerFocused,
    /// A scroll-bar page (trough) area.
    ScrollBarPage,
    /// Scroll-bar control glyphs (arrows / thumb).
    ScrollBarControls,
    /// Generic enabled control text.
    Normal,
    /// A focused control.
    Focused,
    /// A disabled (greyed-out) control.
    Disabled,
    /// A pressed control (e.g. a button mid-click).
    Pressed,
    /// A normal item in an **active** (focused) list. Also the empty-list fill.
    ListNormalActive,
    /// A normal item in an **inactive** (unfocused) list.
    ListNormalInactive,
    /// The focused (cursor) item of an active list.
    ListFocused,
    /// A selected (marked) item.
    ListSelected,
    /// The inter-column divider `│`.
    ListDivider,
    /// Error feedback.
    Error,
    /// Warning feedback.
    Warning,
    /// Informational feedback.
    Info,
    /// Success feedback.
    Success,
    /// Static (label/caption) text.
    StaticText,
    /// A cluster item's normal (unselected) text (a check box / radio button
    /// group).
    ClusterNormal,
    /// A cluster item's selected text (cursor item, cluster focused) — idx 2.
    ClusterSelected,
    /// A cluster item's shortcut highlight in the normal state — idx 3.
    ClusterNormalShortcut,
    /// A cluster item's shortcut highlight in the selected state — idx 4.
    ClusterSelectedShortcut,
    /// A disabled cluster item's text — idx 5.
    ClusterDisabled,
    /// The editor's line/column indicator, normal (owner not dragging).
    IndicatorNormal,
    /// The editor's line/column indicator while its owner is dragging.
    IndicatorDragging,
    /// A button's normal (inactive) face text.
    ButtonNormal,
    /// A default button's face text.
    ButtonDefault,
    /// A selected (pressed) button's face text.
    ButtonSelected,
    /// A disabled button's face text.
    ButtonDisabled,
    /// A button's shortcut highlight in the normal state.
    ButtonNormalShortcut,
    /// A button's shortcut highlight in the default state.
    ButtonDefaultShortcut,
    /// A button's shortcut highlight in the selected state.
    ButtonSelectedShortcut,
    /// A button's drop-shadow cells.
    ButtonShadow,
    /// A label's caption text when **not** lit (its linked control is unfocused).
    LabelNormal,
    /// A label's caption text when **lit** (its linked control is focused).
    LabelLight,
    /// A label's shortcut highlight when **not** lit.
    LabelNormalShortcut,
    /// A label's shortcut highlight when **lit**. Shares the same colour as
    /// [`LabelNormalShortcut`](Role::LabelNormalShortcut) but is kept a distinct
    /// role so future theming can differ.
    LabelLightShortcut,
    /// An input line's field text. A single role serves both the focused and
    /// unfocused field.
    InputNormal,
    /// An input line's selection highlight.
    InputSelected,
    /// An input line's scroll arrows.
    InputArrow,
    /// A scroller's content fill, normal (the realistic case: a scroller inside a
    /// blue window).
    ScrollerNormal,
    /// An editor's selected-text fill.
    ScrollerSelected,
    /// A menu's normal item text. Also the menu-bar background fill.
    MenuNormal,
    /// A menu's normal item shortcut highlight.
    MenuNormalShortcut,
    /// A menu's selected (highlighted) item text.
    MenuSelected,
    /// A menu's selected item shortcut highlight.
    MenuSelectedShortcut,
    /// A menu's disabled (greyed) item text (no shortcut highlight when greyed).
    MenuDisabled,
    /// A menu's selected-but-disabled item text.
    MenuSelectedDisabled,
    /// The status line's normal item text (black on lightgray). Also the row
    /// background fill.
    StatusNormal,
    /// The status line's normal item shortcut highlight (red on lightgray).
    StatusShortcut,
    /// The status line's selected (hovered) item text (black on green).
    StatusSelect,
    /// The status line's selected item shortcut highlight (red on green).
    StatusShortcutSelect,
    /// The status line's disabled (greyed) item text (darkgray on lightgray).
    StatusDisabled,
    /// The status line's selected-but-disabled item text (darkgray on green).
    StatusSelDisabled,
    /// The file-dialog info pane text (path + size/date display), cyan on blue.
    InfoPane,

    // -- Outline viewer ------------------------------------------------------
    /// An outline viewer's normal item (the graph plus an expanded item's text).
    OutlineNormal,
    /// An outline viewer's focused item (the focused row when the viewer is
    /// focused).
    OutlineFocused,
    /// An outline viewer's selected item.
    OutlineSelected,
    /// An outline viewer's collapsed (not-expanded) item — the dimmer text shown
    /// for a collapsed node.
    OutlineNotExpanded,

    /// Window/menu drop shadows — dark gray on black, applied by the shadow pass
    /// ([`DrawCtx::cast_shadow`](crate::view::DrawCtx::cast_shadow)).
    Shadow,
}

/// Number of [`Role`] variants — the fixed length of [`Theme`]'s style array.
pub(crate) const ROLE_COUNT: usize = 75;

/// All role variants in index order (appended families grouped semantically) — used by the theme editor.
pub(crate) const ALL: [Role; ROLE_COUNT] = [
    Role::Background,
    Role::FrameActive,
    Role::FramePassive,
    Role::FrameDragging,
    Role::FrameIcon,
    Role::ScrollBarPage,
    Role::ScrollBarControls,
    Role::Normal,
    Role::Focused,
    Role::Disabled,
    Role::Pressed,
    Role::ListNormalActive,
    Role::ListNormalInactive,
    Role::ListFocused,
    Role::ListSelected,
    Role::ListDivider,
    Role::Error,
    Role::Warning,
    Role::Info,
    Role::Success,
    Role::StaticText,
    Role::ClusterNormal,
    Role::ClusterSelected,
    Role::ClusterNormalShortcut,
    Role::ClusterSelectedShortcut,
    Role::ClusterDisabled,
    Role::IndicatorNormal,
    Role::IndicatorDragging,
    Role::ButtonNormal,
    Role::ButtonDefault,
    Role::ButtonSelected,
    Role::ButtonDisabled,
    Role::ButtonNormalShortcut,
    Role::ButtonDefaultShortcut,
    Role::ButtonSelectedShortcut,
    Role::ButtonShadow,
    Role::LabelNormal,
    Role::LabelLight,
    Role::LabelNormalShortcut,
    Role::LabelLightShortcut,
    Role::InputNormal,
    Role::InputSelected,
    Role::InputArrow,
    Role::ScrollerNormal,
    Role::ScrollerSelected,
    Role::MenuNormal,
    Role::MenuNormalShortcut,
    Role::MenuSelected,
    Role::MenuSelectedShortcut,
    Role::MenuDisabled,
    Role::MenuSelectedDisabled,
    Role::StatusNormal,
    Role::StatusShortcut,
    Role::StatusSelect,
    Role::StatusShortcutSelect,
    Role::StatusDisabled,
    Role::StatusSelDisabled,
    Role::InfoPane,
    Role::OutlineNormal,
    Role::OutlineFocused,
    Role::OutlineSelected,
    Role::OutlineNotExpanded,
    Role::Shadow,
    Role::FrameGrayActive,
    Role::FrameGrayPassive,
    Role::FrameGrayDragging,
    Role::FrameGrayIcon,
    Role::FrameCyanActive,
    Role::FrameCyanPassive,
    Role::FrameCyanDragging,
    Role::FrameCyanIcon,
    Role::HistoryArrow,
    Role::HistorySides,
    Role::HistoryViewerNormal,
    Role::HistoryViewerFocused,
];

impl Role {
    /// Short display name for UI labels (e.g. `"FrameActive"`).
    /// Fits in 16 characters so the theme editor's list column stays readable.
    pub fn name(self) -> &'static str {
        match self {
            Role::Background => "Background",
            Role::FrameActive => "FrameActive",
            Role::FramePassive => "FramePassive",
            Role::FrameDragging => "FrameDragging",
            Role::FrameIcon => "FrameIcon",
            Role::FrameGrayActive => "FrameGrayActive",
            Role::FrameGrayPassive => "FrameGrayPassive",
            Role::FrameGrayDragging => "FrameGrayDrag",
            Role::FrameGrayIcon => "FrameGrayIcon",
            Role::FrameCyanActive => "FrameCyanActive",
            Role::FrameCyanPassive => "FrameCyanPassive",
            Role::FrameCyanDragging => "FrameCyanDrag",
            Role::FrameCyanIcon => "FrameCyanIcon",
            Role::HistoryArrow => "HistoryArrow",
            Role::HistorySides => "HistorySides",
            Role::HistoryViewerNormal => "HistViewerNormal",
            Role::HistoryViewerFocused => "HistViewerFocusd",
            Role::ScrollBarPage => "ScrollBarPage",
            Role::ScrollBarControls => "ScrollBarCtrl",
            Role::Normal => "Normal",
            Role::Focused => "Focused",
            Role::Disabled => "Disabled",
            Role::Pressed => "Pressed",
            Role::ListNormalActive => "ListNormalActive",
            Role::ListNormalInactive => "ListNormalInactv",
            Role::ListFocused => "ListFocused",
            Role::ListSelected => "ListSelected",
            Role::ListDivider => "ListDivider",
            Role::Error => "Error",
            Role::Warning => "Warning",
            Role::Info => "Info",
            Role::Success => "Success",
            Role::StaticText => "StaticText",
            Role::ClusterNormal => "ClusterNormal",
            Role::ClusterSelected => "ClusterSelected",
            Role::ClusterNormalShortcut => "ClusterNormSc",
            Role::ClusterSelectedShortcut => "ClusterSelSc",
            Role::ClusterDisabled => "ClusterDisabled",
            Role::IndicatorNormal => "IndicatorNormal",
            Role::IndicatorDragging => "IndicatorDragg",
            Role::ButtonNormal => "ButtonNormal",
            Role::ButtonDefault => "ButtonDefault",
            Role::ButtonSelected => "ButtonSelected",
            Role::ButtonDisabled => "ButtonDisabled",
            Role::ButtonNormalShortcut => "ButtonNormSc",
            Role::ButtonDefaultShortcut => "ButtonDefSc",
            Role::ButtonSelectedShortcut => "ButtonSelSc",
            Role::ButtonShadow => "ButtonShadow",
            Role::LabelNormal => "LabelNormal",
            Role::LabelLight => "LabelLight",
            Role::LabelNormalShortcut => "LabelNormSc",
            Role::LabelLightShortcut => "LabelLightSc",
            Role::InputNormal => "InputNormal",
            Role::InputSelected => "InputSelected",
            Role::InputArrow => "InputArrow",
            Role::ScrollerNormal => "ScrollerNormal",
            Role::ScrollerSelected => "ScrollerSelected",
            Role::MenuNormal => "MenuNormal",
            Role::MenuNormalShortcut => "MenuNormSc",
            Role::MenuSelected => "MenuSelected",
            Role::MenuSelectedShortcut => "MenuSelSc",
            Role::MenuDisabled => "MenuDisabled",
            Role::MenuSelectedDisabled => "MenuSelDisabled",
            Role::StatusNormal => "StatusNormal",
            Role::StatusShortcut => "StatusShortcut",
            Role::StatusSelect => "StatusSelect",
            Role::StatusShortcutSelect => "StatusScSelect",
            Role::StatusDisabled => "StatusDisabled",
            Role::StatusSelDisabled => "StatusSelDisab",
            Role::InfoPane => "InfoPane",
            Role::OutlineNormal => "OutlineNormal",
            Role::OutlineFocused => "OutlineFocused",
            Role::OutlineSelected => "OutlineSelected",
            Role::OutlineNotExpanded => "OutlineNotExpnd",
            Role::Shadow => "Shadow",
        }
    }

    /// Total mapping of each variant to its index into the style array.
    ///
    /// A `match` (rather than `#[repr(usize)]` games) keeps this explicit and
    /// total; the compiler enforces exhaustiveness when new roles are added.
    fn index(self) -> usize {
        match self {
            Role::Background => 0,
            Role::FrameActive => 1,
            Role::FramePassive => 2,
            Role::FrameDragging => 3,
            Role::FrameIcon => 4,
            Role::ScrollBarPage => 5,
            Role::ScrollBarControls => 6,
            Role::Normal => 7,
            Role::Focused => 8,
            Role::Disabled => 9,
            Role::Pressed => 10,
            Role::ListNormalActive => 11,
            Role::ListNormalInactive => 12,
            Role::ListFocused => 13,
            Role::ListSelected => 14,
            Role::ListDivider => 15,
            Role::Error => 16,
            Role::Warning => 17,
            Role::Info => 18,
            Role::Success => 19,
            Role::StaticText => 20,
            Role::ClusterNormal => 21,
            Role::ClusterSelected => 22,
            Role::ClusterNormalShortcut => 23,
            Role::ClusterSelectedShortcut => 24,
            Role::ClusterDisabled => 25,
            Role::IndicatorNormal => 26,
            Role::IndicatorDragging => 27,
            Role::ButtonNormal => 28,
            Role::ButtonDefault => 29,
            Role::ButtonSelected => 30,
            Role::ButtonDisabled => 31,
            Role::ButtonNormalShortcut => 32,
            Role::ButtonDefaultShortcut => 33,
            Role::ButtonSelectedShortcut => 34,
            Role::ButtonShadow => 35,
            Role::LabelNormal => 36,
            Role::LabelLight => 37,
            Role::LabelNormalShortcut => 38,
            Role::LabelLightShortcut => 39,
            Role::InputNormal => 40,
            Role::InputSelected => 41,
            Role::InputArrow => 42,
            Role::ScrollerNormal => 43,
            Role::ScrollerSelected => 56,
            Role::MenuNormal => 44,
            Role::MenuNormalShortcut => 45,
            Role::MenuSelected => 46,
            Role::MenuSelectedShortcut => 47,
            Role::MenuDisabled => 48,
            Role::MenuSelectedDisabled => 49,
            Role::StatusNormal => 50,
            Role::StatusShortcut => 51,
            Role::StatusSelect => 52,
            Role::StatusShortcutSelect => 53,
            Role::StatusDisabled => 54,
            Role::StatusSelDisabled => 55,
            Role::InfoPane => 57,
            Role::OutlineNormal => 58,
            Role::OutlineFocused => 59,
            Role::OutlineSelected => 60,
            Role::OutlineNotExpanded => 61,
            Role::Shadow => 62,
            Role::FrameGrayActive => 63,
            Role::FrameGrayPassive => 64,
            Role::FrameGrayDragging => 65,
            Role::FrameGrayIcon => 66,
            Role::FrameCyanActive => 67,
            Role::FrameCyanPassive => 68,
            Role::FrameCyanDragging => 69,
            Role::FrameCyanIcon => 70,
            Role::HistoryArrow => 71,
            Role::HistorySides => 72,
            Role::HistoryViewerNormal => 73,
            Role::HistoryViewerFocused => 74,
        }
    }
}

/// Holder for the framework's drawing glyphs — frame corners/tee-connectors,
/// scrollbar arrows, check/radio marks, shadows, window decorations.
///
/// Defaults match the classic CP437/BIOS character set.
///
/// # Scrollbar glyphs
///
/// ```text
/// vChars = { '\x1E', '\x1F', '\xB1', '\xFE', '\xB2' };
/// hChars = { '\x11', '\x10', '\xB1', '\xFE', '\xB2' };
/// ```
/// Indices: `[0]`=back-arrow, `[1]`=fwd-arrow, `[2]`=page/trough, `[3]`=thumb,
/// `[4]`=page-when-no-range.
///
/// # Frame glyphs
///
/// The frame border is drawn from CP437 box characters, stored as **named
/// glyphs** (single- and double-line corners and edges) plus four icon strings
/// that carry the `~`-toggle markers consumed by
/// [`DrawCtx::put_cstr`](crate::view::DrawCtx::put_cstr). The tee/cross glyphs
/// (`frame_tee_*`, `frame_cross`) are seeded for completeness but unused — they
/// would feed the sibling tee-join walk that rstv does not reproduce (see the
/// [`frame`](crate::frame) module docs).
///
/// Box-drawing pieces:
/// ```text
/// ┌ ┐ └ ┘ ─ │   (single-line)
/// ╔ ╗ ╚ ╝ ═ ║   (double-line)
/// close "[~■~]"  zoom "[~↑~]"  un-zoom "[~↕~]"
/// drag "~─┘~"    drag-left "~└─~"
/// ```
///
/// # Turbo Vision heritage
///
/// Ports the glyph tables in `tvtext1.cpp`. The original encoded the frame box as
/// a bit-mask fed table plus a sibling tee-join walk; rstv stores plain named box
/// pieces instead and skips the sibling walk (deviation D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
    // --- Scrollbar glyphs ---
    /// Vertical scrollbar: up-arrow / back-arrow. `vChars[0]` = `'\x1E'` (▲).
    pub sb_v_arrow_back: char,
    /// Vertical scrollbar: down-arrow / fwd-arrow. `vChars[1]` = `'\x1F'` (▼).
    pub sb_v_arrow_fwd: char,
    /// Horizontal scrollbar: left-arrow / back-arrow. `hChars[0]` = `'\x11'` (◄).
    pub sb_h_arrow_back: char,
    /// Horizontal scrollbar: right-arrow / fwd-arrow. `hChars[1]` = `'\x10'` (►).
    pub sb_h_arrow_fwd: char,
    /// Page/trough fill character (both orientations). `vChars[2]` = `'\xB1'` (▒).
    pub sb_page: char,
    /// Thumb/indicator character (both orientations). `vChars[3]` = `'\xFE'` (■).
    pub sb_thumb: char,
    /// Page fill when range is zero (both orientations). `vChars[4]` = `'\xB2'` (▓).
    pub sb_page_no_range: char,

    // --- Frame glyphs — single-line box ---
    /// Single-line top-left corner `┌` (`\xDA`).
    pub frame_tl: char,
    /// Single-line top-right corner `┐` (`\xBF`).
    pub frame_tr: char,
    /// Single-line bottom-left corner `└` (`\xC0`).
    pub frame_bl: char,
    /// Single-line bottom-right corner `┘` (`\xD9`).
    pub frame_br: char,
    /// Single-line horizontal edge `─` (`\xC4`).
    pub frame_h: char,
    /// Single-line vertical edge `│` (`\xB3`).
    pub frame_v: char,

    // --- Frame glyphs — double-line box (active frame) ---
    /// Double-line top-left corner `╔` (`\xC9`).
    pub frame_tl_d: char,
    /// Double-line top-right corner `╗` (`\xBB`).
    pub frame_tr_d: char,
    /// Double-line bottom-left corner `╚` (`\xC8`).
    pub frame_bl_d: char,
    /// Double-line bottom-right corner `╝` (`\xBC`).
    pub frame_br_d: char,
    /// Double-line horizontal edge `═` (`\xCD`).
    pub frame_h_d: char,
    /// Double-line vertical edge `║` (`\xBA`).
    pub frame_v_d: char,

    // --- Frame glyphs — tee/cross joins (unused: sibling walk not reproduced) ---
    /// Single-line left tee `├` (`\xC3`) — unused.
    pub frame_tee_l: char,
    /// Single-line right tee `┤` (`\xB4`) — unused.
    pub frame_tee_r: char,
    /// Single-line top tee `┬` (`\xC2`) — unused.
    pub frame_tee_t: char,
    /// Single-line bottom tee `┴` (`\xC1`) — unused.
    pub frame_tee_b: char,
    /// Single-line cross `┼` (`\xC5`) — unused.
    pub frame_cross: char,

    // --- Frame icon strings — `~`-toggled for `put_cstr` ---
    /// Close icon `"[~■~]"` — `[` `]` in the frame role, `■` in `FrameIcon`.
    pub close_icon: &'static str,
    /// Zoom icon `"[~↑~]"` (window not maximized).
    pub zoom_icon: &'static str,
    /// Un-zoom icon `"[~↕~]"` (window maximized).
    pub unzoom_icon: &'static str,
    /// Resize/drag icon (bottom-right) `"~─┘~"`.
    pub drag_icon: &'static str,
    /// Resize/drag icon (bottom-left) `"~└─~"`.
    pub drag_left_icon: &'static str,

    // --- Indicator glyphs ---
    /// The editor indicator frame `═` — drawn when the owner is **not** dragging.
    pub indicator_frame_normal: char,
    /// The editor indicator frame `─` — drawn while the owner is dragging.
    pub indicator_frame_dragging: char,
    /// The "buffer modified" marker `☼` drawn at column 0.
    pub indicator_modified: char,

    // --- Button shadow glyphs ---
    /// Button shadow `▄` — drawn at the top of the button's right-edge shadow
    /// column (`y == 0`).
    pub button_shadow_top: char,
    /// Button shadow `█` — drawn down the button's right-edge shadow column
    /// (`y > 0`).
    pub button_shadow_side: char,
    /// Button shadow `▀` — the button's bottom-row shadow fill.
    pub button_shadow_bottom: char,

    // --- Input-line glyphs ---
    /// Input-line left-scroll arrow `◄` (U+25C4) — drawn at column 0 when the
    /// field can scroll left.
    pub input_left_arrow: char,
    /// Input-line right-scroll arrow `►` (U+25BA) — drawn at the last column when
    /// the field can scroll right.
    pub input_right_arrow: char,
}

impl Default for Glyphs {
    /// Classic CP437/BIOS glyphs.
    fn default() -> Self {
        Glyphs {
            // Vertical scrollbar arrows: ▲ (0x1E) / ▼ (0x1F)
            sb_v_arrow_back: '\u{25B2}',
            sb_v_arrow_fwd: '\u{25BC}',
            // Horizontal scrollbar arrows: ◄ (0x11) / ► (0x10)
            sb_h_arrow_back: '\u{25C4}',
            sb_h_arrow_fwd: '\u{25BA}',
            // Trough / page fill: ▒ (0xB1)
            sb_page: '\u{2592}',
            // Thumb / indicator: ■ (0xFE)
            sb_thumb: '\u{25A0}',
            // Trough when range is zero: ▓ (0xB2)
            sb_page_no_range: '\u{2593}',

            // Frame box — single-line: ┌ ┐ └ ┘ ─ │
            frame_tl: '\u{250C}',
            frame_tr: '\u{2510}',
            frame_bl: '\u{2514}',
            frame_br: '\u{2518}',
            frame_h: '\u{2500}',
            frame_v: '\u{2502}',

            // Frame box — double-line: ╔ ╗ ╚ ╝ ═ ║
            frame_tl_d: '\u{2554}',
            frame_tr_d: '\u{2557}',
            frame_bl_d: '\u{255A}',
            frame_br_d: '\u{255D}',
            frame_h_d: '\u{2550}',
            frame_v_d: '\u{2551}',

            // Frame tee/cross joins (unused — sibling walk not reproduced): ├ ┤ ┬ ┴ ┼
            frame_tee_l: '\u{251C}',
            frame_tee_r: '\u{2524}',
            frame_tee_t: '\u{252C}',
            frame_tee_b: '\u{2534}',
            frame_cross: '\u{253C}',

            // Frame icon strings (~ toggles the FrameIcon style for the bright glyph):
            //   close "[~■~]"  zoom "[~↑~]"  unZoom "[~↕~]"
            //   drag "~─┘~"    dragLeft "~└─~"
            close_icon: "[~\u{25A0}~]",
            zoom_icon: "[~\u{2191}~]",
            unzoom_icon: "[~\u{2195}~]",
            drag_icon: "~\u{2500}\u{2518}~",
            drag_left_icon: "~\u{2514}\u{2500}~",

            // Indicator: ═ (0xCD) not-dragging, ─ (0xC4) dragging, ☼ (0x0F) modified.
            indicator_frame_normal: '\u{2550}',
            indicator_frame_dragging: '\u{2500}',
            indicator_modified: '\u{263C}',

            // Button shadow: ▄ (0xDC) top, █ (0xDB) side, ▀ (0xDF) bottom.
            button_shadow_top: '\u{2584}',
            button_shadow_side: '\u{2588}',
            button_shadow_bottom: '\u{2580}',

            // Input line: ◄ (0x11) left scroll arrow, ► (0x10) right.
            input_left_arrow: '\u{25C4}',
            input_right_arrow: '\u{25BA}',
        }
    }
}

/// A theme: a fixed [`Role`] → [`Style`] map plus a [`Glyphs`] holder.
///
/// # Turbo Vision heritage
///
/// Collapses the original palette chain and scattered glyph literals into one
/// role-keyed table (deviation D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    styles: [Style; ROLE_COUNT],
    glyphs: Glyphs,
}

impl Theme {
    /// The default theme — the classic Turbo Vision blue look.
    ///
    /// Every value is a `(foreground, background)` BIOS-colour pair. Each role is
    /// set with an inline comment recording how the original framework derived that
    /// colour — the widget's palette string resolved through its realistic owner
    /// (dialog widgets → a gray dialog; window-content widgets → a blue window;
    /// program-owned widgets → one direct hop) down to the final attribute byte.
    /// Those derivation comments are kept deliberately: they are the project's
    /// documented justification for each colour choice (see the `theme` design
    /// notes). Roles marked "rstv-native" have no inherited chain.
    pub fn classic_blue() -> Self {
        // BIOS 4-bit palette reminder: 0=black 1=blue 2=green 3=cyan 4=red
        // 5=magenta 6=brown 7=lightgray 8=darkgray 9=lightblue ... F=white.
        let mut styles = [Style::default(); ROLE_COUNT];
        // The default theme pins canonical true-color RGB (via `Color::bios_rgb`) so
        // contrast is correct regardless of the terminal's BIOS palette. The BIOS
        // nibbles in the call sites below remain as readable documentation of the
        // colour derivation; only the stored color becomes definite RGB. The
        // `Color::Bios` variant remains available for apps that want
        // terminal-palette colors.
        let set = |styles: &mut [Style; ROLE_COUNT], role: Role, fg: u8, bg: u8| {
            styles[role.index()] = Style::new(Color::bios_rgb(fg), Color::bios_rgb(bg));
        };

        // Desktop / frames. Derivation: the frame's color slots resolve through
        // cpFrame into the owner's cpBlueWindow, then into cpAppColor; the
        // background resolves through the desktop's empty (pass-through) palette.
        set(&mut styles, Role::Background, 0x7, 0x1); // lightgray on blue (chain: cpBackground[1]=0x01 → desktop pass-through → cpAppColor[1]=0x71)
        set(&mut styles, Role::FrameActive, 0xF, 0x1); // white on blue (chain: cpFrame[3]=0x02 → cpBlueWindow[2]=0x09 → cpAppColor[9]=0x1F)
        set(&mut styles, Role::FramePassive, 0x7, 0x1); // lightgray on blue (chain: cpFrame[1]=0x01 → cpBlueWindow[1]=0x08 → cpAppColor[8]=0x17)
        set(&mut styles, Role::FrameDragging, 0xA, 0x1); // lightgreen on blue (chain: cpFrame[5]=0x03 → cpBlueWindow[3]=0x0A → cpAppColor[10]=0x1A)
        set(&mut styles, Role::FrameIcon, 0xA, 0x1); // lightgreen on blue (chain: cpFrame[5]=0x03 → cpBlueWindow[3]=0x0A → cpAppColor[10]=0x1A)

        // Gray-scheme frames (dialogs / gray windows). Derivation: the frame's
        // color slots resolve through cpFrame into the OWNER's palette, here
        // cpGrayDialog instead of cpBlueWindow, then into cpAppColor:
        //   active   cpFrame[3]=0x02 → cpGrayDialog[2]=0x21 → cpAppColor[33]=0x7F
        //   passive  cpFrame[1]=0x01 → cpGrayDialog[1]=0x20 → cpAppColor[32]=0x70
        //   dragging cpFrame[5]=0x03 → cpGrayDialog[3]=0x22 → cpAppColor[34]=0x7A
        //   icon     cpFrame[5]=0x03 → cpGrayDialog[3]=0x22 → cpAppColor[34]=0x7A
        set(&mut styles, Role::FrameGrayActive, 0xF, 0x7); // white on lightgray (0x7F)
        set(&mut styles, Role::FrameGrayPassive, 0x0, 0x7); // black on lightgray (0x70)
        set(&mut styles, Role::FrameGrayDragging, 0xA, 0x7); // lightgreen on lightgray (0x7A)
        set(&mut styles, Role::FrameGrayIcon, 0xA, 0x7); // lightgreen on lightgray (0x7A)

        // Cyan-scheme frames (cyan windows). Same cpFrame slots, resolved through
        // cpCyanWindow into cpAppColor:
        set(&mut styles, Role::FrameCyanActive, 0xF, 0x3); // white on cyan (chain: cpFrame[3]=0x02 → cpCyanWindow[2]=0x11 → cpAppColor[17]=0x3F)
        set(&mut styles, Role::FrameCyanPassive, 0x7, 0x3); // lightgray on cyan (chain: cpFrame[1]=0x01 → cpCyanWindow[1]=0x10 → cpAppColor[16]=0x37)
        set(&mut styles, Role::FrameCyanDragging, 0xA, 0x3); // lightgreen on cyan (chain: cpFrame[5]=0x03 → cpCyanWindow[3]=0x12 → cpAppColor[18]=0x3A)
        set(&mut styles, Role::FrameCyanIcon, 0xA, 0x3); // lightgreen on cyan (chain: cpFrame[5]=0x03 → cpCyanWindow[3]=0x12 → cpAppColor[18]=0x3A)

        // History family. The history icon sits in a gray dialog (cpHistory →
        // cpGrayDialog → cpAppColor); the recall viewer adds one more hop through
        // its history-window owner (cpHistoryViewer → cpHistoryWindow →
        // cpGrayDialog → cpAppColor).
        set(&mut styles, Role::HistoryArrow, 0x0, 0x2); // black on green (chain: cpHistory[1]=0x16 → cpGrayDialog[22]=0x35 → cpAppColor[53]=0x20)
        set(&mut styles, Role::HistorySides, 0x2, 0x7); // green on lightgray (chain: cpHistory[2]=0x17 → cpGrayDialog[23]=0x36 → cpAppColor[54]=0x72)
        set(&mut styles, Role::HistoryViewerNormal, 0xF, 0x1); // white on blue (chain: cpHistoryViewer[1]=[2]=[4]=[5]=0x06 → cpHistoryWindow[6]=0x13 → cpGrayDialog[19]=0x32 → cpAppColor[50]=0x1F)
        set(&mut styles, Role::HistoryViewerFocused, 0xF, 0x2); // white on green (chain: cpHistoryViewer[3]=0x07 → cpHistoryWindow[7]=0x14 → cpGrayDialog[20]=0x33 → cpAppColor[51]=0x2F)

        set(&mut styles, Role::ScrollBarPage, 0x1, 0x3); // blue on cyan (chain: cpScrollBar[1]=0x04 → cpBlueWindow[4]=0x0B → cpAppColor[11]=0x31)
        set(&mut styles, Role::ScrollBarControls, 0x1, 0x3); // blue on cyan (chain: cpScrollBar[2]=cpScrollBar[3]=0x05 → cpBlueWindow[5]=0x0C → cpAppColor[12]=0x31)

        // Generic control states — rstv-native roles (no inherited palette chain).
        set(&mut styles, Role::Normal, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::Focused, 0xF, 0x2); // white on green
        set(&mut styles, Role::Disabled, 0x8, 0x1); // darkgray on blue
        set(&mut styles, Role::Pressed, 0xF, 0x2); // white on green

        // List matrix (cpListViewer idx 1..5). Derivation: a list viewer inside a
        // gray dialog — the realistic list-box case: cpListViewer → cpGrayDialog →
        // cpAppColor. Indices 1 and 2 map to the same dialog entry 26, so the
        // active and inactive normals coincide.
        set(&mut styles, Role::ListNormalActive, 0x0, 0x3); // black on cyan (chain: cpListViewer[1]=0x1A → cpGrayDialog[26]=0x39 → cpAppColor[57]=0x30)
        set(&mut styles, Role::ListNormalInactive, 0x0, 0x3); // black on cyan (chain: cpListViewer[2]=0x1A → cpGrayDialog[26]=0x39 → cpAppColor[57]=0x30)
        set(&mut styles, Role::ListFocused, 0xF, 0x2); // white on green (chain: cpListViewer[3]=0x1B → cpGrayDialog[27]=0x3A → cpAppColor[58]=0x2F)
        set(&mut styles, Role::ListSelected, 0xE, 0x3); // yellow on cyan (chain: cpListViewer[4]=0x1C → cpGrayDialog[28]=0x3B → cpAppColor[59]=0x3E)
        set(&mut styles, Role::ListDivider, 0x1, 0x3); // blue on cyan (chain: cpListViewer[5]=0x1D → cpGrayDialog[29]=0x3C → cpAppColor[60]=0x31)

        // Feedback family — rstv-native roles (no inherited chain).
        set(&mut styles, Role::Error, 0xF, 0x4); // white on red
        set(&mut styles, Role::Warning, 0x0, 0x6); // black on brown
        set(&mut styles, Role::Info, 0xF, 0x1); // white on blue
        set(&mut styles, Role::Success, 0xF, 0x2); // white on green

        // Static text + cluster family. Derivation: a static text / cluster inside
        // a gray dialog (the realistic owner): cpStaticText / cpCluster →
        // cpGrayDialog → cpAppColor. Clusters sit on the classic cyan strip (the
        // familiar checkbox/radio look); both shortcut indices map to the same
        // dialog entry 18, so the two shortcut roles coincide.
        set(&mut styles, Role::StaticText, 0x0, 0x7); // black on lightgray (chain: cpStaticText[1]=0x06 → cpGrayDialog[6]=0x25 → cpAppColor[37]=0x70)
        set(&mut styles, Role::ClusterNormal, 0x0, 0x3); // black on cyan (chain: cpCluster[1]=0x10 → cpGrayDialog[16]=0x2F → cpAppColor[47]=0x30)
        set(&mut styles, Role::ClusterSelected, 0xF, 0x3); // white on cyan (chain: cpCluster[2]=0x11 → cpGrayDialog[17]=0x30 → cpAppColor[48]=0x3F)
        set(&mut styles, Role::ClusterNormalShortcut, 0xE, 0x3); // yellow on cyan (chain: cpCluster[3]=0x12 → cpGrayDialog[18]=0x31 → cpAppColor[49]=0x3E)
        set(&mut styles, Role::ClusterSelectedShortcut, 0xE, 0x3); // yellow on cyan (chain: cpCluster[4]=0x12 → cpGrayDialog[18]=0x31 → cpAppColor[49]=0x3E)
        set(&mut styles, Role::ClusterDisabled, 0x8, 0x3); // darkgray on cyan (chain: cpCluster[5]=0x1F → cpGrayDialog[31]=0x3E → cpAppColor[62]=0x38)

        // Indicator (editor row/col display). Derivation: an indicator inside an
        // edit window — a blue window (the edit window does not override the window
        // palette, so cpBlueWindow applies): cpIndicator → cpBlueWindow →
        // cpAppColor.
        set(&mut styles, Role::IndicatorNormal, 0xF, 0x1); // white on blue (chain: cpIndicator[1]=0x02 → cpBlueWindow[2]=0x09 → cpAppColor[9]=0x1F)
        set(&mut styles, Role::IndicatorDragging, 0xA, 0x1); // lightgreen on blue (chain: cpIndicator[2]=0x03 → cpBlueWindow[3]=0x0A → cpAppColor[10]=0x1A)

        // Button family. Derivation: a button inside a gray dialog (the realistic
        // owner): cpButton → cpGrayDialog → cpAppColor. Indices 5..7 all map to the
        // same dialog entry 14, so the three shortcut roles coincide.
        set(&mut styles, Role::ButtonNormal, 0x0, 0x2); // black on green (chain: cpButton[1]=0x0A → cpGrayDialog[10]=0x29 → cpAppColor[41]=0x20)
        set(&mut styles, Role::ButtonDefault, 0xB, 0x2); // lightcyan on green (chain: cpButton[2]=0x0B → cpGrayDialog[11]=0x2A → cpAppColor[42]=0x2B)
        set(&mut styles, Role::ButtonSelected, 0xF, 0x2); // white on green (chain: cpButton[3]=0x0C → cpGrayDialog[12]=0x2B → cpAppColor[43]=0x2F)
        set(&mut styles, Role::ButtonDisabled, 0x8, 0x7); // darkgray on lightgray (chain: cpButton[4]=0x0D → cpGrayDialog[13]=0x2C → cpAppColor[44]=0x78)
        set(&mut styles, Role::ButtonNormalShortcut, 0xE, 0x2); // yellow on green (chain: cpButton[5]=0x0E → cpGrayDialog[14]=0x2D → cpAppColor[45]=0x2E)
        set(&mut styles, Role::ButtonDefaultShortcut, 0xE, 0x2); // yellow on green (chain: cpButton[6]=0x0E → cpGrayDialog[14]=0x2D → cpAppColor[45]=0x2E)
        set(&mut styles, Role::ButtonSelectedShortcut, 0xE, 0x2); // yellow on green (chain: cpButton[7]=0x0E → cpGrayDialog[14]=0x2D → cpAppColor[45]=0x2E)
        set(&mut styles, Role::ButtonShadow, 0x0, 0x7); // black on lightgray (chain: cpButton[8]=0x0F → cpGrayDialog[15]=0x2E → cpAppColor[46]=0x70)

        // Label family. Derivation: a label inside a gray dialog (the realistic
        // owner): cpLabel → cpGrayDialog → cpAppColor. Both shortcut indices map to
        // the same dialog entry 9, so the two shortcut roles coincide.
        set(&mut styles, Role::LabelNormal, 0x0, 0x7); // black on lightgray (chain: cpLabel[1]=0x07 → cpGrayDialog[7]=0x26 → cpAppColor[38]=0x70)
        set(&mut styles, Role::LabelLight, 0xF, 0x7); // white on lightgray (chain: cpLabel[2]=0x08 → cpGrayDialog[8]=0x27 → cpAppColor[39]=0x7F)
        set(&mut styles, Role::LabelNormalShortcut, 0xE, 0x7); // yellow on lightgray (chain: cpLabel[3]=0x09 → cpGrayDialog[9]=0x28 → cpAppColor[40]=0x7E)
        set(&mut styles, Role::LabelLightShortcut, 0xE, 0x7); // yellow on lightgray (chain: cpLabel[4]=0x09 → cpGrayDialog[9]=0x28 → cpAppColor[40]=0x7E)

        // Input line. Derivation: an input line inside a gray dialog (the realistic
        // owner): cpInputLine → cpGrayDialog → cpAppColor. Indices 1 (passive) and
        // 2 (active) both map to dialog entry 0x13, so one role serves both field
        // states: the classic white-on-blue input field over the gray dialog
        // surface.
        set(&mut styles, Role::InputNormal, 0xF, 0x1); // white on blue (chain: cpInputLine[1]=cpInputLine[2]=0x13 → cpGrayDialog[19]=0x32 → cpAppColor[50]=0x1F)
        set(&mut styles, Role::InputSelected, 0xF, 0x2); // white on green (chain: cpInputLine[3]=0x14 → cpGrayDialog[20]=0x33 → cpAppColor[51]=0x2F)
        set(&mut styles, Role::InputArrow, 0xA, 0x1); // lightgreen on blue (chain: cpInputLine[4]=0x15 → cpGrayDialog[21]=0x34 → cpAppColor[52]=0x1A)

        // Scroller / editor content fill. Derivation: a scroller/editor inside a
        // (blue) window — the realistic case, since rstv collapsed per-window
        // palettes into a single role:
        //   cpScroller[1]=0x06 → cpBlueWindow[6]=0x0D → cpAppColor[0x0D]=0x1E (normal)
        //   cpScroller[2]=0x07 → cpBlueWindow[7]=0x0E → cpAppColor[0x0E]=0x71 (selected)
        // (The earlier provisional green 0x28/0x24 was the degenerate "scroller
        // directly on the program, no window remap" resolution — never the case in
        // practice, and it made a live editor render as a flat green field.)
        set(&mut styles, Role::ScrollerNormal, 0xE, 0x1); // yellow on blue (0x1E)
        set(&mut styles, Role::ScrollerSelected, 0x1, 0x7); // blue on lightgray (0x71)

        // Menu family. Derivation: a menu bar/box is owned directly by the program,
        // so cpMenuView resolves in ONE hop into cpAppColor — no window/dialog
        // remap.
        set(&mut styles, Role::MenuNormal, 0x0, 0x7); // black on lightgray (chain: cpMenuView[1]=0x02 → cpAppColor[2]=0x70)
        set(&mut styles, Role::MenuNormalShortcut, 0x4, 0x7); // red on lightgray (chain: cpMenuView[3]=0x04 → cpAppColor[4]=0x74)
        set(&mut styles, Role::MenuSelected, 0x0, 0x2); // black on green (chain: cpMenuView[4]=0x05 → cpAppColor[5]=0x20)
        set(&mut styles, Role::MenuSelectedShortcut, 0x4, 0x2); // red on green (chain: cpMenuView[6]=0x07 → cpAppColor[7]=0x24)
        set(&mut styles, Role::MenuDisabled, 0x8, 0x7); // darkgray on lightgray (chain: cpMenuView[2]=0x03 → cpAppColor[3]=0x78)
        set(&mut styles, Role::MenuSelectedDisabled, 0x8, 0x2); // darkgray on green (chain: cpMenuView[5]=0x06 → cpAppColor[6]=0x28)

        // Status-line family. Derivation: the status line is owned directly by the
        // program, so cpStatusLine resolves in ONE hop into cpAppColor — identical
        // bytes to the menu family.
        set(&mut styles, Role::StatusNormal, 0x0, 0x7); // black on lightgray (chain: cpStatusLine[1]=0x02 → cpAppColor[2]=0x70)
        set(&mut styles, Role::StatusShortcut, 0x4, 0x7); // red on lightgray (chain: cpStatusLine[3]=0x04 → cpAppColor[4]=0x74)
        set(&mut styles, Role::StatusSelect, 0x0, 0x2); // black on green (chain: cpStatusLine[4]=0x05 → cpAppColor[5]=0x20)
        set(&mut styles, Role::StatusShortcutSelect, 0x4, 0x2); // red on green (chain: cpStatusLine[6]=0x07 → cpAppColor[7]=0x24)
        set(&mut styles, Role::StatusDisabled, 0x8, 0x7); // darkgray on lightgray (chain: cpStatusLine[2]=0x03 → cpAppColor[3]=0x78)
        set(&mut styles, Role::StatusSelDisabled, 0x8, 0x2); // darkgray on green (chain: cpStatusLine[5]=0x06 → cpAppColor[6]=0x28)

        // File-info pane. Derivation: cpInfoPane idx 1 → cpGrayDialog[0x1E]=0x3D →
        // cpAppColor[0x3D]=0x13 = BIOS attr (bg<<4)|fg with fg=cyan(3), bg=blue(1).
        set(&mut styles, Role::InfoPane, 0x3, 0x1); // cyan on blue (0x13)

        // Outline viewer. Derivation: an outline viewer inside a (blue) window — the
        // realistic owner (same owner pick as the ScrollerNormal precedent above):
        // cpOutlineViewer → cpBlueWindow → cpAppColor.
        set(&mut styles, Role::OutlineNormal, 0xE, 0x1); // yellow on blue (chain: cpOutlineViewer[1]=0x06 → cpBlueWindow[6]=0x0D → cpAppColor[13]=0x1E)
        set(&mut styles, Role::OutlineFocused, 0x1, 0x7); // blue on lightgray (chain: cpOutlineViewer[2]=0x07 → cpBlueWindow[7]=0x0E → cpAppColor[14]=0x71)
        set(&mut styles, Role::OutlineSelected, 0xA, 0x1); // lightgreen on blue (chain: cpOutlineViewer[3]=0x03 → cpBlueWindow[3]=0x0A → cpAppColor[10]=0x1A)
        set(&mut styles, Role::OutlineNotExpanded, 0xF, 0x1); // white on blue (chain: cpOutlineViewer[4]=0x08 → cpBlueWindow[8]=0x0F → cpAppColor[15]=0x1F)

        // Window/menu drop shadow — the global shadow attribute 0x08.
        set(&mut styles, Role::Shadow, 0x8, 0x0); // darkgray on black

        Theme {
            styles,
            glyphs: Glyphs::default(),
        }
    }

    /// The [`Style`] for `role`. Total — never panics.
    pub fn style(&self, role: Role) -> Style {
        self.styles[role.index()]
    }

    /// Replace the style for `role` in this theme. Used by the theme editor.
    pub fn set_style(&mut self, role: Role, style: Style) {
        self.styles[role.index()] = style;
    }

    /// The theme's glyph holder.
    pub fn glyphs(&self) -> &Glyphs {
        &self.glyphs
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::classic_blue()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant — alias the pub(crate) ALL so tests don't duplicate the list.
    const ALL_ROLES: [Role; ROLE_COUNT] = super::ALL;

    #[test]
    fn index_is_total_and_distinct() {
        let mut seen = [false; ROLE_COUNT];
        for role in ALL_ROLES {
            let i = role.index();
            assert!(i < ROLE_COUNT);
            assert!(!seen[i], "duplicate index {i} for {role:?}");
            seen[i] = true;
        }
        assert!(seen.iter().all(|&b| b), "every index must be covered");
    }

    #[test]
    fn style_is_total_over_all_variants() {
        let t = Theme::classic_blue();
        // Must not panic for any variant.
        for role in ALL_ROLES {
            let _ = t.style(role);
        }
    }

    #[test]
    fn each_role_returns_its_seeded_style() {
        let t = Theme::classic_blue();
        assert_eq!(
            t.style(Role::Background),
            Style::new(Color::bios_rgb(0x7), Color::bios_rgb(0x1))
        );
        assert_eq!(
            t.style(Role::FrameActive),
            Style::new(Color::bios_rgb(0xF), Color::bios_rgb(0x1))
        );
        assert_eq!(
            t.style(Role::Disabled),
            Style::new(Color::bios_rgb(0x8), Color::bios_rgb(0x1))
        );
        assert_eq!(
            t.style(Role::ListSelected),
            Style::new(Color::bios_rgb(0xE), Color::bios_rgb(0x3))
        );
        assert_eq!(
            t.style(Role::Error),
            Style::new(Color::bios_rgb(0xF), Color::bios_rgb(0x4))
        );
        assert_eq!(
            t.style(Role::Success),
            Style::new(Color::bios_rgb(0xF), Color::bios_rgb(0x2))
        );
    }

    #[test]
    fn default_equals_classic_blue() {
        assert_eq!(Theme::default(), Theme::classic_blue());
    }

    #[test]
    fn glyphs_accessor_returns_default() {
        let t = Theme::classic_blue();
        assert_eq!(*t.glyphs(), Glyphs::default());
        // Spot-check the scrollbar glyphs.
        assert_eq!(t.glyphs().sb_page, '\u{2592}');
        assert_eq!(t.glyphs().sb_thumb, '\u{25A0}');
        // Spot-check the frame glyphs.
        assert_eq!(t.glyphs().frame_tl, '\u{250C}'); // ┌
        assert_eq!(t.glyphs().frame_br, '\u{2518}'); // ┘
        assert_eq!(t.glyphs().frame_tl_d, '\u{2554}'); // ╔
        assert_eq!(t.glyphs().frame_h_d, '\u{2550}'); // ═
        assert_eq!(t.glyphs().close_icon, "[~\u{25A0}~]"); // [~■~]
        assert_eq!(t.glyphs().zoom_icon, "[~\u{2191}~]"); // [~↑~]
        assert_eq!(t.glyphs().unzoom_icon, "[~\u{2195}~]"); // [~↕~]
        assert_eq!(t.glyphs().drag_icon, "~\u{2500}\u{2518}~"); // ~─┘~
        assert_eq!(t.glyphs().drag_left_icon, "~\u{2514}\u{2500}~"); // ~└─~
    }
}
