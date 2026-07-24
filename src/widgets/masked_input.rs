//! A masked text field with a reveal eye: an [`InputLine`] (masked) plus a
//! [`RevealEye`](super::RevealEye) in its last column, bundled as one group so
//! the eye is its own Tab stop and reveal is wired internally.

use crate::data::FieldValue;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Group, Rect, View, ViewId, ViewState};
use crate::widgets::{InputLine, RevealEye, RevealEyeConfig};

/// See the module docs.
pub struct MaskedInput {
    group: Group,
    input_id: ViewId,
    eye_id: ViewId,
}

impl MaskedInput {
    /// Build a masked field of `bounds` with byte cap `limit`, echo char `mask`,
    /// and reveal-eye config `cfg`. The input takes all but the last column; the
    /// eye takes the last column.
    pub fn new(bounds: Rect, limit: i32, mask: char, cfg: RevealEyeConfig) -> Self {
        let w = bounds.b.x - bounds.a.x;
        let h = bounds.b.y - bounds.a.y;
        let mut group = Group::new(bounds);

        // MUST be inserted before the eye: `value()` reads the input
        // positionally via `gather_data().next()`, which relies on the input
        // being the first child in the group.
        let mut input = InputLine::with_limit(Rect::new(0, 0, (w - 1).max(1), h), limit);
        input.set_mask(Some(mask));
        let input_id = group.insert(Box::new(input));

        let eye = RevealEye::new(Rect::new((w - 1).max(1), 0, w, h), cfg);
        let eye_id = group.insert(Box::new(eye));

        MaskedInput {
            group,
            input_id,
            eye_id,
        }
    }

    fn input_mut(&mut self) -> Option<&mut InputLine> {
        self.group
            .child_mut(self.input_id)?
            .as_any_mut()?
            .downcast_mut::<InputLine>()
    }
    fn eye_mut(&mut self) -> Option<&mut RevealEye> {
        self.group
            .child_mut(self.eye_id)?
            .as_any_mut()?
            .downcast_mut::<RevealEye>()
    }

    /// The real (unmasked) field value.
    ///
    /// Deliberately takes `&self` rather than `&mut self`: [`View::value`]
    /// also has the name `value` with a `&self` receiver, and Rust's method
    /// resolution picks the first receiver-borrow level (by value, `&self`,
    /// `&mut self`, in that order) at which *any* applicable method exists —
    /// inherent methods win ties at the same level, but a `&mut self` inherent
    /// method never gets a chance to compete against a `&self` trait method,
    /// since the trait method is found first. Matching `View::value`'s `&self`
    /// receiver keeps `mi.value()` resolving to this inherent impl (verified:
    /// with a `&mut self` signature here, `mi.value()` silently called the
    /// `View` trait's default `None` instead). [`Group::gather_data`] conveniently
    /// offers the read in `&self` form (input is the first-inserted, hence
    /// first, child).
    pub fn value(&self) -> Option<FieldValue> {
        self.group.gather_data().into_iter().next().flatten()
    }
    /// Set the field value.
    pub fn set_value(&mut self, v: FieldValue) {
        if let Some(i) = self.input_mut() {
            i.set_value(v);
        }
    }

    #[cfg(test)]
    fn input_ref(&mut self) -> &InputLine {
        self.input_mut().expect("input child")
    }
    #[cfg(test)]
    fn input_bounds(&mut self) -> Rect {
        self.input_mut().expect("input child").state().get_bounds()
    }
}

