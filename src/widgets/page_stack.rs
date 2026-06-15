//! A content multiplexer: a group of child "page" Views of which exactly one is
//! visible at a time. Paired with a [`TabBar`](crate::widgets::TabBar): the bar
//! broadcasts [`Command::TAB_BAR_CHANGED`], this stack (bound to the bar's id)
//! switches the visible page through the pump broker — mirroring how a
//! [`Scroller`](crate::widgets::Scroller) reacts to its `ScrollBar`.
//!
//! # Turbo Vision heritage
//! None — classic Turbo Vision has no notebook/tab-page container, only `TGroup`
//! + `sfVisible`/`show()`/`hide()`. `PageStack` packages exactly that.

use crate::command::Command;
use crate::event::Event;
use crate::view::{Context, Group, Rect, View, ViewId};

/// A stack of page Views showing one at a time. See the [module docs](self).
pub struct PageStack {
    group: Group,
    pages: Vec<ViewId>,
    active: usize,
    /// The bound TabBar id; a `TAB_BAR_CHANGED` from it triggers a page switch.
    tab_bar: Option<ViewId>,
}

impl PageStack {
    /// Empty stack at `bounds`.
    pub fn new(bounds: Rect) -> Self {
        PageStack {
            group: Group::new(bounds),
            pages: Vec::new(),
            active: 0,
            tab_bar: None,
        }
    }

    /// Bind the `TabBar` whose broadcasts drive this stack.
    pub fn bind_tab_bar(&mut self, id: ViewId) {
        self.tab_bar = Some(id);
    }

    /// Insert a page; returns its id. All pages after the first start hidden.
    /// Lay each page at the stack's full local extent before inserting.
    pub fn insert_page(&mut self, view: Box<dyn View>) -> ViewId {
        let id = self.group.insert(view);
        self.pages.push(id);
        if self.pages.len() > 1
            && let Some(v) = self.group.child_mut(id)
        {
            v.state_mut().state.visible = false;
        }
        id
    }

    /// The active page index.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Whether the page with `id` is currently visible. Returns `false` if the
    /// id is not a direct child of this stack's group. Test-only assertion
    /// helper (used by this module's unit tests and the program-level
    /// integration test).
    #[cfg(test)]
    pub(crate) fn page_visible(&mut self, id: ViewId) -> bool {
        self.group
            .child_mut(id)
            .map(|v| v.state().state.visible)
            .unwrap_or(false)
    }

    /// Show page `idx`, hide the rest, move focus to it.
    pub fn set_active(&mut self, idx: usize, ctx: &mut Context) {
        if idx >= self.pages.len() {
            return;
        }
        // Copy ids to avoid borrowing self.pages and self.group simultaneously.
        let pages = self.pages.clone();
        for (i, &pid) in pages.iter().enumerate() {
            self.group.set_visible_descendant(pid, i == idx, ctx);
        }
        self.group.focus_child(self.pages[idx], ctx);
        self.active = idx;
    }
}

#[crate::delegate(to = group, skip(as_any_mut, handle_event))]
impl View for PageStack {
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// React to the bound `TabBar`'s broadcast by queuing a pump sync; then route
    /// the event into the group as usual.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // `source.is_some()` guards against an unbound stack (tab_bar == None)
        // matching its own None via `source == self.tab_bar` (None == None).
        // We then read the bound id off `self.tab_bar` (mirroring the scroller,
        // which passes its bound `h/v_scroll_bar` rather than re-unwrapping
        // `source`) — keeping `request_sync_page_stack`'s `tab_bar: ViewId`.
        if let Event::Broadcast { command, source } = *ev
            && command == Command::TAB_BAR_CHANGED
            && source.is_some()
            && source == self.tab_bar
            && let Some(tab_id) = self.tab_bar
            && let Some(ps_id) = self.group.state().id()
        {
            ctx.request_sync_page_stack(ps_id, tab_id);
        }
        self.group.handle_event(ev, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::TimerQueue;
    use crate::view::{Deferred, DrawCtx, ViewState};
    use std::collections::VecDeque;

    struct Page {
        st: ViewState,
    }

    impl Page {
        fn boxed(b: Rect) -> Box<dyn View> {
            let mut st = ViewState::new(b);
            st.options.selectable = true;
            Box::new(Page { st })
        }
    }

    impl View for Page {
        fn state(&self) -> &ViewState {
            &self.st
        }

        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }

        fn draw(&mut self, _c: &mut DrawCtx) {}
    }

