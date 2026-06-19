//! Input validators that gate what an [`InputLine`](crate::widgets::InputLine)
//! accepts.
//!
//! [`Validator`] is a trait; an input line holds an
//! `Option<Box<dyn Validator>>` and, with no validator, accepts every input.
//! The crate ships several concrete validators: [`FilterValidator`] (an allowed
//! character set), [`RangeValidator`] (a numeric range),
//! [`LookupValidator`] / [`StringLookupValidator`] (membership in a set),
//! [`PXPictureValidator`] (a Paradox picture mask), and the Rust-native
//! [`RegexValidator`].
//!
//! ## Object safety
//!
//! Because the validator is stored as a boxed trait object, [`Validator`] is
//! **object-safe**: every method takes `&self`, no generics, no `Self` return.
//! `validate` is a provided method that dispatches through the overridable
//! `is_valid`/`error`.
//!
//! ## Typed value transfer
//!
//! A validator can also convert between the field's text and a typed
//! [`FieldValue`] — [`Validator::transfer_get`] reports the field's value to a
//! dialog gather, and [`Validator::transfer_set`] formats a value back into the
//! field. Only validators with transfer enabled override these (for example
//! [`RangeValidator`] transfers an [`FieldValue::Int`]); the base returns
//! `None`, leaving the input line to use its plain text value.
//!
//! **Guide:** [Controls](../../../apps/controls.html).
//!
//! # Turbo Vision heritage
//!
//! Ports `TValidator` and its subclasses (`tvalidator.cpp`). The abstract base
//! and its subclass hierarchy become this trait plus concrete impls (deviation
//! D2); the untyped value-transfer hook becomes the typed
//! `transfer_get`/`transfer_set` pair (deviation D10); and the streaming
//! machinery is dropped (deviation D12).

use crate::data::FieldValue;
use crate::view::Context;
use regex_automata::{
    Anchored,
    dfa::{Automaton, StartKind, dense},
    util::start::Config as StartConfig,
};

/// An input validator.
///
/// An [`InputLine`](crate::widgets::InputLine) holds an
/// `Option<Box<dyn Validator>>`; with no validator every input is accepted. The
/// default methods all accept; concrete validators override them.
///
/// # Turbo Vision heritage
///
/// Ports the abstract `TValidator` base class (`tvalidator.cpp`); the subclass
/// hierarchy becomes this trait (deviation D2).
pub trait Validator {
    /// Check (and optionally auto-fill/modify) `s` *as it is being typed*. May
    /// mutate `s` in place (e.g. a picture validator inserting literal
    /// characters). `suppress_fill` asks it not to auto-fill. Default: accept, no
    /// change. Object-safe: `&self`, `s: &mut String`.
    fn is_valid_input(&self, _s: &mut String, _suppress_fill: bool) -> bool {
        true
    }

    /// The final-form check, run when the field must be fully valid (the
    /// modal-OK / focus-release path). Override this to enforce your validity
    /// rule; the base accepts every string. Prefer [`validate`](Validator::validate)
    /// when you also need to pop the error box on failure.
    ///
    /// Default: `true`.
    fn is_valid(&self, _s: &str) -> bool {
        true
    }

    /// Report an invalid final value. Concrete validators pop up a message box via
    /// the async-modal-from-a-view seam ([`Context::request_message_box`],
    /// `answer_to`/`then_command` both `None` — informational, OK-only); the base
    /// is a no-op.
    ///
    /// This is a [`Validator`] method, not a [`View`](crate::view::View) method.
    fn error(&self, _ctx: &mut Context) {}

    /// Validate `s` and, if invalid, pop the error box. Call this at the
    /// field-commit point (modal OK / focus release): it calls `is_valid`, and on
    /// failure calls `error` to display the message box before returning `false`.
    /// Returns `true` immediately when valid without touching the context.
    ///
    /// A provided method; override `is_valid` and `error` rather than this.
    fn validate(&self, s: &str, ctx: &mut Context) -> bool {
        if self.is_valid(s) {
            true
        } else {
            self.error(ctx);
            false
        }
    }

