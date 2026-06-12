//! Injected clock and timer queue.
//!
//! A [`TimerQueue`] holds pending one-shot and periodic timers; the event loop
//! asks it which timers are due and how long until the next one. Time comes
//! from an injected [`Clock`]: [`SystemClock`] in production, [`ManualClock`]
//! in tests. **`Instant::now()` is only called inside [`SystemClock`]** — all
//! timer logic operates on the opaque `u64` millisecond values the clock
//! yields, so tests can advance time deterministically without sleeps.
//!
//! Two structural choices differ from the original:
//!
//! 1. **No re-entrancy dance.** [`TimerQueue::collect_expired`] gathers due ids
//!    into a `Vec` and returns them; the caller dispatches afterwards, so there
//!    is no re-entrant mutation of the queue to guard against. Two invariants
//!    are preserved: a single `now_ms` value is used for the whole pass, and a
//!    periodic timer fires at most once per `collect_expired` call (even if
//!    overdue by several periods — it reschedules forward past `now_ms` via
//!    [`calc_next_expires_at`]).
//!
//! 2. **The clock lives in the event loop, not the queue.** `now_ms` is passed
//!    into [`TimerQueue::collect_expired`], [`TimerQueue::time_until_next`], and
//!    [`TimerQueue::set_timer`] by the caller, which keeps the queue easy to
//!    test and gives `set_timer` the absolute expiry it needs for the
//!    reschedule grid arithmetic.
//!
//! # Turbo Vision heritage
//!
//! Ports `TTimerQueue` (`ttimerqu.cpp`, `system.h`) and its time source
//! `THardwareInfo::getTickCountMs`. The original marked each timer with a
//! per-invocation collect-id so a callback could mutate the list mid-iteration,
//! and stored the clock inside the queue; rstv replaces the callback with a
//! returned id list and injects the clock from the event loop (deviations D9 and
//! D11).

use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Clock — the injected time source
// ---------------------------------------------------------------------------

/// Monotonic millisecond clock.
///
/// Two implementations are provided: [`SystemClock`] for production and
/// [`ManualClock`] for deterministic tests.  **Only [`SystemClock`] may call
/// `Instant::now()`** — all other code receives `u64` ms values from the
/// injected clock.
///
/// # Turbo Vision heritage
///
/// Faithful to `TTimePoint` (a `uint64_t` ms tick count) and
/// `THardwareInfo::getTickCountMs`. The injected clock makes time
/// deterministic in tests (deviation D11).
pub trait Clock {
    /// Monotonic milliseconds since some fixed epoch.
    fn now_ms(&self) -> u64;
}

// ---------------------------------------------------------------------------
// SystemClock — production implementation
// ---------------------------------------------------------------------------

/// Production clock. Captures an `Instant` at construction and returns elapsed
/// ms on each call.
///
/// `Instant::now()` is allowed **only here** — everything else in the timer
/// subsystem takes a `u64` from the caller.
pub struct SystemClock {
    base: Instant,
}

impl SystemClock {
    /// Create a new `SystemClock` whose epoch is now.
    pub fn new() -> Self {
        SystemClock {
            base: Instant::now(), // sole allowed call to Instant::now()
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.base.elapsed().as_millis() as u64
    }
}

// ---------------------------------------------------------------------------
// ManualClock — deterministic test clock
// ---------------------------------------------------------------------------

/// Test clock. Holds the current time in a [`Cell`] so `&self` can both read
/// the time (via the [`Clock`] trait) and advance it (via
/// [`set`](ManualClock::set) / [`advance`](ManualClock::advance)).
pub struct ManualClock {
    ms: Cell<u64>,
}

impl ManualClock {
    /// Create a manual clock at `start_ms`.
    pub fn new(start_ms: u64) -> Self {
        ManualClock {
            ms: Cell::new(start_ms),
        }
    }

    /// Overwrite the current time.
    pub fn set(&self, ms: u64) {
        self.ms.set(ms);
    }

