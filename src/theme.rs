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
    /// A normal (unselected, unfocused) list item.
    ListNormal,
    /// A focused list (its cursor item, list not selected).
    ListFocused,
    /// A selected list item in an unfocused list.
    ListSelected,
    /// The selected item in a focused list.
    ListSelectedFocused,
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
}

/// Number of [`Role`] variants — the fixed length of [`Theme`]'s style array.
const ROLE_COUNT: usize = 35;

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
            Role::ListNormal => 11,
            Role::ListFocused => 12,
            Role::ListSelected => 13,
            Role::ListSelectedFocused => 14,
            Role::Error => 15,
            Role::Warning => 16,
            Role::Info => 17,
            Role::Success => 18,
            Role::StaticText => 19,
            Role::ClusterNormal => 20,
            Role::ClusterSelected => 21,
            Role::ClusterNormalShortcut => 22,
            Role::ClusterSelectedShortcut => 23,
            Role::ClusterDisabled => 24,
            Role::IndicatorNormal => 25,
            Role::IndicatorDragging => 26,
            Role::ButtonNormal => 27,
            Role::ButtonDefault => 28,
            Role::ButtonSelected => 29,
            Role::ButtonDisabled => 30,
            Role::ButtonNormalShortcut => 31,
            Role::ButtonDefaultShortcut => 32,
            Role::ButtonSelectedShortcut => 33,
            Role::ButtonShadow => 34,
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
        set(&mut styles, Role::ScrollBarPage, 0x1, 0x3); // blue on cyan
        set(&mut styles, Role::ScrollBarControls, 0x1, 0x3); // blue on cyan

        // Generic control states.
        set(&mut styles, Role::Normal, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::Focused, 0xF, 0x2); // white on green
        set(&mut styles, Role::Disabled, 0x8, 0x1); // darkgray on blue
        set(&mut styles, Role::Pressed, 0xF, 0x2); // white on green

        // List matrix.
        set(&mut styles, Role::ListNormal, 0x7, 0x1); // lightgray on blue
        set(&mut styles, Role::ListFocused, 0xF, 0x1); // white on blue
        set(&mut styles, Role::ListSelected, 0x0, 0x3); // black on cyan
        set(&mut styles, Role::ListSelectedFocused, 0xF, 0x2); // white on green

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
        // shortcut), darkgray-on-lightgray when disabled. The shadow uses the
        // classic dark drop-shadow attribute rather than the literal chain value
        // (which is not shadow-like). Realigns with `TODO(row 34 gray theming)`.
        set(&mut styles, Role::ButtonNormal, 0x0, 0x2); // black on green
        set(&mut styles, Role::ButtonDefault, 0xB, 0x2); // light cyan on green
        set(&mut styles, Role::ButtonSelected, 0xF, 0x2); // white on green
        set(&mut styles, Role::ButtonDisabled, 0x8, 0x7); // darkgray on lightgray
        set(&mut styles, Role::ButtonNormalShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ButtonDefaultShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ButtonSelectedShortcut, 0xE, 0x2); // yellow on green
        set(&mut styles, Role::ButtonShadow, 0x8, 0x0); // darkgray on black

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
        Role::ListNormal,
        Role::ListFocused,
        Role::ListSelected,
        Role::ListSelectedFocused,
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
