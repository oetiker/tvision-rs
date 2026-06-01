//! Generational view-identity allocator — deviation **D3**.
//!
//! ## Why this exists
//!
//! C++ Turbo Vision threads raw `TView*` pointers in every direction: `owner`
//! (upward), a circular `next`/`prev` sibling ring, and `current`/`selected`
//! cross-links inside groups. Rust forbids aliased mutable references, so D3
//! splits the pointer web into two clean pieces:
//!
//! * **Downward ownership is a tree** — a `Group` owns
//!   `children: Vec<Box<dyn View>>`. *(Later row, not here.)*
//! * **Up/sideways links become [`ViewId`] handles** — a generational index that
//!   can be stored cheaply anywhere without borrowing the tree.
//!
//! [`ViewArena`] is *not* a view store. It does not own or contain views; the
//! `Group` tree does that. Its sole job is to **mint reuse-safe, collision-free
//! identities** and **validate** them, so a stale handle to a freed view cannot
//! accidentally match a newer view that reused the same slot — the use-after-free
//! hazard that raw `TView*` carried.
//!
//! Resolving a `ViewId` to an actual `&dyn View` is a tree walk performed by the
//! downward context ([`Context`], row 22) — not here.
//!
//! ## Design
//!
//! Each slot carries a `u32` generation counter. When a slot is freed and then
//! reused, its generation is incremented before a new `ViewId` is issued. Any old
//! `ViewId` that stored the previous generation will therefore fail [`ViewArena::is_valid`]
//! even though it names the same slot index — the stale handle is dead.
//!
//! Generation `0` is never a valid live generation; the first use of any slot starts
//! at generation `1`. This is enforced by storing the generation in a [`NonZeroU32`],
//! which simultaneously gives `Option<ViewId>` the same size as `ViewId` (niche
//! optimization — no extra discriminant byte). Use `Option<ViewId>` (not a sentinel
//! value) to represent "no link / null".
//!
//! [`Context`]: crate::view
use std::num::NonZeroU32;

// ── ViewId ────────────────────────────────────────────────────────────────────

/// A lightweight, reuse-safe handle to a view. Faithful to D3.
///
/// A `ViewId` is just two `u32`s (index + generation) and is `Copy`. It
/// carries no reference into any arena or tree, so it can be stored freely —
/// in sibling links, focus stacks, event listeners — without borrowing the
/// view tree.
///
/// The only contract is *validity*: check [`ViewArena::is_valid`] before using
/// a handle, and discard it after the view is freed.
///
/// `Option<ViewId>` is the idiomatic "nullable handle"; it costs no extra size
/// thanks to the `NonZeroU32` niche in the generation field.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ViewId {
    index: u32,
    generation: NonZeroU32,
}

impl ViewId {
    /// The slot index — primarily useful for debugging / tests.
    #[inline]
    pub fn index(self) -> u32 {
        self.index
    }

    /// The generation at the time this handle was issued.
    #[inline]
    pub fn generation(self) -> NonZeroU32 {
        self.generation
    }
}

// ── ViewArena ─────────────────────────────────────────────────────────────────

/// One slot in the arena.
struct Slot {
    /// The current (or last) generation of this slot.
    ///
    /// A plain `u32` here (not `NonZeroU32`) because a freshly pushed slot
    /// starts at `0` and is bumped to `1` on first allocation. The invariant
    /// `generation >= 1` is upheld by [`ViewArena::alloc`].
    generation: u32,
    /// Whether the slot is currently occupied by a live view identity.
    occupied: bool,
}

/// Generational identity allocator for view handles (D3, INFRA row 17).
///
/// `ViewArena` mints [`ViewId`] handles and validates them. It does **not**
/// own or store views — the view tree (`Group`) does that.
///
/// ```
/// use tvision::view::{ViewArena, ViewId};
///
/// let mut arena = ViewArena::new();
/// let id = arena.alloc();
/// assert!(arena.is_valid(id));
/// arena.free(id);
/// assert!(!arena.is_valid(id));
/// ```
pub struct ViewArena {
    slots: Vec<Slot>,
    /// Indices of freed slots available for reuse.
    free: Vec<u32>,
}

