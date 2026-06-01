//! Help contexts — deviation **D1** (identity as a namespaced static string).
//!
//! Ports the `hc*` help-context family (`views.h`). Exactly like [`Command`],
//! a help context's *identity* changes from TV's hand-assigned `int` (used only
//! to index a help file's topic table) to a **namespaced static string**:
//! [`HelpCtx`] is an opaque newtype around `&'static str`. App- and view-defined
//! contexts mint their own collision-safely via [`HelpCtx::custom`] under their
//! own dotted prefix.
//!
//! [`Command`]: crate::command::Command

/// A Turbo Vision help context. Faithful to TV's `hc*` family (`views.h`),
/// which were plain `int`s; per D1 the identity is now a **namespaced static
/// string** so app/view-defined contexts cannot collide.
///
/// Equality and hashing compare the string *contents*.
///
/// [`Default`] is [`HelpCtx::NO_CONTEXT`] (TV's `hcNoContext`).
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

    /// `hcNoContext` — also the zero/default context.
    pub const NO_CONTEXT: HelpCtx = HelpCtx("tv.no_context");
    /// `hcDragging` — active while a view is being dragged (see
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