    /// Whether the validator's status is OK — consulted when the field must
    /// commit. The base never enters a non-OK status, so the default is `true`;
    /// [`PXPictureValidator`] overrides to report a malformed mask.
    fn is_status_ok(&self) -> bool {
        true
    }

    /// Report the field's current text `s` as a typed [`FieldValue`] during a
    /// dialog **gather** walk. Return `Some(typed value)` when the validator has
    /// transfer enabled and can parse the text; return `None` to leave the input
    /// line carrying its plain text (the default, meaning "I don't transfer").
    ///
    /// Override this together with [`transfer_set`](Validator::transfer_set) to
    /// participate in dialog data transfer.
    fn transfer_get(&self, _s: &str) -> Option<FieldValue> {
        None
    }

    /// Format a typed [`FieldValue`] back to the field's text during a dialog
    /// **scatter** walk. Return `Some(text)` when transfer is enabled and `v`
    /// is the type this validator handles; return `None` to leave the input line
    /// on its own text path (the default).
    ///
    /// Override this together with [`transfer_get`](Validator::transfer_get) to
    /// participate in dialog data transfer.
    fn transfer_set(&self, _v: &FieldValue) -> Option<String> {
        None
    }
}

// ── Concrete validators ──────────────────────────────────────────────────────

/// Accepts only characters drawn from an allowed set: every character of the
/// input must be a member of `valid_chars`. Empty input passes.
///
/// Membership is tested **per Unicode `char`** (`valid_chars.contains(c)`). On
/// an invalid final value, `error` pops up an OK-only error message box.
///
/// # Turbo Vision heritage
///
/// Ports `TFilterValidator` (`tvalidator.cpp`), which tests membership per byte
/// where tvision-rs tests per Unicode `char` (identical for the ASCII charsets these
/// validators carry). The streaming machinery is dropped (deviation D12).
pub struct FilterValidator {
    valid_chars: String,
}

impl FilterValidator {
    /// Build a filter that accepts only characters in `valid_chars`. Pass any
    /// `Into<String>` — typically a string literal like `"0123456789"` or
    /// `"+-0123456789"`. Membership is tested per Unicode `char`.
    ///
    /// Attach the result to an [`InputLine`](crate::widgets::InputLine) as
    /// its validator to restrict what the user can type and what the final
    /// value may contain.
    pub fn new(valid_chars: impl Into<String>) -> Self {
        Self {
            valid_chars: valid_chars.into(),
        }
    }
}

impl Validator for FilterValidator {
    /// Returns `true` iff every `char` of `s` is in `valid_chars`. An empty
    /// string passes (vacuously all characters are valid). Use this at the
    /// final-commit check; for the while-typing path see `is_valid_input`.
    fn is_valid(&self, s: &str) -> bool {
        s.chars().all(|c| self.valid_chars.contains(c))
    }

    /// Returns `true` iff every `char` of `s` is in `valid_chars` — the same
    /// rule as `is_valid`, applied per-keystroke. Never mutates `s` (a filter
    /// never auto-fills); `suppress_fill` is ignored.
    fn is_valid_input(&self, s: &mut String, _suppress_fill: bool) -> bool {
        self.is_valid(s)
    }

