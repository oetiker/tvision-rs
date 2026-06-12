//! Global view-identity minter.
//!
//! ## Why this exists
//!
//! A view tree needs links in every direction: each view to its parent, each
//! view to its siblings, and each group to its focused/selected child. Rust
//! forbids aliased mutable references, so those links cannot all be live
//! pointers. rstv splits them into two clean pieces:
//!
//! * **Downward ownership is a tree** — a `Group` owns
//!   `children: Vec<Box<dyn View>>`.
//! * **Up/sideways links become [`ViewId`] handles** — a lightweight identity
//!   that can be stored cheaply anywhere without borrowing the tree.
//!
//! A [`ViewId`] is *not* an index into any store. It is a globally-unique
//! identity, minted once at [`Group::insert`](crate::view::Group::insert) and
//! stamped into the view's own [`ViewState`](crate::view::ViewState). Resolving a
//! `ViewId` back to a `&mut dyn View` is a tree-walk
//! ([`View::find_mut`](crate::view::View::find_mut)) performed by the downward
//! context — not here.
//!
//! ## Design
//!
//! Ids are **process-global and monotonic**: a single `static AtomicU64`
//! counter, never reset, never reused. A stale handle (its view removed) simply
//! matches nothing in the tree-walk; there is no slot to alias, so no
//! generational validation is needed (the earlier per-`Group` generational arena
//! guarded an ABA hazard that the by-identity child scan cannot have). The
//! `NonZeroU64` gives `Option<ViewId>` a niche, so it costs no extra size; use
//! `Option<ViewId>` (not a sentinel value) to represent "no link / null". The
//! `u64` space never realistically exhausts (mirrors `TimerId`).
//!
//! # Turbo Vision heritage
//! Replaces the raw `TView*` pointer web (`owner`, the `next`/`prev` sibling
//! ring, `current`/`selected`) with owned tree edges plus by-value identity
//! handles (deviation D3).

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// A lightweight, globally-unique view identity. `Copy`, carries no
/// reference into the tree, so it can be stored freely (sibling links, focus
/// stacks, capture handlers). Identity is `ViewId` equality.
///
/// Ids are **process-global and monotonic** — minted once at `Group::insert`,
/// never reused. A stale handle (its view removed) therefore matches nothing and
/// simply fails to resolve via [`View::find_mut`](crate::view::View::find_mut);
/// there is no slot to alias, so no generational validation is needed. The
/// `NonZeroU64` gives `Option<ViewId>` a niche (no discriminant word). The `u64`
/// space never realistically exhausts (mirrors `TimerId`).
///
/// # Turbo Vision heritage
/// The by-value successor to a raw `TView*` used for `owner` / sibling /
/// `current` links (deviation D3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ViewId(NonZeroU64);

/// The global id counter. Starts at 1 so the first id is non-zero.
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

impl ViewId {
    /// Mint a fresh, globally-unique id. Called by
    /// [`Group::insert`](crate::view::Group::insert).
    pub fn next() -> ViewId {
        let n = NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed);
        // fetch_add starts at 1 and only increases; n is never 0 in practice.
        ViewId(NonZeroU64::new(n).expect("view id counter starts at 1 and increases"))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn niche_optimization() {
        // The NonZeroU64 gives Option<ViewId> a niche so it needs no extra
        // discriminant word.
        assert_eq!(size_of::<Option<ViewId>>(), size_of::<ViewId>());
    }

    #[test]
    fn next_returns_distinct_strictly_increasing_ids() {
        // The counter is process-global (shared across all tests), so only
        // relative distinctness/ordering within this test is meaningful — never
        // assert literal id values.
        let a = ViewId::next();
        let b = ViewId::next();
        let c = ViewId::next();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        assert!(a.0 < b.0, "ids are strictly increasing");
        assert!(b.0 < c.0, "ids are strictly increasing");
    }
}