impl ViewArena {
    /// Create an empty arena.
    pub fn new() -> Self {
        ViewArena {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Allocate a new `ViewId`.
    ///
    /// If there is a free slot available it is reused (with a bumped
    /// generation); otherwise a new slot is appended with generation `1`.
    ///
    /// ### Generation overflow
    ///
    /// A slot's generation counter is a `u32`. Overflow (`u32::MAX + 1`) is
    /// astronomically unlikely in practice (it would require allocating and
    /// freeing the same slot ~4 billion times), but is handled safely: if
    /// `generation.checked_add(1)` returns `None` the slot is **retired** —
    /// it is not pushed back onto the free list and will never be reused.
    /// A new slot is pushed instead. This avoids wrapping the generation back
    /// to `0` (which would break the `NonZeroU32` invariant and potentially
    /// re-validate stale handles).
    pub fn alloc(&mut self) -> ViewId {
        // Try to reuse a free slot (skip any that overflowed — already retired).
        while let Some(idx) = self.free.pop() {
            let slot = &mut self.slots[idx as usize];
            debug_assert!(!slot.occupied, "free list contained an occupied slot");
            // Bump generation; retire the slot if it would overflow.
            match slot.generation.checked_add(1) {
                Some(next_gen) => {
                    slot.generation = next_gen;
                    slot.occupied = true;
                    // SAFETY: next_gen >= 2 (slot was used at least once before) > 0.
                    let generation =
                        NonZeroU32::new(next_gen).expect("next_gen is checked_add result > 0");
                    return ViewId {
                        index: idx,
                        generation,
                    };
                }
                None => {
                    // This slot is exhausted — leave it unoccupied forever (retired).
                    // Fall through to try the next free slot or push a new one.
                }
            }
        }

        // No reusable slot — push a new one at generation 1.
        let idx = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            occupied: true,
        });
        ViewId {
            index: idx,
            // SAFETY: 1 != 0.
            generation: NonZeroU32::new(1).expect("1 is non-zero"),
        }
    }

    /// Free a view handle, making its slot available for reuse.
    ///
    /// If `id` is not currently valid (stale, double-free, out-of-range) this
    /// is a **no-op** — it does not panic.
    pub fn free(&mut self, id: ViewId) {
        if self.is_valid(id) {
            let slot = &mut self.slots[id.index as usize];
            slot.occupied = false;
            self.free.push(id.index);
        }
    }

    /// Return `true` iff `id` refers to a currently-live slot.
    ///
    /// Specifically: the slot index is in range, the slot is occupied, and
    /// the slot's generation matches the handle's generation.
    pub fn is_valid(&self, id: ViewId) -> bool {
        let idx = id.index as usize;
        idx < self.slots.len()
            && self.slots[idx].occupied
            && self.slots[idx].generation == id.generation.get()
    }

    /// Number of currently-occupied (live) slots.
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.occupied).count()
    }

    /// `true` when no views are currently allocated.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ViewArena {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn niche_optimization() {
        // NonZeroU32 in the generation field gives Option<ViewId> a niche so
        // it needs no extra discriminant word.
        assert_eq!(size_of::<Option<ViewId>>(), size_of::<ViewId>());
    }

    #[test]
    fn alloc_returns_distinct_ids() {
        let mut arena = ViewArena::new();
        let a = arena.alloc();
        let b = arena.alloc();
        let c = arena.alloc();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        assert!(arena.is_valid(a));
        assert!(arena.is_valid(b));
        assert!(arena.is_valid(c));
    }

    #[test]
    fn free_invalidates_id() {
        let mut arena = ViewArena::new();
        let id = arena.alloc();
        assert!(arena.is_valid(id));
        arena.free(id);
        assert!(!arena.is_valid(id));
    }

    #[test]
    fn slot_reuse_with_higher_generation() {
        let mut arena = ViewArena::new();
        let old = arena.alloc();
        let old_idx = old.index();
        let old_gen = old.generation();

        arena.free(old);
        assert!(!arena.is_valid(old));

        // Next alloc should reuse the same slot index with a strictly higher generation.
        let new_id = arena.alloc();
        assert_eq!(new_id.index(), old_idx, "slot index should be reused");
        assert!(
            new_id.generation() > old_gen,
            "generation must increase on reuse"
        );

        // Core safety property: the stale handle must not match the reused slot.
        assert!(!arena.is_valid(old), "stale handle must remain invalid");
        assert!(arena.is_valid(new_id), "new handle must be valid");
    }

    #[test]
    fn stale_free_is_noop() {
        let mut arena = ViewArena::new();
        let old = arena.alloc();
        arena.free(old);

        // A subsequent alloc reuses the slot.
        let live = arena.alloc();
        assert_eq!(live.index(), old.index());

        // Freeing the stale old handle must not invalidate the live one.
        arena.free(old);
        assert!(arena.is_valid(live), "live handle must survive stale free");
    }

    #[test]
    fn double_free_is_noop() {
        let mut arena = ViewArena::new();
        let id = arena.alloc();
        arena.free(id);
        arena.free(id); // second free: no-op, no panic
        assert!(!arena.is_valid(id));
    }

    #[test]
    fn len_and_is_empty() {
        let mut arena = ViewArena::new();
        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);

        let a = arena.alloc();
        let b = arena.alloc();
        assert_eq!(arena.len(), 2);
        assert!(!arena.is_empty());

        arena.free(a);
        assert_eq!(arena.len(), 1);

        arena.free(b);
        assert_eq!(arena.len(), 0);
        assert!(arena.is_empty());
    }

    #[test]
    fn different_generations_not_equal() {
        let mut arena = ViewArena::new();
        let first = arena.alloc();
        arena.free(first);
        let second = arena.alloc();

        // Same index, different generation → different ViewId.
        assert_eq!(first.index(), second.index());
        assert_ne!(first, second);
    }

    #[test]
    fn default_is_same_as_new() {
        let a = ViewArena::new();
        let b = ViewArena::default();
        assert_eq!(a.len(), b.len());
        assert!(a.is_empty());
        assert!(b.is_empty());
    }
}
