//! Theme: a `Role` → [`Style`] map plus a glyph holder — deviation **D7**
//! (partial row 16).
//!
//! C++ Turbo Vision resolves colours by walking an owner chain of
//! length-prefixed palette strings (`getPalette`/`getColor`) and scatters drawing
//! glyphs (frame corners, scrollbar arrows, marks, shadows) as literals through
//! widget source. Per D7 a single [`Theme`] owns both: a view asks
//! `ctx.theme.style(Role::FrameActive)` and (later) reaches glyphs through
//! [`Glyphs`]. State → role resolution is centralized at each widget's
//! `getColor` → `Role` mapping, which lands when `TFrame`/`TButton` are ported.
//!
//! [`Role`] is a **first-party closed enum** (not a newtype): third parties do
//! not add roles. It **grows per-widget** — seeded here with exactly D7's
//! enumerated needs (active/passive/dragging frames; the
//! normal/focused/disabled/pressed quartet; the list-state matrix; the
//! error/warning/info/success family).

use crate::color::{Color, Style};

/// A semantic colour role. Faithful to D7's "resolve state → role in one
/// centralized mapper": each `getPalette`/`getColor` call site in the C++ maps
/// to one named `Role` here.
///
/// This enum is **closed and first-party** (not app-extensible) and grows as
/// later widgets are ported and need new roles.
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
    /// An active (focused) **gray-scheme** frame (row 34: `TDialog` /
    /// `wpGrayWindow`). The frame selects the `FrameGray*` family when its
    /// owner's [`WindowPalette`](crate::window::WindowPalette) is `Gray`.
    FrameGrayActive,
    /// A passive (unfocused) gray-scheme frame.
    FrameGrayPassive,
    /// A gray-scheme frame being dragged/resized.
    FrameGrayDragging,
    /// A gray-scheme frame icon (close/zoom/resize glyphs).
    FrameGrayIcon,
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
    /// `cpListViewer` idx 1 (`getColor(1)`) — a normal item in an **active**
    /// (selected+active) list. Also the `<empty>` / focusedColor-unused fill.
    ListNormalActive,
    /// `cpListViewer` idx 2 (`getColor(2)`) — a normal item in an **inactive**
    /// (not selected+active) list.
    ListNormalInactive,
    /// `cpListViewer` idx 3 (`getColor(3)`) — the focused (cursor) item of an
    /// active list.
    ListFocused,
    /// `cpListViewer` idx 4 (`getColor(4)`) — a selected item (`isSelected`).
    ListSelected,
    /// `cpListViewer` idx 5 (`getColor(5)`) — the inter-column divider `│`.
    ListDivider,
    /// Error feedback.
    Error,
    /// Warning feedback.
    Warning,
    /// Informational feedback.
    Info,
    /// Success feedback.
    Success,
    /// Static (label/caption) text — `TStaticText` palette index 6 (row 36).
    StaticText,
    /// A cluster item's normal (unselected) text — `TCluster` palette idx 1.
    ClusterNormal,
    /// A cluster item's selected text (cursor item, cluster focused) — idx 2.
    ClusterSelected,
    /// A cluster item's shortcut highlight in the normal state — idx 3.
    ClusterNormalShortcut,
    /// A cluster item's shortcut highlight in the selected state — idx 4.
    ClusterSelectedShortcut,
    /// A disabled cluster item's text — idx 5.
    ClusterDisabled,
    /// `TIndicator` normal (not-dragging) row/col display — `cpIndicator` idx 1.
    IndicatorNormal,
    /// `TIndicator` while its owner is dragging — `cpIndicator` idx 2.
    IndicatorDragging,
    /// `TButton` normal (inactive) face text — `cpButton` idx 1
    /// (`getColor(0x0501)` lo).
    ButtonNormal,
    /// `TButton` default-button face text (active, `amDefault`) — `cpButton`
    /// idx 2 (`getColor(0x0602)` lo).
    ButtonDefault,
    /// `TButton` selected (active + `sfSelected`) face text — `cpButton` idx 3
    /// (`getColor(0x0703)` lo).
    ButtonSelected,
    /// `TButton` disabled face text — `cpButton` idx 4 (`getColor(0x0404)`,
    /// used for both lo and hi).
    ButtonDisabled,
    /// `TButton` shortcut highlight in the normal state — `cpButton` idx 5
    /// (`getColor(0x0501)` hi).
    ButtonNormalShortcut,
    /// `TButton` shortcut highlight in the default state — `cpButton` idx 6
    /// (`getColor(0x0602)` hi).
    ButtonDefaultShortcut,
    /// `TButton` shortcut highlight in the selected state — `cpButton` idx 7
    /// (`getColor(0x0703)` hi).
    ButtonSelectedShortcut,
    /// `TButton` drop-shadow cells — `cpButton` idx 8 (`getColor(8)`).
    ButtonShadow,
    /// `TLabel` caption text when **not** lit (linked control unfocused) —
    /// `cpLabel "\x07\x08\x09\x09"`, `getColor(0x0301)` lo (dialog palette idx 7).
    LabelNormal,
    /// `TLabel` caption text when **lit** (linked control focused) —
    /// `getColor(0x0402)` lo (dialog palette idx 8).
    LabelLight,
    /// `TLabel` shortcut highlight when **not** lit — `getColor(0x0301)` hi
    /// (dialog palette idx 9; cpLabel maps idx 3 → entry 9).
    LabelNormalShortcut,
    /// `TLabel` shortcut highlight when **lit** — `getColor(0x0402)` hi (dialog
    /// palette idx 9 as well; cpLabel maps idx 4 → entry 9, so this equals
    /// [`LabelNormalShortcut`](Role::LabelNormalShortcut) but is kept a distinct
    /// role so future theming can differ).
    LabelLightShortcut,
    /// `TInputLine` field text — `cpInputLine "\x13\x13\x14\x15"` idx 1 (passive)
    /// **and** idx 2 (active); both map to dialog entry `0x13`, so a single role
    /// serves the focused and unfocused field (`getColor((sfFocused)?2:1)`).
    InputNormal,
    /// `TInputLine` selection highlight — `cpInputLine` idx 3 (`0x14`).
    InputSelected,
    /// `TInputLine` scroll arrows — `cpInputLine` idx 4 (`0x15`).
    InputArrow,
    /// `TScroller` content fill, normal — `cpScroller "\x06\x07"` idx 1 (`0x06`),
    /// the app-direct color `cpAppColor[6] = 0x28` (fg 8 on bg 2). **Provisional**;
    /// a scroller inside a window remaps via the palette chain.
    ScrollerNormal,
    /// `TEditor` selected-text fill — `cpScroller "\x06\x07"` idx 2 (`0x07`),
    /// the app-direct color `cpAppColor[7] = 0x24` (fg 4 on bg 2). Used by
    /// `TEditor::formatLine` for text inside the selection (`getColorAt`).
    ScrollerSelected,
    /// `TMenuView` normal item text (`cpMenuView "\x02\x03\x04\x05\x06\x07"`):
    /// `getColor(0x0301)` lo → palette idx 1. Also the menu-bar background fill.
    MenuNormal,
    /// `TMenuView` normal item shortcut highlight: `getColor(0x0301)` hi → palette
    /// idx 3.
    MenuNormalShortcut,
    /// `TMenuView` selected (highlighted) item text: `getColor(0x0604)` lo →
    /// palette idx 4.
    MenuSelected,
    /// `TMenuView` selected item shortcut highlight: `getColor(0x0604)` hi →
    /// palette idx 6.
    MenuSelectedShortcut,
    /// `TMenuView` disabled (greyed) item text: `getColor(0x0202)` → palette idx 2
    /// for both lo and hi (no shortcut highlight when greyed).
    MenuDisabled,
    /// `TMenuView` selected-but-disabled item text: `getColor(0x0505)` → palette
    /// idx 5 for both lo and hi.
    MenuSelectedDisabled,
    /// `TStatusLine` normal item text (`cpStatusLine`): `cNormal = getColor(0x0301)`
    /// lo → palette idx 1 (`0x70`, black on lightgray). Also the row background fill.
    StatusNormal,
    /// `TStatusLine` normal item shortcut highlight: `cNormal` hi → palette idx 3
    /// (`0x74`, red on lightgray).
    StatusShortcut,
    /// `TStatusLine` selected (hovered) item text: `cSelect = getColor(0x0604)`
    /// lo → palette idx 4 (`0x20`, black on green).
    StatusSelect,
    /// `TStatusLine` selected item shortcut highlight: `cSelect` hi → palette idx 6
    /// (`0x24`, red on green).
    StatusShortcutSelect,
    /// `TStatusLine` disabled (greyed) item text: `cNormDisabled = getColor(0x0202)`
    /// → palette idx 2 (`0x78`, darkgray on lightgray) for both lo and hi.
    StatusDisabled,
    /// `TStatusLine` selected-but-disabled item text: `cSelDisabled =
    /// getColor(0x0505)` → palette idx 5 (`0x28`, darkgray on green) for both
    /// lo and hi.
    StatusSelDisabled,
    /// `TFileInfoPane` text (path + size/date display) — `cpInfoPane "\x1E"`
    /// idx 1 (`getColor(0x01)`, row 78). Resolved through the classic gray-dialog
    /// palette chain: `cpInfoPane` idx 1 → `cpGrayDialog` idx `0x1E` (30) = `0x3D`
    /// → `cpAppColor[0x3D]` = **`0x13`** = BIOS attr fg=cyan(3) on bg=blue(1).
    InfoPane,

    // -- row 89: TOutlineViewer (`cpOutlineViewer "\x6\x7\x3\x8"`) -------------
    /// `TOutlineViewer` normal item — `cpOutlineViewer` idx 1 (the graph + an
    /// expanded item's text).
    OutlineNormal,
    /// `TOutlineViewer` focused item — `cpOutlineViewer` idx 2 (the focused row
    /// when the viewer holds `sfFocused`).
    OutlineFocused,
    /// `TOutlineViewer` selected item — `cpOutlineViewer` idx 3.
    OutlineSelected,
    /// `TOutlineViewer` not-expanded item — `cpOutlineViewer` idx 4 (the dimmer
    /// text shown for a collapsed node, the `color >> 8` of the normal pair).
    OutlineNotExpanded,

    /// Window/menu drop shadows — the global C++ `shadowAttr = 0x08`
    /// (tview.cpp:36), dark gray on black. Not a palette entry in the C++
    /// (it is a file-scope constant); themed here per D7. Applied by the D8
    /// shadow pass ([`DrawCtx::cast_shadow`](crate::view::DrawCtx::cast_shadow)).
    Shadow,
}

