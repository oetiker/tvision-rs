//! Input validators — `TValidator` ported per deviation **D2** (row 35).
//!
//! In C++ Turbo Vision, `TValidator` is an *abstract* base class whose concrete
//! subclasses (`TPXPictureValidator`, `TFilterValidator`, `TRangeValidator`,
//! `TLookupValidator`, …) gate what a `TInputLine` accepts. Per D2 inheritance
//! becomes a **trait**: [`Validator`] is the trait an input line holds as
//! `Option<Box<dyn Validator>>`. The concrete validators are later rows
//! (`TFilterValidator`/`TRangeValidator` etc.); only the abstract base lands
//! here, so every default method simply accepts.
//!
//! ## Object safety
//!
//! A `TInputLine` stores its validator as a boxed trait object
//! (`Option<Box<dyn Validator>>`), so [`Validator`] must be **object-safe**:
//! every method takes `&self`, no generics, no `Self` return. `validate` is
//! non-virtual in C++ (it calls the virtual `isValid`/`error`), so it stays a
//! provided method here.
//!
//! ## Deviations from PORT-ORDER row 35's note
//!
//! PORT-ORDER lists a `transfer(void*, TVTransfer)` hook. It has **no overrider
//! until `TRangeValidator` (row 59)** and **no caller** until then (its only
//! callers are `TInputLine::dataSize`/`getData`/`setData`, which under D10 become
//! the typed [`value`](crate::view::View::value)/[`set_value`](crate::view::View::set_value)
//! protocol — see `src/data.rs`). Building `transfer` now would be a dead stub,
//! so it is **deliberately omitted**; it lands with its first overrider/consumer
//! (row 59). The slot-in point is breadcrumbed in `InputLine::value`/`set_value`.

use crate::data::FieldValue;

/// An input validator — `TValidator` (D2: abstract base → trait).
///
/// A [`TInputLine`](crate::widgets::InputLine) holds an
/// `Option<Box<dyn Validator>>`; with no validator every input is accepted. The
/// default methods all accept (faithful to the abstract base, which returns
/// `True`/`vsOk`); concrete validators (later rows) override them.
pub trait Validator {
    /// `TValidator::isValidInput` — check (and optionally auto-fill/modify) `s`
    /// *as it is being typed*. May mutate `s` in place (e.g. a picture validator
    /// inserting literal characters). `suppress_fill` (`noAutoFill` /
    /// `voFill`-suppression) asks it not to auto-fill. Default: accept, no
    /// change. Object-safe: `&self`, `s: &mut String`.
    fn is_valid_input(&self, _s: &mut String, _suppress_fill: bool) -> bool {
        true
    }

    /// `TValidator::isValid` — the final-form check, run when the field must be
    /// fully valid (the modal-OK / focus-release path). Default: accept.
    fn is_valid(&self, _s: &str) -> bool {
        true
    }

    /// `TValidator::error` — report an invalid final value. Concrete validators
    /// pop up a message box; the abstract base is a no-op.
    ///
    /// TODO(msgbox row 63): wire this to a real message box. No-op until then;
    /// `validate` still returns `false`, so the failure is observable.
    fn error(&self) {}

    /// `TValidator::validate` — **non-virtual in C++**: report the error and fail
    /// iff [`is_valid`](Validator::is_valid) is false, else succeed. Kept as a
    /// provided method (it dispatches through the overridable `is_valid`/`error`).
    fn validate(&self, s: &str) -> bool {
        if self.is_valid(s) {
            true
        } else {
            self.error();
            false
        }
    }

    /// Whether the validator's status is `vsOk` (`TValidator::status == vsOk`) —
    /// consulted by `TInputLine::valid(cmValid)`. The abstract base never sets a
    /// non-OK status, so the default is `true`; `TPXPictureValidator` (row 62)
    /// overrides to report a syntax error (`vsSyntax`).
    fn is_status_ok(&self) -> bool {
        true
    }

    /// `TValidator::transfer(…, vtGetData)` under **D10**. `Some(typed value)`
    /// only when the validator has transfer enabled (C++ `options & voTransfer`);
    /// `None` means "I don't transfer — the input line keeps its text value".
    /// Base: `None`. (`vtDataSize` is moot under D10 — the typed value carries its
    /// own size.)
    ///
    /// NOTE: this is a [`Validator`]-trait method, **not** a `View`-trait method —
    /// there is deliberately no `tvision-macros/src/specs.rs` forwarder and no
    /// `delegate_view` spy entry for it.
    fn transfer_get(&self, _s: &str) -> Option<FieldValue> {
        None
    }

