//! The modal dialog window — see the [module docs](super) for the overview.

use crate::command::Command;
use crate::event::{Event, Key};
use crate::view::{Context, DragMode, GrowMode, Point, Rect, View, ViewId};
// These are used only by the test module (via `use super::*`).
#[cfg(test)]
use crate::view::{DrawCtx, StateFlag, ViewState};
use crate::window::{Window, WindowFlags, WindowPalette};

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

/// A modal dialog window: a [`Window`] with dialog-specific field overrides and
/// the Esc/Enter/ok-cancel key handling.
///
/// Build with [`Dialog::new`], then run it modally via
/// [`Program::exec_view`](crate::app::Program::exec_view). See the
/// [module docs](super) for the overview.
///
/// # Turbo Vision heritage
/// Ports `TDialog` (`tdialog.cpp`/`dialogs.h`), which derived from the window
/// class. That inheritance is embed-and-delegate composition (deviation D2): the
/// dialog holds a [`Window`] and forwards to it.
pub struct Dialog {
    /// The embedded window. The dialog *is-a* window: its state, draw, frame, and
    /// most event routing are the window's.
    window: Window,
    /// How [`button_row`](Self::button_row) sizes its faces. Default
    /// [`ButtonLayout::Classic`], so existing dialogs are byte-for-byte unchanged.
    button_layout: crate::dialog::ButtonLayout,
    /// The minimum button face width — a floor in the autosize layouts, and the
    /// exact width in [`Classic`](crate::dialog::ButtonLayout::Classic). Defaults
    /// to [`STD_BUTTON`](crate::dialog::STD_BUTTON)'s width.
    button_min_width: i32,
}

impl Dialog {
    /// Construct the dialog over `bounds` with an optional `title`.
    ///
    /// Use `Dialog` instead of [`Window`] whenever the view will be run modally
    /// via [`Program::exec_view`](crate::app::Program::exec_view): the dialog
    /// sets sensible modal defaults (gray scheme, move+close decoration, no
    /// number, no grow-with-owner) and wires the Esc/Enter/OK/Cancel key
    /// handling so the caller gets a result command back without writing any
    /// extra event logic.
    ///
    /// After construction, populate the dialog with [`insert_child`](Self::insert_child)
    /// or the convenience helper [`button_row`](Self::button_row), then pass the
    /// `Dialog` to `exec_view`.
    ///
    /// The four overrides applied here:
    /// * **Window number 0** — no number is drawn in the title bar.
    /// * **Decoration flags `move | close`** (no grow, no zoom) — the frame shows
    ///   a close icon but neither a grow handle nor a zoom icon.
    /// * **GrowMode all-zero** — the dialog does not track its owner's resize.
    /// * **[`WindowPalette::Gray`]** — the gray dialog color scheme; pushed down
    ///   to the frame child (via the window's internal `set_palette`) so the frame
    ///   renders the `Role::FrameGray*` family immediately.
    pub fn new(bounds: Rect, title: Option<String>) -> Self {
        // Window number 0 -> no number drawn.
        let mut window = Window::new(bounds, title, 0);
        // flags = move | close (no grow, no zoom). set_flags re-pushes to the
        // frame so it draws no zoom icon.
        window.set_flags(WindowFlags {
            r#move: true,
            close: true,
            ..WindowFlags::default()
        });
        // A dialog does not track its owner's resize.
        window.set_grow_mode(GrowMode::default());
        // The gray dialog color scheme; propagates to the frame child.
        window.set_palette(WindowPalette::Gray);
        Dialog {
            window,
            button_layout: crate::dialog::ButtonLayout::Classic,
            button_min_width: crate::dialog::STD_BUTTON.x,
        }
    }