/// Number of [`Role`] variants — the fixed length of [`Theme`]'s style array.
const ROLE_COUNT: usize = 67;

impl Role {
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
        }
    }
}

/// Holder for the framework's drawing glyphs — frame corners/tee-connectors,
/// scrollbar arrows, check/radio marks, shadows, window decorations.
///
/// The glyph tables grow **per-widget** as each control is ported (D7,
/// row 9 convention). Fields are added here as each widget row is done;
/// defaults match the classic CP437/BIOS character set that magiblot's
/// `tvtext1.cpp` seeds.
///
/// # Scrollbar glyphs (row 25)
///
/// Taken verbatim from `tvtext1.cpp`:
/// ```text
/// TScrollChars vChars = { '\x1E', '\x1F', '\xB1', '\xFE', '\xB2' };
/// TScrollChars hChars = { '\x11', '\x10', '\xB1', '\xFE', '\xB2' };
/// ```
/// Indices: `[0]`=back-arrow, `[1]`=fwd-arrow, `[2]`=page/trough, `[3]`=thumb,
/// `[4]`=page-when-no-range.
///
/// # Frame glyphs (row 24)
///
/// `TFrame` (`tframe.cpp` / `framelin.cpp`) draws its border from CP437 box
/// chars. magiblot encodes them as a 5-bit `frameChars[33]` mask table fed by
/// `initFrame[19]`, plus the sibling tee-join walk. Under D3 the sibling walk is
/// **deferred** (a frame can't reach its siblings), so we instead store the box
/// pieces as **named glyphs** (single- and double-line) and draw plain
/// corners/edges — byte-identical to C++ for the common case (no `ofFramed`
/// sibling touching the border). The four icon strings carry the `~`-toggle
/// markers consumed by [`DrawCtx::put_cstr`](crate::view::DrawCtx::put_cstr).
///
/// The tee/cross glyphs (`frame_tee_*`, `frame_cross`) are seeded for
/// completeness but are **unused this row** (they feed the deferred sibling
/// walk).
///
/// CP437 → Unicode mapping (from `tvtext1.cpp`):
/// ```text
/// ┌ \xDA  ┐ \xBF  └ \xC0  ┘ \xD9  ─ \xC4  │ \xB3   (single)
/// ╔ \xC9  ╗ \xBB  ╚ \xC8  ╝ \xBC  ═ \xCD  ║ \xBA   (double)
/// closeIcon "[~■~]"  zoomIcon "[~↑~]"  unZoomIcon "[~↕~]"
/// dragIcon "~─┘~"    dragLeftIcon "~└─~"
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
    // --- Scrollbar glyphs (row 25) ---
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

    // --- Frame glyphs (row 24) — single-line box ---
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

    // --- Frame glyphs (row 24) — double-line box (active frame) ---
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

    // --- Frame glyphs (row 24) — tee/cross joins (DEFERRED sibling walk) ---
    /// Single-line left tee `├` (`\xC3`) — unused this row.
    pub frame_tee_l: char,
    /// Single-line right tee `┤` (`\xB4`) — unused this row.
    pub frame_tee_r: char,
    /// Single-line top tee `┬` (`\xC2`) — unused this row.
    pub frame_tee_t: char,
    /// Single-line bottom tee `┴` (`\xC1`) — unused this row.
    pub frame_tee_b: char,
    /// Single-line cross `┼` (`\xC5`) — unused this row.
    pub frame_cross: char,

    // --- Frame icon strings (row 24) — `~`-toggled for `put_cstr` ---
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

    // --- Indicator glyphs (row 45) ---
    /// `TIndicator::dragFrame` (`\xCD` ═) — drawn when the owner is **not**
    /// dragging (the C++ field name is inverted; ported verbatim).
    pub indicator_frame_normal: char,
    /// `TIndicator::normalFrame` (`\xC4` ─) — drawn while the owner is dragging.
    pub indicator_frame_dragging: char,
    /// The "buffer modified" marker drawn at column 0 (`char 15`, ☼).
    pub indicator_modified: char,

    // --- Button shadow glyphs (row 37) ---
    /// `TButton::shadows[0]` (`\xDC` ▄) — drawn at the top of the button's
    /// right-edge shadow column (`y == 0`).
    pub button_shadow_top: char,
    /// `TButton::shadows[1]` (`\xDB` █) — drawn down the button's right-edge
    /// shadow column (`y > 0`).
    pub button_shadow_side: char,
    /// `TButton::shadows[2]` (`\xDF` ▀) — the button's bottom-row shadow fill.
    pub button_shadow_bottom: char,

    // --- Input-line glyphs (row 39) ---
    /// `TInputLine::leftArrow` (`\x11` ◄ U+25C4) — drawn at column 0 when the
    /// field can scroll left.
    pub input_left_arrow: char,
    /// `TInputLine::rightArrow` (`\x10` ► U+25BA) — drawn at the last column when
    /// the field can scroll right.
    pub input_right_arrow: char,
}