    /// `TValidator::transfer(…, vtSetData)` under **D10** — format a typed value
    /// back to the field's text. `Some(text)` only when transfer-enabled AND `v`
    /// is the type this validator handles; `None` → the input line falls back to
    /// its Text path. Base: `None`.
    ///
    /// NOTE: like [`transfer_get`](Validator::transfer_get), this is a
    /// [`Validator`]-trait method, **not** a `View`-trait method — no `specs.rs`
    /// forwarder / `delegate_view` spy entry exists for it.
    fn transfer_set(&self, _v: &FieldValue) -> Option<String> {
        None
    }
}

// ── Concrete validators (rows 58, 60, 61) ────────────────────────────────────

/// `TFilterValidator` (row 58) — accepts only characters from an allowed set.
///
/// C++ origin: `TFilterValidator::isValid` / `isValidInput` both use
/// `strspn(s, validChars) == strlen(s)`, i.e. **every character of `s` must
/// be a member of `validChars`**. Empty input passes (both spans are 0).
///
/// ## Deviations
/// - `validChars` (`char*`) → owned `String` (D1 naming + Rust ownership).
/// - Membership is tested **per Unicode `char`** (`valid_chars.contains(c)`),
///   where C++ `strspn` tests **per byte**. For the realistic ASCII charsets a
///   filter validator carries these are identical; they would only diverge if
///   `valid_chars` itself held multibyte characters (then the C++ matches
///   individual UTF-8 bytes, we match whole chars) — not a case any caller hits.
/// - Streaming (`read`/`write`/`name`) dropped project-wide (D12).
/// - Destructor (`delete[] validChars`) moot — Rust drop handles it.
/// - `error()` body is a TODO breadcrumb (row-63 `messageBox` not yet built).
pub struct FilterValidator {
    valid_chars: String,
}

impl FilterValidator {
    /// `TFilterValidator(TStringView aValidChars)` — build a filter from the
    /// set of accepted characters.
    pub fn new(valid_chars: impl Into<String>) -> Self {
        Self {
            valid_chars: valid_chars.into(),
        }
    }
}

impl Validator for FilterValidator {
    /// `TFilterValidator::isValid` — every char of `s` must be in `valid_chars`.
    fn is_valid(&self, s: &str) -> bool {
        s.chars().all(|c| self.valid_chars.contains(c))
    }

    /// `TFilterValidator::isValidInput` — same check applied while typing;
    /// `suppress_fill` is ignored (Filter never auto-fills).
    fn is_valid_input(&self, s: &mut String, _suppress_fill: bool) -> bool {
        self.is_valid(s)
    }

    /// `TFilterValidator::error` — C++ calls `messageBox(mfError|mfOKButton, …)`.
    /// TODO(row 63): messageBox(mfError|mfOKButton, "Invalid character in input")
    fn error(&self) {}
}

/// `TLookupValidator` (row 60) — the realized abstract base for lookup-style
/// validators.
///
/// In C++, `TLookupValidator` is an abstract intermediate class whose sole
/// purpose is to route `isValid` through a virtual `lookup(s)`. Its own
/// `lookup` returns `True` (accept-all). Per **D2** (inheritance →
/// trait/composition), this virtual indirection **collapses**: each concrete
/// lookup validator folds `lookup()` directly into its `is_valid`. This
/// unit-struct therefore realises the *abstract base's own behavior* —
/// accept-all — and nothing else. `TStringLookupValidator` (row 61) is the
/// concrete override.
///
/// ## Object safety
/// Stored as `Box<dyn Validator>` alongside other validators.
pub struct LookupValidator;

impl LookupValidator {
    /// Construct the accept-all lookup base (analogous to `TLookupValidator()`).
    pub fn new() -> Self {
        Self
    }
}

impl Default for LookupValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Accept-all: all default methods from the base `Validator` trait suffice.
impl Validator for LookupValidator {}

