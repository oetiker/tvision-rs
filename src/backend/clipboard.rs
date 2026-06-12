//! The clipboard fallback chain that backs every copy/paste in rstv:
//! **native first, internal buffer only on failure**. The copy order is
//! native → OSC 52 emit → internal; paste is native → internal. The rungs:
//!
//! 1. **Native** — a [`NativeClipboard`] provider (production: [`ArboardClipboard`]
//!    under the `os-clipboard` feature). `None` when init failed or the feature
//!    is off.
//! 2. **OSC 52 emit** (copy only) — fire-and-forget escape sequence to the
//!    terminal, which puts the text on the clipboard of the machine the
//!    terminal runs on (the win over SSH). The sequence is always emitted once
//!    this rung is reached; because the input parser belongs to crossterm we
//!    cannot probe whether the terminal accepted it, so we report the internal
//!    fallback (`false`) after emitting.
//! 3. **Internal buffer** — the last resort, written only on the non-native
//!    path so it never shadows a live native clipboard with stale text.
//!
//! There is **no OSC 52 read** rung: reading requires capability probes (a TERM
//! allowlist plus XTGETTCAP/XTQALLOWED queries) and a reply arriving through the
//! input parser — which crossterm owns, so rstv has no place to receive it.
//! Terminals also gate clipboard *reads* behind security opt-ins, making a blind
//! read request useless.
//!
//! The chain is unit-testable without a terminal or a real clipboard: the
//! native rung is a trait object and the OSC 52 sink is any `io::Write`.
//!
//! # Turbo Vision heritage
//! Ports the `TClipboard` text path (`tclipbrd.cpp`), whose copy is native-first
//! with the internal buffer touched only after the native write fails. The Unix
//! native + OSC 52 emit rungs follow `unixcon.cpp` / `termio.cpp`. The original
//! detected an OSC 52 capability before claiming success; since crossterm owns
//! the input parser, rstv cannot probe and instead always reports the internal
//! fallback after emitting.

use std::io;

/// A native (OS-level) clipboard provider — the first rung of the chain.
///
/// Implementations must be **fallible per operation**, not just at
/// construction: e.g. a Wayland session without the data-control protocol
/// constructs fine but fails on `set`. The chain falls through on every
/// failed call (see [`ClipboardChain::set`]).
pub(crate) trait NativeClipboard {
    /// Write `text` to the OS clipboard. `false` = this rung failed; the
    /// chain falls through.
    fn set(&mut self, text: &str) -> bool;

    /// Read the OS clipboard. `None` = unavailable or empty; the chain falls
    /// through to the internal buffer.
    fn get(&mut self) -> Option<String>;
}

/// The fallback chain (module docs have the full order).
pub(crate) struct ClipboardChain {
    /// The native rung. `None` = init failed / `os-clipboard` feature off.
    native: Option<Box<dyn NativeClipboard>>,
    /// Last-resort internal buffer — written only when the native rung failed
    /// (no stale shadow on native-capable systems).
    local: String,
}

impl ClipboardChain {
    /// Build a chain with an explicit (possibly absent) native rung.
    pub(crate) fn new(native: Option<Box<dyn NativeClipboard>>) -> Self {
        ClipboardChain {
            native,
            local: String::new(),
        }
    }

    /// Build the production chain: the arboard native rung when the
    /// `os-clipboard` feature is on (and its init succeeds), no native rung
    /// otherwise. Init failure is swallowed — clipboard absence must not fail
    /// backend construction.
    pub(crate) fn with_os_native() -> Self {
        #[cfg(feature = "os-clipboard")]
        let native: Option<Box<dyn NativeClipboard>> =
            ArboardClipboard::new().map(|c| Box::new(c) as Box<dyn NativeClipboard>);
        #[cfg(not(feature = "os-clipboard"))]
        let native: Option<Box<dyn NativeClipboard>> = None;
        Self::new(native)
    }

    /// Run the copy chain (module docs): native → OSC 52 emit → internal.
    ///
    /// Returns `true` only when the native rung took the text — and then
    /// emits **no** OSC 52 (no double-emit, and a blind OSC sequence on a dumb
    /// terminal risks on-screen garbage). On the
    /// non-native path the OSC 52 sequence is queued to `osc52_out`
    /// fire-and-forget (write errors ignored), the internal mirror is
    /// written, and `false` is returned per the `Backend::set_clipboard`
    /// contract ("fell back to internal").
    pub(crate) fn set(&mut self, text: &str, osc52_out: &mut dyn io::Write) -> bool {
        if let Some(native) = self.native.as_mut()
            && native.set(text)
        {
            return true;
        }
        // OSC 52 emit — fire-and-forget (always emitted once this rung is
        // reached). `Command::write_ansi` into a String,
        // then queue the bytes — `crossterm::queue!` itself can't target a
        // `&mut dyn Write` (its `by_ref()` needs a sized writer).
        use crossterm::Command as _;
        let mut seq = String::new();
        let _ = crossterm::clipboard::CopyToClipboard::to_clipboard_from(text).write_ansi(&mut seq);
        let _ = osc52_out.write_all(seq.as_bytes());
        // Internal mirror — non-native path only.
        self.local = text.to_string();
        false
    }

