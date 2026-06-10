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
use crate::view::Context;
use regex_automata::{
    Anchored,
    dfa::{Automaton, StartKind, dense},
    util::start::Config as StartConfig,
};

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
    /// pop up a message box via the async-modal-from-a-view seam
    /// ([`Context::request_message_box`], `answer_to`/`then_command` both `None` —
    /// informational, OK-only); the abstract base is a no-op.
    ///
    /// **NOT a `View` method** — no `tvision-macros/src/specs.rs` forwarder.
    fn error(&self, _ctx: &mut Context) {}

    /// `TValidator::validate` — **non-virtual in C++**: report the error and fail
    /// iff [`is_valid`](Validator::is_valid) is false, else succeed. Kept as a
    /// provided method (it dispatches through the overridable `is_valid`/`error`).
    /// Threads `&mut Context` so a failing validator's `error` can request its box.
    fn validate(&self, s: &str, ctx: &mut Context) -> bool {
        if self.is_valid(s) {
            true
        } else {
            self.error(ctx);
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
/// - `error()` is live: `messageBox(mfError|mfOKButton, …)` via the
///   async-modal-from-a-view seam (`ctx.request_message_box`, informational, OK-only).
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

    /// `TFilterValidator::error` — `messageBox(mfError|mfOKButton, …)` via the
    /// async-modal-from-a-view seam (informational, OK-only).
    fn error(&self, ctx: &mut Context) {
        ctx.request_message_box(
            "Invalid character in input".to_string(),
            crate::dialog::MessageBoxKind::Error,
            crate::dialog::MessageBoxButtons::ok(),
            None,
            None,
        );
    }
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
/// - `error()` is live: `messageBox(mfError|mfOKButton, …)` via the
///   async-modal-from-a-view seam (`ctx.request_message_box`, informational, OK-only).
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

    /// `TStringLookupValidator::error` — `messageBox(mfError|mfOKButton, …)` via the
    /// async-modal-from-a-view seam (informational, OK-only).
    fn error(&self, ctx: &mut Context) {
        ctx.request_message_box(
            "Input is not in list of valid strings".to_string(),
            crate::dialog::MessageBoxKind::Error,
            crate::dialog::MessageBoxButtons::ok(),
            None,
            None,
        );
    }
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
/// - `error()` is live: `messageBox(mfError|mfOKButton, …)` via the
///   async-modal-from-a-view seam (`ctx.request_message_box`, informational, OK-only).
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
    /// `messageBox(mfError|mfOKButton, "Value not in the range %ld to %ld", min, max)`
    /// via the async-modal-from-a-view seam (informational, OK-only).
    fn error(&self, ctx: &mut Context) {
        ctx.request_message_box(
            format!("Value not in the range {} to {}", self.min, self.max),
            crate::dialog::MessageBoxKind::Error,
            crate::dialog::MessageBoxButtons::ok(),
            None,
            None,
        );
    }
}

// ── TPXPictureValidator (row 62) ─────────────────────────────────────────────

/// `TPicResult` — the result of running a Paradox picture mask against an input.
///
/// Mirrors the C++ enum `TPicResult {prComplete, prIncomplete, prEmpty, prError,
/// prSyntax, prAmbiguous, prIncompNoFill}` (`include/tvision/validate.h`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PicResult {
    Complete,
    Incomplete,
    Empty,
    Error,
    Syntax,
    Ambiguous,
    IncompNoFill,
}

/// `isNumber(char)` — ASCII digit.
fn is_number(ch: u8) -> bool {
    ch.is_ascii_digit()
}

/// `isLetter(char)` — C++ does `ch &= 0xdf; return 'A' <= ch <= 'Z'`. The `& 0xdf`
/// folds the ASCII case bit, so both `a..z` and `A..Z` qualify. Ported as the
/// same bit trick to stay byte-faithful.
fn is_letter(ch: u8) -> bool {
    (ch & 0xdf).is_ascii_uppercase()
}

/// `isSpecial(char, const char* special)` — membership of `ch` in the byte set
/// `special` (C++ `memchr`).
fn is_special(ch: u8, special: &[u8]) -> bool {
    special.contains(&ch)
}

/// `isComplete(TPicResult)` — `prComplete || prAmbiguous`.
fn is_complete(r: PicResult) -> bool {
    matches!(r, PicResult::Complete | PicResult::Ambiguous)
}

/// `isIncomplete(TPicResult)` — `prIncomplete || prIncompNoFill`.
fn is_incomplete(r: PicResult) -> bool {
    matches!(r, PicResult::Incomplete | PicResult::IncompNoFill)
}

/// C++ `uppercase(char)` — ASCII uppercase fold.
fn uppercase(ch: u8) -> u8 {
    ch.to_ascii_uppercase()
}

/// Read `pic[i]` faithfully to the C++ `char*`: an in-range byte, or the NUL
/// terminator (`0`) when `i` is at/past the end. C++ relies on the trailing NUL
/// to stop scans (e.g. `pic[index]` after `index++` reaching `strlen(pic)`, or
/// `pic[j+1]` in `checkComplete`); replicating it keeps every `pic[...]` read
/// panic-free and bit-faithful.
fn pic_at(pic: &[u8], i: i32) -> u8 {
    if i < 0 {
        return 0;
    }
    pic.get(i as usize).copied().unwrap_or(0)
}