/// `TStringLookupValidator` (row 61) — valid iff the input exactly matches
/// one entry in an owned list of strings.
///
/// C++ `lookup(s)` used `TStringCollection::firstThat(stringMatch, …)` where
/// `stringMatch` is `strcmp == 0`. Per **D2**, the virtual `lookup()` collapses
/// into `is_valid`; the `TStringCollection*` becomes an owned `Vec<String>`.
/// `newStringList` replaces the list; C++ `destroy(strings)` cleanup is moot
/// under Rust ownership.
///
/// ## Deviations
/// - `TStringCollection*` → `Vec<String>` (owned, no heap-manual management).
/// - `lookup()` virtual collapse into `is_valid` (D2).
/// - Streaming dropped (D12). Destructor moot.
/// - `error()` is a TODO breadcrumb (row-63 `messageBox` not yet built).
pub struct StringLookupValidator {
    strings: Vec<String>,
}

impl StringLookupValidator {
    /// `TStringLookupValidator(TStringCollection* aStrings)` — build from a list
    /// of accepted strings.
    pub fn new(strings: Vec<String>) -> Self {
        Self { strings }
    }

    /// `TStringLookupValidator::newStringList` — replace the accepted-string list.
    /// C++ `destroy(strings)` cleanup is moot; the old `Vec` is dropped here.
    pub fn new_string_list(&mut self, strings: Vec<String>) {
        self.strings = strings;
    }
}

impl Validator for StringLookupValidator {
    /// `TStringLookupValidator::lookup` (collapsed into `is_valid` per D2) —
    /// accepts `s` iff it exactly matches (`strcmp == 0`) some entry in the list.
    fn is_valid(&self, s: &str) -> bool {
        self.strings.iter().any(|x| x == s)
    }

    /// `TStringLookupValidator::error` — C++ calls `messageBox(mfError|mfOKButton, …)`.
    /// TODO(row 63): messageBox(mfError|mfOKButton, "Input is not in list of valid strings")
    fn error(&self) {}
}

/// `TRangeValidator` (row 59) — `TRangeValidator : public TFilterValidator`.
///
/// A numeric validator: gates input through a digit charset filter (embedded
/// [`FilterValidator`], D2 "Range IS-A Filter"), then on the final check parses
/// the text and requires it to fall within `[min, max]`.
///
/// ## C++ origin
/// `TRangeValidator(aMin, aMax)` selects its charset by sign of `aMin`:
/// `validUnsignedChars = "+0123456789"` when `aMin >= 0`, else
/// `validSignedChars = "+-0123456789"`. `isValid` overrides `TFilterValidator`'s
/// (charset gate, then `sscanf("%ld")`, then range); `isValidInput` is **not**
/// overridden — it is inherited from `TFilterValidator` (charset-only while
/// typing, **no** range check). `transfer` does typed get/set of a `long` when
/// `options & voTransfer` is set.
///
/// ## Deviations
/// - Inheritance → embed-and-delegate (**D2**): a `FilterValidator` field, built
///   from the sign-selected charset; `is_valid_input` forwards to it.
/// - The `options & voTransfer` bit → a single `transfer_enabled: bool` (C++
///   `options` defaults to 0, so transfer is **OFF** by default). No general
///   `options` bitfield / `voFill` / `voReserved` is built — Range uses only this.
/// - `transfer` (`TVTransfer`) → the typed **D10** [`transfer_get`] /
///   [`transfer_set`] pair over [`FieldValue::Int`]. `vtDataSize` is moot (D10).
/// - `min`/`max` are `i32` (C++ `int32_t`); C++'s `value` is `long` but `isValid`
///   bounds it into `[min, max] ⊆ i32`, so [`FieldValue::Int`] is faithful.
/// - Streaming (`read`/`write`/`name`) dropped project-wide (D12); destructor moot.
/// - `error()` is a TODO breadcrumb (row-63 `messageBox` not yet built).
///
/// [`transfer_get`]: Validator::transfer_get
/// [`transfer_set`]: Validator::transfer_set
pub struct RangeValidator {
    /// Embedded filter (D2: Range IS-A Filter) — the sign-selected digit charset.
    filter: FilterValidator,
    min: i32,
    max: i32,
    /// C++ `options & voTransfer`; default OFF (C++ `options` defaults to 0).
    transfer_enabled: bool,
}

/// Parse the leading numeric value of a range-validator field — the **D10**
/// successor to C++ `sscanf(s, "%ld")`.
///
/// ## Deliberate deviation (sscanf vs `str::parse`)
/// C++ `sscanf("%ld")` parses a *leading* optional-sign+digits run and **ignores
/// trailing junk** (`"12+3"` → `12`). Rust `str::parse::<i32>()` is **stricter**:
/// it rejects trailing junk and a lone `"+"`/`"-"`. Because the charset filter
/// already restricts the field to `[+-0-9]`, clean numeric input behaves
/// identically; the only divergence is pathological mid-string sign/junk (e.g.
/// `"12+3"`), which `sscanf` truncate-accepts and we reject — an acceptable,
/// stricter simplification. We `.trim()` first (whitespace is not in the charset,
/// so this only matters for direct callers) and never panic.
fn parse_long(s: &str) -> Option<i32> {
    s.trim().parse::<i32>().ok()
}