    /// Pop an OK-only "Invalid character in input" error box via
    /// [`Context::request_message_box`]. Called automatically by
    /// [`validate`](Validator::validate) when [`is_valid`](FilterValidator::is_valid)
    /// returns `false`.
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

/// The accept-all base for lookup-style validators — it accepts every input.
///
/// Use `LookupValidator` as a stand-in when you need a no-op validator in a
/// context that expects a lookup-style type, or as the starting point before
/// you know which concrete validator to plug in. For actual membership testing,
/// use [`StringLookupValidator`] instead.
///
/// # Turbo Vision heritage
///
/// Ports `TLookupValidator` (`tvalidator.cpp`), an abstract intermediate that
/// routed validity through a virtual `lookup` step. In tvision-rs the indirection
/// collapses: each concrete lookup validator folds the lookup directly into its
/// `is_valid`, so this type only realises the base's accept-all behaviour.
pub struct LookupValidator;

impl LookupValidator {
    /// Construct the accept-all lookup base.
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

/// Valid iff the input exactly matches one entry in an owned list of strings.
/// On an invalid final value, `error` pops up an OK-only error message box.
///
/// Validation is a linear scan (`O(n)`) over the list, which preserves the
/// caller's order (UI pickers may rely on it). [`StringLookupValidator::new_string_list`] replaces
/// the whole list.
///
/// # Turbo Vision heritage
///
/// Ports `TStringLookupValidator` (`tvalidator.cpp`). The lookup folds into
/// `is_valid` (deviation D2), the string collection becomes an owned
/// `Vec<String>`, and the streaming machinery is dropped (deviation D12).
///
/// C++ `TStringLookupValidator` held a *sorted* collection and binary-searched
/// (`O(log n)`). For the small fixed lists these validators carry, the linear
/// scan is simpler and fast enough; order preservation is the deliberate
/// trade-off.
pub struct StringLookupValidator {
    strings: Vec<String>,
}

impl StringLookupValidator {
    /// Build a validator that accepts only strings in `strings`. Pass an owned
    /// `Vec<String>` of the allowed values; the order is preserved and used as
    /// the iteration order for membership tests. Attach the result to an
    /// [`InputLine`](crate::widgets::InputLine) to restrict the final value to
    /// one of the listed entries.
    pub fn new(strings: Vec<String>) -> Self {
        Self { strings }
    }

    /// Replace the accepted-string list at runtime, dropping the previous `Vec`.
    ///
    /// Call this when the set of valid entries changes after the validator is
    /// already in use — e.g. a dependent field whose allowed values depend on
    /// another control's selection. The new `strings` order becomes the
    /// membership-test iteration order, exactly as in [`new`](Self::new); pass an
    /// empty `Vec` to reject every input until the list is repopulated.
    ///
    /// # Turbo Vision heritage
    /// Ports `TStringLookupValidator::newStringList`. The C++ `newStringList(nil)`
    /// form (dispose the list without installing a replacement) has no analog —
    /// pass an empty `Vec` instead.
    pub fn new_string_list(&mut self, strings: Vec<String>) {
        self.strings = strings;
    }
}

impl Validator for StringLookupValidator {
    /// Accepts `s` iff it exactly matches some entry in the list.
    fn is_valid(&self, s: &str) -> bool {
        self.strings.iter().any(|x| x == s)
    }

    /// Pop an OK-only "Input is not in list of valid strings" error box via
    /// [`Context::request_message_box`]. Called automatically by
    /// [`validate`](Validator::validate) when
    /// [`is_valid`](StringLookupValidator::is_valid) returns `false`.
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

/// A numeric validator: it gates input through a digit charset filter (an
/// embedded [`FilterValidator`]), then on the final check parses the text and
/// requires it to fall within `[min, max]`.
///
/// Whether the field also transfers its value as an [`FieldValue::Int`] is
/// off by default — enable it with [`set_transfer`](RangeValidator::set_transfer).
/// `min`/`max` are `i32`. On an out-of-range final value, `error` pops up an
/// OK-only message box naming the range.
///
/// # Turbo Vision heritage
///
/// Ports `TRangeValidator` (`tvalidator.cpp`), a filter subclass. The charset is
/// selected by the sign of `min`. Its final check adds a parse + range test on
/// top of the charset gate, while the while-typing check is the plain charset
/// filter (so a partial out-of-range number is accepted as input). Inheritance
/// becomes embed-and-delegate over a [`FilterValidator`] field (deviation D2);
/// the untyped value-transfer becomes the typed `transfer_get`/`transfer_set`
/// pair over [`FieldValue::Int`] (deviation D10); and the streaming machinery is
/// dropped (deviation D12).
pub struct RangeValidator {
    /// Embedded filter — the sign-selected digit charset.
    filter: FilterValidator,
    min: i32,
    max: i32,
    /// Whether the field also transfers its value; default OFF.
    transfer_enabled: bool,
}

/// Parse the leading numeric value of a range-validator field.
///
/// Rust's `str::parse::<i32>()` is stricter than a `%ld`-style scan: it rejects
/// trailing junk and a lone `"+"`/`"-"`. Because the charset filter already
/// restricts the field to `[+-0-9]`, clean numeric input behaves the same; the
/// only divergence is pathological mid-string sign/junk (e.g. `"12+3"`), which a
/// leading-run scan truncate-accepts and we reject — an acceptable, stricter
/// simplification. We `.trim()` first (whitespace is not in the charset, so this
/// only matters for direct callers) and never panic.
fn parse_long(s: &str) -> Option<i32> {
    s.trim().parse::<i32>().ok()
}

impl RangeValidator {
    /// Build a range validator that requires the entered integer to be in
    /// `[min, max]` (inclusive). The embedded charset filter is selected by
    /// `min`: non-negative → digits + `'+'`; negative → digits + `'+'` +
    /// `'-'`. Call [`set_transfer`](RangeValidator::set_transfer) afterward
    /// to enable typed `i32` transfer for dialog gather/scatter; transfer is
    /// **off by default**.
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