/// `toGroupEnd(int& i, int termCh)` — advance `i` past one character or one
/// balanced picture group (`[...]` / `{...}`), stopping at `termCh`. A free
/// function reading only `pic` (C++ member fn touches no `jndex`/`input`); kept
/// free so callers can pass either a local cursor or `self.index` without an
/// aliasing borrow of `self` (see `skip_to_comma`).
fn to_group_end(pic: &[u8], i: &mut i32, term_ch: i32) {
    let mut brk_level = 0i32;
    let mut brc_level = 0i32;
    loop {
        if *i == term_ch {
            return;
        }
        match pic_at(pic, *i) {
            b'[' => brk_level += 1,
            b']' => brk_level -= 1,
            b'{' => brc_level += 1,
            b'}' => brc_level -= 1,
            b';' => *i += 1,
            _ => {}
        }
        *i += 1;
        if brk_level == 0 && brc_level == 0 {
            break;
        }
    }
}

/// `Boolean TPXPictureValidator::syntaxCheck()` — free function (reads only the
/// mask). Rejects an empty mask, a mask ending in `;`, or unbalanced
/// `[]`/`{}` nesting.
fn syntax_check(pic: &[u8]) -> bool {
    if pic.is_empty() {
        return false;
    }
    if pic[pic.len() - 1] == b';' {
        return false;
    }
    let mut i = 0i32;
    let mut brk_level = 0i32;
    let mut brc_level = 0i32;
    let len = pic.len() as i32;
    while i < len {
        match pic_at(pic, i) {
            b'[' => brk_level += 1,
            b']' => brk_level -= 1,
            b'{' => brc_level += 1,
            b'}' => brc_level -= 1,
            b';' => i += 1,
            _ => {}
        }
        i += 1;
    }
    brk_level == 0 && brc_level == 0
}

/// The transient picture scanner — the per-`picture()`-call scratch state.
///
/// In C++ `index`/`jndex` are member variables only because threading them
/// through the ~6 mutually-recursive helpers by hand is tedious; they are reset
/// to 0 at the top of every `picture()` call, i.e. they are **per-call scratch**,
/// not persistent validator state. Our [`Validator`] methods are `&self`
/// (object-safe), so the scanning state lives here, created fresh per call.
///
/// ## Byte-faithful to the C++ `char*` machine
/// C++ operates on `char*` byte-by-byte (`uppercase`, `& 0xdf`, `input[jndex]=ch`).
/// We port at the **byte level** (`&[u8]` / `Vec<u8>`); picture masks and the
/// inputs to such fields are ASCII, so this is exact. Multibyte UTF-8 in such a
/// field is out of scope (same posture as `FilterValidator`'s byte-vs-char note).
struct Picture<'a> {
    /// The mask (C++ `pic`).
    pic: &'a [u8],
    /// The working input buffer (C++ `input`); mutated in place and may GROW via
    /// autofill.
    input: Vec<u8>,
    /// C++ `index` — cursor into `pic`.
    index: i32,
    /// C++ `jndex` — cursor into `input`.
    jndex: i32,
}

impl<'a> Picture<'a> {
    fn new(pic: &'a [u8], input: Vec<u8>) -> Self {
        Self {
            pic,
            input,
            index: 0,
            jndex: 0,
        }
    }

    /// `pic[index]` faithful read (NUL past end).
    fn pic_at(&self, i: i32) -> u8 {
        pic_at(self.pic, i)
    }

    /// `input[jndex]` faithful read (NUL past end). Used where C++ may touch the
    /// terminator.
    fn input_at(&self, j: i32) -> u8 {
        if j < 0 {
            return 0;
        }
        self.input.get(j as usize).copied().unwrap_or(0)
    }

    /// `consume(char ch, char* input)` — write `ch` into `input[jndex]`, advance
    /// both cursors. (`scan`'s `jndex >= strlen(input)` guard ensures `jndex` is
    /// in range before any call, so the index-assign is safe.)
    fn consume(&mut self, ch: u8) {
        self.input[self.jndex as usize] = ch;
        self.index += 1;
        self.jndex += 1;
    }

    /// `skipToComma(int termCh)` — advance `index` over groups until a comma
    /// separator or `termCh`; step past the comma. Returns whether `index <
    /// termCh` (i.e. there is another alternative to try).
    fn skip_to_comma(&mut self, term_ch: i32) -> bool {
        loop {
            // `to_group_end` mutates a cursor; copy `self.index` across the call
            // to avoid aliasing `&self` (free fn reads only `pic`).
            let mut idx = self.index;
            to_group_end(self.pic, &mut idx, term_ch);
            self.index = idx;
            if self.index == term_ch || self.pic_at(self.index) == b',' {
                break;
            }
        }
        if self.pic_at(self.index) == b',' {
            self.index += 1;
        }
        self.index < term_ch
    }

    /// `calcTerm(int termCh)` — the end index of the group starting at `index`.
    fn calc_term(&self, term_ch: i32) -> i32 {
        let mut k = self.index;
        to_group_end(self.pic, &mut k, term_ch);
        k
    }

    /// `iteration(char* input, int inTerm)` — the `*[n]<group>` repeat operator.
    /// `index` points at the `*`. Reads the optional repeat count, then runs the
    /// group exactly `itr` times (count given) or greedily (count 0 → "any
    /// number").
    fn iteration(&mut self, in_term: i32) -> PicResult {
        let mut itr = 0i32;
        let mut rslt = PicResult::Error;

        self.index += 1; // Skip '*'

        // Retrieve number
        while is_number(self.pic_at(self.index)) {
            itr = itr * 10 + (self.pic_at(self.index) - b'0') as i32;
            self.index += 1;
        }

        let k = self.index;
        let term_ch = self.calc_term(in_term);

        // If Itr is 0 allow any number, otherwise enforce the number
        if itr != 0 {
            for _l in 1..=itr {
                self.index = k;
                rslt = self.process(term_ch);
                if !is_complete(rslt) {
                    // Empty means incomplete since all are required
                    if rslt == PicResult::Empty {
                        rslt = PicResult::Incomplete;
                    }
                    return rslt;
                }
            }
        } else {
            loop {
                self.index = k;
                rslt = self.process(term_ch);
                if rslt != PicResult::Complete {
                    break;
                }
            }
            if rslt == PicResult::Empty || rslt == PicResult::Error {
                self.index += 1;
                rslt = PicResult::Ambiguous;
            }
        }
        self.index = term_ch;

        rslt
    }