    /// Advance the current time by `delta_ms`.
    pub fn advance(&self, delta_ms: u64) {
        self.ms.set(self.ms.get() + delta_ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.ms.get()
    }
}

/// Lets a shared [`ManualClock`] be injected (as `Box<Rc<ManualClock>>`) while a
/// test retains a clone to advance time — the boxed `dyn Clock` an event loop
/// owns is otherwise un-advanceable. `Clock` reads through `&self`, and
/// `ManualClock`'s interior [`Cell`] makes advancing through a shared `Rc`
/// sound on the single thread the event loop runs on. **Test-only** to keep the
/// public clock API minimal.
#[cfg(test)]
impl Clock for std::rc::Rc<ManualClock> {
    fn now_ms(&self) -> u64 {
        (**self).now_ms()
    }
}

// ---------------------------------------------------------------------------
// TimerId — a cancelable handle
// ---------------------------------------------------------------------------

/// A handle to a pending timer. Monotonically-increasing id allocated by
/// [`TimerQueue`]; `Copy + Eq + Hash` so callers can store it and compare it in
/// event handlers.
///
/// # Turbo Vision heritage
///
/// Faithful to `TTimerId` (`system.h`), which was a raw `TTimer*`; rstv uses an
/// opaque integer handle instead (no pointer arithmetic, and `u64` never
/// realistically exhausts).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TimerId(u64);

// ---------------------------------------------------------------------------
// Internal timer entry
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TimerEntry {
    /// Absolute ms when this timer next fires.
    expires_at: u64,
    /// `None` = one-shot; `Some(ms)` = periodic with this period. (The original
    /// used a signed sentinel — negative for one-shot — where rstv uses `Option`.)
    period_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// calc_next_expires_at
// ---------------------------------------------------------------------------

/// Catch-up-aware reschedule for periodic timers.
///
/// ```text
/// // Pre: expires_at <= now && period > 0
/// return (1 + (now - expires_at + period) / period) * period
///            + expires_at - period;
/// ```
///
/// The arithmetic advances `expires_at` to the next grid point strictly after
/// `now`, skipping any missed periods.  It is **not** a simple `expires_at +
/// period` — it aligns to the period grid even when many periods have elapsed.
///
/// # Overflow analysis
/// With u64 ms the product `(1 + …) * period` is at least `2 * period`. The
/// subsequent `+ expires_at - period` subtracts one period back, so the final
/// result equals `product + expires_at - period`.  Because `product >= period`,
/// `product + expires_at - period >= expires_at`, and wrapping cannot occur for
/// realistic ms timestamps.  **The `- period` must execute last** (not
/// `expires_at - period + product`) to avoid underflow when
/// `expires_at < period`.
///
/// # Panics
/// Panics in debug mode (and is undefined in release) if `period == 0`.
/// Callers must guarantee `period > 0` and `expires_at <= now`.
pub fn calc_next_expires_at(expires_at: u64, now: u64, period: u64) -> u64 {
    (1 + (now - expires_at + period) / period) * period + expires_at - period
}

// ---------------------------------------------------------------------------
// TimerQueue
// ---------------------------------------------------------------------------

/// A queue of pending one-shot and periodic timers.
///
/// [`collect_expired`](Self::collect_expired) returns the due timers as a
/// `Vec<TimerId>` for the caller to dispatch, and the current time `now_ms` is
/// passed in by the caller rather than stored here (see the module docs).
///
/// Durations are stored internally as `u64` milliseconds (truncated from
/// [`Duration`]).  **Sub-millisecond periods truncate to 0 and are rejected.**
///
/// # Turbo Vision heritage
///
/// Ports `TTimerQueue` (`ttimerqu.cpp`, `system.h`).
#[derive(Debug, Default)]
pub struct TimerQueue {
    timers: HashMap<TimerId, TimerEntry>,
    next_id: u64,
}

impl TimerQueue {
    /// Create an empty timer queue.
    pub fn new() -> Self {
        TimerQueue::default()
    }

    /// Arm a new timer.
    ///
    /// - `now_ms` — the current clock value at the moment of arming.  Must be
    ///   supplied by the caller (the clock lives in the event loop, not here).
    ///   Call `clock.now_ms()` immediately before `set_timer`.
    /// - `timeout` — delay until first expiry.
    /// - `period` — `None` for a one-shot; `Some(d)` for a repeating timer.
    ///   `d` must convert to at least 1 ms (sub-millisecond periods are
    ///   rejected with a `debug_assert`; in release they silently become
    ///   one-shot-like one-ms timers — document this in your integration code).
    ///
    /// Returns a [`TimerId`] that can be passed to [`kill_timer`](Self::kill_timer).
    pub fn set_timer(
        &mut self,
        now_ms: u64,
        timeout: Duration,
        period: Option<Duration>,
    ) -> TimerId {
        let timeout_ms = timeout.as_millis() as u64;
        let period_ms = period.map(|d| {
            let ms = d.as_millis() as u64;
            debug_assert!(
                ms > 0,
                "timer period must be at least 1 ms; sub-ms periods are unsupported"
            );
            ms
        });

        let id = TimerId(self.next_id);
        self.next_id += 1;

        self.timers.insert(
            id,
            TimerEntry {
                expires_at: now_ms + timeout_ms,
                period_ms,
            },
        );
        id
    }

    /// Cancel a pending timer.
    ///
    /// No-op if `id` is unknown or has already fired (one-shot).
    pub fn kill_timer(&mut self, id: TimerId) {
        self.timers.remove(&id);
    }

    /// Fire all timers due at `now_ms` and return their ids (in unspecified order).
    ///
    /// Instead of calling a user-supplied callback mid-iteration, it collects due
    /// ids into a [`Vec`] and returns them.  The caller then dispatches each id
    /// (posts a command, calls a callback, etc.).
    ///
    /// **Invariants:**
    /// - A single `now_ms` snapshot is used for the whole pass.
    /// - A periodic timer fires **at most once** per call, even if overdue by
    ///   several periods — it is rescheduled forward past `now_ms` via
    ///   [`calc_next_expires_at`].
    /// - One-shot timers are removed from the queue upon firing.
    pub fn collect_expired(&mut self, now_ms: u64) -> Vec<TimerId> {
        let mut due = Vec::new();

        // Collect due ids with their period first, then mutate — avoids
        // borrow-checker issues in a single pass.
        let mut to_reschedule: Vec<(TimerId, u64, u64)> = Vec::new(); // (id, expires_at, period_ms)
        let mut to_remove: Vec<TimerId> = Vec::new();

        for (&id, entry) in &self.timers {
            if entry.expires_at <= now_ms {
                due.push(id);
                match entry.period_ms {
                    Some(period_ms) if period_ms > 0 => {
                        to_reschedule.push((id, entry.expires_at, period_ms));
                    }
                    _ => {
                        to_remove.push(id);
                    }
                }
            }
        }

        // Reschedule periodic timers.
        for (id, expires_at, period_ms) in to_reschedule {
            if let Some(entry) = self.timers.get_mut(&id) {
                entry.expires_at = calc_next_expires_at(expires_at, now_ms, period_ms);
            }
        }

        // Remove one-shot timers.
        for id in to_remove {
            self.timers.remove(&id);
        }

        due
    }

    /// Milliseconds until the next timer expires.
    ///
    /// - `None` if the queue is empty.
    /// - `Some(Duration::ZERO)` if any timer is already due.
    /// - `Some(remaining)` otherwise (the minimum over all timers).
    ///
    /// Returning `Option<Duration>` avoids any artificial cap on the wait time.
    pub fn time_until_next(&self, now_ms: u64) -> Option<Duration> {
        if self.timers.is_empty() {
            return None;
        }
        let mut min_remaining: Option<u64> = None;
        for entry in self.timers.values() {
            if entry.expires_at <= now_ms {
                return Some(Duration::ZERO);
            }
            let remaining = entry.expires_at - now_ms;
            min_remaining = Some(match min_remaining {
                None => remaining,
                Some(prev) => prev.min(remaining),
            });
        }
        min_remaining.map(Duration::from_millis)
    }

    /// Whether the queue has no pending timers.
    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    /// Number of pending timers.
    pub fn len(&self) -> usize {
        self.timers.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: a ManualClock starting at 0.
    fn clock0() -> ManualClock {
        ManualClock::new(0)
    }

    // -----------------------------------------------------------------------
    // calc_next_expires_at
    // -----------------------------------------------------------------------

    #[test]
    fn calc_next_expires_at_basic() {
        // expires_at=100, period=30, now=205
        // (1 + (205-100+30)/30)*30 + 100 - 30
        // = (1 + 135/30)*30 + 70
        // = (1 + 4)*30 + 70
        // = 150 + 70 = 220
        let next = calc_next_expires_at(100, 205, 30);
        assert_eq!(next, 220);
        assert!(next > 205, "rescheduled expiry must be strictly after now");
    }

    #[test]
    fn calc_next_expires_at_exactly_on_boundary() {
        // expires_at=0, period=50, now=100 (exactly 2 periods elapsed)
        // (1 + (100-0+50)/50)*50 + 0 - 50
        // = (1 + 3)*50 - 50 = 200 - 50 = 150
        let next = calc_next_expires_at(0, 100, 50);
        assert_eq!(next, 150);
        assert!(next > 100);
    }

    #[test]
    fn calc_next_expires_at_one_period_behind() {
        // expires_at=100, period=50, now=130 (< 1 full period behind)
        // (1 + (130-100+50)/50)*50 + 100 - 50
        // = (1 + 1)*50 + 50 = 150
        let next = calc_next_expires_at(100, 130, 50);
        assert_eq!(next, 150);
        assert!(next > 130);
    }

    #[test]
    fn calc_next_expires_at_large_expires_at() {
        // Verify no underflow when expires_at > period.
        // expires_at=1000, period=30, now=1005
        // (1 + (1005-1000+30)/30)*30 + 1000 - 30
        // = (1 + 1)*30 + 970 = 60 + 970 = 1030
        let next = calc_next_expires_at(1000, 1005, 30);
        assert_eq!(next, 1030);
        assert!(next > 1005);
    }

    // -----------------------------------------------------------------------
    // ManualClock
    // -----------------------------------------------------------------------

    #[test]
    fn manual_clock_new_set_advance() {
        let c = ManualClock::new(1000);
        assert_eq!(c.now_ms(), 1000);
        c.set(5000);
        assert_eq!(c.now_ms(), 5000);
        c.advance(100);
        assert_eq!(c.now_ms(), 5100);
        c.advance(0);
        assert_eq!(c.now_ms(), 5100);
    }

    #[test]
    fn system_clock_monotonic() {
        let c = SystemClock::new();
        let t0 = c.now_ms();
        let t1 = c.now_ms();
        assert!(t1 >= t0);
    }

    // -----------------------------------------------------------------------
    // TimerQueue — one-shot
    // -----------------------------------------------------------------------

    #[test]
    fn one_shot_fires_once_then_gone() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let id = q.set_timer(clock.now_ms(), Duration::from_millis(100), None);
        assert_eq!(q.len(), 1);

        // Before expiry — nothing fires.
        clock.advance(50);
        let fired = q.collect_expired(clock.now_ms());
        assert!(fired.is_empty());
        assert_eq!(q.len(), 1);

        // At expiry — fires once.
        clock.advance(50);
        let fired = q.collect_expired(clock.now_ms());
        assert_eq!(fired, vec![id]);
        assert!(q.is_empty(), "one-shot must be removed after firing");

        // After — nothing more.
        clock.advance(200);
        let fired = q.collect_expired(clock.now_ms());
        assert!(fired.is_empty());
    }

    #[test]
    fn one_shot_fires_when_overdue() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let id = q.set_timer(clock.now_ms(), Duration::from_millis(10), None);

        // Well past expiry.
        clock.advance(999);
        let fired = q.collect_expired(clock.now_ms());
        assert_eq!(fired, vec![id]);
        assert!(q.is_empty());
    }

    // -----------------------------------------------------------------------
    // TimerQueue — periodic
    // -----------------------------------------------------------------------

    #[test]
    fn periodic_fires_at_most_once_per_collect() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let period = Duration::from_millis(30);
        let id = q.set_timer(clock.now_ms(), period, Some(period));
        assert_eq!(q.len(), 1);

        // First period: advance to t=30 (exactly on expiry).
        clock.advance(30);
        let fired = q.collect_expired(clock.now_ms());
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0], id);
        assert_eq!(q.len(), 1, "periodic timer stays in queue");