    /// Choose how [`button_row`](Self::button_row) sizes its faces (default
    /// [`Classic`](crate::dialog::ButtonLayout::Classic)).
    ///
    /// Leave it Classic — the default — and every face is the fixed minimum
    /// width, exactly as before, so no existing dialog changes. Switch to
    /// [`Uniform`](crate::dialog::ButtonLayout::Uniform) or
    /// [`Ragged`](crate::dialog::ButtonLayout::Ragged) when a row carries labels
    /// longer than the minimum ("Discard", "Keep editing"), which would otherwise
    /// render hard against the drop shadow. Call before `button_row`.
    pub fn set_button_layout(&mut self, layout: crate::dialog::ButtonLayout) {
        self.button_layout = layout;
    }

    /// Set the minimum button face width — a floor in the
    /// [`Uniform`](crate::dialog::ButtonLayout::Uniform)/[`Ragged`](crate::dialog::ButtonLayout::Ragged)
    /// layouts, and the *exact* width in
    /// [`Classic`](crate::dialog::ButtonLayout::Classic) (where the minimum is
    /// also the maximum). Defaults to [`STD_BUTTON`](crate::dialog::STD_BUTTON)'s
    /// width; raise it for wider fixed faces, e.g. to match a house style. Call
    /// before [`button_row`](Self::button_row).
    pub fn set_button_min_width(&mut self, width: i32) {
        self.button_min_width = width;
    }

    /// Insert a child view into the dialog's embedded window/group.
    ///
    /// Exposed publicly so that example/application code can assemble custom
    /// dialogs by adding their fields, buttons, and labels before running the
    /// dialog modally.
    pub fn insert_child(&mut self, view: Box<dyn View>) -> ViewId {
        self.window.insert_child(view)
    }