    /// `group(char* input, int inTerm)` — a `{...}` (required) or `[...]`
    /// (optional) bracketed picture group. `index` points at the opening bracket.
    fn group(&mut self, in_term: i32) -> PicResult {
        let term_ch = self.calc_term(in_term);
        self.index += 1;
        let rslt = self.process(term_ch - 1);

        if !is_incomplete(rslt) {
            self.index = term_ch;
        }

        rslt
    }

    /// `checkComplete(TPicResult rslt, int termCh)` — on an incomplete result,
    /// see whether all that remains in the mask is optional (`[...]` groups or
    /// unbounded `*` iterations); if so the input is ambiguously complete.
    fn check_complete(&mut self, rslt: PicResult, term_ch: i32) -> PicResult {
        let mut rslt = rslt;
        let mut j = self.index;
        let mut status = true;

        if is_incomplete(rslt) {
            // Skip optional pieces
            while status {
                match self.pic_at(j) {
                    b'[' => {
                        to_group_end(self.pic, &mut j, term_ch);
                    }
                    b'*' => {
                        if !is_number(self.pic_at(j + 1)) {
                            j += 1;
                        }
                        to_group_end(self.pic, &mut j, term_ch);
                    }
                    _ => {
                        status = false;
                    }
                }
            }

            if j == term_ch {
                rslt = PicResult::Ambiguous;
            }
        }

        rslt
    }

    /// `scan(char* input, int termCh)` — match the input against one comma-free
    /// run of the mask (up to `termCh` or a `,`), consuming input as it goes.
    fn scan(&mut self, term_ch: i32) -> PicResult {
        let r_scan = PicResult::Error;
        let mut rslt = PicResult::Empty;

        while self.index != term_ch && self.pic_at(self.index) != b',' {
            if self.jndex >= self.input.len() as i32 {
                return self.check_complete(rslt, term_ch);
            }

            let ch = self.input_at(self.jndex);
            match self.pic_at(self.index) {
                b'#' => {
                    if !is_number(ch) {
                        return PicResult::Error;
                    } else {
                        self.consume(ch);
                    }
                }
                b'?' => {
                    if !is_letter(ch) {
                        return PicResult::Error;
                    } else {
                        self.consume(ch);
                    }
                }
                b'&' => {
                    if !is_letter(ch) {
                        return PicResult::Error;
                    } else {
                        self.consume(uppercase(ch));
                    }
                }
                b'!' => {
                    self.consume(uppercase(ch));
                }
                b'@' => {
                    self.consume(ch);
                }
                b'*' => {
                    rslt = self.iteration(term_ch);
                    if !is_complete(rslt) {
                        return rslt;
                    }
                    if rslt == PicResult::Error {
                        rslt = PicResult::Ambiguous;
                    }
                }
                b'{' => {
                    rslt = self.group(term_ch);
                    if !is_complete(rslt) {
                        return rslt;
                    }
                }
                b'[' => {
                    rslt = self.group(term_ch);
                    if is_incomplete(rslt) {
                        return rslt;
                    }
                    if rslt == PicResult::Error {
                        rslt = PicResult::Ambiguous;
                    }
                }
                _ => {
                    // Literal arm. C++ (tvalidat.cpp:438-451): a `;`-escape
                    // advances past the `;` to the escaped literal; the typed
                    // char must match the mask literal case-insensitively (a
                    // typed space matches any literal — it gets overwritten);
                    // otherwise the run fails. The byte CONSUMED is always
                    // `pic[index]` (the MASK byte), so the buffer is normalized
                    // to the mask's literal — NOT the typed `ch`.
                    if self.pic_at(self.index) == b';' {
                        self.index += 1;
                    }
                    if uppercase(self.pic_at(self.index)) != uppercase(ch) && ch != b' ' {
                        return r_scan;
                    }
                    self.consume(self.pic_at(self.index));
                }
            }

            if rslt == PicResult::Ambiguous {
                rslt = PicResult::IncompNoFill;
            } else {
                rslt = PicResult::Incomplete;
            }
        }

        if rslt == PicResult::IncompNoFill {
            PicResult::Ambiguous
        } else {
            PicResult::Complete
        }
    }

    /// `process(char* input, int termCh)` — try each comma-separated alternative
    /// in the mask run, backtracking on error/incomplete; tracks the best
    /// (farthest-consuming) incomplete to disambiguate.
    fn process(&mut self, term_ch: i32) -> PicResult {
        let mut incomp = false;
        let mut old_i = self.index;
        let old_j = self.jndex;
        let mut incomp_j = 0i32;
        let mut incomp_i = 0i32;
        let mut rslt;
        let mut r_process;

        loop {
            rslt = self.scan(term_ch);

            // Only accept completes if they make it farther in the input
            //   stream from the last incomplete
            if rslt == PicResult::Complete && incomp && self.jndex < incomp_j {
                rslt = PicResult::Incomplete;
                self.jndex = incomp_j;
            }

            if rslt == PicResult::Error || rslt == PicResult::Incomplete {
                r_process = rslt;

                if !incomp && rslt == PicResult::Incomplete {
                    incomp = true;
                    incomp_i = self.index;
                    incomp_j = self.jndex;
                }
                self.index = old_i;
                self.jndex = old_j;
                if !self.skip_to_comma(term_ch) {
                    if incomp {
                        r_process = PicResult::Incomplete;
                        self.index = incomp_i;
                        self.jndex = incomp_j;
                    }
                    return r_process;
                }
                old_i = self.index;
            }

            if rslt != PicResult::Error && rslt != PicResult::Incomplete {
                break;
            }
        }

        if rslt == PicResult::Complete && incomp {
            PicResult::Ambiguous
        } else {
            rslt
        }
    }

