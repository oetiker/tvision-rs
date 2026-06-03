//! `TDialog` — see the [module docs](super) for the deviation summary.

use crate::command::Command;
use crate::event::{Event, Key};
use crate::view::{Context, DrawCtx, GrowMode, Point, Rect, StateFlag, View, ViewId, ViewState};
use crate::window::{Window, WindowFlags, WindowPalette};

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

/// `TDialog` — a modal dialog window: a [`Window`] with dialog-specific field
/// overrides and the Esc/Enter/ok-cancel key handling (D2, row 34).
///
/// Build with [`Dialog::new`], then run it modally via
/// [`Program::exec_view`](crate::app::Program::exec_view). See the
/// [module docs](super) for the deviations and the deferrals.
pub struct Dialog {
    /// The embedded window (D2). `Dialog` *is-a* `TWindow`: its state, draw,
    /// frame, and most event routing are the window's.
    window: Window,
}

impl Dialog {
    /// `TDialog::TDialog(bounds, aTitle)` — construct the dialog.
    ///
    /// Ports the C++ ctor faithfully (`tdialog.cpp`):
    /// ```cpp
    /// TWindow( bounds, aTitle, wnNoNumber )   // number = 0 -> no number
    /// growMode = 0;                           // dialogs do NOT grow with the owner
    /// flags = wfMove | wfClose;               // NOT wfGrow, NOT wfZoom
    /// palette = dpGrayDialog;                 // gray scheme (theming deferred)
    /// ```
    ///
    /// `wnNoNumber == 0`, so the window draws no number. The flag override is
    /// **re-pushed to the frame** by [`Window::set_flags`], so the frame shows no
    /// zoom icon. Gray theming is recorded but deferred (the frame still renders
    /// the blue scheme; see the module docs).
    pub fn new(bounds: Rect, title: Option<String>) -> Self {
        // TWindow(bounds, aTitle, wnNoNumber): number 0 -> no number.
        let mut window = Window::new(bounds, title, 0);
        // flags = wfMove | wfClose (NOT grow, NOT zoom). set_flags re-pushes to the
        // frame so it draws no zoom icon.
        window.set_flags(WindowFlags {
            r#move: true,
            close: true,
            ..WindowFlags::default()
        });
        // growMode = 0: a dialog does not track its owner's resize.
        window.set_grow_mode(GrowMode::default());
        // palette = dpGrayDialog (gray theming deferred; recorded only).
        window.set_palette(WindowPalette::Gray);
        Dialog { window }
    }
}

impl View for Dialog {
    // -- delegated to the embedded window (D2) ------------------------------

    fn state(&self) -> &ViewState {
        self.window.state()
    }

    fn state_mut(&mut self) -> &mut ViewState {
        self.window.state_mut()
    }