    /// Reach a direct child of the dialog's embedded window/group by id.
    ///
    /// Mirrors [`Window::child_mut`]; used by `FileDialog` to run a child's
    /// post-insert, context-bearing init (e.g. reading a directory listing) and to
    /// read it back via `as_any_mut` + downcast.
    pub fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.window.child_mut(id)
    }

    /// Gather the dialog's whole record as one ordered
    /// [`FieldValue::List`](crate::data::FieldValue). Forwards to
    /// [`Window::gather_list`].
    pub fn gather_list(&self) -> crate::data::FieldValue {
        self.window.gather_list()
    }

    /// Scatter an ordered [`FieldValue::List`](crate::data::FieldValue) record
    /// back into the dialog. Forwards to [`Window::scatter_list`].
    pub fn scatter_list(&mut self, record: &crate::data::FieldValue, ctx: &mut Context) {
        self.window.scatter_list(record, ctx);
    }

    /// Insert a conventional button row: 10×2 buttons,
    /// [`BUTTON_GAP`](crate::dialog::BUTTON_GAP) apart, top edge at
    /// `height - BUTTON_ROW_FROM_BOTTOM`. `align` centers or right-groups the row.
    /// Returns the inserted ids in the given order.
    ///
    /// Face widths follow the dialog's [`ButtonLayout`](crate::dialog::ButtonLayout)
    /// and minimum (see [`set_button_layout`](Self::set_button_layout) /
    /// [`set_button_min_width`](Self::set_button_min_width)); the default is
    /// `Classic` at [`STD_BUTTON`], i.e. every face a fixed 10 columns. Switch to
    /// `Uniform` or `Ragged` for labels longer than the minimum ("Discard", "Keep
    /// editing"), which would otherwise render hard against the drop shadow.
    pub fn button_row(
        &mut self,
        buttons: &[(&str, Command, crate::widgets::ButtonFlags)],
        align: crate::dialog::ButtonRowAlign,
    ) -> Vec<ViewId> {
        use crate::dialog::layout::{
            BUTTON_GAP, BUTTON_ROW_FROM_BOTTOM, MARGIN_RIGHT, STD_BUTTON, button_face_width,
        };
        use crate::dialog::{ButtonLayout, ButtonRowAlign};
        use crate::widgets::Button;
        let size = self.state().size;
        let n = buttons.len() as i32;
        if n == 0 {
            return Vec::new();
        }
        let min = self.button_min_width;
        // Per-button face widths, floored at the minimum. Classic ignores the
        // labels entirely (min is both floor and ceiling); Uniform shares the
        // widest natural face; Ragged sizes each face to its own label.
        let widths: Vec<i32> = match self.button_layout {
            ButtonLayout::Classic => buttons.iter().map(|_| min).collect(),
            ButtonLayout::Uniform => {
                let w = buttons
                    .iter()
                    .map(|(t, _, _)| button_face_width(t))
                    .max()
                    .unwrap_or(min)
                    .max(min);
                buttons.iter().map(|_| w).collect()
            }
            ButtonLayout::Ragged => buttons
                .iter()
                .map(|(t, _, _)| button_face_width(t).max(min))
                .collect(),
        };
        let span: i32 = widths.iter().sum::<i32>() + (n - 1) * BUTTON_GAP;
        let left = match align {
            ButtonRowAlign::Center => (size.x - span) / 2,
            ButtonRowAlign::Right => size.x - MARGIN_RIGHT - span,
        };
        let top = size.y - BUTTON_ROW_FROM_BOTTOM;
        let mut ids = Vec::with_capacity(buttons.len());
        let mut x = left;
        for ((title, command, flags), w) in buttons.iter().zip(&widths) {
            let b = Button::new(
                Rect::new(x, top, x + w, top + STD_BUTTON.y),
                title,
                *command,
                *flags,
            );
            ids.push(self.insert_child(Box::new(b)));
            x += w + BUTTON_GAP;
        }
        ids
    }

    /// Override the decoration flags after construction.
    ///
    /// Mirrors [`Window::set_flags`]; used by `FileDialog` and `ChDirDialog` to
    /// add the grow flag on top of the Dialog defaults (`move | close`). Re-pushes
    /// to the frame child so the grow handle draws immediately.
    pub fn set_flags(&mut self, flags: WindowFlags) {
        self.window.set_flags(flags);
    }

    /// Read the current decoration flags.
    ///
    /// Mirrors [`Window::flags`]; exposed so `FileDialog` / `ChDirDialog` tests
    /// can assert the grow flag is set post-construction.
    pub fn flags(&self) -> WindowFlags {
        self.window.flags()
    }

    /// Override the colour scheme after construction. Mirrors [`Window::set_palette`].
    pub fn set_palette(&mut self, palette: WindowPalette) {
        self.window.set_palette(palette);
    }

    /// Override the grow mode after construction. Mirrors [`Window::set_grow_mode`].
    pub fn set_grow_mode(&mut self, grow_mode: GrowMode) {
        self.window.set_grow_mode(grow_mode);
    }

    /// Override the drag mode after construction. Mirrors [`Window::set_drag_mode`].
    pub fn set_drag_mode(&mut self, drag_mode: DragMode) {
        self.window.set_drag_mode(drag_mode);
    }

    /// Raise the dialog's interactive-resize floor (forwards to the embedded
    /// [`Window`]; see [`Window::set_min_size`]).
    pub fn set_min_size(&mut self, min: Point) {
        self.window.set_min_size(min);
    }

    /// Builder form of [`set_flags`](Self::set_flags).
    pub fn with_flags(mut self, flags: WindowFlags) -> Self {
        self.set_flags(flags);
        self
    }

    /// Builder form of [`set_palette`](Self::set_palette).
    pub fn with_palette(mut self, palette: WindowPalette) -> Self {
        self.set_palette(palette);
        self
    }

    /// Builder form of [`set_grow_mode`](Self::set_grow_mode).
    pub fn with_grow_mode(mut self, grow_mode: GrowMode) -> Self {
        self.set_grow_mode(grow_mode);
        self
    }

    /// Builder form of [`set_drag_mode`](Self::set_drag_mode).
    pub fn with_drag_mode(mut self, drag_mode: DragMode) -> Self {
        self.set_drag_mode(drag_mode);
        self
    }

    /// Builder form of [`set_min_size`](Self::set_min_size).
    pub fn with_min_size(mut self, min: Point) -> Self {
        self.set_min_size(min);
        self
    }
}

