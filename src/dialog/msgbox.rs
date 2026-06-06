//! `messageBox` / `messageBoxRect` — modal alert/confirmation dialogs (row 63, PART 1).
//!
//! Faithful Rust port of `source/tvision/msgbox.cpp` (`messageBoxRect` +
//! `messageBox`). The `inputBox` half is **deferred** (needs D10 dialog
//! gather/scatter).
//!
//! ## D-rules applied
//!
//! * **D1** — drop `T` prefix; `snake_case` methods.
//! * **D5** — flag word (`ushort aOptions`) → typed API:
//!   [`MessageBoxKind`] (the title, C++ `aOptions & 0x3`) and
//!   [`MessageBoxButtons`] (the button mask, C++ `0x0100 << i`).
//! * **D8** — whole-tree redraw; no `drawView` calls needed.
//! * **D9** — `execView` / destroy live in [`Program`](crate::app::Program); this
//!   module only constructs the dialog (the pure builder).
//!
//! ## `selectNext(False)` / initial focus (faithful)
//!
//! C++ calls `dialog->selectNext(False)` after inserting the buttons to focus the
//! first selectable child (i.e. the FIRST button in [Yes, No, OK, Cancel] order).
//! `build_message_box` returns the [`ViewId`] of that first button as the second
//! tuple element. [`Program::message_box_rect`](crate::app::Program::message_box_rect)
//! passes it to `exec_view_with_completion` as `initial_focus`, which calls
//! `focus_descendant` after open — faithfully replicating `selectNext(False)`.
//!
//! ## Button behavior (note)
//!
//! All message-box buttons are `bfNormal` (NOT `bfDefault`). A button fires when
//! focused (Space / Alt+hotkey) or on a direct mouse click — the existing
//! `Button::handle_event` implements these paths (focused-Space and Alt+hotkey
//! arms the animation timer, which fires the command on its expiry).

use super::Dialog;
use crate::command::Command;
use crate::view::{Rect, ViewId};
use crate::widgets::StaticText;
use crate::widgets::{Button, ButtonFlags};

// ---------------------------------------------------------------------------
// Public option types (D5 — replacing the raw `ushort aOptions` flag word)
// ---------------------------------------------------------------------------

/// The dialog title — ports the `aOptions & 0x3` nibble from `msgbox.h`.
///
/// C++ constants: `mfWarning=0x0000`, `mfError=0x0001`,
/// `mfInformation=0x0002`, `mfConfirmation=0x0003`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageBoxKind {
    /// `mfWarning` — "Warning" title.
    Warning,
    /// `mfError` — "Error" title.
    Error,
    /// `mfInformation` — "Information" title.
    Information,
    /// `mfConfirmation` — "Confirm" title.
    Confirmation,
}

impl MessageBoxKind {
    /// The dialog title string, faithful to `tvtext2.cpp` `Titles[]`.
    fn title(self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Information => "Information",
            Self::Confirmation => "Confirm",
        }
    }
}

/// The set of buttons to show — ports the `0x0100 << i` button-mask nibble.
///
/// C++ constants: `mfYesButton=0x0100`, `mfNoButton=0x0200`,
/// `mfOKButton=0x0400`, `mfCancelButton=0x0800`.
///
/// Build with one of the convenience constructors ([`MessageBoxButtons::ok`],
/// [`MessageBoxButtons::ok_cancel`], etc.) or with struct-update syntax.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessageBoxButtons {
    /// `mfYesButton` — show a "~Y~es" button firing [`Command::YES`].
    pub yes: bool,
    /// `mfNoButton` — show a "~N~o" button firing [`Command::NO`].
    pub no: bool,
    /// `mfOKButton` — show an "O~K~" button firing [`Command::OK`].
    pub ok: bool,
    /// `mfCancelButton` — show a "~C~ancel" button firing [`Command::CANCEL`].
    pub cancel: bool,
}