    /// `picture(char* input, Boolean autoFill)` — the top-level driver. Resets
    /// the cursors, runs `process`, applies the trailing-input/autofill logic,
    /// and maps the internal `Ambiguous`/`IncompNoFill` results to their public
    /// `Complete`/`Incomplete` equivalents.
    fn run(&mut self, auto_fill: bool) -> PicResult {
        if !syntax_check(self.pic) {
            return PicResult::Syntax;
        }

        if self.input.is_empty() {
            return PicResult::Empty;
        }

        self.jndex = 0;
        self.index = 0;

        let mut rslt = self.process(self.pic.len() as i32);

        if rslt != PicResult::Error && self.jndex < self.input.len() as i32 {
            rslt = PicResult::Error;
        }

        if rslt == PicResult::Incomplete && auto_fill {
            let mut reprocess = false;

            while self.index < self.pic.len() as i32
                && !is_special(self.pic_at(self.index), b"#?&!@*{}[],")
            {
                if self.pic_at(self.index) == b';' {
                    self.index += 1;
                }
                // C++ writes input[end]=pic[index]; input[end+1]=0 — i.e. append
                // one byte (Vec carries its own length, so no NUL is stored).
                self.input.push(self.pic_at(self.index));
                self.index += 1;
                reprocess = true;
            }

            self.jndex = 0;
            self.index = 0;
            if reprocess {
                rslt = self.process(self.pic.len() as i32);
            }
        }

        match rslt {
            PicResult::Ambiguous => PicResult::Complete,
            PicResult::IncompNoFill => PicResult::Incomplete,
            other => other,
        }
    }
}

/// `TPXPictureValidator` (row 62) — the Paradox picture-mask validator.
///
/// Validates and auto-fills input against a Paradox "picture" mask: `#` digit,
/// `?` letter, `&` letter→uppercase, `!` any→uppercase, `@` any, `*` repeat,
/// `{}`/`[]` required/optional groups, `,` alternatives, `;` literal-escape, and
/// any other character is a literal. The matching engine is a recursive state
/// machine ported verbatim from `tvalidat.cpp`.
///
/// ## Design (the idiomatic-Rust crux)
/// C++ keeps the scan cursors `index`/`jndex` as member variables purely to
/// thread them through the mutually-recursive helpers; they are reset per
/// `picture()` call, i.e. they are **per-call scratch**, not validator state.
/// Our [`Validator`] methods are `&self` (object-safe — stored as
/// `Box<dyn Validator>`), so the scanning state lives in a transient
/// [`Picture`] created fresh per call. Operation is byte-level, faithful to the
/// C++ `char*` machine (see [`Picture`]).
///
/// ## Deviations / drops
/// - Streaming (`read`/`write`/`build`/`name`, D12) and the destructor
///   (`delete[] pic`) dropped.
/// - C++ `isValid` copies `s` into a 256-byte stack buffer; we do **not**
///   replicate the 256 cap (the `Vec` grows). Real inputs are maxLen-bounded, so
///   this is a safe, documented deviation.
/// - C++ guards `(pic == 0)`; our `pic` is always a (possibly empty) `String`,
///   never null — an empty/invalid mask yields `Syntax`/`Empty` and the same
///   booleans fall out, so no null check is needed.
/// - `error()` is a row-63 `messageBox` breadcrumb (message preserved below).
pub struct PXPictureValidator {
    /// The mask (C++ `pic`, owned).
    pic: String,
    /// C++ `options & voFill` — auto-fill literals while typing.
    auto_fill: bool,
    /// C++ `status == vsOk` (`false` ⇒ `vsSyntax`). Set in [`new`](Self::new).
    status_ok: bool,
}

impl PXPictureValidator {
    /// `TPXPictureValidator(TStringView aPic, Boolean autoFill)`.
    ///
    /// Runs `picture("", False)` on EMPTY input as a syntax probe: for a
    /// well-formed mask, empty input yields `prEmpty`, so status stays OK; any
    /// other result means the mask syntax is bad and status becomes `vsSyntax`.
    pub fn new(pic: impl Into<String>, auto_fill: bool) -> Self {
        let pic = pic.into();
        let mut p = Picture::new(pic.as_bytes(), Vec::new());
        // C++: status = vsSyntax iff picture(s, False) != prEmpty.
        let status_ok = p.run(false) == PicResult::Empty;
        Self {
            pic,
            auto_fill,
            status_ok,
        }
    }
}

impl Validator for PXPictureValidator {
    /// `isValidInput(char* s, Boolean suppressFill)` — `doFill = voFill &&
    /// !suppressFill`; returns `picture(s, doFill) != prError`. MUTATES `s` in
    /// place (autofill of literals + uppercase transforms — the whole point of a
    /// picture validator).
    fn is_valid_input(&self, s: &mut String, suppress_fill: bool) -> bool {
        let do_fill = self.auto_fill && !suppress_fill;
        let mut p = Picture::new(self.pic.as_bytes(), s.as_bytes().to_vec());
        let r = p.run(do_fill);
        *s = String::from_utf8_lossy(&p.input).into_owned();
        r != PicResult::Error
    }

    /// `isValid(const char* s)` — returns `picture(copy_of_s, False) ==
    /// prComplete`. No write-back (C++ scans a stack copy).
    fn is_valid(&self, s: &str) -> bool {
        let mut p = Picture::new(self.pic.as_bytes(), s.as_bytes().to_vec());
        p.run(false) == PicResult::Complete
    }

    /// `TValidator::status == vsOk` — overrides the base; `vsSyntax` ⇒ `false`.
    fn is_status_ok(&self) -> bool {
        self.status_ok
    }

