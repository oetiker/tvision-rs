//! Desktop-layer views — the desktop surface and its backdrop.
//!
//! [`Background`] is the simplest concrete view: it fills its owner's bounds
//! with a repeated pattern character. [`Desktop`] is the group-embedding
//! desktop that owns a [`Background`], hosts the application's windows, and gives
//! `Program` a named real desktop.
//!
//! **Guide:** [Windows & the desktop](../../../apps/windows.html).

mod background;
// The module file is named `desktop.rs` (matching the `Desktop` type) inside the
// `desktop` layer module; the inner-name match is intentional, not a smell.
#[allow(clippy::module_inception)]
mod desktop;

pub use background::Background;
pub use desktop::Desktop;