    /// `TDialog` does not override `draw`; it inherits `TWindow`/`TGroup` drawing.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.window.draw(ctx);
    }

    /// `TDialog::handleEvent` — delegate to `TWindow::handleEvent` **first**
    /// (faithful order), then the dialog's own keys + modal-result commands:
    /// ```cpp
    /// TWindow::handleEvent(event);
    /// switch (event.what) {
    ///   case evKeyDown:
    ///     case kbEsc:   -> evCommand cmCancel,  putEvent, clearEvent
    ///     case kbEnter: -> evBroadcast cmDefault, putEvent, clearEvent
    ///   case evCommand:
    ///     case cmOK/cmCancel/cmYes/cmNo:
    ///        if (state & sfModal) { endModal(command); clearEvent; }
    /// }
    /// ```
    ///
    /// C++ clears the event then `putEvent`s the new one; `ctx.post`/`ctx.broadcast`
    /// enqueue for a *later* pump, so clearing first then posting is equivalent.
    /// Each arm self-guards: if the window delegation consumed the event it is
    /// already `Nothing` and none of the matches fire.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // TWindow::handleEvent FIRST (faithful order).
        self.window.handle_event(ev, ctx);

        match *ev {
            // kbEsc -> post cmCancel, clear. (putEvent == ctx.post.)
            Event::KeyDown(k) if k.key == Key::Esc => {
                ev.clear();
                ctx.post(Command::CANCEL);
            }
            // kbEnter -> broadcast cmDefault, clear. source = None (the C++
            // infoPtr is 0 here: the broadcast concerns no particular view).
            Event::KeyDown(k) if k.key == Key::Enter => {
                ev.clear();
                ctx.broadcast(Command::DEFAULT, None);
            }
            // cmOK/cmCancel/cmYes/cmNo while sfModal -> endModal(command), clear.
            // The sfModal check is folded into the guard, so a non-modal result
            // command is left live for normal routing (the discriminating no-modal
            // case in `ok_does_not_end_modal_when_not_modal`).
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

    /// `TDialog::valid` — `cmCancel` is **always** valid (cancelling a dialog can
    /// never be vetoed); otherwise defer to the embedded group (`TGroup::valid`,
    /// which aggregates the children — the future `cmCanCloseForm` veto lands
    /// here via a validating control, deferred).
    fn valid(&self, cmd: Command) -> bool {
        if cmd == Command::CANCEL {
            true
        } else {
            self.window.valid(cmd)
        }
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.window.set_state(flag, enable, ctx);
    }

    fn awaken(&mut self) {
        self.window.awaken();
    }

    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        self.window.size_limits(owner_size)
    }

    // NOTE: `calc_bounds` is deliberately NOT overridden (mirrors `Window`): the
    // trait default routes through `Dialog::size_limits` -> `Window::size_limits`
    // (the 16x6 floor), so an owner-driven resize still honours the window
    // minimum. Delegating to `Window::calc_bounds` is impossible (it is not on the
    // trait surface as a delegate); the default is exactly right here.

    fn change_bounds(&mut self, bounds: Rect) {
        self.window.change_bounds(bounds);
    }

    fn cursor_request(&self) -> Option<Point> {
        self.window.cursor_request()
    }

    fn find_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.window.find_mut(id)
    }

    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        self.window.remove_descendant(id, ctx)
    }

    /// Delegate focus-by-id into the embedded window (a dialog's labels + controls
    /// live in its group), so a `TLabel` inside a dialog can focus its link.
    fn focus_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        self.window.focus_descendant(id, ctx)
    }

    /// `TDialog` is constructed with `wnNoNumber`, so `Window::number` already
    /// returns `None`; delegate for faithfulness.
    fn number(&self) -> Option<i16> {
        self.window.number()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{KeyEvent, KeyModifiers};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::timer::TimerQueue;
    use crate::view::Deferred;
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
    /// the group for `cmCancel` but defers to it for other commands.
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
        fn valid(&self, _cmd: Command) -> bool {
            false
        }
    }

    // -- 1. ctor -------------------------------------------------------------

    #[test]
    fn new_ports_dialog_ctor_defaults() {
        let d = Dialog::new(Rect::new(0, 0, 40, 12), Some("Setup".into()));
        // flags = wfMove | wfClose (NOT grow, NOT zoom).
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
        // palette = Gray.
        assert_eq!(d.window.palette(), WindowPalette::Gray);
        // wnNoNumber -> number None.
        assert_eq!(View::number(&d), None, "wnNoNumber -> no number");
    }

    /// The frame shows **no zoom icon and no number** (the flags-pushed-to-frame
    /// check). Inherited blue frame is fine (gray theming deferred).
    #[test]
    fn dialog_frame_has_no_zoom_icon_no_number_snapshot() {
        let theme = Theme::classic_blue();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut d = Dialog::new(Rect::new(0, 0, 24, 8), Some("Setup".into()));
        // Select -> active frame (double-line border + icons), so the absence of a
        // zoom icon is meaningful (an active wfZoom window would show one).
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

    // -- 2. Esc posts cmCancel -----------------------------------------------

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

    // -- 3. Enter broadcasts cmDefault ---------------------------------------

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
        assert!(ev.is_nothing(), "Enter consumed (clearEvent)");
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast {
                    command: Command::DEFAULT,
                    source: None
                }
            )),
            "Enter broadcasts cmDefault with no subject view"
        );
    }

    // -- 4. cmOK/cmCancel end the modal iff sfModal --------------------------

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
        assert!(ev.is_nothing(), "cmOK consumed while modal");
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::OK))),
            "cmOK while sfModal queues EndModal(OK)"
        );
    }

    #[test]
    fn ok_does_not_end_modal_when_not_modal() {
        let mut d = Dialog::new(Rect::new(0, 0, 30, 10), Some("D".into()));
        // sfModal NOT set.
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

        // cmCancel bypasses the child and is always valid.
        assert!(
            View::valid(&d, Command::CANCEL),
            "cmCancel always valid (cannot be vetoed)"
        );
        // Any other command defers to the group, which is false here.
        assert!(
            !View::valid(&d, Command::OK),
            "other command defers to the group (an invalid child vetoes)"
        );
    }
}
