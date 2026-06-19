//! Modal alert, confirmation, and input dialogs.
//!
//! Builders for the standard pop-up dialogs: a body of text with a title
//! ([`build_message_box`]) and a single-field prompt ([`build_input_box`]). Each
//! returns a ready-to-run [`Dialog`]; the program runs it modally and returns the
//! command that closed it.
//!
//! The option types are typed instead of a packed flag word: [`MessageBoxKind`]
//! picks the title and [`MessageBoxButtons`] selects which of [Yes, No, OK, Cancel]
//! to show, in that fixed order.
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
//! (Space / Alt+hotkey) or on a direct mouse click — see [`Button`]
//! (focused-Space and Alt+hotkey arm the animation timer, which fires the command
//! on expiry).
//!
//! # Turbo Vision heritage
//! Ports the `messageBox` / `inputBox` free functions and their `…Rect` variants
//! (`msgbox.cpp`; titles and button labels from `tvtext2.cpp`). The original
//! packed `aOptions` flag word becomes the typed [`MessageBoxKind`] +
//! [`MessageBoxButtons`] (deviation D5); running the dialog modally and destroying
//! it afterward live in [`Program`](crate::app::Program), so this module is a pure
//! builder (deviation D9).

use super::Dialog;
use crate::command::Command;
use crate::view::{Rect, ViewId};
use crate::widgets::StaticText;
use crate::widgets::{Button, ButtonFlags};
use crate::widgets::{InputLine, Label, LimitMode};

// ---------------------------------------------------------------------------
// Public option types (replacing the raw `ushort aOptions` flag word)
// ---------------------------------------------------------------------------

/// The dialog title — which kind of alert this is.
///
/// Pass to [`Program::message_box`] or [`Program::message_box_rect`] as the
/// `kind` argument. Each variant maps to a fixed title string shown in the
/// dialog frame:
///
/// - [`Warning`](Self::Warning) — "Warning": an unexpected but recoverable condition.
/// - [`Error`](Self::Error) — "Error": an operation failed.
/// - [`Information`](Self::Information) — "Information": a neutral informational notice.
/// - [`Confirmation`](Self::Confirmation) — "Confirm": asking the user to approve an action.
///
/// # Turbo Vision heritage
///
/// Replaces the C++ `mfWarning`/`mfError`/`mfInformation`/`mfConfirmation`
/// nibble packed into the `aOptions Word` (`msgbox.cpp`); the type is now a
/// closed enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageBoxKind {
    /// "Warning" title — an unexpected but recoverable condition.
    Warning,
    /// "Error" title — an operation failed.
    Error,
    /// "Information" title — a neutral notice.
    Information,
    /// "Confirm" title — asking for user approval before proceeding.
    Confirmation,
}

impl MessageBoxKind {
    /// The dialog title string for this kind.
    fn title(self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Information => "Information",
            Self::Confirmation => "Confirm",
        }
    }
}

/// The set of buttons to show in a message box.
///
/// Each field, when set, adds one button along the bottom of the dialog. They
/// always appear in [Yes, No, OK, Cancel] order regardless of which are enabled.
/// The value returned by [`Program::message_box`] is the [`Command`] of the
/// button the user pressed.
///
/// Use one of the convenience constructors for the most common combinations:
///
/// | Constructor | Buttons | Use when |
/// |---|---|---|
/// | [`ok()`](Self::ok) | OK | informational notice (only one choice) |
/// | [`ok_cancel()`](Self::ok_cancel) | OK, Cancel | confirm / discard |
/// | [`yes_no()`](Self::yes_no) | Yes, No | binary decision without escape |
/// | [`yes_no_cancel()`](Self::yes_no_cancel) | Yes, No, Cancel | save-before-close |
///
/// For an unusual combination use struct-update syntax:
/// `MessageBoxButtons { yes: true, cancel: true, ..Default::default() }`.
///
/// # Turbo Vision heritage
///
/// Replaces the C++ `mfOKButton`/`mfCancelButton`/`mfYesButton`/`mfNoButton`
/// flags packed into the high byte of `aOptions` (`msgbox.cpp`). The type is
/// now a struct-of-bools; `mfOKCancel`/`mfYesNoCancel`
/// shorthands become named constructors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessageBoxButtons {
    /// Show a "~Y~es" button (fires [`Command::YES`]).
    ///
    /// Use for binary decisions or save-before-close dialogs.
    pub yes: bool,
    /// Show a "~N~o" button (fires [`Command::NO`]).
    ///
    /// Pair with `yes` for a binary decision; add `cancel` to allow escape.
    pub no: bool,
    /// Show an "O~K~" button (fires [`Command::OK`]).
    ///
    /// The most common single-button choice for informational or confirmation
    /// dialogs. Do not combine with `yes`/`no` in the same dialog.
    pub ok: bool,
    /// Show a "~C~ancel" button (fires [`Command::CANCEL`]).
    ///
    /// Always safe to add — allows the user to dismiss without taking action.
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
// Button label / command tables
// ---------------------------------------------------------------------------

/// Button labels in [Yes, No, OK, Cancel] order. The `~X~` markup is parsed by
/// [`Button::new`] as the hotkey letter.
const BUTTON_NAMES: [&str; 4] = ["~Y~es", "~N~o", "O~K~", "~C~ancel"];

/// The command each button fires, in [Yes, No, OK, Cancel] order.
const BUTTON_COMMANDS: [Command; 4] = [Command::YES, Command::NO, Command::OK, Command::CANCEL];

// ---------------------------------------------------------------------------
// Pure dialog builder
// ---------------------------------------------------------------------------