        // time_until_next should now point to the rescheduled expiry.
        let until = q.time_until_next(clock.now_ms());
        assert!(
            until.is_some() && until.unwrap() > Duration::ZERO,
            "rescheduled timer must not be immediately due again"
        );

        // Advance well past several periods in one step — still fires at most once.
        clock.advance(300); // many periods elapsed
        let fired = q.collect_expired(clock.now_ms());
        assert_eq!(
            fired.len(),
            1,
            "periodic fires at most once per collect_expired"
        );
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn periodic_reschedules_forward() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let period = Duration::from_millis(50);
        let _id = q.set_timer(clock.now_ms(), period, Some(period));

        // Fire once at t=50.
        clock.advance(50);
        q.collect_expired(clock.now_ms());

        // Now the timer should be rescheduled past t=50.
        let until = q.time_until_next(clock.now_ms()).unwrap();
        assert!(
            until > Duration::ZERO,
            "rescheduled expiry must be in the future"
        );

        // Fire again at t=100.
        clock.advance(50);
        let fired = q.collect_expired(clock.now_ms());
        assert_eq!(fired.len(), 1);
        assert_eq!(q.len(), 1);
    }

    // -----------------------------------------------------------------------
    // TimerQueue — kill_timer
    // -----------------------------------------------------------------------

    #[test]
    fn kill_timer_cancels_pending() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let id = q.set_timer(clock.now_ms(), Duration::from_millis(100), None);
        assert_eq!(q.len(), 1);

        q.kill_timer(id);
        assert!(q.is_empty());

        clock.advance(200);
        let fired = q.collect_expired(clock.now_ms());
        assert!(fired.is_empty(), "killed timer must not fire");
    }

    #[test]
    fn kill_timer_unknown_id_is_noop() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let id = q.set_timer(clock.now_ms(), Duration::from_millis(100), None);
        let phantom = TimerId(id.0 + 999);

        q.kill_timer(phantom); // must not panic
        assert_eq!(q.len(), 1, "killing unknown id must not remove real timer");

        q.kill_timer(phantom); // second call also no-op
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn kill_timer_after_oneshot_fires_is_noop() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let id = q.set_timer(clock.now_ms(), Duration::from_millis(10), None);
        clock.advance(20);
        q.collect_expired(clock.now_ms()); // fires and removes

        q.kill_timer(id); // must not panic
        assert!(q.is_empty());
    }

    #[test]
    fn kill_timer_updates_time_until_next() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        // Two timers: one at t=50, one at t=200.
        let early = q.set_timer(clock.now_ms(), Duration::from_millis(50), None);
        let _late = q.set_timer(clock.now_ms(), Duration::from_millis(200), None);

        assert_eq!(
            q.time_until_next(clock.now_ms()),
            Some(Duration::from_millis(50))
        );

        q.kill_timer(early);
        assert_eq!(
            q.time_until_next(clock.now_ms()),
            Some(Duration::from_millis(200)),
            "after killing the soonest timer, next timeout must reflect the remaining one"
        );
    }

    // -----------------------------------------------------------------------
    // TimerQueue — time_until_next
    // -----------------------------------------------------------------------

    #[test]
    fn time_until_next_none_when_empty() {
        let q = TimerQueue::new();
        assert_eq!(q.time_until_next(0), None);
    }

    #[test]
    fn time_until_next_zero_when_overdue() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        q.set_timer(clock.now_ms(), Duration::from_millis(10), None);
        clock.advance(100);

        assert_eq!(q.time_until_next(clock.now_ms()), Some(Duration::ZERO));
    }

    #[test]
    fn time_until_next_remaining_otherwise() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        q.set_timer(clock.now_ms(), Duration::from_millis(100), None);
        clock.advance(40);

        assert_eq!(
            q.time_until_next(clock.now_ms()),
            Some(Duration::from_millis(60))
        );
    }

    #[test]
    fn time_until_next_reflects_soonest_of_several() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        q.set_timer(clock.now_ms(), Duration::from_millis(200), None);
        q.set_timer(clock.now_ms(), Duration::from_millis(50), None);
        q.set_timer(clock.now_ms(), Duration::from_millis(100), None);

        assert_eq!(
            q.time_until_next(clock.now_ms()),
            Some(Duration::from_millis(50))
        );
    }

    // -----------------------------------------------------------------------
    // TimerQueue — multiple timers fire together
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_timers_fire_in_same_collect() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let id1 = q.set_timer(clock.now_ms(), Duration::from_millis(10), None);
        let id2 = q.set_timer(clock.now_ms(), Duration::from_millis(20), None);
        let id3 = q.set_timer(clock.now_ms(), Duration::from_millis(5), None);

        clock.advance(30); // all three are overdue
        let mut fired = q.collect_expired(clock.now_ms());
        fired.sort_by_key(|id| id.0); // sort for deterministic assert

        let mut expected = vec![id1, id2, id3];
        expected.sort_by_key(|id| id.0);

        assert_eq!(fired, expected);
        assert!(q.is_empty());
    }

    #[test]
    fn mix_of_periodic_and_oneshot_in_same_collect() {
        let clock = clock0();
        let mut q = TimerQueue::new();

        let one_shot = q.set_timer(clock.now_ms(), Duration::from_millis(10), None);
        let periodic = q.set_timer(
            clock.now_ms(),
            Duration::from_millis(10),
            Some(Duration::from_millis(50)),
        );

        clock.advance(10);
        let fired = q.collect_expired(clock.now_ms());
        assert_eq!(fired.len(), 2);
        assert!(fired.contains(&one_shot));
        assert!(fired.contains(&periodic));

        // One-shot gone; periodic still present.
        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());
    }
}
