//! End-to-end render pipeline test (D11, row 19).
//!
//! Proves the full pipeline:
//!   Buffer paint → Renderer diff → HeadlessBackend.draw → snapshot
//!
//! These tests are the verification backbone: every widget test from Phase 1
//! onward follows the same pattern.

use tvision::backend::{HeadlessBackend, Renderer};
use tvision::color::{Color, Style};
use tvision::event::{Event, Key, KeyModifiers};
use tvision::screen::Buffer;

/// Full end-to-end pipeline: paint cells, render, snapshot.
///
/// Paints "Hello" in bright-white-on-blue (BIOS 0xF/0x1) at row 0, leaves
/// the remaining 7 columns and rows 1–2 as default cells.  The cursor is
/// placed at (5, 0).
#[test]
fn renders_text_to_headless_snapshot() {
    let (backend, screen) = HeadlessBackend::new(12, 3);
    let mut r = Renderer::new(Box::new(backend));
    r.set_cursor(Some((5, 0)));
    r.render(|buf: &mut Buffer| {
        let s = Style::new(Color::Bios(0xF), Color::Bios(0x1));
        for (i, ch) in "Hello".chars().enumerate() {
            let c = buf.get_mut(i as u16, 0);
            c.set_char(ch);
            c.set_style(s);
        }
    });
    insta::assert_snapshot!(screen.snapshot());
}

/// Proves the headless event queue: injected events come back from poll_event.
#[test]
fn headless_event_queue_roundtrip() {
    let (backend, screen) = HeadlessBackend::new(10, 3);
    let mut r = Renderer::new(Box::new(backend));

    // Inject a Ctrl+C key event.
    screen.push_key(
        Key::Char('c'),
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    // Inject a plain Enter.
    screen.push_event(Event::KeyDown(tvision::event::KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));

    // Poll should return the first injected event.
    let ev0 = r.backend_mut().poll_event(None);
    match ev0 {
        Some(Event::KeyDown(k)) => {
            assert_eq!(k.key, Key::Char('c'));
            assert!(k.modifiers.ctrl);
            assert!(!k.modifiers.shift);
            assert!(!k.modifiers.alt);
        }
        other => panic!("expected KeyDown(Char('c')+ctrl), got {other:?}"),
    }

    // Second event: Enter.
    let ev1 = r.backend_mut().poll_event(None);
    match ev1 {
        Some(Event::KeyDown(k)) => {
            assert_eq!(k.key, Key::Enter);
            assert_eq!(k.modifiers, KeyModifiers::default());
        }
        other => panic!("expected KeyDown(Enter), got {other:?}"),
    }

    // Queue is now empty.
    let ev2 = r.backend_mut().poll_event(None);
    assert!(ev2.is_none(), "expected empty queue, got {ev2:?}");
}