    /// Enable/disable typed value transfer. Default OFF — until enabled,
    /// [`transfer_get`](Validator::transfer_get) /
    /// [`transfer_set`](Validator::transfer_set) return `None` and the input line
    /// keeps its text value.
    pub fn set_transfer(&mut self, enabled: bool) {
        self.transfer_enabled = enabled;
    }
}

impl Validator for RangeValidator {
    /// Returns `true` iff `s` passes the charset gate, parses as an `i32`,
    /// and falls within `[min, max]` (inclusive). All three conditions must
    /// hold; the charset gate fires first so malformed input is caught early
    /// without an attempted parse.
    fn is_valid(&self, s: &str) -> bool {
        self.filter.is_valid(s) && parse_long(s).is_some_and(|v| v >= self.min && v <= self.max)
    }

    /// Charset-only while typing, **no** range check (delegated straight to the
    /// embedded filter). So a partial, out-of-range number is accepted as input;
    /// the range is enforced only at [`is_valid`](RangeValidator::is_valid) (the
    /// final check).
    fn is_valid_input(&self, s: &mut String, suppress_fill: bool) -> bool {
        self.filter.is_valid_input(s, suppress_fill)
    }

    /// When transfer is enabled, report the field text as [`FieldValue::Int`]. A
    /// failed parse falls back to `Int(0)`; transfer only runs on already-valid
    /// data, so that fallback is unreachable-but-safe.
    fn transfer_get(&self, s: &str) -> Option<FieldValue> {
        self.transfer_enabled
            .then(|| FieldValue::Int(parse_long(s).unwrap_or(0)))
    }

    /// Format an [`Int`] back to text. `None` when transfer is disabled or `v` is
    /// not an `Int` (the input line then takes its text path).
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