impl RangeValidator {
    /// `TRangeValidator(aMin, aMax)`. `min >= 0` → `"+0123456789"` (unsigned),
    /// else `"+-0123456789"` (signed). Transfer is OFF by default.
    pub fn new(min: i32, max: i32) -> Self {
        let chars = if min >= 0 {
            "+0123456789"
        } else {
            "+-0123456789"
        };
        Self {
            filter: FilterValidator::new(chars),
            min,
            max,
            transfer_enabled: false,
        }
    }

    /// Enable/disable the typed `transfer` (C++ `options |= voTransfer`). Default
    /// OFF — until enabled, [`transfer_get`](Validator::transfer_get) /
    /// [`transfer_set`](Validator::transfer_set) return `None` and the input line
    /// keeps its text value.
    pub fn set_transfer(&mut self, enabled: bool) {
        self.transfer_enabled = enabled;
    }
}

impl Validator for RangeValidator {
    /// `TRangeValidator::isValid` — charset gate (`TFilterValidator::isValid`)
    /// first, then parse, then the `[min, max]` range check.
    fn is_valid(&self, s: &str) -> bool {
        self.filter.is_valid(s) && parse_long(s).is_some_and(|v| v >= self.min && v <= self.max)
    }

    /// **Inherited** `TFilterValidator::isValidInput` — charset-only while typing,
    /// **no** range check (Range does not override `isValidInput`). So a partial,
    /// out-of-range number is accepted as input; the range is enforced only at
    /// [`is_valid`](RangeValidator::is_valid) (final check).
    fn is_valid_input(&self, s: &mut String, suppress_fill: bool) -> bool {
        self.filter.is_valid_input(s, suppress_fill)
    }

    /// `TRangeValidator::transfer(…, vtGetData)` (D10) — when transfer is enabled,
    /// the field text as [`FieldValue::Int`]. A failed parse falls back to
    /// `Int(0)`: C++ leaves `value` uninitialized on a failed `sscanf`, but
    /// transfer only runs on already-valid data, so this is unreachable-but-safe.
    fn transfer_get(&self, s: &str) -> Option<FieldValue> {
        self.transfer_enabled
            .then(|| FieldValue::Int(parse_long(s).unwrap_or(0)))
    }

