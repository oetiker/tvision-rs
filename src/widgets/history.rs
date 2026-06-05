//! Process-global, byte-budget-bounded history store for input fields.
//!
//! Each "channel" is a small `u8` id that groups one input field's recall
//! list.  Entries are stored oldest-first globally; `history_str(id, 0)`
//! returns the oldest surviving entry for that id.
//!
//! # Deviation from C++ (`histlist.cpp`)
//!
//! The C++ implementation keeps a hidden **front sentinel** record (written by
//! `clearHistory` / `initHistory`) and `advanceStringPointer` always skips it
//! before matching.  A side-effect: once the budget is first exceeded and the
//! sentinel is evicted, the *actual* globally-oldest entry becomes the new
//! front and `advanceStringPointer` skips it — hiding it from
//! `historyCount`/`historyStr`.  This is a byte-block bookkeeping artifact,
//! not intentional designed behavior.
//!
//! **We model the clean contract: no sentinel, no front-skip — every
//! non-evicted entry is readable.**  Pre-overflow behavior is identical to
//! C++; the only divergence is a single hidden globally-oldest entry that the
//! C++ implementation would lose after the budget is first exceeded.  This
//! deviation is intentional and documented here so it is not mistaken for a
//! missing behavior.
//!
//! One precision note: because C++ carries its 3-byte front sentinel inside
//! its `used` accounting, C++'s real-entry budget is 3 bytes tighter, so its
//! first-eviction byte boundary differs from ours by 3 bytes.  This is a
//! direct consequence of the no-sentinel model above, not a separate
//! divergence.

use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct HistRec {
    id: u8,
    str: String,
}

impl HistRec {
    /// Byte cost of one entry, matching the C++ `len = str.size() + 3` formula.
    fn cost(&self) -> usize {
        cost_of(&self.str)
    }
}

/// Byte cost of a candidate string — the single source of truth for the C++
/// `len = str.size() + 3` formula.
fn cost_of(s: &str) -> usize {
    s.len() + 3
}

// ---------------------------------------------------------------------------
// Thread-local store
//
// Thread-local is deliberate: `libtest` runs each `#[test]` on its own
// thread, giving each test a pristine store — no `Mutex` needed, and this
// faithfully models the single-threaded C++ design.
// ---------------------------------------------------------------------------

thread_local! {
    static HISTORY: RefCell<Vec<HistRec>> = const { RefCell::new(Vec::new()) };
}

