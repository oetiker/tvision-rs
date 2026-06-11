//! Proves `#[delegate(to = inner)]` injects a working forwarder for every
//! currently-known `View` method.
//!
//! The empty `impl View for D {}` compiling at all proves every one of the
//! 27 generated signatures matches the trait exactly. The behavioral assertions
//! prove completeness: no forwarder silently missing.

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};

use tvision::backend::{HeadlessBackend, Renderer};
use tvision::delegate;
use tvision::screen::Buffer;
use tvision::theme::Theme;
use tvision::timer::TimerQueue;
use tvision::view::Deferred;
use tvision::{
    Command, CommandSet, Context, DrawCtx, Event, FieldValue, HelpCtx, Point, Rect, StateFlag,
    View, ViewId, ViewState,
};

// ---------------------------------------------------------------------------
// Spy — records every method call it receives.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Spy {
    st: ViewState,
    seen: RefCell<HashSet<&'static str>>,
}

impl Spy {
    fn mark(&self, m: &'static str) {
        self.seen.borrow_mut().insert(m);
    }
}

impl View for Spy {
    fn state(&self) -> &ViewState {
        self.mark("state");
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        self.seen.borrow_mut().insert("state_mut");
        &mut self.st
    }
    fn draw(&mut self, _ctx: &mut DrawCtx) {
        self.mark("draw");
    }
    fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) {
        self.mark("handle_event");
    }
    fn set_state(&mut self, _f: StateFlag, _e: bool, _ctx: &mut Context) {
        self.mark("set_state");
    }
    fn valid(&mut self, _c: Command, _ctx: &mut Context) -> bool {
        self.mark("valid");
        true
    }
    fn set_modal_answer(&mut self, _c: Command) {
        self.mark("set_modal_answer");
    }
    fn value(&self) -> Option<FieldValue> {
        self.mark("value");
        None
    }
    fn set_value(&mut self, _v: FieldValue) {
        self.mark("set_value");
    }
    fn awaken(&mut self) {
        self.mark("awaken");
    }
    fn size_limits(&self, o: Point) -> (Point, Point) {
        self.mark("size_limits");
        (o, o)
    }
    fn calc_bounds(&mut self, _o: Point, _d: Point) -> Rect {
        self.mark("calc_bounds");
        Rect::new(0, 0, 0, 0)
    }
    fn change_bounds(&mut self, _b: Rect) {
        self.mark("change_bounds");
    }
    fn cursor_request(&self) -> Option<Point> {
        self.mark("cursor_request");
        None
    }
    fn find_mut(&mut self, _id: ViewId) -> Option<&mut dyn View> {
        self.mark("find_mut");
        None
    }
    fn remove_descendant(&mut self, _id: ViewId, _ctx: &mut Context) -> bool {
        self.mark("remove_descendant");
        false
    }
    fn focus_descendant(&mut self, _id: ViewId, _ctx: &mut Context) -> bool {
        self.mark("focus_descendant");
        false
    }
    fn settle_currency(&mut self, _ctx: &mut Context) {
        self.mark("settle_currency");
    }
    fn set_visible_descendant(&mut self, _id: ViewId, _visible: bool, _ctx: &mut Context) -> bool {
        self.mark("set_visible_descendant");
        false
    }
    fn number(&self) -> Option<i16> {
        self.mark("number");
        None
    }
    fn grabs_focus_on_click(&self) -> bool {
        self.mark("grabs_focus_on_click");
        true
    }
    fn select_window_num(&mut self, _n: i16, _ctx: &mut Context) -> bool {
        self.mark("select_window_num");
        false
    }
    fn tile(&mut self, _r: Rect) {
        self.mark("tile");
    }
    fn cascade(&mut self, _r: Rect) {
        self.mark("cascade");
    }
    fn apply_list_scroll(&mut self, _h: Option<i32>, _v: Option<i32>, _ctx: &mut Context) {
        self.mark("apply_list_scroll");
    }
    fn update_menu_commands(&mut self, _cs: &CommandSet) {
        self.mark("update_menu_commands");
    }
    fn set_menu_current(&mut self, _current: Option<usize>) {
        self.mark("set_menu_current");
    }
    fn get_help_ctx(&self) -> HelpCtx {
        self.mark("get_help_ctx");
        HelpCtx::NO_CONTEXT
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.mark("as_any_mut");
        None
    }
    fn descendant_global_bounds(&self, _id: ViewId, _acc: Point) -> Option<Rect> {
        self.mark("descendant_global_bounds");
        None
    }
}

// ---------------------------------------------------------------------------
// D — pure delegator: empty impl, macro injects ALL 27 forwarders.
// ---------------------------------------------------------------------------

struct D {
    inner: Spy,
}

#[delegate(to = inner)]
impl View for D {}

// ---------------------------------------------------------------------------
// Main test: call every method and assert the spy recorded each one.
// ---------------------------------------------------------------------------

/// Build a minimal Context (same pattern as src/capture.rs tests).
fn make_ctx<'a>(
    out: &'a mut VecDeque<Event>,
    timers: &'a mut TimerQueue,
    deferred: &'a mut Vec<Deferred>,
) -> Context<'a> {
    Context::new(out, timers, 0, deferred)
}