impl Default for Glyphs {
    /// Classic CP437/BIOS glyphs, faithful to magiblot's `tvtext1.cpp`.
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

            // Frame tee/cross joins (deferred sibling walk): ├ ┤ ┬ ┴ ┼
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

            // Indicator (row 45): ═ (0xCD) not-dragging, ─ (0xC4) dragging, ☼ (0x0F) modified.
            indicator_frame_normal: '\u{2550}',
            indicator_frame_dragging: '\u{2500}',
            indicator_modified: '\u{263C}',

            // Button shadow (row 37): ▄ (0xDC) top, █ (0xDB) side, ▀ (0xDF) bottom.
            button_shadow_top: '\u{2584}',
            button_shadow_side: '\u{2588}',
            button_shadow_bottom: '\u{2580}',

            // Input line (row 39): ◄ (0x11) left scroll arrow, ► (0x10) right.
            input_left_arrow: '\u{25C4}',
            input_right_arrow: '\u{25BA}',
        }
    }
}

/// A theme: a fixed `Role` → [`Style`] map plus a [`Glyphs`] holder (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    styles: [Style; ROLE_COUNT],
    glyphs: Glyphs,
}

impl Theme {
    /// The default theme — the classic Turbo-Vision blue look.
    ///
    /// **Provisional colours.** These BIOS values reproduce a plausible classic
    /// blue palette, but real per-role fidelity lands later when `TFrame` /
    /// `TButton` etc. map their `getColor` indices onto [`Role`]s; do not treat
    /// the exact values here as authoritative.
    pub fn classic_blue() -> Self {
        // BIOS 4-bit palette reminder: 0=black 1=blue 2=green 3=cyan 4=red
        // 5=magenta 6=brown 7=lightgray 8=darkgray 9=lightblue ... F=white.
        let mut styles = [Style::default(); ROLE_COUNT];
        let set = |styles: &mut [Style; ROLE_COUNT], role: Role, fg: u8, bg: u8| {
            styles[role.index()] = Style::new(Color::Bios(fg), Color::Bios(bg));
        };

        // Desktop / frames.
        set(&mut styles, Role::Background, 0x7, 0x1); // lightgray on blue
        set(&mut styles, Role::FrameActive, 0xF, 0x1); // white on blue
        set(&mut styles, Role::FramePassive, 0x7, 0x1); // lightgray on blue
        set(&mut styles, Role::FrameDragging, 0xE, 0x1); // yellow on blue
        set(&mut styles, Role::FrameIcon, 0xA, 0x1); // light green on blue

        // Gray-scheme frames (row 34: TDialog / wpGrayWindow). Faithful palette
        // chains — TFrame's color slots (tframe.cpp) resolve through cpFrame
        // "\x01\x01\x02\x02\x03" into the OWNER's palette, here cpGrayDialog
        // (dialogs.h) instead of cpBlueWindow, then into cpAppColor (app.h):
        //   active   getColor(0x0503) lo: cpFrame[3]=0x02 → cpGrayDialog[2]=0x21 → cpAppColor[33]=0x7F
        //   passive  getColor(0x0101):    cpFrame[1]=0x01 → cpGrayDialog[1]=0x20 → cpAppColor[32]=0x70
        //   dragging getColor(0x0505):    cpFrame[5]=0x03 → cpGrayDialog[3]=0x22 → cpAppColor[34]=0x7A
        //   icon     getColor(0x0503) hi: cpFrame[5]=0x03 → cpGrayDialog[3]=0x22 → cpAppColor[34]=0x7A
        set(&mut styles, Role::FrameGrayActive, 0xF, 0x7); // white on lightgray (0x7F)
        set(&mut styles, Role::FrameGrayPassive, 0x0, 0x7); // black on lightgray (0x70)
        set(&mut styles, Role::FrameGrayDragging, 0xA, 0x7); // lightgreen on lightgray (0x7A)
        set(&mut styles, Role::FrameGrayIcon, 0xA, 0x7); // lightgreen on lightgray (0x7A)

        set(&mut styles, Role::ScrollBarPage, 0x1, 0x3); // blue on cyan
        set(&mut styles, Role::ScrollBarControls, 0x1, 0x3); // blue on cyan

        // Generic control states.
        set(&mut styles, Role::Normal, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::Focused, 0xF, 0x2); // white on green
        set(&mut styles, Role::Disabled, 0x8, 0x1); // darkgray on blue
        set(&mut styles, Role::Pressed, 0xF, 0x2); // white on green

        // List matrix (cpListViewer idx 1..5). Provisional colors — the C++
        // cpListViewer maps into the owning window/dialog's gray scheme; the
        // window-scheme remap lands with TListBox (row 48) / the window palettes.
        // TODO(window-scheme remap): derive these from the owning view's scheme.
        set(&mut styles, Role::ListNormalActive, 0x7, 0x1); // idx 1: lightgray on blue
        set(&mut styles, Role::ListNormalInactive, 0x8, 0x1); // idx 2: darkgray on blue
        set(&mut styles, Role::ListFocused, 0xF, 0x1); // idx 3: white on blue
        set(&mut styles, Role::ListSelected, 0x0, 0x3); // idx 4: black on cyan
        set(&mut styles, Role::ListDivider, 0x7, 0x1); // idx 5: lightgray on blue

        // Feedback family.
        set(&mut styles, Role::Error, 0xF, 0x4); // white on red
        set(&mut styles, Role::Warning, 0x0, 0x6); // black on brown
        set(&mut styles, Role::Info, 0xF, 0x1); // white on blue
        set(&mut styles, Role::Success, 0xF, 0x2); // white on green

        // Static text + cluster family (rows 36/38). Provisional values modelled
        // on the classic gray-dialog look (`cpStaticText`/`cpCluster` resolved for
        // a gray dialog): black on lightgray, red shortcut accents, green for the
        // selected/cursor item. Not authoritative — they realign with the deferred
        // gray/cyan dialog theming (`TODO(row 34 gray theming)`).
        set(&mut styles, Role::StaticText, 0x0, 0x7); // black on lightgray
        set(&mut styles, Role::ClusterNormal, 0x0, 0x7); // black on lightgray
        set(&mut styles, Role::ClusterSelected, 0xF, 0x2); // white on green
        set(&mut styles, Role::ClusterNormalShortcut, 0x4, 0x7); // red on lightgray
        set(&mut styles, Role::ClusterSelectedShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ClusterDisabled, 0x8, 0x7); // darkgray on lightgray

        // Indicator (editor row/col display, row 45). Provisional, modelled on the
        // classic editor-indicator look (`cpIndicator`): black on cyan normally,
        // bright while the owner is dragging. Realigns with editor theming later.
        set(&mut styles, Role::IndicatorNormal, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::IndicatorDragging, 0xF, 0x3); // white on cyan

        // Button family (row 37). Provisional values resolved through the classic
        // palette chain `cpButton` → `cpGrayDialog` → `cpAppColor` for a gray
        // dialog: green-faced buttons (black text, white when selected, yellow
        // shortcut), darkgray-on-lightgray when disabled. The shadow follows the
        // literal chain value 0x70 (black on lightgray): black half-block shadow
        // glyphs over the gray dialog surface. Realigns with `TODO(row 34 gray
        // theming)`.
        set(&mut styles, Role::ButtonNormal, 0x0, 0x2); // black on green
        set(&mut styles, Role::ButtonDefault, 0xB, 0x2); // light cyan on green
        set(&mut styles, Role::ButtonSelected, 0xF, 0x2); // white on green
        set(&mut styles, Role::ButtonDisabled, 0x8, 0x7); // darkgray on lightgray
        set(&mut styles, Role::ButtonNormalShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ButtonDefaultShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ButtonSelectedShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ButtonShadow, 0x0, 0x7); // black on lightgray (chain: cpButton[8]=0x0F → cpGrayDialog[15]=0x2E → cpAppColor[46]=0x70)

        // Label family (row 41). Provisional values modelled on the classic
        // gray-dialog `cpLabel` chain (dialog palette idx 7/8/9): black on
        // lightgray when not lit, brighter white when lit (linked control
        // focused), red shortcut accent (identical in both states, since cpLabel
        // maps both shortcut indices to dialog entry 9). Not authoritative — they
        // realign with the deferred gray dialog theming (`TODO(row 34 gray
        // theming)`).
        set(&mut styles, Role::LabelNormal, 0x0, 0x7); // black on lightgray
        set(&mut styles, Role::LabelLight, 0xF, 0x7); // white on lightgray (lit)
        set(&mut styles, Role::LabelNormalShortcut, 0x4, 0x7); // red on lightgray
        set(&mut styles, Role::LabelLightShortcut, 0x4, 0x7); // red on lightgray

        // Input line (row 39). Provisional values modelled on the classic
        // gray-dialog `cpInputLine` chain (`"\x13\x13\x14\x15"`): a cyan field
        // (black text) for both passive and active, a green selection
        // highlight, and a brighter arrow colour. Not authoritative — they
        // realign with the deferred gray dialog theming (`TODO(row 34 gray
        // theming)`).
        set(&mut styles, Role::InputNormal, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::InputSelected, 0xF, 0x2); // white on green
        set(&mut styles, Role::InputArrow, 0xE, 0x3); // yellow on cyan

        // Scroller / editor content fill (rows 27, 66). Faithful to the C++ palette
        // chain for a TScroller/TEditor inside a (blue) window — the realistic case,
        // since rstv collapsed per-window palettes into a single Role (D7):
        //   cpScroller[1]=0x06 → cpBlueWindow[6]=0x0D → cpAppColor[0x0D]=0x1E (normal)
        //   cpScroller[2]=0x07 → cpBlueWindow[7]=0x0E → cpAppColor[0x0E]=0x71 (selected)
        // (The earlier provisional green 0x28/0x24 was the degenerate "scroller
        // directly on the program, no window remap" resolution — never the case in
        // practice, and it made a live editor render as a flat green field.)
        set(&mut styles, Role::ScrollerNormal, 0xE, 0x1); // yellow on blue (0x1E)
        set(&mut styles, Role::ScrollerSelected, 0x1, 0x7); // blue on lightgray (0x71)

        // Menu family (rows 50/51). Provisional values modelled on the classic
        // menu look: a lightgray-on-black bar (`cpMenuView` resolves through
        // `cpMenuBar`/`cpMenuView` into the app gray scheme), a green highlight
        // for the selected item, a red shortcut accent, and darkgray for greyed
        // items. Not authoritative — they realign with the deferred gray theming.
        // TODO(row 34 gray theming): realign provisional menu colours.
        set(&mut styles, Role::MenuNormal, 0x0, 0x7); // idx 1: black on lightgray
        set(&mut styles, Role::MenuNormalShortcut, 0x4, 0x7); // idx 3: red on lightgray
        set(&mut styles, Role::MenuSelected, 0xF, 0x2); // idx 4: white on green
        set(&mut styles, Role::MenuSelectedShortcut, 0xE, 0x2); // idx 6: yellow on green
        set(&mut styles, Role::MenuDisabled, 0x8, 0x7); // idx 2: darkgray on lightgray
        set(&mut styles, Role::MenuSelectedDisabled, 0x8, 0x2); // idx 5: darkgray on green

        // Status-line family (rows 47/53). Provisional values decoded from the
        // classic `cpStatusLine` bytes (resolved through `cpAppColor`), each
        // attr byte being `bg<<4 | fg`: idx1 `0x70` (black on lightgray),
        // idx2 `0x78` (darkgray on lightgray), idx3 `0x74` (red on lightgray),
        // idx4 `0x20` (black on green), idx5 `0x28` (darkgray on green), idx6
        // `0x24` (red on green). Not authoritative — they realign with the
        // deferred gray theming. TODO(row 34 gray theming): realign provisional
        // status-line colours.
        set(&mut styles, Role::StatusNormal, 0x0, 0x7); // 0x70: black on lightgray
        set(&mut styles, Role::StatusShortcut, 0x4, 0x7); // 0x74: red on lightgray
        set(&mut styles, Role::StatusSelect, 0x0, 0x2); // 0x20: black on green
        set(&mut styles, Role::StatusShortcutSelect, 0x4, 0x2); // 0x24: red on green
        set(&mut styles, Role::StatusDisabled, 0x8, 0x7); // 0x78: darkgray on lightgray
        set(&mut styles, Role::StatusSelDisabled, 0x8, 0x2); // 0x28: darkgray on green

        // File-info pane (row 78). Faithful palette chain `cpInfoPane "\x1E"`
        // idx 1 → `cpGrayDialog[0x1E]` = `0x3D` → `cpAppColor[0x3D]` = `0x13` =
        // BIOS attr `(bg<<4)|fg` with fg=cyan(3), bg=blue(1).
        set(&mut styles, Role::InfoPane, 0x3, 0x1); // cyan on blue (0x13)

        // Outline viewer (row 89). Provisional values modelled on the list-box
        // look (the C++ `cpOutlineViewer` resolves through the dialog/app gray
        // scheme); they realign with the deferred gray theming.
        set(&mut styles, Role::OutlineNormal, 0x7, 0x1); // lightgray on blue
        set(&mut styles, Role::OutlineFocused, 0xF, 0x1); // white on blue
        set(&mut styles, Role::OutlineSelected, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::OutlineNotExpanded, 0x8, 0x1); // darkgray on blue

        // Window/menu drop shadow — exactly C++ `shadowAttr = 0x08` (tview.cpp:36).
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

    /// The theme's glyph holder (an empty stub until row 9 / per-widget, D7).
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

    /// Every variant, used to assert totality and to seed expected values.
    const ALL_ROLES: [Role; ROLE_COUNT] = [
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
    ];

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
            Style::new(Color::Bios(0x7), Color::Bios(0x1))
        );
        assert_eq!(
            t.style(Role::FrameActive),
            Style::new(Color::Bios(0xF), Color::Bios(0x1))
        );
        assert_eq!(
            t.style(Role::Disabled),
            Style::new(Color::Bios(0x8), Color::Bios(0x1))
        );
        assert_eq!(
            t.style(Role::ListSelected),
            Style::new(Color::Bios(0x0), Color::Bios(0x3))
        );
        assert_eq!(
            t.style(Role::Error),
            Style::new(Color::Bios(0xF), Color::Bios(0x4))
        );
        assert_eq!(
            t.style(Role::Success),
            Style::new(Color::Bios(0xF), Color::Bios(0x2))
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
        // Spot-check the scrollbar glyphs (row 25).
        assert_eq!(t.glyphs().sb_page, '\u{2592}');
        assert_eq!(t.glyphs().sb_thumb, '\u{25A0}');
        // Spot-check the frame glyphs (row 24).
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
