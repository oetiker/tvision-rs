//! Typed dialog-data transfer — the value currency moved between controls and
//! the dialog that owns them, and the open seam for third-party components.
//!
//! A control exposes its current value as a [`FieldValue`] via the
//! [`value`](crate::view::View::value)/[`set_value`](crate::view::View::set_value)
//! pair on the [`View`](crate::view::View) trait. A dialog gathers the whole
//! record by walking its children in order
//! ([`Group::gather_data`](crate::view::Group::gather_data) →
//! `Vec<Option<FieldValue>>`, the positional primitive;
//! [`Group::gather_list`](crate::view::Group::gather_list) → one ordered
//! [`FieldValue::List`]) and distributes edited values back the same way
//! ([`Group::scatter_data`](crate::view::Group::scatter_data) /
//! [`Group::scatter_list`](crate::view::Group::scatter_list)).
//!
//! [`FieldValue`] carries the well-known shapes a control transfers
//! ([`Text`](FieldValue::Text), [`Int`](FieldValue::Int),
//! [`Bool`](FieldValue::Bool), [`Bits`](FieldValue::Bits) for cluster controls,
//! [`List`](FieldValue::List) for a whole record) plus
//! [`Custom`](FieldValue::Custom) — the open escape for payloads a user-written
//! component invents. `Color` is deliberately NOT a `FieldValue` (it is a
//! 4-variant enum and rides the by-value `exec_view_with` path).
//!
//! **Extensibility:** see the [extensibility guide](../../../apps/extensibility.html)
//! for the three open paths and the `Custom` / [`value_as`](FieldValue::value_as)
//! contract (runtime-checked, fail-loud, typed at the edges).
//!
//! **Guide:** [Dialogs & data](../../../apps/dialogs.html).
//!
//! # Turbo Vision heritage
//!
//! The original moved dialog data through an untyped getter/setter protocol over a
//! raw record (`getData`/`setData`/`dataSize`, anonymous `void*`). tvision-rs replaces
//! that with this typed value currency (deviation D10); the `Custom` seam keeps the
//! original's openness to arbitrary payloads without its loss of type safety.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// Marker for a user-invented payload carried in [`FieldValue::Custom`].
///
/// Blanket-implemented for every `'static` type that is [`Debug`], so component
/// authors implement nothing — they just put their type in a `FieldValue::custom`.
/// `Debug` is required so a payload is inspectable (and so [`FieldValue`] keeps a
/// derived `Debug`). The `as_any_rc` bridge lets the typed accessors downcast.
pub trait CustomValue: Any + fmt::Debug {
    /// Upcast `Rc<Self>` to `Rc<dyn Any>` so [`FieldValue::as_custom`] /
    /// [`FieldValue::value_as`] can `downcast`.
    fn as_any_rc(self: Rc<Self>) -> Rc<dyn Any>;
}