#[test]
fn delegate_forwards_every_known_view_method() {
    let mut d = D {
        inner: Spy::default(),
    };

    // -- state / state_mut --------------------------------------------------
    let _ = d.state();
    let _ = d.state_mut();

    // -- handle_event -------------------------------------------------------
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        let mut ev = Event::Nothing;
        d.handle_event(&mut ev, &mut ctx);
    }

    // -- set_state ----------------------------------------------------------
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        d.set_state(StateFlag::Active, true, &mut ctx);
    }

    // -- valid --------------------------------------------------------------
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        let _ = d.valid(Command::OK, &mut ctx);
    }

    // -- set_modal_answer ---------------------------------------------------
    d.set_modal_answer(Command::YES);

    // -- value / set_value --------------------------------------------------
    let _ = d.value();
    d.set_value(FieldValue::Text(String::new()));

    // -- awaken -------------------------------------------------------------
    d.awaken();

    // -- size_limits --------------------------------------------------------
    let _ = d.size_limits(Point::new(80, 25));

    // -- calc_bounds --------------------------------------------------------
    let _ = d.calc_bounds(Point::new(80, 25), Point::new(0, 0));

    // -- change_bounds ------------------------------------------------------
    d.change_bounds(Rect::new(0, 0, 10, 5));

    // -- cursor_request -----------------------------------------------------
    let _ = d.cursor_request();

    // -- find_mut / remove_descendant / focus_descendant -------------------
    let id = ViewId::next();
    let _ = d.find_mut(id);

    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        let _ = d.remove_descendant(id, &mut ctx);
    }

    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        let _ = d.focus_descendant(id, &mut ctx);
    }

    // -- settle_currency / set_visible_descendant -----------------------------
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        d.settle_currency(&mut ctx);
        let _ = d.set_visible_descendant(id, false, &mut ctx);
    }

    // -- number / grabs_focus_on_click --------------------------------------
    let _ = d.number();
    let _ = d.grabs_focus_on_click();

    // -- select_window_num --------------------------------------------------
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        let _ = d.select_window_num(1, &mut ctx);
    }

    // -- tile / cascade -----------------------------------------------------
    d.tile(Rect::new(0, 0, 80, 24));
    d.cascade(Rect::new(0, 0, 80, 24));

    // -- apply_list_scroll --------------------------------------------------
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
        d.apply_list_scroll(Some(0), Some(0), &mut ctx);
    }

    // -- update_menu_commands -----------------------------------------------
    d.update_menu_commands(&CommandSet::new());

    // -- set_menu_current ---------------------------------------------------
    d.set_menu_current(Some(0));

    // -- get_help_ctx -------------------------------------------------------
    let _ = d.get_help_ctx();

    // -- as_any_mut ---------------------------------------------------------
    let _ = d.as_any_mut();

    // -- descendant_global_bounds -------------------------------------------
    let _ = d.descendant_global_bounds(id, Point::new(0, 0));

    // -- draw (needs a DrawCtx; use the HeadlessBackend pattern) -----------
    {
        let theme = Theme::classic_blue();
        let (backend, _screen) = HeadlessBackend::new(10, 5);
        let mut r = Renderer::new(Box::new(backend));
        // Capture `d` into the closure by reborrow.
        let d_ref = &mut d;
        r.render(|buf: &mut Buffer| {
            let mut dc = DrawCtx::new(buf, &theme, Rect::new(0, 0, 10, 5), Point::new(0, 0));
            d_ref.draw(&mut dc);
        });
    }

    // -- Assert every method was reached ------------------------------------
    let seen = d.inner.seen.borrow();
    // MAINTENANCE: keep in sync with trait View's methods and
    // tvision-macros/src/specs.rs (`view()`). See the note in view.rs.
    let expected: &[&str] = &[
        "state",
        "state_mut",
        "draw",
        "handle_event",
        "set_state",
        "valid",
        "set_modal_answer",
        "value",
        "set_value",
        "awaken",
        "size_limits",
        "calc_bounds",
        "change_bounds",
        "cursor_request",
        "find_mut",
        "remove_descendant",
        "focus_descendant",
        "settle_currency",
        "set_visible_descendant",
        "number",
        "grabs_focus_on_click",
        "select_window_num",
        "tile",
        "cascade",
        "apply_list_scroll",
        "update_menu_commands",
        "set_menu_current",
        "get_help_ctx",
        "as_any_mut",
        "descendant_global_bounds",
    ];
    for m in expected {
        assert!(
            seen.contains(*m),
            "spy did not record a hit for `{m}` — forwarder missing or not called"
        );
    }
}

// ---------------------------------------------------------------------------
// Skipper test: `number` is listed in skip(...) — must NOT be forwarded.
// ---------------------------------------------------------------------------

struct Skipper {
    inner: Spy,
}

// `number` is skipped: not provided, not forwarded → uses the View default (None).
#[delegate(to = inner, skip(number))]
impl View for Skipper {}

#[test]
fn skip_leaves_method_at_trait_default() {
    let s = Skipper {
        inner: Spy::default(),
    };
    // The trait default for `number()` is `None`.
    assert_eq!(s.number(), None);
    // The spy must NOT have been called (we got the default, not a forwarder).
    assert!(
        !s.inner.seen.borrow().contains("number"),
        "skip(number) must suppress the forwarder; spy must not be reached"
    );
}