#[crate::delegate(to = group)]
impl View for MaskedInput {
    fn state(&self) -> &ViewState {
        self.group.state()
    }
    fn state_mut(&mut self) -> &mut ViewState {
        self.group.state_mut()
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    // Without these, the `#[delegate]` macro would auto-forward `value`/
    // `set_value` to `self.group.value()`/`self.group.set_value()` — and
    // `Group` itself doesn't override either (both default to the `View`
    // trait's no-op), so a `MaskedInput` embedded as a `Box<dyn View>` in an
    // enclosing dialog's `Group` (the normal case: `gather_data`/`scatter_data`
    // drive a dialog's OK/scatter passes through exactly that trait-object
    // path) would silently gather `None` and ignore `scatter_data`, instead of
    // reading/writing the real password. Forwarding to the inherent
    // `value`/`set_value` here (verified not to recurse — method resolution is
    // by the receiver's static type, not by which impl block the call sits in)
    // keeps both the direct `mi.value()` call and the trait-object path
    // consistent.
    fn value(&self) -> Option<FieldValue> {
        self.value()
    }
    fn set_value(&mut self, v: FieldValue) {
        self.set_value(v);
    }
    // `Group::scatter_data` (a dialog's normal scatter pass) calls
    // `set_value_ctx`, not `set_value` — same hazard as above: left to the
    // macro, it forwards to `self.group.set_value_ctx(v, ctx)`, whose default
    // body calls `Group`'s own (no-op) `set_value`, never reaching this type's
    // real field. Override it too, ignoring `ctx` exactly as the trait's
    // default does (this field needs no `Context` to update its text).
    fn set_value_ctx(&mut self, v: FieldValue, ctx: &mut Context) {
        let _ = ctx;
        self.set_value(v);
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Active-line only: the eye shows while this field has focus.
        let focused = self.group.state().state.focused;
        if let Some(eye) = self.eye_mut() {
            eye.state_mut().state.visible = focused;
        }
        self.group.draw(ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Group routes the event to the focused child (input caret or eye),
        // handles Tab between them, and fans Timer/Broadcast to both.
        self.group.handle_event(ev, ctx);
        // Re-sync the input's reveal from the eye after every event. The eye
        // broadcasts REVEAL_CHANGED on any intent change, so this pass runs
        // promptly even for a capture-delivered MouseUp.
        let revealing = self.eye_mut().map(|e| e.is_revealing()).unwrap_or(false);
        if let Some(input) = self.input_mut() {
            input.set_reveal(revealing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel};
    use crate::timer::TimerQueue;
    use crate::view::Deferred;
    use crate::{Event, FieldValue, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(mi: &mut MaskedInput, f: impl FnOnce(&mut MaskedInput, &mut Context) -> R) -> R {
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(mi, &mut ctx)
    }

    fn built() -> MaskedInput {
        let mut mi = MaskedInput::new(Rect::new(0, 0, 12, 1), 64, '•', RevealEyeConfig::default());
        // Give real ids as Group::insert would in a live tree, then seed a value.
        mi.set_value(FieldValue::Text("pw".into()));
        mi
    }

    #[test]
    fn value_returns_the_real_text() {
        let mi = built();
        assert_eq!(mi.value(), Some(FieldValue::Text("pw".into())));
    }

    /// Regression: a dialog gathers/scatters its fields through `Group`'s
    /// `gather_data`/`scatter_data`, which call `View::value`/`set_value`
    /// through the `Box<dyn View>` vtable — a different path than calling
    /// `mi.value()` directly on the concrete type. Without the explicit
    /// `value`/`set_value` overrides in `impl View for MaskedInput` (that
    /// forward to the inherent methods), the `#[delegate]` macro would instead
    /// forward to `Group`'s own (no-op) `View::value`/`set_value`, and an
    /// embedded `MaskedInput` would silently gather `None` / ignore scatter.
    #[test]
    fn value_round_trips_through_the_view_trait_object() {
        let bounds = Rect::new(0, 0, 20, 5);
        let mut host = Group::new(bounds);
        let mi_id = host.insert(Box::new(built()));

        let gathered = host.gather_data();
        assert_eq!(
            gathered,
            vec![Some(FieldValue::Text("pw".into()))],
            "gather_data must read the real text through the Box<dyn View> path"
        );

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        host.scatter_data(&[Some(FieldValue::Text("newpw".into()))], &mut ctx);

        let mi = host
            .child_mut(mi_id)
            .unwrap()
            .as_any_mut()
            .unwrap()
            .downcast_mut::<MaskedInput>()
            .unwrap();
        assert_eq!(
            mi.value(),
            Some(FieldValue::Text("newpw".into())),
            "scatter_data must write through the Box<dyn View> path too"
        );
    }

    #[test]
    fn eye_press_reveals_the_paired_input() {
        let mut mi = built();
        // Press the eye (its own cell = last column). Route through the group.
        let down = Event::MouseDown(MouseEvent {
            position: Point::new(11, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        });
        with_ctx(&mut mi, |mi, ctx| {
            let mut d = down;
            mi.handle_event(&mut d, ctx);
        });
        assert!(mi.input_ref().reveal, "holding the eye reveals the input");

        let up = Event::MouseUp(MouseEvent {
            position: Point::new(11, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        });
        with_ctx(&mut mi, |mi, ctx| {
            let mut u = up;
            mi.handle_event(&mut u, ctx);
        });
        assert!(!mi.input_ref().reveal, "releasing hides the input again");
    }

    #[test]
    fn input_occupies_all_but_the_last_column() {
        let mut mi = built();
        let ib = mi.input_bounds();
        assert_eq!(
            ib,
            Rect::new(0, 0, 11, 1),
            "input is width-1, eye holds the last column"
        );
    }
}
