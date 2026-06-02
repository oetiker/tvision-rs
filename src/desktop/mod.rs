//! Desktop-layer views — `TDeskTop` and `TBackground` (rows 29–30).
//!
//! Row 29 [`Background`] is the simplest concrete view: fills its owner's bounds
//! with a repeated pattern character. Row 30 [`Desktop`] is the `TGroup`-embedding
//! desktop group that owns a [`Background`] and gives `Program` a named real
//! desktop.

mod background;
// The module file is named `desktop.rs` (matching the `Desktop` type) inside the
// `desktop` layer module; the inner-name match is intentional, not a smell.
#[allow(clippy::module_inception)]
mod desktop;

pub use background::Background;
pub use desktop::Desktop;