    /// Pop an OK-only error box naming the valid range ("Value not in the
    /// range {min} to {max}") via [`Context::request_message_box`]. Called
    /// automatically by [`validate`](Validator::validate) when
    /// [`is_valid`](RangeValidator::is_valid) returns `false`.
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

// ── PXPictureValidator ───────────────────────────────────────────────────────

/// The result of running a Paradox picture mask against an input: complete,
/// incomplete, empty, a runtime error, a malformed mask, an ambiguous match, or
/// incomplete-with-no-fill.
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

/// ASCII digit.
fn is_number(ch: u8) -> bool {
    ch.is_ascii_digit()
}

/// ASCII letter. Implemented as `(ch & 0xdf).is_ascii_uppercase()`: clearing
/// `0x20` folds the case bit, so both `a..z` and `A..Z` qualify. Kept as the bit
/// trick to stay byte-faithful to the mask engine.
fn is_letter(ch: u8) -> bool {
    (ch & 0xdf).is_ascii_uppercase()
}

/// Membership of `ch` in the byte set `special`.
fn is_special(ch: u8, special: &[u8]) -> bool {
    special.contains(&ch)
}

/// A complete match: fully complete or an accepted ambiguous match.
fn is_complete(r: PicResult) -> bool {
    matches!(r, PicResult::Complete | PicResult::Ambiguous)
}

/// An incomplete match, with or without fill.
fn is_incomplete(r: PicResult) -> bool {
    matches!(r, PicResult::Incomplete | PicResult::IncompNoFill)
}

/// ASCII uppercase fold.
fn uppercase(ch: u8) -> u8 {
    ch.to_ascii_uppercase()
}

/// Read mask byte `pic[i]`: an in-range byte, or `0` (a NUL terminator) when `i`
/// is at/past the end. The mask engine relies on a trailing NUL to stop its
/// scans, so synthesising one for out-of-range reads keeps every mask access
/// panic-free and bit-faithful.
fn pic_at(pic: &[u8], i: i32) -> u8 {
    if i < 0 {
        return 0;
    }
    pic.get(i as usize).copied().unwrap_or(0)
}

/// Advance `i` past one character or one balanced picture group (`[...]` /
/// `{...}`), stopping at `term_ch`. A free function reading only the mask; kept
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

/// Validate the mask's own syntax. Rejects an empty mask, a mask ending in `;`,
/// or unbalanced `[]`/`{}` nesting.
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

/// The transient picture scanner — per-match-call scratch state.
///
/// The two cursors (`index` into the mask, `jndex` into the input) are reset to 0
/// at the start of every match and are pure per-call scratch, not persistent
/// validator state. The [`Validator`] methods are `&self` (object-safe), so this
/// scratch lives in a fresh `Picture` created per call rather than in the
/// validator.
///
/// ## Byte-level by design
/// The scanner works byte-by-byte (`& 0xdf` case fold, in-place input writes), so
/// it is ported over `&[u8]` / `Vec<u8>`. Picture masks and the inputs to such
/// fields are ASCII, making this exact; multibyte UTF-8 in such a field is out of
/// scope (same posture as [`FilterValidator`]'s byte-vs-char note).
struct Picture<'a> {
    /// The mask.
    pic: &'a [u8],
    /// The working input buffer; mutated in place and may GROW via autofill.
    input: Vec<u8>,
    /// Cursor into the mask.
    index: i32,
    /// Cursor into the input.
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

    /// Read a mask byte (`0` past the end).
    fn pic_at(&self, i: i32) -> u8 {
        pic_at(self.pic, i)
    }

    /// Read an input byte (`0` past the end or before the start). Used where the
    /// scan may touch the terminator.
    fn input_at(&self, j: i32) -> u8 {
        if j < 0 {
            return 0;
        }
        self.input.get(j as usize).copied().unwrap_or(0)
    }

    /// Write `ch` into the input at `jndex`, then advance both cursors. (The
    /// scanner's end-of-input guard ensures `jndex` is in range before any call,
    /// so the write is safe.)
    fn consume(&mut self, ch: u8) {
        self.input[self.jndex as usize] = ch;
        self.index += 1;
        self.jndex += 1;
    }

    /// Advance `index` over groups until a comma separator or `term_ch`, then step
    /// past the comma. Returns whether `index < term_ch` (i.e. there is another
    /// alternative to try).
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

    /// The end index of the group starting at `index`.
    fn calc_term(&self, term_ch: i32) -> i32 {
        let mut k = self.index;
        to_group_end(self.pic, &mut k, term_ch);
        k
    }

    /// The `*[n]<group>` repeat operator. `index` points at the `*`. Reads the
    /// optional repeat count, then runs the group exactly `itr` times (count
    /// given) or greedily (count 0 → "any number").
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

    /// A `{...}` (required) or `[...]` (optional) bracketed picture group. `index`
    /// points at the opening bracket.
    fn group(&mut self, in_term: i32) -> PicResult {
        let term_ch = self.calc_term(in_term);
        self.index += 1;
        let rslt = self.process(term_ch - 1);

        if !is_incomplete(rslt) {
            self.index = term_ch;
        }

        rslt
    }