/// Maximum byte budget shared across **all** ids (faithful to C++ `historySize`).
const HISTORY_SIZE: usize = 1024;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Total bytes currently used by all entries.
fn used_bytes(history: &[HistRec]) -> usize {
    history.iter().map(HistRec::cost).sum()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Add `str` to the history channel identified by `id`.
///
/// Operation order (faithful to C++):
/// 1. Ignore empty strings.
/// 2. Remove any existing duplicate for this `(id, str)` pair.
/// 3. Evict globally-oldest entries until the new entry fits within the budget.
/// 4. Append the new entry (newest position).
pub fn history_add(id: u8, str: &str) {
    if str.is_empty() {
        return;
    }

    HISTORY.with(|h| {
        let mut history = h.borrow_mut();

        // Step 2 — remove existing duplicate (at most one can exist).
        history.retain(|e| !(e.id == id && e.str == str));

        // Step 3 — evict globally-oldest entries until the new entry fits.
        let new_cost = cost_of(str);
        while used_bytes(&history) + new_cost > HISTORY_SIZE {
            if history.is_empty() {
                // The string alone exceeds the budget; bail out.
                return;
            }
            history.remove(0);
        }

        // Step 4 — append (newest).
        history.push(HistRec {
            id,
            str: str.to_string(),
        });
    });
}

/// Return the number of history entries for `id`.
///
/// `history_str(id, 0)` is the oldest; `history_str(id, count-1)` is the
/// newest.
#[must_use]
pub fn history_count(id: u8) -> usize {
    HISTORY.with(|h| h.borrow().iter().filter(|e| e.id == id).count())
}

/// Return the entry at `index` (oldest-first) for `id`, or `None` if out of
/// range.
#[must_use]
pub fn history_str(id: u8, index: usize) -> Option<String> {
    HISTORY.with(|h| {
        h.borrow()
            .iter()
            .filter(|e| e.id == id)
            .nth(index)
            .map(|e| e.str.clone())
    })
}

/// Remove all history entries for all ids.
pub fn clear_history() {
    HISTORY.with(|h| h.borrow_mut().clear());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Basic round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn add_count_str_round_trip() {
        clear_history();
        history_add(1, "first");
        history_add(1, "second");
        assert_eq!(history_count(1), 2);
        // oldest → newest
        assert_eq!(history_str(1, 0), Some("first".into()));
        assert_eq!(history_str(1, 1), Some("second".into()));
    }

    // -----------------------------------------------------------------------
    // Empty string is ignored
    // -----------------------------------------------------------------------

    #[test]
    fn empty_string_ignored() {
        clear_history();
        history_add(2, "");
        assert_eq!(history_count(2), 0);
        history_add(2, "real");
        history_add(2, "");
        assert_eq!(history_count(2), 1);
    }

    // -----------------------------------------------------------------------
    // Per-id isolation
    // -----------------------------------------------------------------------

    #[test]
    fn per_id_isolation() {
        clear_history();
        history_add(1, "alpha");
        history_add(2, "beta");
        assert_eq!(history_count(1), 1);
        assert_eq!(history_count(2), 1);
        assert_eq!(history_str(1, 0), Some("alpha".into()));
        assert_eq!(history_str(2, 0), Some("beta".into()));
        // id 1 cannot see id 2's entry
        assert_eq!(history_str(1, 1), None);
        assert_eq!(history_str(2, 1), None);
    }

    // -----------------------------------------------------------------------
    // Dedup moves entry to newest position
    //
    // Bite: a no-dedup implementation gives count==3 and order "a","b","a".
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_moves_to_newest() {
        clear_history();
        history_add(3, "a");
        history_add(3, "b");
        history_add(3, "a"); // duplicate of first → moves to newest
        assert_eq!(history_count(3), 2, "duplicate must be collapsed");
        // "b" is now the older one, "a" is the newest
        assert_eq!(history_str(3, 0), Some("b".into()));
        assert_eq!(history_str(3, 1), Some("a".into()));
    }

    // -----------------------------------------------------------------------
    // Out-of-range index → None
    // -----------------------------------------------------------------------

    #[test]
    fn out_of_range_returns_none() {
        clear_history();
        history_add(4, "only");
        assert_eq!(history_str(4, 0), Some("only".into()));
        assert_eq!(history_str(4, 1), None);
        assert_eq!(history_str(4, 99), None);
        assert_eq!(history_str(4, 0), Some("only".into())); // unchanged after query
    }

    // -----------------------------------------------------------------------
    // Global byte-budget eviction across ids
    //
    // Design: fill with id=10 entries (each 50+3=53 bytes) until near-full,
    // then add an id=11 entry.  The oldest id=10 entry must be evicted first.
    //
    // We use `format!("{:050}", i)` to guarantee every string is exactly 50
    // bytes regardless of the number of decimal digits in `i`.
    //
    // Bite: a per-id budget model would evict from id=11's budget (empty) and
    // would refuse or evict from the wrong side.
    // -----------------------------------------------------------------------

    #[test]
    fn global_eviction_across_ids() {
        clear_history();
        // Each entry: format!("{:050}", i) → len=50, cost=53.
        // 19 × 53 = 1007 bytes — just under the 1024-byte limit.
        let make_entry = |i: u32| format!("{:050}", i);
        for i in 0..19u32 {
            history_add(10, &make_entry(i));
        }
        // Sanity: all 19 entries fit without eviction.
        assert_eq!(
            history_count(10),
            19,
            "19 × 53 = 1007 ≤ 1024, nothing evicted yet"
        );
        let oldest_id10 = make_entry(0);
        assert_eq!(
            history_str(10, 0),
            Some(oldest_id10.clone()),
            "oldest entry is index 0"
        );

        // Adding one id=11 entry (also 53 bytes) pushes total to 1007+53=1060 > 1024.
        // The globally-oldest entry (an id=10 entry) must be evicted to make room.
        let id11_entry = make_entry(999);
        history_add(11, &id11_entry);

        // id=11 entry must exist.
        assert_eq!(history_count(11), 1);
        assert_eq!(history_str(11, 0), Some(id11_entry));

        // The oldest id=10 entry was evicted (global FIFO, not per-id).
        assert_eq!(
            history_count(10),
            18,
            "one id=10 entry must have been evicted"
        );
        assert_ne!(
            history_str(10, 0),
            Some(oldest_id10),
            "oldest id=10 entry must have been evicted"
        );
    }

    // -----------------------------------------------------------------------
    // Dedup-before-evict: re-adding an existing string must not evict an
    // unrelated entry.
    //
    // Strategy:
    //   • Add 19 entries of cost 53 (len=50) under id=20: total 1007 bytes.
    //   • Add one "canary" entry of cost 17 (len=14): total 1024 bytes (full).
    //   • Re-add the newest of the 19 big entries (already in the store).
    //     – dedup removes it first: 1024-53 = 971 bytes.
    //     – new entry cost 53: 971+53 = 1024 ≤ 1024 → no eviction triggered.
    //   • Assert canary still present (not evicted as collateral).
    //
    // Bite: without dedup-before-evict the store would be at 1024 bytes before
    // the duplicate is removed, triggering an eviction of the oldest entry.
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_before_evict_no_collateral_eviction() {
        clear_history();
        // 19 entries of len=50 (cost=53) — always exactly 50 bytes via {:050}.
        // 19 × 53 = 1007 bytes.
        let make_big = |i: u32| format!("{:050}", i);
        for i in 0..19u32 {
            history_add(20, &make_big(i));
        }

        // Canary: len=14, cost=17 → total 1007+17=1024 (exactly full).
        let canary: String = "c".repeat(14);
        history_add(20, &canary);
        assert_eq!(history_count(20), 20, "20 entries, 1024 bytes");

        // Re-add the newest big entry (make_big(18), already at back of store).
        // dedup removes it first: 1024-53 = 971 bytes.
        // Re-inserting costs 53: 971+53 = 1024 ≤ 1024 → no eviction triggered.
        let newest_big = make_big(18);
        history_add(20, &newest_big);

        // Count must remain 20: dedup freed one slot, re-insert fills it, no net eviction.
        assert_eq!(
            history_count(20),
            20,
            "count must remain 20 — no collateral eviction"
        );

        // Canary must still be present.
        let found_canary =
            (0..history_count(20)).any(|i| history_str(20, i) == Some(canary.clone()));
        assert!(
            found_canary,
            "canary must not have been evicted as collateral damage"
        );
    }

    // -----------------------------------------------------------------------
    // clear_history empties all ids
    // -----------------------------------------------------------------------

    #[test]
    fn clear_empties_all_ids() {
        clear_history();
        history_add(50, "foo");
        history_add(51, "bar");
        assert_eq!(history_count(50), 1);
        assert_eq!(history_count(51), 1);
        clear_history();
        assert_eq!(history_count(50), 0);
        assert_eq!(history_count(51), 0);
        assert_eq!(history_str(50, 0), None);
        assert_eq!(history_str(51, 0), None);
    }
}