    /// Run the paste chain: native → internal buffer → `None`.
    ///
    /// A native rung that exists but fails (or is empty) at read time falls
    /// through to the internal buffer. No OSC 52 read — see the module docs.
    pub(crate) fn get(&mut self) -> Option<String> {
        if let Some(native) = self.native.as_mut()
            && let Some(text) = native.get()
        {
            return Some(text);
        }
        if self.local.is_empty() {
            None
        } else {
            Some(self.local.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// ArboardClipboard — the production native rung (`os-clipboard` feature)
// ---------------------------------------------------------------------------

/// Native rung backed by [`arboard`].
///
/// Constructed **once** at backend construction and kept for the backend's
/// lifetime — required on X11, where arboard's serving thread keeps the
/// selection alive for the app lifetime (X11 clipboard is an offer, not a
/// store). Caveat: without a clipboard manager the contents still vanish when
/// the app exits — a subprocess-backed rung (e.g. `xclip`, which outlives the
/// app) would be a future [`NativeClipboard`] impl closing that gap.
#[cfg(feature = "os-clipboard")]
pub(crate) struct ArboardClipboard {
    inner: arboard::Clipboard,
}

#[cfg(feature = "os-clipboard")]
impl ArboardClipboard {
    /// `None` when the platform clipboard cannot be opened (e.g. no display
    /// over SSH). The error is deliberately not propagated — the chain just
    /// runs without its native rung.
    pub(crate) fn new() -> Option<Self> {
        arboard::Clipboard::new()
            .ok()
            .map(|inner| ArboardClipboard { inner })
    }
}

#[cfg(feature = "os-clipboard")]
impl NativeClipboard for ArboardClipboard {
    fn set(&mut self, text: &str) -> bool {
        self.inner.set_text(text).is_ok()
    }

    fn get(&mut self) -> Option<String> {
        // Err covers both "no clipboard" and "clipboard empty / non-text" —
        // either way the chain falls through.
        self.inner.get_text().ok()
    }
}

// ---------------------------------------------------------------------------
// Tests — stub native rung + Vec<u8> OSC sink, no terminal needed
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Scriptable native rung: `accept` controls per-op success, `stored` is
    /// what a successful `set` wrote / `get` returns.
    struct StubNative {
        accept: bool,
        stored: Option<String>,
    }

    impl NativeClipboard for StubNative {
        fn set(&mut self, text: &str) -> bool {
            if self.accept {
                self.stored = Some(text.to_string());
                true
            } else {
                false
            }
        }

        fn get(&mut self) -> Option<String> {
            if self.accept {
                self.stored.clone()
            } else {
                None
            }
        }
    }

    fn chain_with(accept: bool, stored: Option<&str>) -> ClipboardChain {
        ClipboardChain::new(Some(Box::new(StubNative {
            accept,
            stored: stored.map(str::to_string),
        })))
    }

    #[test]
    fn native_success_no_osc_no_mirror() {
        let mut chain = chain_with(true, None);
        let mut sink: Vec<u8> = Vec::new();
        assert!(chain.set("hello", &mut sink), "native took it → true");
        assert!(sink.is_empty(), "no OSC 52 emitted on the native path");
        assert!(
            chain.local.is_empty(),
            "mirror untouched on the native path"
        );
        // And the native rung serves the read back.
        assert_eq!(chain.get().as_deref(), Some("hello"));
    }

    #[test]
    fn native_fail_emits_osc52_and_mirrors() {
        let mut chain = chain_with(false, None);
        let mut sink: Vec<u8> = Vec::new();
        assert!(!chain.set("hello", &mut sink), "fell back → false");
        let osc = String::from_utf8_lossy(&sink);
        assert!(osc.contains("\x1b]52;"), "OSC 52 sequence emitted: {osc:?}");
        // base64("hello") = aGVsbG8=
        assert!(
            osc.contains("aGVsbG8="),
            "payload is base64-encoded: {osc:?}"
        );
        assert_eq!(chain.local, "hello", "internal mirror updated");
    }

    #[test]
    fn no_native_path_equals_native_fail_path() {
        let mut chain = ClipboardChain::new(None);
        let mut sink: Vec<u8> = Vec::new();
        assert!(!chain.set("hello", &mut sink));
        assert!(String::from_utf8_lossy(&sink).contains("\x1b]52;"));
        assert_eq!(chain.local, "hello");
        assert_eq!(chain.get().as_deref(), Some("hello"));
    }

    #[test]
    fn get_order_native_then_internal_then_none() {
        // Native rung answers → its text wins, internal mirror ignored.
        let mut chain = chain_with(true, Some("native"));
        chain.local = "internal".to_string();
        assert_eq!(chain.get().as_deref(), Some("native"));

        // Native rung empty/failing → internal buffer.
        let mut chain = chain_with(false, Some("native"));
        chain.local = "internal".to_string();
        assert_eq!(chain.get().as_deref(), Some("internal"));

        // Nothing anywhere → None.
        let mut chain = ClipboardChain::new(None);
        assert_eq!(chain.get(), None);
    }

    #[test]
    fn per_op_failure_falls_through() {
        // The native rung exists (init "succeeded") but fails AT OPERATION
        // TIME — the Wayland-without-data-control shape. The chain must still
        // land the text in the mirror and serve it back.
        let mut chain = chain_with(false, None);
        let mut sink: Vec<u8> = Vec::new();
        assert!(!chain.set("fallback", &mut sink));
        assert_eq!(chain.get().as_deref(), Some("fallback"));
    }
}