    /// On an incomplete result, see whether all that remains in the mask is
    /// optional (`[...]` groups or unbounded `*` iterations); if so the input is
    /// ambiguously complete.
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

    /// Match the input against one comma-free run of the mask (up to `term_ch` or
    /// a `,`), consuming input as it goes.
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
                    // Literal arm: a `;`-escape advances past the `;` to the
                    // escaped literal; the typed char must match the mask literal
                    // case-insensitively (a typed space matches any literal — it
                    // gets overwritten); otherwise the run fails. The byte CONSUMED
                    // is always the MASK byte, so the buffer is normalized to the
                    // mask's literal — NOT the typed `ch`.
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

    /// Try each comma-separated alternative in the mask run, backtracking on
    /// error/incomplete; tracks the best (farthest-consuming) incomplete to
    /// disambiguate.
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

    /// The top-level match driver. Resets the cursors, runs the alternative
    /// matcher, applies the trailing-input/autofill logic, and maps the internal
    /// `Ambiguous`/`IncompNoFill` results to their public `Complete`/`Incomplete`
    /// equivalents.
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
                // Append one mask byte to the input (the Vec carries its own
                // length, so no NUL terminator is stored).
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

/// The Paradox picture-mask validator.
///
/// Validates and auto-fills input against a Paradox "picture" mask: `#` digit,
/// `?` letter, `&` letter→uppercase, `!` any→uppercase, `@` any, `*` repeat,
/// `{}`/`[]` required/optional groups, `,` alternatives, `;` literal-escape, and
/// any other character is a literal. On an invalid final value, `error` pops up
/// an OK-only message box.
///
/// The matching engine is a recursive state machine. Its scan cursors are pure
/// per-call scratch, not validator state, so — because the [`Validator`] methods
/// are `&self` (object-safe) — that scratch lives in a transient [`Picture`]
/// created fresh per call. Operation is byte-level.
///
/// # Turbo Vision heritage
///
/// Ports `TPXPictureValidator`, with the matching engine taken verbatim from
/// `tvalidat.cpp`. The streaming machinery is dropped (deviation D12). Where the
/// original copies the input into a fixed 256-byte stack buffer, tvision-rs lets the
/// backing `String` grow instead (real inputs are length-bounded by the field).
/// No null-mask guard is needed: `pic` is always a (possibly empty) `String`,
/// and an empty or malformed mask yields the same `Syntax`/`Empty` results.
pub struct PXPictureValidator {
    /// The mask, owned.
    pic: String,
    /// Auto-fill literals while typing.
    auto_fill: bool,
    /// Whether the mask is well-formed (`false` on a syntax error). Set in
    /// [`new`](Self::new).
    status_ok: bool,
}

impl PXPictureValidator {
    /// Build a Paradox picture validator from `pic` (the mask) and
    /// `auto_fill` (whether to insert mask literals while the user types).
    ///
    /// The constructor runs the engine on empty input as a **syntax probe**:
    /// a well-formed mask returns `Empty`, leaving `is_status_ok()` true;
    /// any other result means the mask is malformed and `is_status_ok()`
    /// returns `false`. Check `is_status_ok()` after construction if you want
    /// to report a bad mask before the field is even used.
    pub fn new(pic: impl Into<String>, auto_fill: bool) -> Self {
        let pic = pic.into();
        let mut p = Picture::new(pic.as_bytes(), Vec::new());
        // status_ok iff the empty-input probe yields Empty (a well-formed mask).
        let status_ok = p.run(false) == PicResult::Empty;
        Self {
            pic,
            auto_fill,
            status_ok,
        }
    }
}

impl Validator for PXPictureValidator {
    /// Run the mask while typing (auto-filling unless `suppress_fill`); returns
    /// whether the result is not an error. MUTATES `s` in place — autofill of
    /// literals plus uppercase transforms is the whole point of a picture
    /// validator.
    fn is_valid_input(&self, s: &mut String, suppress_fill: bool) -> bool {
        let do_fill = self.auto_fill && !suppress_fill;
        let mut p = Picture::new(self.pic.as_bytes(), s.as_bytes().to_vec());
        let r = p.run(do_fill);
        *s = String::from_utf8_lossy(&p.input).into_owned();
        r != PicResult::Error
    }