/// Build the message-box [`Dialog`] without running it.
///
/// Assembles the dialog and its children; running it modally and destroying it
/// afterward live in
/// [`Program::message_box_rect`](crate::app::Program::message_box_rect).
///
/// `bounds` is the dialog's bounding rect (position + size). `msg` is the
/// body text (rendered by [`StaticText`], word-wrapped). `kind` picks the
/// title. `buttons` selects which of [Yes, No, OK, Cancel] to show, in that
/// fixed order.
///
/// Returns `(dialog, first_button_id)` where `first_button_id` is the
/// [`ViewId`] of the FIRST inserted button (the first enabled in
/// [Yes, No, OK, Cancel] order), or `None` if no buttons were requested.
/// The caller passes it as the initial-focus target.
pub(crate) fn build_message_box(
    bounds: Rect,
    msg: &str,
    kind: MessageBoxKind,
    buttons: MessageBoxButtons,
) -> (Dialog, Option<ViewId>) {
    let w = bounds.b.x - bounds.a.x; // dialog width  (== size.x)
    let h = bounds.b.y - bounds.a.y; // dialog height (== size.y)

    let mut dialog = Dialog::new(bounds, Some(kind.title().into()));

    // Static text at (3, 2, w-2, h-3) — the text area inside the frame.
    dialog.insert_child(Box::new(StaticText::new(
        Rect::new(3, 2, w - 2, h - 3),
        msg,
    )));

    // Build each selected button (in [Yes, No, OK, Cancel] order, skipping unset).
    //
    // Buttons are 10 columns wide (the Rect::new(0,0,10,2) ctor). Centering:
    //   x starts at -2; for each button x += button_width + 2 = +12 each.
    //   After the loop: x = (w - x) / 2.  Then place each at (x, h-3) and advance x.

    let button_flags = [buttons.yes, buttons.no, buttons.ok, buttons.cancel];
    let button_width = 10_i32;

    // Compute centering offset. x starts at -2, then each button adds
    // (width + 2) = 12, so total span = -2 + button_count * 12.
    let button_count = button_flags.iter().filter(|&&b| b).count() as i32;
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
                ButtonFlags::new(), // normal button — all flags false
            );
            buttons_to_insert.push((b, x));
            x += button_width + 2;
        }
    }

    // Insert each button at its computed position (move_to sets origin, keeps size).
    // Track the ViewId of the FIRST button inserted — the first selectable child
    // in insertion order, which becomes the initial focus.
    let mut first_button_id: Option<ViewId> = None;
    for (mut b, bx) in buttons_to_insert {
        // Set origin = (bx, h-3); size unchanged (10x2).
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

/// Build the single-field input [`Dialog`] without running it.
///
/// Assembles the dialog; running it modally, seeding the initial text, and
/// reading the result live in
/// [`Program::input_box_rect`](crate::app::Program::input_box_rect).
///
/// Layout (`w = bounds.b.x - bounds.a.x`, `h = bounds.b.y - bounds.a.y`):
/// * **InputLine** at `(4 + label_size, 2, w - 3, 3)` — inserted FIRST so it is
///   the first selectable child and gets the initial focus.
/// * **Label** at `(2, 2, 3 + label_size, 3)`, linked to the input line.
/// * **OK** button (default) at `(w - 24, h - 4, w - 14, h - 2)` → [`Command::OK`].
/// * **Cancel** button (normal) at `(w - 12, h - 4, w - 2, h - 2)` →
///   [`Command::CANCEL`].
///
/// Returns `(dialog, input_id)` where `input_id` is the [`ViewId`] of the lone
/// [`InputLine`]. It doubles as the initial-focus target AND the single-field
/// value target: the caller seeds the initial string into it before running and
/// reads the final string out of it on a non-cancel result (the typed value
/// currency, [`FieldValue::Text`](crate::data::FieldValue::Text)).
pub(crate) fn build_input_box(
    bounds: Rect,
    title: &str,
    label: &str,
    limit: i32,
) -> (Dialog, ViewId) {
    let w = bounds.b.x - bounds.a.x; // dialog width  (== size.x)
    let h = bounds.b.y - bounds.a.y; // dialog height (== size.y)

    // Label width in columns (byte length; ASCII labels only in practice).
    let label_size = label.len() as i32;

    let mut dialog = Dialog::new(bounds, Some(title.into()));

    // 1. Input line with byte-limit `limit` and no validator. Inserted FIRST so it
    //    is the first selectable child and gets the initial focus.
    let input_id = dialog.insert_child(Box::new(InputLine::new(
        Rect::new(4 + label_size, 2, w - 3, 3),
        limit,
        None,
        LimitMode::MaxBytes,
    )));

    // 2. Label linked to the input line.
    dialog.insert_child(Box::new(Label::new(
        Rect::new(2, 2, 3 + label_size, 3),
        label,
        Some(input_id),
    )));

    // 3. OK button (default).
    dialog.insert_child(Box::new(Button::new(
        Rect::new(w - 24, h - 4, w - 14, h - 2),
        "O~K~",
        Command::OK,
        ButtonFlags {
            default: true,
            ..Default::default()
        },
    )));

    // 4. Cancel button (normal), offset 12 columns right of OK → x range
    //    [w - 12, w - 2].
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

    /// Input box at 60x8: a "Name" label, an input field, and OK + Cancel buttons.
    #[test]
    fn snapshot_input_box() {
        let (mut d, _input_id) = build_input_box(Rect::new(0, 0, 60, 8), "Title", "Name", 20);
        insta::assert_snapshot!(render_dialog(&mut d, 60, 8));
    }

    /// Scatter unit test: after building + setting the input line's value, its
    /// `value()` reads back as `FieldValue::Text(initial)` — the seed/read-back
    /// round-trip used by `Program::input_box_rect`.
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
