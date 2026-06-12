//! Help contexts.
//!
//! A [`HelpCtx`] identifies which help topic applies to the focused view — the
//! status line and a help viewer use it to show context-sensitive help. Like
//! [`Command`], a help context's identity is a **namespaced static string**, so
//! app- and view-defined contexts mint their own collision-safely via
//! [`HelpCtx::custom`] under their own dotted prefix.
//!
//! [`Command`]: crate::command::Command
//!
//! # Turbo Vision heritage
//!
//! Ports the `hc*` help-context family (`views.h`). Contexts were originally plain
//! `int`s used to index a help file's topic table; here the identity is a
//! namespaced `&'static str` instead (deviation D1).

/// Identifies which help topic applies to the focused view.
///
/// A help context's identity is a **namespaced static string** so app/view-
/// defined contexts cannot collide. Equality and hashing compare the string
/// *contents*.
///
/// [`Default`] is [`HelpCtx::NO_CONTEXT`].
///
/// # Turbo Vision heritage
///
/// Ports the `hc*` family (`views.h`), which were plain `int`s; here the identity
/// is a namespaced `&'static str` (deviation D1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct HelpCtx(&'static str);

impl Default for HelpCtx {
    fn default() -> Self {
        HelpCtx::NO_CONTEXT
    }
}

impl HelpCtx {
    /// Mint an application- or view-specific help context from a namespaced
    /// name. Pick a dotted prefix unique to your app/view so it cannot collide
    /// with the framework's `tv.*` vocabulary or another extension's.
    pub const fn custom(name: &'static str) -> HelpCtx {
        HelpCtx(name)
    }

    /// The context's namespaced name (e.g. `"tv.no_context"`).
    pub const fn name(self) -> &'static str {
        self.0
    }

    /// No help topic — also the zero/default context.
    pub const NO_CONTEXT: HelpCtx = HelpCtx("tv.no_context");
    /// The context active while a view is being dragged (see
    /// [`ViewState::get_help_ctx`](crate::view::ViewState::get_help_ctx)).
    pub const DRAGGING: HelpCtx = HelpCtx("tv.dragging");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_round_trips() {
        let h = HelpCtx::custom("myapp.editor");
        assert_eq!(h.name(), "myapp.editor");
    }

    #[test]
    fn default_is_no_context() {
        assert_eq!(HelpCtx::default(), HelpCtx::NO_CONTEXT);
        assert_eq!(HelpCtx::NO_CONTEXT.name(), "tv.no_context");
    }

    #[test]
    fn constants_distinct() {
        assert_ne!(HelpCtx::NO_CONTEXT, HelpCtx::DRAGGING);
        assert_eq!(HelpCtx::DRAGGING.name(), "tv.dragging");
    }
}