impl MessageBoxButtons {
    /// A single "O~K~" button (the most common choice).
    pub fn ok() -> Self {
        Self {
            ok: true,
            ..Default::default()
        }
    }

    /// "O~K~" + "~C~ancel" buttons.
    pub fn ok_cancel() -> Self {
        Self {
            ok: true,
            cancel: true,
            ..Default::default()
        }
    }

    /// "~Y~es" + "~N~o" + "~C~ancel" buttons.
    pub fn yes_no_cancel() -> Self {
        Self {
            yes: true,
            no: true,
            cancel: true,
            ..Default::default()
        }
    }

    /// "~Y~es" + "~N~o" buttons.
    pub fn yes_no() -> Self {
        Self {
            yes: true,
            no: true,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Button label / command tables (faithful to C++ `buttonName[]` / `commands[]`)
// ---------------------------------------------------------------------------

/// C++ `buttonName[]` in order [Yes, No, OK, Cancel] — `tvtext2.cpp`.
/// The `~X~` markup is parsed by `Button::new` as the hotkey letter.
const BUTTON_NAMES: [&str; 4] = ["~Y~es", "~N~o", "O~K~", "~C~ancel"];

/// C++ `commands[]` in order [Yes, No, OK, Cancel].
const BUTTON_COMMANDS: [Command; 4] = [Command::YES, Command::NO, Command::OK, Command::CANCEL];

// ---------------------------------------------------------------------------
// Pure dialog builder
// ---------------------------------------------------------------------------

/// Build a [`Dialog`] for `messageBoxRect` without executing it.
///
/// Faithful port of the construction half of `messageBoxRect` in
/// `msgbox.cpp` — everything except the `execView`/`destroy` tail, which
/// lives in [`Program::message_box_rect`](crate::app::Program::message_box_rect).
///
/// `bounds` is the dialog's bounding rect (position + size). `msg` is the
/// body text (rendered by [`StaticText`], word-wrapped). `kind` picks the
/// title. `buttons` selects which of [Yes, No, OK, Cancel] to show, in that
/// fixed C++ order.
///
/// Returns `(dialog, first_button_id)` where `first_button_id` is the
/// [`ViewId`] of the FIRST inserted button (the first enabled in
/// [Yes, No, OK, Cancel] order), or `None` if no buttons were requested.
/// The caller passes it to `exec_view_with_completion` as `initial_focus`
/// to replicate C++'s `selectNext(False)`.
pub(crate) fn build_message_box(
    bounds: Rect,
    msg: &str,
    kind: MessageBoxKind,
    buttons: MessageBoxButtons,
) -> (Dialog, Option<ViewId>) {
    let w = bounds.b.x - bounds.a.x; // dialog width  (== size.x)
    let h = bounds.b.y - bounds.a.y; // dialog height (== size.y)

    let mut dialog = Dialog::new(bounds, Some(kind.title().into()));

    // TStaticText at (3, 2, w-2, h-3) — the text area inside the frame.
    dialog.insert_child(Box::new(StaticText::new(
        Rect::new(3, 2, w - 2, h - 3),
        msg,
    )));

    // Build each selected button (in [Yes, No, OK, Cancel] order, skipping unset),
    // faithfully porting the C++ loop:
    //   for i=0..3: if (aOptions & (0x0100 << i)) buttonList[count++] = new TButton(…)
    //
    // Buttons are 10 columns wide (the Rect::new(0,0,10,2) ctor). Centering:
    //   x starts at -2; for each button x += button_width + 2 = +12 each.
    //   After the loop: x = (w - x) / 2.  Then place each at (x, h-3) and advance x.

    let button_flags = [buttons.yes, buttons.no, buttons.ok, buttons.cancel];
    let button_width = 10_i32;

    // Compute centering offset.
    let button_count = button_flags.iter().filter(|&&b| b).count() as i32;
    // C++ x starts at -2, then each button adds (width + 2) = 12.
    // So after the loop: x = -2 + button_count * 12.
    let total_x = -2 + button_count * (button_width + 2);
    let mut x = (w - total_x) / 2;

    // Insert buttons and set their positions.
    let mut buttons_to_insert: Vec<(Button, i32)> = Vec::new();
    for i in 0..4 {
        if button_flags[i] {
            let b = Button::new(
                Rect::new(0, 0, button_width, 2),
                BUTTON_NAMES[i],
                BUTTON_COMMANDS[i],
                ButtonFlags::new(), // bfNormal — all false
            );
            buttons_to_insert.push((b, x));
            x += button_width + 2;
        }
    }

    // Insert each button at its computed position (moveTo = set origin keeping size).
    // Track the ViewId of the FIRST button inserted — this is what C++'s
    // selectNext(False) would focus (the first selectable child in insertion order).
    let mut first_button_id: Option<ViewId> = None;
    for (mut b, bx) in buttons_to_insert {
        // C++ buttonList[i]->moveTo(x, dialog->size.y - 3):
        // moveTo on the view state sets origin = (bx, h-3), size unchanged (10x2).
        b.state.move_to(bx, h - 3);
        let vid = dialog.insert_child(Box::new(b));
        if first_button_id.is_none() {
            first_button_id = Some(vid);
        }
    }

    (dialog, first_button_id)
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
    use crate::view::{DrawCtx, View};

    fn render_dialog(d: &mut dyn View, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = d.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            d.draw(&mut dc);
        });
        screen.snapshot()
    }

    // -- unit tests: option types --------------------------------------------

    #[test]
    fn message_box_buttons_constructors() {
        let ok = MessageBoxButtons::ok();
        assert!(ok.ok);
        assert!(!ok.yes && !ok.no && !ok.cancel);

        let ok_cancel = MessageBoxButtons::ok_cancel();
        assert!(ok_cancel.ok && ok_cancel.cancel);
        assert!(!ok_cancel.yes && !ok_cancel.no);

        let ync = MessageBoxButtons::yes_no_cancel();
        assert!(ync.yes && ync.no && ync.cancel);
        assert!(!ync.ok);

        let yn = MessageBoxButtons::yes_no();
        assert!(yn.yes && yn.no);
        assert!(!yn.ok && !yn.cancel);
    }

    #[test]
    fn message_box_kind_titles() {
        assert_eq!(MessageBoxKind::Warning.title(), "Warning");
        assert_eq!(MessageBoxKind::Error.title(), "Error");
        assert_eq!(MessageBoxKind::Information.title(), "Information");
        assert_eq!(MessageBoxKind::Confirmation.title(), "Confirm");
    }

    // -- snapshot tests (D11) -----------------------------------------------

    /// Error dialog with a single OK button — the most common alert layout.
    /// Box is 40x9; OK+Cancel centering: total_x = -2 + 1*12 = 10; x = (40-10)/2 = 15.
    #[test]
    fn snapshot_error_ok() {
        let bounds = Rect::new(0, 0, 40, 9);
        let (mut d, _first) = build_message_box(
            bounds,
            "An error occurred.",
            MessageBoxKind::Error,
            MessageBoxButtons::ok(),
        );
        insta::assert_snapshot!(render_dialog(&mut d, 40, 9));
    }

    /// Confirmation dialog with Yes/No/Cancel — 3 buttons.
    /// total_x = -2 + 3*12 = 34; x = (40-34)/2 = 3.
    #[test]
    fn snapshot_confirm_yes_no_cancel() {
        let bounds = Rect::new(0, 0, 40, 9);
        let (mut d, _first) = build_message_box(
            bounds,
            "Are you sure?",
            MessageBoxKind::Confirmation,
            MessageBoxButtons::yes_no_cancel(),
        );
        insta::assert_snapshot!(render_dialog(&mut d, 40, 9));
    }
}