    /// `error()` — C++ `messageBox(mfError|mfOKButton, "Error in picture
    /// format.\n %s", pic)` via the async-modal-from-a-view seam (informational,
    /// OK-only).
    fn error(&self, ctx: &mut Context) {
        ctx.request_message_box(
            format!("Error in picture format.\n {}", self.pic),
            crate::dialog::MessageBoxKind::Error,
            crate::dialog::MessageBoxButtons::ok(),
            None,
            None,
        );
    }
}

// ── RegexValidator (rstv extension) ─────────────────────────────────────────

/// Error returned when compiling a [`RegexValidator`] pattern.
///
/// A thin wrapper over the regex-automata build error so the dependency's
/// exact error type does not leak into the public API.
#[derive(Debug)]
pub struct RegexError(String);

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RegexError {}

/// rstv-original **extension** (NOT a Turbo Vision port): a [`Validator`]
/// driven by a regular expression, giving the same two-phase behavior as
/// [`PXPictureValidator`] — `is_valid` = the input fully matches the pattern;
/// `is_valid_input` = the input is still a *prefix of some complete match*
/// ("could it still become valid?") — but expressed as a regex so callers can
/// leverage regex knowledge instead of Paradox picture-mask syntax.
///
/// ## Pattern semantics
/// The pattern describes the **complete field value** and is implicitly fully
/// anchored. The user pattern is compiled twice (see [`new`](Self::new)); the
/// effective pattern is `(?:<pattern>)\z` built with an anchored start state.
/// Three pieces cooperate:
/// - **Start anchor** comes from the DFA's anchored-start configuration
///   (`StartKind::Anchored`).
/// - **End anchor**: for [`is_valid`](Validator::is_valid) it is the
///   `next_eoi_state` + `is_match_state` step; the `\z` in the wrapped pattern
///   is what drives [`is_valid_input`](Validator::is_valid_input)'s dead-state
///   rejection (without it, no trailing input could ever be ruled out).
/// - The **`(?:…)` non-capturing group** fixes alternation precedence: it
///   anchors the *whole* alternation, so `cat|dog` means "(cat or dog), then
///   end" rather than "cat, or (dog then `\z`)".
///
/// To stop a malformed pattern from escaping that wrap (e.g. an unbalanced `)`
/// that would close `(?:…)` early and silently defeat end-anchoring), the bare
/// user pattern is validated for self-containment before wrapping — see
/// [`new`](Self::new).
///
/// ## Per-keystroke viability (`is_valid_input`)
/// Uses the DFA **dead-state** test: walk the anchored DFA over the bytes of
/// the input; if a dead state (one from which no match is ever reachable) is
/// reached, the input cannot become valid no matter what is appended → reject.
/// Unlike [`PXPictureValidator`], `RegexValidator` performs **no autofill** and
/// never mutates the input string.
///
/// ## Construction cost and thread safety
/// The DFA is compiled once in [`new`](Self::new) and is thereafter immutable.
/// `RegexValidator` is `Send + Sync`. `\d`/`\w`/`\s` and other Perl-compatible
/// character classes are available (`unicode-perl` feature is enabled).
///
/// [`PXPictureValidator`]: crate::PXPictureValidator
pub struct RegexValidator {
    dfa: dense::DFA<Vec<u32>>,
    /// The original user-supplied pattern, retained for [`Debug`] and [`error`](Validator::error).
    pattern: String,
}

impl std::fmt::Debug for RegexValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The DFA's internal repr is not Debug-friendly to print; show the pattern only.
        f.debug_struct("RegexValidator")
            .field("pattern", &self.pattern)
            .finish()
    }
}

impl RegexValidator {
    /// Compile `pattern` as a description of a complete, valid field value.
    ///
    /// The pattern is wrapped as `(?:<pattern>)\z` (end-anchored) and the DFA
    /// is built with `StartKind::Anchored` (start-anchored). Returns `Err` if
    /// the pattern is syntactically invalid or its DFA exceeds build limits.
    ///
    /// The bare pattern is built and validated *first*, before wrapping: a
    /// pattern that is not self-contained — e.g. an unbalanced `)` that would
    /// close the `(?:…)` group early and silently defeat end-anchoring (the
    /// `\z` binding only part of the pattern) — is rejected here. A balanced
    /// bare pattern cannot break out of the wrap, so this closes that hole.
    pub fn new(pattern: &str) -> Result<Self, RegexError> {
        let cfg = dense::Config::new().start_kind(StartKind::Anchored);
        // Reject patterns that aren't self-contained (paren-injection guard).
        dense::Builder::new()
            .configure(cfg.clone())
            .build(pattern)
            .map_err(|e| RegexError(e.to_string()))?;
        let wrapped = format!(r"(?:{pattern})\z");
        let dfa = dense::Builder::new()
            .configure(cfg)
            .build(&wrapped)
            .map_err(|e| RegexError(e.to_string()))?;
        Ok(Self {
            dfa,
            pattern: pattern.to_string(),
        })
    }

    /// Walk the anchored DFA over the bytes of `s`.
    ///
    /// Returns `(dead, state)` where `dead == true` means the DFA reached a
    /// dead state (no continuation can ever match). `StartKind::Anchored`
    /// guarantees the anchored start state exists, so the `start_state` call
    /// never errors — the `expect` documents that build-time invariant.
    fn walk(&self, s: &str) -> (bool, regex_automata::util::primitives::StateID) {
        let mut st = self
            .dfa
            .start_state(&StartConfig::new().anchored(Anchored::Yes))
            .expect("anchored start state (DFA built with StartKind::Anchored)");
        for &b in s.as_bytes() {
            st = self.dfa.next_state(st, b);
            if self.dfa.is_dead_state(st) {
                return (true, st);
            }
        }
        (false, st)
    }
}