    /// Returns `true` iff `s` fully satisfies the mask (i.e. the engine returns
    /// `Complete`). Runs the matcher on a copy with auto-fill off, so `s` is
    /// never mutated. Use this at the final-commit check; for the while-typing
    /// path see [`is_valid_input`](PXPictureValidator::is_valid_input).
    fn is_valid(&self, s: &str) -> bool {
        let mut p = Picture::new(self.pic.as_bytes(), s.as_bytes().to_vec());
        p.run(false) == PicResult::Complete
    }

    /// Whether the mask itself is well-formed — overrides the base accept-all to
    /// report a malformed mask.
    fn is_status_ok(&self) -> bool {
        self.status_ok
    }

    /// Pop an OK-only error box quoting the malformed mask ("Error in picture
    /// format. {pic}") via [`Context::request_message_box`]. Called
    /// automatically by [`validate`](Validator::validate) when
    /// [`is_valid`](PXPictureValidator::is_valid) returns `false`.
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

// ── RegexValidator (tvision-rs extension) ─────────────────────────────────────────

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

/// tvision-rs-original **extension** (NOT a Turbo Vision port): a [`Validator`]
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
    /// The regex analogue of [`PicResult::Complete`] in the picture validator:
    /// both the start and end anchors must be satisfied.
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
    /// Equivalent to "incomplete (or better)" in the picture-validator model:
    /// rejects only when the DFA has reached a dead state (no continuation can
    /// ever lead to a match). Does **not** mutate `s` — unlike
    /// [`PXPictureValidator`], there is no autofill.
    fn is_valid_input(&self, s: &mut String, _suppress_fill: bool) -> bool {
        !self.walk(s).0
    }

    /// Report an invalid final value via the async-modal-from-a-view seam
    /// (informational, OK-only).
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

    // ── FilterValidator ───────────────────────────────────────────────────────

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

    // ── LookupValidator ───────────────────────────────────────────────────────

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

    // ── StringLookupValidator ─────────────────────────────────────────────────

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

    // ── RangeValidator ────────────────────────────────────────────────────────

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
        // DISCRIMINATING: the while-typing check is the embedded filter's — it
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

    // ── PXPictureValidator ────────────────────────────────────────────────────
    //
    // Golden vectors hand-traced against the reference engine (`tvalidat.cpp`).
    // If the implementation disagrees with one, the implementation (or the trace)
    // is wrong — re-check against the reference, don't weaken the assertion.

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
    /// optional `[-####]` (a literal dash plus four required digits):
    /// - "12345" → five digits then the optional `[...]` group; the trailing
    ///   all-optional remainder is skipped → complete.
    /// - "12345-678" → the dash plus only three of four required digits → the
    ///   group is incomplete (not complete).
    /// - "12345-6789" → dash + all four digits consumed → complete.
    ///
    /// NOTE: a 3-digit lead (`"###[-####]"`) only accepts 3 digits or 3-dash-4
    /// (8 chars); the 5-digit ZIP (`"#####[-####]"`) is what yields the intended
    /// complete/incomplete/complete trio, so that is the mask used here.
    #[test]
    fn pic_optional_zip_plus_four() {
        let v = PXPictureValidator::new("#####[-####]", false);
        assert!(v.is_valid("12345")); // optional group skipped → complete
        assert!(!v.is_valid("12345-678")); // partial +4 → incomplete
        assert!(v.is_valid("12345-6789")); // full +4 → complete
    }

    /// Literal letter in the mask: the buffer is normalized to the MASK's case,
    /// not the typed case. The scanner consumes the mask's literal byte
    /// regardless of how it was typed (the match is case-insensitive). Mask
    /// "N##", typed "n12" → buffer "N12".
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

    // ── RegexValidator (tvision-rs extension) ──────────────────────────────────────
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