    /// `TRangeValidator::transfer(…, vtSetData)` (D10) — format an [`Int`] back to
    /// text (`sprintf("%ld")`). `None` when transfer is disabled or `v` is not an
    /// `Int` (the input line then takes its Text path).
    ///
    /// [`Int`]: FieldValue::Int
    fn transfer_set(&self, v: &FieldValue) -> Option<String> {
        if !self.transfer_enabled {
            return None;
        }
        match v {
            FieldValue::Int(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /// `TRangeValidator::error` — C++ pops
    /// `messageBox(mfError|mfOKButton, "Value not in the range %ld to %ld", min, max)`.
    /// TODO(row 63): messageBox(mfError|mfOKButton, "Value not in the range {min} to {max}")
    fn error(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The abstract base accepts everything and reports OK.
    struct AcceptAll;
    impl Validator for AcceptAll {}

    /// A validator that rejects anything not equal to its target — exercises the
    /// non-default path through `validate`/`is_valid`.
    struct OnlyExact(&'static str);
    impl Validator for OnlyExact {
        fn is_valid(&self, s: &str) -> bool {
            s == self.0
        }
    }

    #[test]
    fn default_methods_accept() {
        let v = AcceptAll;
        let mut s = String::from("anything");
        assert!(v.is_valid_input(&mut s, false));
        assert_eq!(s, "anything", "default is_valid_input does not modify");
        assert!(v.is_valid("x"));
        assert!(v.validate("x"));
        assert!(v.is_status_ok());
    }

    #[test]
    fn validate_fails_when_is_valid_false() {
        let v = OnlyExact("ok");
        assert!(v.validate("ok"));
        assert!(!v.validate("nope"));
        assert!(v.is_valid("ok"));
        assert!(!v.is_valid("nope"));
    }

    #[test]
    fn is_object_safe() {
        // Compiles only if Validator is object-safe (the InputLine storage form).
        let v: Box<dyn Validator> = Box::new(OnlyExact("ok"));
        assert!(v.validate("ok"));
        assert!(!v.validate("no"));
    }

    // ── FilterValidator (row 58) ──────────────────────────────────────────────

    #[test]
    fn filter_accepts_all_valid_chars() {
        let v = FilterValidator::new("0123456789");
        assert!(v.is_valid("42"));
        assert!(v.is_valid("0"));
    }

    #[test]
    fn filter_rejects_out_of_set_char() {
        let v = FilterValidator::new("0123456789");
        // 'a' is not in the digit set — must reject.
        assert!(!v.is_valid("12a3"));
        assert!(!v.is_valid("a"));
    }

    #[test]
    fn filter_accepts_empty_string() {
        // strspn("", validChars) == strlen("") == 0 → accepts.
        let v = FilterValidator::new("abc");
        assert!(v.is_valid(""));
    }

    #[test]
    fn filter_is_valid_input_agrees_with_is_valid() {
        let v = FilterValidator::new("abc");
        let mut good = String::from("ab");
        let mut bad = String::from("ax");
        assert!(v.is_valid_input(&mut good, false));
        assert!(!v.is_valid_input(&mut bad, false));
        // is_valid_input must not mutate the string.
        assert_eq!(good, "ab");
        assert_eq!(bad, "ax");
    }

    #[test]
    fn filter_validate_returns_false_on_rejected() {
        let v = FilterValidator::new("abc");
        assert!(v.validate("abc"));
        assert!(!v.validate("abx")); // 'x' not in set
    }

    #[test]
    fn filter_object_safe_as_boxed_trait() {
        // Verifies the storage form `Option<Box<dyn Validator>>` compiles.
        let v: Box<dyn Validator> = Box::new(FilterValidator::new("abc"));
        assert!(v.is_valid("ab"));
        assert!(!v.is_valid("az"));
    }

    // ── LookupValidator (row 60) ──────────────────────────────────────────────

    #[test]
    fn lookup_validator_accepts_anything() {
        let v = LookupValidator::new();
        assert!(v.is_valid(""));
        assert!(v.is_valid("anything at all"));
        assert!(v.validate("whatever"));
        assert!(v.is_status_ok());
    }

    #[test]
    fn lookup_validator_default_impl() {
        // Default::default() must work.
        let v: LookupValidator = Default::default();
        let mut s = String::from("test");
        assert!(v.is_valid_input(&mut s, false));
    }

    #[test]
    fn lookup_validator_object_safe() {
        let v: Box<dyn Validator> = Box::new(LookupValidator::new());
        assert!(v.validate("foo"));
    }

    // ── StringLookupValidator (row 61) ────────────────────────────────────────

    #[test]
    fn string_lookup_accepts_exact_member() {
        let v = StringLookupValidator::new(vec!["yes".into(), "no".into()]);
        assert!(v.is_valid("yes"));
        assert!(v.is_valid("no"));
    }

    #[test]
    fn string_lookup_rejects_non_member() {
        let v = StringLookupValidator::new(vec!["yes".into(), "no".into()]);
        assert!(!v.is_valid("maybe"));
        assert!(!v.is_valid("YES")); // case-sensitive (strcmp == 0)
    }

    #[test]
    fn string_lookup_rejects_prefix_or_substring() {
        // "ye" is a prefix of "yes" — exact match only (strcmp semantics).
        let v = StringLookupValidator::new(vec!["yes".into()]);
        assert!(!v.is_valid("ye"));
        assert!(!v.is_valid("es"));
        assert!(!v.is_valid("yes ")); // trailing space → not equal
    }

    #[test]
    fn string_lookup_rejects_against_empty_list() {
        let v = StringLookupValidator::new(vec![]);
        assert!(!v.is_valid("anything"));
    }

    #[test]
    fn string_lookup_new_string_list_replaces_set() {
        let mut v = StringLookupValidator::new(vec!["old".into()]);
        assert!(v.is_valid("old"));
        assert!(!v.is_valid("new"));
        v.new_string_list(vec!["new".into()]);
        assert!(!v.is_valid("old")); // was-accepted now rejected
        assert!(v.is_valid("new")); // was-rejected now accepted
    }

    #[test]
    fn string_lookup_validate_false_on_non_member() {
        let v = StringLookupValidator::new(vec!["ok".into()]);
        assert!(v.validate("ok"));
        assert!(!v.validate("bad"));
    }

    #[test]
    fn string_lookup_object_safe() {
        let v: Box<dyn Validator> = Box::new(StringLookupValidator::new(vec!["x".into()]));
        assert!(v.is_valid("x"));
        assert!(!v.is_valid("y"));
    }

    // ── RangeValidator (row 59) ───────────────────────────────────────────────

    #[test]
    fn range_is_valid_in_range_accepts() {
        let v = RangeValidator::new(1, 100);
        assert!(v.is_valid("1"));
        assert!(v.is_valid("50"));
        assert!(v.is_valid("100"));
    }

    #[test]
    fn range_is_valid_below_min_rejects() {
        let v = RangeValidator::new(10, 100);
        assert!(!v.is_valid("9"));
        assert!(!v.is_valid("0"));
    }

    #[test]
    fn range_is_valid_above_max_rejects() {
        let v = RangeValidator::new(1, 10);
        assert!(!v.is_valid("11"));
        assert!(!v.is_valid("999"));
    }

    #[test]
    fn range_is_valid_rejects_non_charset_at_filter_gate() {
        // 'a' is not in "+0123456789" — the filter gate fires before any parse.
        let v = RangeValidator::new(0, 100);
        assert!(!v.is_valid("1a2"));
        assert!(!v.is_valid("abc"));
    }

    #[test]
    fn range_is_valid_rejects_sign_only_string() {
        // "+" passes the charset gate but `parse_long` fails → reject.
        let v = RangeValidator::new(0, 100);
        assert!(!v.is_valid("+"));
        let signed = RangeValidator::new(-5, 5);
        assert!(!signed.is_valid("-"));
    }

    #[test]
    fn range_charset_selected_by_sign_of_min() {
        // DISCRIMINATING: the signed validator's charset accepts '-', the unsigned
        // one rejects it at the FILTER gate (before any parse).
        let signed = RangeValidator::new(-10, 10);
        assert!(signed.is_valid("-5"));
        let unsigned = RangeValidator::new(0, 10);
        assert!(!unsigned.is_valid("-5")); // '-' not in unsigned charset
    }

    #[test]
    fn range_is_valid_input_is_charset_only_not_range_checked() {
        // DISCRIMINATING: isValidInput is INHERITED from TFilterValidator — it
        // checks charset only, NOT the range. "999" is accepted as input even
        // though is_valid("999") is false for range 1..=10. Would fail if someone
        // "helpfully" range-checked during typing.
        let v = RangeValidator::new(1, 10);
        let mut s = String::from("999");
        assert!(v.is_valid_input(&mut s, false));
        assert!(!v.is_valid("999"));
        assert_eq!(
            s, "999",
            "is_valid_input must not mutate (Filter never fills)"
        );
    }

    #[test]
    fn range_transfer_disabled_by_default_returns_none() {
        let v = RangeValidator::new(0, 100);
        assert_eq!(v.transfer_get("42"), None);
        assert_eq!(v.transfer_set(&FieldValue::Int(42)), None);
    }

    #[test]
    fn range_transfer_enabled_round_trips_int() {
        let mut v = RangeValidator::new(0, 100);
        v.set_transfer(true);
        assert_eq!(v.transfer_get("42"), Some(FieldValue::Int(42)));
        assert_eq!(v.transfer_set(&FieldValue::Int(42)), Some("42".to_string()));
    }

    #[test]
    fn range_transfer_set_wrong_type_returns_none() {
        let mut v = RangeValidator::new(0, 100);
        v.set_transfer(true);
        // Text is not the type RangeValidator transfers → None (Text fallback path).
        assert_eq!(v.transfer_set(&FieldValue::Text("42".into())), None);
    }

    #[test]
    fn range_transfer_get_unparseable_falls_back_to_zero() {
        // Unreachable-but-safe: transfer only runs on already-valid data; a bad
        // parse falls back to Int(0) rather than panicking.
        let mut v = RangeValidator::new(0, 100);
        v.set_transfer(true);
        assert_eq!(v.transfer_get("+"), Some(FieldValue::Int(0)));
    }

    #[test]
    fn range_object_safe_as_boxed_trait() {
        let v: Box<dyn Validator> = Box::new(RangeValidator::new(1, 10));
        assert!(v.is_valid("5"));
        assert!(!v.is_valid("11"));
    }
}
