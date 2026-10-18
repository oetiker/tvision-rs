# Third-party components & data interchange

tvision-rs gives your own widgets the same *typed* data interchange the
framework's controls use. C++ Turbo Vision did this with `void*`/`getData`;
here type safety is kept. There are **three open paths**, each typed at the
layer that owns the type.

## 1. A modal that returns a value: `exec_view_with`

A component launched modally returns *any* native type by value (see
[Modal `execView`](../port/modal.html#getting-a-result-back-exec_view_with)). The
result type is yours; the framework never names it.

## 2. Field data: `FieldValue`, including `Custom`

A control exposes its value as a
[`FieldValue`](../api/tvision_rs/data/enum.FieldValue.html). The well-known shapes
(`Text`/`Int`/`Bool`/`Bits`/`List`) interoperate with framework widgets and generic
consumers. For a payload your component invents, use `FieldValue::Custom`:

```rust,ignore
#[derive(Debug, PartialEq)]
pub struct DateRange { pub start: Date, pub end: Date } // export this type!

impl View for DateRangePicker {
    fn value(&self) -> Option<FieldValue> { Some(FieldValue::custom(self.range.clone())) }
}

// the consumer (your code, or anyone who depends on your crate):
let range = fv.value_as::<DateRange>()?;   // loud: a mismatch is a descriptive error
```

`Custom` is **runtime-checked and fail-loud**: `value_as::<T>()` returns a
descriptive `Result` (a mismatch announces itself at first execution), while
`as_custom::<T>()` returns `Option` for `match`-style reads. It is type-*safe*
(a wrong type fails closed and is never misread), but it is not compile-*checked*
across the `value()` boundary, because the value crosses the object-safe `dyn View`
boundary. The exported payload type *is* the contract; one test exercising the
producer→consumer exchange pins it. (Caveat: `TypeId` is per-version, so a diamond
dependency pulling your crate at two incompatible versions yields distinct types
that won't cross. Handle this by dependency discipline.)

A distributable component can offer **both** a typed `Custom(MyType)` and a generic
scalar/`List` projection, so consumers who don't depend on your types can still
read it. Two of your *own* tightly-coupled components can skip `FieldValue`
entirely and share a typed `Rc<RefCell<MyState>>` for full compile-time checking.
`Custom` is only needed for the framework's *generic* plumbing.

## 3. Notification: a custom `Command` broadcast

`Command` is an open newtype: mint your own and broadcast it
(`Event::Broadcast`) to notify siblings; read the data they then expose via path 2.
