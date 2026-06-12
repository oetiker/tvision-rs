//! Modal alert, confirmation, and input dialogs — `messageBox` and `inputBox`.
//!
//! Builders for the standard pop-up dialogs: a body of text with a title
//! ([`build_message_box`]) and a single-field prompt ([`build_input_box`]). Each
//! returns a ready-to-run [`Dialog`]; the program runs it modally and returns the
//! command that closed it.
//!
//! The option types replace the C++ raw flag word: [`MessageBoxKind`] picks the
//! title and [`MessageBoxButtons`] selects which of [Yes, No, OK, Cancel] to show,
//! in that fixed order.
//!
//! ## Initial focus
//!
//! After inserting the buttons, focus goes to the first selectable child (the
//! first button in [Yes, No, OK, Cancel] order). `build_message_box` returns that
//! button's [`ViewId`], which
//! [`Program::message_box_rect`](crate::app::Program::message_box_rect) passes as
//! the initial-focus target.
//!
//! ## Button behavior
//!
//! All message-box buttons are normal (not default). A button fires when focused
//! (Space / Alt+hotkey) or on a direct mouse click — see `Button::handle_event`
//! (focused-Space and Alt+hotkey arm the animation timer, which fires the command
//! on expiry).
//!
//! # Turbo Vision heritage
//! Ports `messageBox` / `messageBoxRect` / `inputBox` / `inputBoxRect`
//! (`msgbox.cpp`, titles/labels from `tvtext2.cpp`). The C++ `ushort aOptions`
//! flag word becomes the typed [`MessageBoxKind`] + [`MessageBoxButtons`]
//! (deviation D5); `execView`/destroy live in [`Program`](crate::app::Program),
//! so this module is a pure builder (deviation D9).

use super::Dialog;
use crate::command::Command;
use crate::view::{Rect, ViewId};
use crate::widgets::StaticText;
use crate::widgets::{Button, ButtonFlags};
use crate::widgets::{InputLine, Label, LimitMode};

// ---------------------------------------------------------------------------
// Public option types (replacing the raw `ushort aOptions` flag word)
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
// Pure input-box builder
// ---------------------------------------------------------------------------

/// Build a [`Dialog`] for `inputBoxRect` without executing it.
///
/// Faithful port of the construction half of `inputBoxRect` in `msgbox.cpp` —
/// everything before `selectNext(False)`/`setData`/`execView`, which live in
/// [`Program::input_box_rect`](crate::app::Program::input_box_rect).
///
/// Layout (faithful, `w = bounds.b.x - bounds.a.x`, `h = bounds.b.y - bounds.a.y`):
/// * **InputLine** at `(4 + label_size, 2, w - 3, 3)` — inserted FIRST so it is
///   the first selectable child (`selectNext(False)` initial focus target).
/// * **Label** at `(2, 2, 3 + label_size, 3)`, linked to the input line.
/// * **OK** button (`bfDefault`) at `(w - 24, h - 4, w - 14, h - 2)` → [`Command::OK`].
/// * **Cancel** button (`bfNormal`) at `(w - 12, h - 4, w - 2, h - 2)` →
///   [`Command::CANCEL`]. (C++ `r.a.x += 12; r.b.x += 12` from the OK rect. The
///   trailing dead `+= 12` after Cancel — which in C++ set up a never-used third
///   button — is faithfully dropped.)
///
/// Returns `(dialog, input_id)` where `input_id` is the [`ViewId`] of the lone
/// [`InputLine`]. It doubles as the `selectNext(False)` initial-focus target AND
/// the single-field gather/scatter target (C++ `setData`/`getData`): the caller
/// scatters the initial string into it before exec and gathers the final string
/// out of it on a non-cancel result (the typed value currency,
/// [`FieldValue::Text`](crate::data::FieldValue::Text)).
pub(crate) fn build_input_box(
    bounds: Rect,
    title: &str,
    label: &str,
    limit: i32,
) -> (Dialog, ViewId) {
    let w = bounds.b.x - bounds.a.x; // dialog width  (== size.x)
    let h = bounds.b.y - bounds.a.y; // dialog height (== size.y)

    // C++ aLabel.size() = byte length of the TStringView; ASCII labels only in practice.
    let label_size = label.len() as i32;

    let mut dialog = Dialog::new(bounds, Some(title.into()));

    // 1. TInputLine(r, limit) — `new TInputLine(r, limit)` uses ilMaxBytes (the
    //    default), so maxLen = limit - 1. No validator. Inserted FIRST so it is the
    //    first selectable child (selectNext(False) focuses it).
    let input_id = dialog.insert_child(Box::new(InputLine::new(
        Rect::new(4 + label_size, 2, w - 3, 3),
        limit,
        None,
        LimitMode::MaxBytes,
    )));

    // 2. TLabel(r, aLabel, control) — linked to the input line.
    dialog.insert_child(Box::new(Label::new(
        Rect::new(2, 2, 3 + label_size, 3),
        label,
        Some(input_id),
    )));

    // 3. TButton(okText, cmOK, bfDefault).
    dialog.insert_child(Box::new(Button::new(
        Rect::new(w - 24, h - 4, w - 14, h - 2),
        "O~K~",
        Command::OK,
        ButtonFlags {
            default: true,
            ..Default::default()
        },
    )));

    // 4. TButton(cancelText, cmCancel, bfNormal). C++ `r.a.x += 12; r.b.x += 12`
    //    from the OK rect → x range [w - 12, w - 2].
    dialog.insert_child(Box::new(Button::new(
        Rect::new(w - 12, h - 4, w - 2, h - 2),
        "~C~ancel",
        Command::CANCEL,
        ButtonFlags::new(),
    )));

    (dialog, input_id)
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

    // -- snapshot tests ------------------------------------------------------

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

    /// Input box (`inputBoxRect`) at 60x8: a "Name" label, an input field, and
    /// OK + Cancel buttons. Mirrors the C++ `inputBox` default geometry.
    #[test]
    fn snapshot_input_box() {
        let (mut d, _input_id) = build_input_box(Rect::new(0, 0, 60, 8), "Title", "Name", 20);
        insta::assert_snapshot!(render_dialog(&mut d, 60, 8));
    }

    /// Scatter unit test: after building + setting the input line's value, its
    /// `value()` reads back as `FieldValue::Text(initial)` — the gather/scatter
    /// round-trip used by `Program::input_box_rect` (C++ `setData`/`getData`).
    #[test]
    fn input_box_scatter_value_round_trip() {
        use crate::data::FieldValue;
        use crate::view::View;

        let (mut d, input_id) = build_input_box(Rect::new(0, 0, 60, 8), "Title", "Name", 20);
        if let Some(v) = d.find_mut(input_id) {
            v.set_value(FieldValue::Text("hello".to_string()));
        }
        assert_eq!(
            d.find_mut(input_id).and_then(|v| v.value()),
            Some(FieldValue::Text("hello".to_string())),
            "scattered initial text reads back through value()"
        );
    }
}