    fn ctx_run<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut d: Vec<Deferred> = vec![];
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut d);
        f(&mut ctx)
    }

    #[test]
    fn first_page_visible_rest_hidden_after_insert() {
        let mut ps = PageStack::new(Rect::new(0, 0, 20, 10));
        let p0 = ps.insert_page(Page::boxed(Rect::new(0, 0, 20, 10)));
        let p1 = ps.insert_page(Page::boxed(Rect::new(0, 0, 20, 10)));
        assert!(ps.page_visible(p0), "first page is visible");
        assert!(!ps.page_visible(p1), "second page is hidden");
    }

    #[test]
    fn set_active_shows_one_hides_rest() {
        let mut ps = PageStack::new(Rect::new(0, 0, 20, 10));
        let p0 = ps.insert_page(Page::boxed(Rect::new(0, 0, 20, 10)));
        let p1 = ps.insert_page(Page::boxed(Rect::new(0, 0, 20, 10)));
        ctx_run(|ctx| ps.set_active(1, ctx));
        assert_eq!(ps.active(), 1);
        assert!(
            !ps.page_visible(p0),
            "page 0 hidden after switching to page 1"
        );
        assert!(ps.page_visible(p1), "page 1 visible after switching to it");
    }

    #[test]
    fn set_active_out_of_range_is_no_op() {
        let mut ps = PageStack::new(Rect::new(0, 0, 20, 10));
        ps.insert_page(Page::boxed(Rect::new(0, 0, 20, 10)));
        ctx_run(|ctx| ps.set_active(5, ctx));
        assert_eq!(ps.active(), 0, "out-of-range index leaves active unchanged");
    }

    #[test]
    fn handle_event_queues_page_stack_sync_on_tab_bar_changed() {
        use crate::command::Command;
        use crate::view::ViewId;

        let bounds = Rect::new(0, 0, 20, 10);
        // Build a group to host the PageStack so it gets an id.
        let mut group = Group::new(bounds);
        let mut ps = PageStack::new(bounds);
        ps.insert_page(Page::boxed(bounds));
        ps.insert_page(Page::boxed(bounds));

        // Mint a fake tab_bar id (non-null).
        let fake_tab_bar = ViewId::next();
        ps.bind_tab_bar(fake_tab_bar);

        let ps_id = group.insert(Box::new(ps));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::TAB_BAR_CHANGED,
                source: Some(fake_tab_bar),
            };
            group
                .find_mut(ps_id)
                .unwrap()
                .handle_event(&mut ev, &mut ctx);
        }
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::PageStackSync { .. })),
            "TAB_BAR_CHANGED from bound tab_bar must queue PageStackSync"
        );
    }

    #[test]
    fn handle_event_ignores_tab_bar_changed_from_unknown_source() {
        use crate::command::Command;
        use crate::view::ViewId;

        let bounds = Rect::new(0, 0, 20, 10);
        let mut group = Group::new(bounds);
        let mut ps = PageStack::new(bounds);
        ps.insert_page(Page::boxed(bounds));

        // Bind to one id but broadcast from a different one.
        let bound_bar = ViewId::next();
        let other_bar = ViewId::next();
        ps.bind_tab_bar(bound_bar);

        let ps_id = group.insert(Box::new(ps));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::TAB_BAR_CHANGED,
                source: Some(other_bar),
            };
            group
                .find_mut(ps_id)
                .unwrap()
                .handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !deferred
                .iter()
                .any(|d| matches!(d, Deferred::PageStackSync { .. })),
            "broadcast from a different source must NOT queue PageStackSync"
        );
    }

    // -----------------------------------------------------------------------
    // Snapshot tests — show-one / hide-rest at the pixel level
    // -----------------------------------------------------------------------

    /// Build a 20×3 `PageStack` with two `StaticText` pages ("PAGE ZERO" /
    /// "PAGE ONE"), switch to `active`, render through a `HeadlessBackend`, and
    /// return the snapshot. Only the active page's text must appear.
    fn render(active: usize) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::widgets::StaticText;

        let theme = Theme::classic_blue();
        let bounds = Rect::new(0, 0, 20, 3);
        let mut ps = PageStack::new(bounds);
        ps.insert_page(Box::new(StaticText::new(bounds, "PAGE ZERO")));
        ps.insert_page(Box::new(StaticText::new(bounds, "PAGE ONE")));
        // Switch via the public API (needs a Context for the focus/visibility ops).
        ctx_run(|ctx| ps.set_active(active, ctx));

        let (backend, screen) = HeadlessBackend::new(20, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            ps.draw(&mut DrawCtx::new(buf, &theme, bounds, bounds.a));
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_first_page_active() {
        insta::assert_snapshot!(render(0));
    }

    #[test]
    fn snapshot_second_page_active() {
        insta::assert_snapshot!(render(1));
    }
}
