//! Typed dialog-data transfer — the value currency moved between controls and
//! the dialog that owns them.
//!
//! A control exposes its current value as a [`FieldValue`] via the
//! [`value`](crate::view::View::value)/[`set_value`](crate::view::View::set_value)
//! pair on the [`View`](crate::view::View) trait. A dialog gathers the whole
//! record by walking its children in order
//! ([`Group::gather_data`](crate::view::Group::gather_data) returns a
//! `Vec<Option<FieldValue>>`) and distributes edited values back the same way
//! ([`Group::scatter_data`](crate::view::Group::scatter_data), which routes
//! through the context-aware setter that `ListBox` overrides to republish its
//! scrollbar).
//!
//! [`FieldValue`] carries one variant per kind of value a control can hold.
//! Only the kinds an actual control transfers are present:
//! [`Text`](FieldValue::Text) for input lines and
//! [`Int`](FieldValue::Int) for scrollbars. Cluster controls (check boxes, radio
//! buttons) interpret their packed bit value internally and do not participate in
//! dialog data transfer, so there is no `Bits` variant; the color picker likewise
//! reports its color through a dedicated accessor rather than a `FieldValue`.
//!
//! **Guide:** [Dialogs & data](../../../apps/dialogs.html).
//!
//! # Turbo Vision heritage
//!
//! The original moved dialog data through an untyped getter/setter protocol: each
//! control copied its raw value into/out of an untyped record at a hand-tracked
//! offset, and a dialog gathered the record by walking its children. rstv replaces
//! that protocol with this typed value currency (deviation D10).

/// The typed unit of dialog data transfer.
///
/// Carries one variant per kind of value a control transfers. Cluster controls
/// (check boxes, radio buttons) keep their bit value internal and the color
/// picker uses a dedicated accessor, so neither has a variant here.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    /// A text field's contents (an input line).
    Text(String),
    /// An integer value (a scroll bar's position; read by the scroller via
    /// [`View::value`](crate::view::View::value)).
    Int(i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_variant_round_trips() {
        let v = FieldValue::Text("hello".to_string());
        assert_eq!(v, FieldValue::Text("hello".to_string()));
        assert_ne!(v, FieldValue::Text("world".to_string()));
        let FieldValue::Text(s) = v else {
            panic!("expected Text");
        };
        assert_eq!(s, "hello");
    }

    #[test]
    fn int_variant_round_trips() {
        let v = FieldValue::Int(42);
        assert_eq!(v, FieldValue::Int(42));
        assert_ne!(v, FieldValue::Int(7));
        // Distinct from Text even when "equal-looking".
        assert_ne!(FieldValue::Int(0), FieldValue::Text("0".to_string()));
        let FieldValue::Int(n) = v else {
            panic!("expected Int");
        };
        assert_eq!(n, 42);
    }
}