#[crate::delegate(
    to = window,
    skip(
        apply_scroll_sync,
        as_any_mut,
        calc_bounds,
        grabs_focus_on_click,
        select_window_num,
        set_value,
        value
    )
)]
impl View for Dialog {
    /// Lets the embedded window route the event first, then applies the dialog's
    /// own keys and modal-result commands:
    ///
    /// * **Esc** posts a [`Command::CANCEL`] command.
    /// * **Enter** broadcasts [`Command::DEFAULT`] so the default button fires.
    /// * An [`Command::OK`] / `CANCEL` / `YES` / `NO` command, when this dialog is
    ///   running modally, ends the modal loop with that command as the result.
    ///
    /// Each arm self-guards: if the window routing already consumed the event it is
    /// now [`Event::Nothing`] and none of the matches fire.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Let the embedded window route the event first.
        self.window.handle_event(ev, ctx);

        match *ev {
            // Esc -> post a Cancel command, then consume the key.
            Event::KeyDown(k) if k.key == Key::Esc => {
                ev.clear();
                ctx.post(Command::CANCEL);
            }
            // Enter -> broadcast Default so the default button fires. `source` is
            // None: the broadcast concerns no particular view.
            Event::KeyDown(k) if k.key == Key::Enter => {
                ev.clear();
                ctx.broadcast(Command::DEFAULT, None);
            }
            // OK/Cancel/Yes/No while modal -> end the modal loop with this result.
            // The modal check is folded into the guard, so a non-modal result
            // command is left live for normal routing (see the no-modal case in
            // `ok_does_not_end_modal_when_not_modal`).
            Event::Command(c)
                if matches!(
                    c,
                    Command::OK | Command::CANCEL | Command::YES | Command::NO
                ) && self.window.state().state.modal =>
            {
                ctx.end_modal(c);
                ev.clear();
            }
            _ => {}
        }
    }

    /// [`Command::CANCEL`] is **always** valid (cancelling a dialog can never be
    /// vetoed); otherwise defer to the embedded group, which aggregates the
    /// children — a control with a failing
    /// [`Validator`](crate::validate::Validator) vetoes the close through this path.
    fn valid(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        if cmd == Command::CANCEL {
            true
        } else {
            self.window.valid(cmd, ctx)
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
    use crate::dialog::ButtonRowAlign;
    use crate::event::{KeyEvent, KeyModifiers};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::timer::TimerQueue;
    use crate::view::Deferred;
    use crate::widgets::ButtonFlags;
    use std::collections::VecDeque;

    fn with_ctx<R>(
        out: &mut VecDeque<Event>,
        timers: &mut TimerQueue,
        deferred: &mut Vec<Deferred>,
        f: impl FnOnce(&mut Context) -> R,
    ) -> R {
        let mut ctx = Context::new(out, timers, 0, deferred);
        f(&mut ctx)
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    /// A child view whose `valid` is always false — proves `Dialog::valid` bypasses
    /// the group for Cancel but defers to it for other commands.
    struct AlwaysInvalid {
        st: ViewState,
    }
    impl AlwaysInvalid {
        fn boxed(bounds: Rect) -> Box<dyn View> {
            let mut st = ViewState::new(bounds);
            st.options.selectable = true;
            Box::new(AlwaysInvalid { st })
        }
    }
    impl View for AlwaysInvalid {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn valid(&mut self, _cmd: Command, _ctx: &mut Context) -> bool {
            false
        }
    }

    // -- 1. ctor -------------------------------------------------------------

    #[test]
    fn new_ports_dialog_ctor_defaults() {
        let d = Dialog::new(Rect::new(0, 0, 40, 12), Some("Setup".into()));
        // flags = move | close (NOT grow, NOT zoom).
        assert_eq!(
            d.window.flags(),
            WindowFlags {
                r#move: true,
                close: true,
                grow: false,
                zoom: false,
            },
            "dialog flags = wfMove | wfClose"
        );
        // growMode = 0 (all false).
        let gm = d.state().grow_mode;
        assert!(
            !gm.lo_x && !gm.lo_y && !gm.hi_x && !gm.hi_y && !gm.rel && !gm.fixed,
            "growMode = 0 (dialog does not track owner resize)"
        );
        // palette = Gray — AND pushed down into the frame child (the frame
        // renders the FrameGray* role family).
        assert_eq!(d.window.palette(), WindowPalette::Gray);
        let mut d = d;
        let frame_id = d.window.frame_id();
        let frame = d
            .window
            .child_mut(frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::frame::Frame>())
            .expect("dialog window has a Frame child");
        assert_eq!(
            frame.palette(),
            WindowPalette::Gray,
            "set_palette(Gray) must propagate to the frame child"
        );
        // wnNoNumber -> number None.
        assert_eq!(View::number(&d), None, "wnNoNumber -> no number");
    }

    /// The frame shows **no zoom icon and no number** (the flags-pushed-to-frame
    /// check). The frame renders the gray dialog scheme.
    #[test]
    fn dialog_frame_has_no_zoom_icon_no_number_snapshot() {
        let theme = Theme::classic_blue();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut d = Dialog::new(Rect::new(0, 0, 24, 8), Some("Setup".into()));
        // Select -> active frame (double-line border + icons), so the absence of a
        // zoom icon is meaningful (an active zoomable window would show one).
        with_ctx(&mut out, &mut timers, &mut deferred, |ctx| {
            View::set_state(&mut d, StateFlag::Selected, true, ctx)
        });

        let mut view: Box<dyn View> = Box::new(d);
        let (backend, screen) = HeadlessBackend::new(24, 8);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- 2. Esc posts a Cancel command ---------------------------------------

    #[test]
    fn esc_posts_cm_cancel_and_clears() {
        let mut d = Dialog::new(Rect::new(0, 0, 30, 10), Some("D".into()));
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ev = key(Key::Esc);
        with_ctx(&mut out, &mut timers, &mut deferred, |ctx| {
            d.handle_event(&mut ev, ctx)
        });
        assert!(ev.is_nothing(), "Esc consumed (clearEvent)");
        assert!(
            out.iter().any(|e| *e == Event::Command(Command::CANCEL)),
            "Esc posts cmCancel"
        );
    }

    // -- 3. Enter broadcasts Default -----------------------------------------

    #[test]
    fn enter_broadcasts_cm_default_and_clears() {
        let mut d = Dialog::new(Rect::new(0, 0, 30, 10), Some("D".into()));
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ev = key(Key::Enter);
        with_ctx(&mut out, &mut timers, &mut deferred, |ctx| {
            d.handle_event(&mut ev, ctx)
        });
        assert!(ev.is_nothing(), "Enter consumed");
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast {
                    command: Command::DEFAULT,
                    source: None
                }
            )),
            "Enter broadcasts Default with no subject view"
        );
    }

    // -- 4. OK/Cancel end the modal iff the dialog is modal -------------------

    #[test]
    fn ok_ends_modal_when_modal() {
        let mut d = Dialog::new(Rect::new(0, 0, 30, 10), Some("D".into()));
        d.state_mut().state.modal = true;
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ev = Event::Command(Command::OK);
        with_ctx(&mut out, &mut timers, &mut deferred, |ctx| {
            d.handle_event(&mut ev, ctx)
        });
        assert!(ev.is_nothing(), "OK consumed while modal");
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::OK))),
            "OK while modal queues EndModal(OK)"
        );
    }

    #[test]
    fn ok_does_not_end_modal_when_not_modal() {
        let mut d = Dialog::new(Rect::new(0, 0, 30, 10), Some("D".into()));
        // modal flag NOT set.
        assert!(!d.state().state.modal);
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ev = Event::Command(Command::OK);
        with_ctx(&mut out, &mut timers, &mut deferred, |ctx| {
            d.handle_event(&mut ev, ctx)
        });
        // Discriminating: the command must NOT be consumed and NO EndModal queued.
        assert!(
            !ev.is_nothing(),
            "cmOK left live when not modal (not consumed)"
        );
        assert!(
            !deferred.iter().any(|x| matches!(x, Deferred::EndModal(_))),
            "no EndModal queued when not modal"
        );
    }

    // -- 5. valid veto -------------------------------------------------------

    #[test]
    fn valid_cancel_always_true_other_defers_to_group() {
        let mut d = Dialog::new(Rect::new(0, 0, 30, 10), Some("D".into()));
        // Insert an always-invalid child so the group's valid(other) is false.
        d.window
            .insert_child(AlwaysInvalid::boxed(Rect::new(2, 2, 10, 5)));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        // Cancel bypasses the child and is always valid.
        assert!(
            with_ctx(&mut out, &mut timers, &mut deferred, |ctx| View::valid(
                &mut d,
                Command::CANCEL,
                ctx
            )),
            "cmCancel always valid (cannot be vetoed)"
        );
        // Any other command defers to the group, which is false here.
        assert!(
            !with_ctx(&mut out, &mut timers, &mut deferred, |ctx| View::valid(
                &mut d,
                Command::OK,
                ctx
            )),
            "other command defers to the group (an invalid child vetoes)"
        );
    }

    // -- 6. consumer API: public setters/builders ----------------------------

    #[test]
    fn consumer_can_add_grow_flag_and_change_palette() {
        use crate::window::{WindowFlags, WindowPalette};
        let d = Dialog::new(Rect::new(0, 0, 40, 12), Some("Resizable".into()))
            .with_flags(WindowFlags {
                r#move: true,
                close: true,
                grow: true,
                ..WindowFlags::default()
            })
            .with_palette(WindowPalette::Cyan);
        assert!(d.flags().grow, "consumer added the grow flag publicly");
        assert!(d.flags().r#move && d.flags().close);
        // palette pushed to the frame child:
        let mut d = d;
        let frame_id = d.window.frame_id();
        let frame = d
            .window
            .child_mut(frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::frame::Frame>())
            .expect("dialog window has a Frame child");
        assert_eq!(frame.palette(), WindowPalette::Cyan);
    }

    // -- 7. set_min_size / with_min_size forwarders ----

    /// `set_min_size` forwards to the embedded window: the raised floor is
    /// visible through the dialog's (delegated) `size_limits`.
    #[test]
    fn set_min_size_forwards_to_window() {
        let mut d = Dialog::new(Rect::new(0, 0, 40, 15), Some("T".into()));
        d.set_min_size(Point::new(60, 20));
        let (min, _) = View::size_limits(&d, Point::new(100, 40));
        assert_eq!(min, Point::new(60, 20), "setter forwards");

        let d2 = Dialog::new(Rect::new(0, 0, 40, 15), None).with_min_size(Point::new(50, 21));
        let (min2, _) = View::size_limits(&d2, Point::new(100, 40));
        assert_eq!(min2, Point::new(50, 21), "builder forwards");
    }

    // -- 8. button_row -------------------------------------------------------

    #[test]
    fn button_row_center_places_two_buttons_symmetrically() {
        let mut d = Dialog::new(Rect::new(0, 0, 40, 12), Some("D".into()));
        let ids = d.button_row(
            &[
                (
                    "~O~K",
                    Command::OK,
                    ButtonFlags {
                        default: true,
                        ..ButtonFlags::new()
                    },
                ),
                ("~C~ancel", Command::CANCEL, ButtonFlags::new()),
            ],
            ButtonRowAlign::Center,
        );
        assert_eq!(ids.len(), 2);
        let b0 = d.child_mut(ids[0]).unwrap().state().get_bounds();
        let b1 = d.child_mut(ids[1]).unwrap().state().get_bounds();
        assert_eq!((b0.a.x, b0.a.y), (9, 9), "centered, row top = h-3");
        assert_eq!(b1.a.x, 9 + 10 + 2, "after gap");
        assert_eq!((b0.b.x - b0.a.x, b0.b.y - b0.a.y), (10, 2));
    }

    // -- 9. gather_list / scatter_list forwarders ----------------------------

    #[test]
    fn dialog_gather_scatter_list_round_trips() {
        use crate::data::FieldValue;
        use crate::widgets::{InputLine, LimitMode};
        let mut d = Dialog::new(Rect::new(0, 0, 40, 12), Some("t".to_string()));
        d.insert_child(Box::new(InputLine::new(
            Rect::new(2, 2, 20, 3),
            20,
            None,
            LimitMode::MaxBytes,
        )));
        d.insert_child(Box::new(InputLine::new(
            Rect::new(2, 4, 20, 5),
            20,
            None,
            LimitMode::MaxBytes,
        )));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        with_ctx(&mut out, &mut timers, &mut deferred, |ctx| {
            d.scatter_list(
                &FieldValue::List(vec![
                    FieldValue::Text("a".into()),
                    FieldValue::Text("b".into()),
                ]),
                ctx,
            );
        });
        assert_eq!(
            d.gather_list(),
            FieldValue::List(vec![
                FieldValue::Text("a".into()),
                FieldValue::Text("b".into()),
            ]),
            "Dialog forwards gather/scatter_list to its embedded Group"
        );
    }

    #[test]
    fn button_row_right_groups_against_right_margin() {
        let mut d = Dialog::new(Rect::new(0, 0, 40, 12), Some("D".into()));
        let ids = d.button_row(
            &[
                ("~O~K", Command::OK, ButtonFlags::new()),
                ("~C~ancel", Command::CANCEL, ButtonFlags::new()),
            ],
            ButtonRowAlign::Right,
        );
        assert_eq!(
            d.child_mut(ids[1]).unwrap().state().get_bounds().b.x,
            38,
            "right edge at w - MARGIN_RIGHT"
        );
        assert_eq!(d.child_mut(ids[0]).unwrap().state().get_bounds().a.x, 16);
    }

    /// `Uniform`: a row carrying a label too wide for the standard face widens
    /// every button to one shared width, stays right-grouped against the margin,
    /// and keeps the gap — so the long label renders with its padding column
    /// instead of butting against the drop shadow.
    #[test]
    fn button_row_uniform_widens_every_button_to_the_widest_label() {
        let mut d = Dialog::new(Rect::new(0, 0, 60, 12), Some("D".into()));
        d.set_button_layout(crate::dialog::ButtonLayout::Uniform);
        let ids = d.button_row(
            &[
                ("~R~e-create", Command::YES, ButtonFlags::new()),
                ("~D~iscard", Command::NO, ButtonFlags::new()),
                ("~K~eep editing", Command::CANCEL, ButtonFlags::new()),
            ],
            ButtonRowAlign::Right,
        );
        let bounds: Vec<_> = ids
            .iter()
            .map(|id| d.child_mut(*id).unwrap().state().get_bounds())
            .collect();
        // "Keep editing" is 12 columns → face 16, shared by all three.
        for b in &bounds {
            assert_eq!(b.b.x - b.a.x, 16, "every button takes the widest face");
        }
        assert_eq!(bounds[2].b.x, 58, "still right-grouped at w - MARGIN_RIGHT");
        assert_eq!(bounds[1].a.x, bounds[0].a.x + 16 + 2, "face + BUTTON_GAP");
        assert_eq!(bounds[2].a.x, bounds[1].a.x + 16 + 2);
    }

    /// `Ragged`: each button sized to its own label, so faces differ within the
    /// row. The span still sums the varying widths and stays right-grouped.
    #[test]
    fn button_row_ragged_sizes_each_button_to_its_own_label() {
        let mut d = Dialog::new(Rect::new(0, 0, 60, 12), Some("D".into()));
        d.set_button_layout(crate::dialog::ButtonLayout::Ragged);
        let ids = d.button_row(
            &[
                ("~O~K", Command::OK, ButtonFlags::new()),
                ("~K~eep editing", Command::CANCEL, ButtonFlags::new()),
            ],
            ButtonRowAlign::Right,
        );
        let w0 = {
            let b = d.child_mut(ids[0]).unwrap().state().get_bounds();
            b.b.x - b.a.x
        };
        let w1 = {
            let b = d.child_mut(ids[1]).unwrap().state().get_bounds();
            b.b.x - b.a.x
        };
        // "OK" (2 cols) floors at STD_BUTTON = 10; "Keep editing" (12) → 16.
        assert_eq!(w0, 10, "short label floored at the minimum");
        assert_eq!(w1, 16, "long label sized to itself");
        let b1 = d.child_mut(ids[1]).unwrap().state().get_bounds();
        assert_eq!(b1.b.x, 58, "row still ends at w - MARGIN_RIGHT");
    }

    /// `Classic` (the default) ignores labels entirely: even a long one keeps the
    /// fixed minimum face, so existing dialogs are byte-for-byte unchanged. This
    /// is the backward-compat guarantee that lets the feature ship as additive.
    #[test]
    fn button_row_classic_keeps_the_fixed_minimum_face() {
        let mut d = Dialog::new(Rect::new(0, 0, 60, 12), Some("D".into()));
        // No set_button_layout call — default Classic at STD_BUTTON.
        let ids = d.button_row(
            &[("~K~eep editing", Command::CANCEL, ButtonFlags::new())],
            ButtonRowAlign::Right,
        );
        let b = d.child_mut(ids[0]).unwrap().state().get_bounds();
        assert_eq!(
            b.b.x - b.a.x,
            10,
            "Classic keeps the fixed STD_BUTTON width regardless of label"
        );
    }

    /// The minimum width is a floor in the autosize layouts and the exact width
    /// in Classic. Raising it past a label's natural face widens Classic faces and
    /// lifts short faces in Ragged/Uniform.
    #[test]
    fn button_min_width_raises_the_floor_and_is_exact_in_classic() {
        // Classic: min is also max — a short label gets the raised fixed width.
        let mut d = Dialog::new(Rect::new(0, 0, 60, 12), Some("D".into()));
        d.set_button_min_width(14);
        let ids = d.button_row(
            &[("~O~K", Command::OK, ButtonFlags::new())],
            ButtonRowAlign::Right,
        );
        let b = d.child_mut(ids[0]).unwrap().state().get_bounds();
        assert_eq!(
            b.b.x - b.a.x,
            14,
            "Classic uses the minimum as the exact width"
        );

        // Uniform: a min above the widest natural face lifts every button to it.
        let mut d = Dialog::new(Rect::new(0, 0, 60, 12), Some("D".into()));
        d.set_button_layout(crate::dialog::ButtonLayout::Uniform);
        d.set_button_min_width(20);
        let ids = d.button_row(
            &[
                ("~O~K", Command::OK, ButtonFlags::new()),
                ("~D~iscard", Command::NO, ButtonFlags::new()),
            ],
            ButtonRowAlign::Right,
        );
        for id in ids {
            let b = d.child_mut(id).unwrap().state().get_bounds();
            assert_eq!(b.b.x - b.a.x, 20, "min above the natural face wins");
        }
    }
}
