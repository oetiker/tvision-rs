pub mod layout;

/// How the seam *after* a given pane looks and behaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DividerStyle {
    /// Always drawn; grab-and-drag anytime.
    Line,
    /// Clean look; only a small grab nub at the midpoint.
    Handle,
    /// Invisible & seamless in normal use, but resizable in reconfig mode.
    Hidden,
    /// Invisible AND immovable — a permanent boundary, even in reconfig mode.
    Locked,
}

impl DividerStyle {
    /// Whether a *live* mouse drag may grab this divider in normal use.
    pub fn draggable_live(&self) -> bool {
        matches!(self, DividerStyle::Line | DividerStyle::Handle)
    }
    /// Whether reconfig mode may move this divider.
    pub fn movable_in_reconfig(&self) -> bool {
        !matches!(self, DividerStyle::Locked)
    }
}

#[cfg(test)]
mod divider_tests {
    use super::*;

    #[test]
    fn draggability_matrix() {
        assert!(DividerStyle::Line.draggable_live());
        assert!(DividerStyle::Handle.draggable_live());
        assert!(!DividerStyle::Hidden.draggable_live());
        assert!(!DividerStyle::Locked.draggable_live());

        assert!(DividerStyle::Hidden.movable_in_reconfig());
        assert!(!DividerStyle::Locked.movable_in_reconfig());
    }
}
