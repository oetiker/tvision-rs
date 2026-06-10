//! `TDialog` — see the [module docs](super) for the deviation summary.

use crate::command::Command;
use crate::event::{Event, Key};
use crate::view::{Context, GrowMode, Rect, View, ViewId};
// These are used only by the test module (via `use super::*`).
#[cfg(test)]
use crate::view::{DrawCtx, StateFlag, ViewState};
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

    /// Insert a child view into the dialog's embedded window/group.
    ///
    /// Mirrors [`Window::insert_child`]; used by the row-34 `exec_view` veto
    /// test and by the row-63 msgbox (first intended production consumer).
    /// Previously `#[cfg(test)]`; promoted to `pub(crate)` for symmetry with
    /// `Window::insert_child` and to unblock msgbox 63.
    #[allow(dead_code)] // production consumer: msgbox 63 (not yet ported)
    pub(crate) fn insert_child(&mut self, view: Box<dyn View>) -> ViewId {
        self.window.insert_child(view)
    }

    /// Reach a direct child of the dialog's embedded window/group by id.
    ///
    /// Mirrors [`Window::child_mut`]; used by `FileDialog` (row 79) to run a
    /// child's post-insert, ctx-bearing init (e.g. `FileList::read_directory`)
    /// and to read it back via `as_any_mut` + downcast.
    pub(crate) fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.window.child_mut(id)
    }

    /// Override the decoration flags after construction.
    ///
    /// Mirrors [`Window::set_flags`]; used by `FileDialog` (row 79) and
    /// `ChDirDialog` (row 80) to add `wfGrow` on top of the Dialog defaults
    /// (`wfMove | wfClose`). Re-pushes to the frame child so the grow handle
    /// draws immediately.
    pub(crate) fn set_flags(&mut self, flags: crate::window::WindowFlags) {
        self.window.set_flags(flags);
    }

    /// Read the current decoration flags.
    ///
    /// Mirrors [`Window::flags`]; exposed so `FileDialog` / `ChDirDialog` tests
    /// can assert the `wfGrow` bit is set post-construction.
    pub(crate) fn flags(&self) -> crate::window::WindowFlags {
        self.window.flags()
    }
}

#[crate::delegate(
    to = window,
    skip(
        apply_list_scroll,
        as_any_mut,
        calc_bounds,
        grabs_focus_on_click,
        select_window_num,
        set_value,
        value
    )
)]
impl View for Dialog {
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
        fn valid(&mut self, _cmd: Command, _ctx: &mut Context) -> bool {
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
        // palette = Gray — AND pushed down into the frame child (row 34 gray
        // theming: the frame renders the FrameGray* role family).
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
    /// check). The frame renders the gray dialog scheme (row 34 gray theming).
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

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        // cmCancel bypasses the child and is always valid.
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
}