impl Validator for RegexValidator {
    /// The whole input matches the pattern (a complete, valid value).
    ///
    /// Equivalent to `prComplete` in the picture-validator model: both start
    /// and end anchors must be satisfied.
    fn is_valid(&self, s: &str) -> bool {
        let (dead, st) = self.walk(s);
        if dead {
            return false;
        }
        let st = self.dfa.next_eoi_state(st);
        self.dfa.is_match_state(st)
    }

    /// The input is still a prefix of some complete match — "could it become
    /// valid if the user keeps typing?".
    ///
    /// Equivalent to `prIncomplete` (or better) in the picture-validator model:
    /// rejects only when the DFA has reached a dead state (no continuation can
    /// ever lead to a match). Does **not** mutate `s` — unlike
    /// [`PXPictureValidator`], there is no autofill.
    fn is_valid_input(&self, s: &mut String, _suppress_fill: bool) -> bool {
        !self.walk(s).0
    }

    /// Report an invalid final value via the async-modal-from-a-view seam
    /// (informational, OK-only). rstv-original (no C++ counterpart).
    fn error(&self, ctx: &mut Context) {
        ctx.request_message_box(
            format!("Input does not match pattern: {}", self.pattern),
            crate::dialog::MessageBoxKind::Error,
            crate::dialog::MessageBoxButtons::ok(),
            None,
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `validator.validate(s, ctx)` over a throwaway [`Context`] and return the
    /// bool (the test never inspects the requested box here — that is covered by the
    /// program-level async-modal tests). Also returns whether a box was requested.
    fn vd<V: Validator + ?Sized>(v: &V, s: &str) -> bool {
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        v.validate(s, &mut ctx)
    }

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
        assert!(vd(&v, "x"));
        assert!(v.is_status_ok());
    }

    #[test]
    fn validate_fails_when_is_valid_false() {
        let v = OnlyExact("ok");
        assert!(vd(&v, "ok"));
        assert!(!vd(&v, "nope"));
        assert!(v.is_valid("ok"));
        assert!(!v.is_valid("nope"));
    }

    #[test]
    fn is_object_safe() {
        // Compiles only if Validator is object-safe (the InputLine storage form).
        let v: Box<dyn Validator> = Box::new(OnlyExact("ok"));
        assert!(vd(&*v, "ok"));
        assert!(!vd(&*v, "no"));
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
        assert!(vd(&v, "abc"));
        assert!(!vd(&v, "abx")); // 'x' not in set
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
        assert!(vd(&v, "whatever"));
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
        assert!(vd(&*v, "foo"));
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
        assert!(vd(&v, "ok"));
        assert!(!vd(&v, "bad"));
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

    // ── PXPictureValidator (row 62) ───────────────────────────────────────────
    //
    // Golden vectors hand-traced against `tvalidat.cpp`. If a port disagrees with
    // one, the port (or the trace) is wrong — re-read the C++, don't weaken the
    // assertion.

    /// Mask "###" (three required digits), autoFill=false.
    #[test]
    fn pic_three_digits_is_valid() {
        let v = PXPictureValidator::new("###", false);
        assert!(v.is_valid("123")); // prComplete
        assert!(!v.is_valid("12")); // prIncomplete
        assert!(!v.is_valid("1234")); // prError (jndex < len after complete)
        assert!(!v.is_valid("12a")); // prError
        assert!(!v.is_valid("")); // prEmpty != Complete
        assert!(v.is_status_ok()); // well-formed mask
    }

    /// Mask "###" autoFill=false — isValidInput accepts partial/incomplete (only
    /// prError fails) and does NOT mutate a non-filling field.
    #[test]
    fn pic_three_digits_is_valid_input() {
        let v = PXPictureValidator::new("###", false);

        let mut s = String::from("12");
        assert!(v.is_valid_input(&mut s, false)); // prIncomplete != Error
        assert_eq!(s, "12"); // unchanged (no autofill)

        let mut s = String::from("12a");
        assert!(!v.is_valid_input(&mut s, false)); // prError

        let mut s = String::from("1234");
        assert!(!v.is_valid_input(&mut s, false)); // prError
    }

    /// Mask "&&&" autoFill=true — three letters, uppercased. THE most
    /// discriminating mutation test: "abc" → "ABC".
    #[test]
    fn pic_uppercase_autofill_mutates() {
        let v = PXPictureValidator::new("&&&", true);
        let mut s = String::from("abc");
        assert!(v.is_valid_input(&mut s, false));
        assert_eq!(s, "ABC"); // & uppercases each consumed letter
        assert!(v.is_valid("abc")); // prComplete (& matches lowercase letters)
    }

    /// Mask "!" autoFill=true — one char, uppercased: "a" → "A".
    #[test]
    fn pic_bang_uppercases_single() {
        let v = PXPictureValidator::new("!", true);
        let mut s = String::from("a");
        assert!(v.is_valid_input(&mut s, false));
        assert_eq!(s, "A");
    }

    /// Mask "##:##" autoFill=true — the literal ':' auto-fills after "12":
    /// "12" → "12:". And a full "12:34" is complete.
    #[test]
    fn pic_literal_colon_autofills() {
        let v = PXPictureValidator::new("##:##", true);
        let mut s = String::from("12");
        assert!(v.is_valid_input(&mut s, false));
        assert_eq!(s, "12:"); // autofill tail appends the literal ':'
        assert!(v.is_valid("12:34")); // prComplete
    }

    /// Syntax / status: trailing ';' is a syntax error → vsSyntax.
    #[test]
    fn pic_trailing_semicolon_is_syntax_error() {
        let v = PXPictureValidator::new("##;", false);
        assert!(!v.is_status_ok());
    }

    /// Syntax / status: unbalanced '[' → vsSyntax.
    #[test]
    fn pic_unbalanced_bracket_is_syntax_error() {
        let v = PXPictureValidator::new("[##", false);
        assert!(!v.is_status_ok());
    }

    /// Syntax / status: well-formed mask → OK.
    #[test]
    fn pic_well_formed_status_ok() {
        let v = PXPictureValidator::new("##", false);
        assert!(v.is_status_ok());
    }

    /// Object safety: the boxed-trait storage form (`Option<Box<dyn Validator>>`).
    #[test]
    fn pic_object_safe_as_boxed_trait() {
        let v: Box<dyn Validator> = Box::new(PXPictureValidator::new("###", false));
        assert!(v.is_valid("123"));
        assert!(!v.is_valid("12"));
    }

    /// Optional group: "#####[-####]" zip+4. Five required digits then an
    /// optional `[-####]` (a literal dash plus four required digits). Traced
    /// against the C++ engine (and confirmed against the prompt's stated intent):
    /// - "12345" → five digits then the optional `[...]` group; `checkComplete`
    ///   skips the trailing all-optional remainder → prAmbiguous → prComplete.
    /// - "12345-678" → the dash plus only three of four required digits → the
    ///   group is incomplete → prIncomplete (not complete).
    /// - "12345-6789" → dash + all four digits consumed → prComplete.
    ///
    /// NOTE: the prompt sketched this with a 3-digit lead (`"###[-####]"`), but
    /// that mask only accepts 3 digits or 3-dash-4 (8 chars); the 5-digit ZIP
    /// (`"#####[-####]"`) is what yields the prompt's intended
    /// complete/incomplete/complete trio, so the mask is corrected here.
    #[test]
    fn pic_optional_zip_plus_four() {
        let v = PXPictureValidator::new("#####[-####]", false);
        assert!(v.is_valid("12345")); // optional group skipped → complete
        assert!(!v.is_valid("12345-678")); // partial +4 → incomplete
        assert!(v.is_valid("12345-6789")); // full +4 → complete
    }

    /// Literal letter in the mask: the buffer is normalized to the MASK's case,
    /// not the typed case. C++ `scan`'s default arm always `consume(pic[index])`
    /// (tvalidat.cpp:450) — the matched literal byte, regardless of how it was
    /// typed (case-insensitive match). Mask "N##", typed "n12" → buffer "N12".
    #[test]
    fn pic_literal_letter_normalizes_to_mask_case() {
        let v = PXPictureValidator::new("N##", false);
        let mut s = String::from("n12");
        assert!(v.is_valid_input(&mut s, false)); // 'n' matches literal 'N'
        assert_eq!(s, "N12"); // consume(pic[index]) writes the mask's 'N'
        assert!(v.is_valid("N12")); // exact case also complete
        assert!(!v.is_valid("X12")); // wrong literal → not complete
    }

    /// Comma alternatives — exercises the `skip_to_comma` backtracking path.
    ///
    /// SEMANTICS (hand-traced, then confirmed empirically against the engine):
    /// `process` tries each comma-separated branch in order and returns as soon
    /// as one **completes**; the next branch is tried only when the current one
    /// errors or is incomplete. Crucially, the outer `picture()` then rejects any
    /// run that completed a branch but left input unconsumed (`jndex < len` →
    /// prError). So for `"###,#####"` the FIRST branch (`###`) completes on a
    /// 3-digit input and a 5-digit input has trailing "45" → prError — the second
    /// branch is NOT reached. The alternatives are "either form, but the first
    /// matching one wins", not "match the longest".
    #[test]
    fn pic_comma_alternatives() {
        let v = PXPictureValidator::new("###,#####", false);
        assert!(v.is_valid("123")); // first branch (3 digits) completes
        assert!(!v.is_valid("12345")); // first branch completes, "45" left → error
        assert!(!v.is_valid("1234")); // first completes, "4" left → error
        assert!(!v.is_valid("12")); // both branches need more → incomplete
        // A mask where the SHORT branch can't complete the input falls
        // through to the long branch:
        let v2 = PXPictureValidator::new("##,####", false);
        assert!(v2.is_valid("12")); // first branch (2 digits) completes
        assert!(!v2.is_valid("1234")); // first completes, "34" left → error
    }

    /// `*` iteration operator and `{}` required group — the two most complex arms
    /// (`iteration`/`group`). Hand-traced and confirmed empirically:
    /// - `"*3#"` (`*` count 3, then `#`) = exactly three digits: "123" complete,
    ///   "12" incomplete, "1234" error (trailing), "" empty.
    /// - `"*#"` (`*` count 0 = greedy "any number") = any run of digits: "12345"
    ///   complete (greedy consumes all → ambiguous → complete), "1" complete,
    ///   "" empty.
    /// - `"{###}"` (required group of three digits): "123" complete, "12"
    ///   incomplete, "12a" error.
    #[test]
    fn pic_iteration_and_group() {
        let exact3 = PXPictureValidator::new("*3#", false);
        assert!(exact3.is_valid("123")); // counted iteration: 3 digits complete
        assert!(!exact3.is_valid("12")); // only 2 → incomplete
        assert!(!exact3.is_valid("1234")); // 4th digit trailing → error
        assert!(!exact3.is_valid("")); // empty → not complete

        let any = PXPictureValidator::new("*#", false);
        assert!(any.is_valid("12345")); // greedy iteration → ambiguous → complete
        assert!(any.is_valid("1")); // a single digit is complete too
        assert!(!any.is_valid("")); // empty → not complete

        let grp = PXPictureValidator::new("{###}", false);
        assert!(grp.is_valid("123")); // required group satisfied
        assert!(!grp.is_valid("12")); // group short → incomplete
        assert!(!grp.is_valid("12a")); // 'a' not a digit → error
    }

    // ── RegexValidator (rstv extension) ──────────────────────────────────────
    //
    // Golden vectors verified against the working spike implementation.
    // If a test disagrees with its comment, the implementation is wrong — do
    // not weaken the assertion.

    /// SSN pattern `\d{3}-\d{2}-\d{4}` — `is_valid` complete/incomplete/too-long.
    #[test]
    fn regex_ssn_is_valid() {
        let v = RegexValidator::new(r"\d{3}-\d{2}-\d{4}").unwrap();
        assert!(v.is_valid("123-45-6789")); // complete
        assert!(!v.is_valid("123-45-678")); // incomplete (one digit short)
        assert!(!v.is_valid("123-45-67890")); // too long
    }

    /// SSN pattern — `is_valid_input` viable-prefix test.
    #[test]
    fn regex_ssn_is_valid_input() {
        let v = RegexValidator::new(r"\d{3}-\d{2}-\d{4}").unwrap();
        assert!(v.is_valid_input(&mut "123".into(), false)); // viable prefix
        assert!(v.is_valid_input(&mut "123-45-678".into(), false)); // one digit still possible
        assert!(v.is_valid_input(&mut "123-45-6789".into(), false)); // complete is also viable
        assert!(!v.is_valid_input(&mut "123-45-67890".into(), false)); // 5th trailing digit → dead
        assert!(!v.is_valid_input(&mut "12a".into(), false)); // letter kills the DFA
    }

    /// Three-digit pattern `\d{3}` — complete/incomplete/too-long + empty.
    #[test]
    fn regex_three_digits_is_valid() {
        let v = RegexValidator::new(r"\d{3}").unwrap();
        assert!(v.is_valid("123")); // complete
        assert!(!v.is_valid("12")); // incomplete
        assert!(!v.is_valid("1234")); // too long
    }

    /// Three-digit pattern — `is_valid_input` including empty and 4th-digit dead.
    #[test]
    fn regex_three_digits_is_valid_input() {
        let v = RegexValidator::new(r"\d{3}").unwrap();
        assert!(v.is_valid_input(&mut "".into(), false)); // empty: still viable
        assert!(v.is_valid_input(&mut "12".into(), false)); // two digits: still viable
        assert!(!v.is_valid_input(&mut "1234".into(), false)); // 4th digit → dead
        assert!(!v.is_valid_input(&mut "12a".into(), false)); // letter → dead
    }

    /// `cat|dog` — the whole alternation is anchored at both ends. End-anchoring
    /// for `is_valid` comes from the `next_eoi_state`+`is_match_state` step; the
    /// `(?:…)` group ensures `\z` binds the *whole* alternation (not just one
    /// branch), so neither "cat…" nor "dog…" can pass with trailing input.
    #[test]
    fn regex_alternation_end_anchored() {
        let v = RegexValidator::new(r"cat|dog").unwrap();
        assert!(v.is_valid("cat")); // complete
        assert!(v.is_valid("dog")); // complete
        assert!(!v.is_valid("do")); // incomplete
        assert!(!v.is_valid("cats")); // end-anchored: trailing 's' makes it fail
    }

    /// Paren-injection guard: a pattern that is not self-contained — e.g. an
    /// unbalanced `)` that would close the `(?:…)` wrap early and silently
    /// defeat end-anchoring — is rejected at construction.
    #[test]
    fn regex_paren_injection_is_rejected() {
        // `cat)|(.*` would build `(?:cat)|(.*)\z`, binding `\z` only to the
        // second branch — accepting arbitrary trailing input. Must be Err.
        assert!(RegexValidator::new("cat)|(.*").is_err());
        assert!(RegexValidator::new(")evil(").is_err());
    }

    /// `cat|dog` — `is_valid_input` viable prefixes.
    #[test]
    fn regex_alternation_is_valid_input() {
        let v = RegexValidator::new(r"cat|dog").unwrap();
        assert!(v.is_valid_input(&mut "c".into(), false)); // prefix of "cat"
        assert!(v.is_valid_input(&mut "d".into(), false)); // prefix of "dog"
        assert!(v.is_valid_input(&mut "ca".into(), false)); // prefix of "cat"
        assert!(!v.is_valid_input(&mut "x".into(), false)); // not a prefix of either
    }

    /// Start+end anchoring: a valid sub-match that is not at the start/end must fail.
    #[test]
    fn regex_anchoring_both_ends() {
        let v = RegexValidator::new(r"cat").unwrap();
        assert!(!v.is_valid("xcat")); // leading 'x' breaks start anchor
        assert!(!v.is_valid("catx")); // trailing 'x' breaks end anchor
        assert!(!v.is_valid_input(&mut "xc".into(), false)); // 'x' is not a valid prefix
    }

    /// No-mutation guarantee: `is_valid_input` must not modify the string.
    #[test]
    fn regex_no_mutation() {
        let v = RegexValidator::new(r"\d{3}").unwrap();
        let mut s = "12".to_string();
        v.is_valid_input(&mut s, false);
        assert_eq!(s, "12");
    }

    /// Invalid pattern returns `Err` rather than panicking.
    #[test]
    fn regex_invalid_pattern_returns_err() {
        assert!(RegexValidator::new("(").is_err()); // unbalanced paren
    }

    /// Object safety: `RegexValidator` is storable as `Box<dyn Validator>`.
    #[test]
    fn regex_object_safe() {
        let v: Box<dyn Validator> = Box::new(RegexValidator::new(r"\d+").unwrap());
        assert!(v.is_valid("42"));
        assert!(!v.is_valid("4x"));
    }

    /// `is_status_ok` uses the trait default (true) — construction failure is
    /// reported via `Err`, not a bad status on a successfully-built validator.
    #[test]
    fn regex_status_ok_after_successful_construction() {
        let v = RegexValidator::new(r"\d+").unwrap();
        assert!(v.is_status_ok());
    }
}