impl<T: Any + fmt::Debug> CustomValue for T {
    fn as_any_rc(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

/// The typed unit of dialog data transfer (the D10 value currency).
///
/// Well-known shapes ([`Text`](FieldValue::Text)/[`Int`](FieldValue::Int)/
/// [`Bool`](FieldValue::Bool)/[`Bits`](FieldValue::Bits)/[`List`](FieldValue::List))
/// are fully type-checked and interoperate with framework controls and generic
/// consumers. [`Custom`](FieldValue::Custom) is the open seam for user-invented
/// payloads: the framework moves it opaquely and a consumer downcasts at the edge
/// (runtime-checked; see [`value_as`](FieldValue::value_as)).
#[derive(Clone, Debug)]
pub enum FieldValue {
    /// A text field's string contents, used by `InputLine::value` and
    /// `Memo::value`. Produced during a dialog gather walk and consumed
    /// during scatter. Replaces the raw `TMemoData` buffer that
    /// `TMemo::getData`/`setData` filled in the original.
    Text(String),
    /// An integer value (e.g. a scroll bar's position).
    Int(i32),
    /// A boolean field.
    Bool(bool),
    /// A packed bit word — a cluster control's value (check boxes: a bitmask;
    /// radio buttons: the selected index). Faithful to `TCluster::value`. NOT a
    /// packed `Color` (`Color` is a 4-variant enum and rides the by-value path).
    Bits(u32),
    /// An ordered record — the typed image of C++ `getData(void *rec)`'s
    /// offset-addressed child walk (positional, anonymous). See
    /// [`Group::gather_list`](crate::view::Group::gather_list).
    List(Vec<FieldValue>),
    /// A user-invented payload, carried opaquely. Construct with
    /// [`custom`](FieldValue::custom); read with [`value_as`](FieldValue::value_as)
    /// (loud) or [`as_custom`](FieldValue::as_custom) (`Option`). Equality is
    /// **pointer identity** (two `Custom`s are equal iff they share the `Rc`).
    Custom(Rc<dyn CustomValue>),
}

impl FieldValue {
    /// Wrap a user payload as [`Custom`](FieldValue::Custom).
    pub fn custom<T: Any + fmt::Debug>(v: T) -> Self {
        FieldValue::Custom(Rc::new(v))
    }

    /// Read a [`Custom`](FieldValue::Custom) payload as `T`, or `None` if this is
    /// not a `Custom` of type `T` (fail closed). For a descriptive error instead,
    /// use [`value_as`](Self::value_as).
    pub fn as_custom<T: Any>(&self) -> Option<Rc<T>> {
        match self {
            FieldValue::Custom(rc) => rc.clone().as_any_rc().downcast::<T>().ok(),
            _ => None,
        }
    }

    /// Read a [`Custom`](FieldValue::Custom) payload as `T`, **loudly**: a type
    /// mismatch returns a descriptive [`FieldTypeError`] (so stale producer/
    /// consumer wiring announces itself at first execution) rather than a silent
    /// `None`. This is the recommended accessor for third-party components.
    pub fn value_as<T: Any>(&self) -> Result<Rc<T>, FieldTypeError> {
        match self {
            FieldValue::Custom(rc) => {
                rc.clone()
                    .as_any_rc()
                    .downcast::<T>()
                    .map_err(|_| FieldTypeError {
                        expected: std::any::type_name::<T>(),
                        found: "a different Custom payload type",
                    })
            }
            other => Err(FieldTypeError {
                expected: std::any::type_name::<T>(),
                found: other.variant_name(),
            }),
        }
    }

    /// The variant name, for diagnostics.
    fn variant_name(&self) -> &'static str {
        match self {
            FieldValue::Text(_) => "Text",
            FieldValue::Int(_) => "Int",
            FieldValue::Bool(_) => "Bool",
            FieldValue::Bits(_) => "Bits",
            FieldValue::List(_) => "List",
            FieldValue::Custom(_) => "Custom",
        }
    }
}

impl PartialEq for FieldValue {
    /// Scalars and `List` compare by value; `Custom` compares by **pointer
    /// identity** (`Rc::ptr_eq`) — the framework cannot compare opaque user
    /// payloads by value, so two `Custom`s are equal iff they share the `Rc`.
    fn eq(&self, other: &Self) -> bool {
        use FieldValue::*;
        match (self, other) {
            (Text(a), Text(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Bits(a), Bits(b)) => a == b,
            (List(a), List(b)) => a == b,
            (Custom(a), Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A [`FieldValue::value_as`] type mismatch — names the expected type and the
/// found variant/payload, so a contract mismatch fails loudly.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldTypeError {
    /// The Rust type the caller asked for (`std::any::type_name`).
    pub expected: &'static str,
    /// What was actually present (a variant name, or a different `Custom` type).
    pub found: &'static str,
}

impl fmt::Display for FieldTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FieldValue type mismatch: expected {}, found {}",
            self.expected, self.found
        )
    }
}

impl std::error::Error for FieldTypeError {}

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

    #[test]
    fn new_scalar_variants_round_trip() {
        assert_eq!(FieldValue::Bool(true), FieldValue::Bool(true));
        assert_ne!(FieldValue::Bool(true), FieldValue::Bool(false));
        assert_eq!(FieldValue::Bits(0b101), FieldValue::Bits(0b101));
        assert_eq!(
            FieldValue::List(vec![FieldValue::Int(1), FieldValue::Text("a".into())]),
            FieldValue::List(vec![FieldValue::Int(1), FieldValue::Text("a".into())]),
        );
        // Distinct kinds never compare equal.
        assert_ne!(FieldValue::Bool(true), FieldValue::Int(1));
        assert_ne!(FieldValue::Bits(0), FieldValue::Int(0));
    }

    #[derive(Debug, PartialEq)]
    struct DateRange {
        start: i32,
        end: i32,
    }

    #[test]
    fn custom_round_trips_via_as_custom() {
        let fv = FieldValue::custom(DateRange { start: 1, end: 9 });
        let got = fv
            .as_custom::<DateRange>()
            .expect("downcast to the stored type");
        assert_eq!(*got, DateRange { start: 1, end: 9 });
        // Wrong type → None (fail closed).
        assert!(fv.as_custom::<String>().is_none());
    }

    #[test]
    fn value_as_is_loud_on_mismatch() {
        let fv = FieldValue::custom(DateRange { start: 1, end: 9 });
        assert!(fv.value_as::<DateRange>().is_ok(), "matching type succeeds");

        // Wrong Custom type → descriptive error, not None.
        let err = fv.value_as::<String>().unwrap_err();
        assert!(
            err.expected.contains("String"),
            "names the expected type: {err}"
        );

        // A scalar read as a Custom → error naming the found variant.
        let scalar = FieldValue::Int(3);
        let err = scalar.value_as::<DateRange>().unwrap_err();
        assert_eq!(err.found, "Int", "names the found variant");
    }

    #[test]
    fn custom_equality_is_pointer_identity() {
        let a = FieldValue::custom(DateRange { start: 1, end: 2 });
        let b = a.clone(); // Rc clone — same allocation
        let c = FieldValue::custom(DateRange { start: 1, end: 2 }); // distinct allocation
        assert_eq!(a, b, "clones share the Rc, so they are equal by identity");
        assert_ne!(
            a, c,
            "distinct allocations are not equal even with equal contents"
        );
    }
}
