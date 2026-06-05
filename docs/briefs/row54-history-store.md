# Row 54 — history store (`historyAdd`/`historyCount`/`historyStr`/`clearHistory`)

**Tag:** MECHANICAL (with one FOUNDATION-ish seam: the process-global store holder).
**C++ source:** `source/tvision/histlist.cpp` (+ decls `include/tvision/util.h:61-63`).
**Rust module:** `src/widgets/history.rs` (new file) — the shared dependency for
rows 55 (`THistoryViewer`), 56 (`THistoryWindow`), 57 (`THistory`).

## What this is

A **process-global, ordered, byte-budget-bounded** store of recently-entered
strings, each tagged with a small `u8` id (one input-field's history channel).
C++ implements it as a flat 1024-byte block of variable-length `HistRec`
records (`{ id: uchar, len: uchar, str: char[] }`) with manual `memcpy`
bookkeeping. We port the **observable contract**, not the byte layout.

## The faithful contract (port THIS, four load-bearing rules)

1. **Single global byte budget, GLOBAL FIFO eviction.** One shared budget
   `HISTORY_SIZE = 1024` bytes across **all** ids (NOT per-id). Each entry costs
   `str.len() + 3` bytes (`str.len()` = **UTF-8 byte length**, faithful to
   `TStringView::size()` — use `.len()`, never `.chars().count()`). When an
   insert would exceed the budget, evict the **globally-oldest** entry
   (front of the list), regardless of its id, repeatedly until it fits.

2. **Index order is OLDEST→NEWEST per id.** `history_str(id, 0)` returns the
   oldest surviving entry for that id; appends go to the back (newest). **Do NOT
   invert** — row 55 (`THistoryViewer`) owns display-order inversion and depends
   on this exact convention. (Trace: C++ `startId` → `curRec = front`,
   `advanceStringPointer` walks forward, `historyStr` advances `index+1` times.)

3. **`history_add` operation order (load-bearing):**
   1. if `str` is empty → return (add nothing);
   2. **remove any existing duplicate** `(id, str)` entry **first**
      (`retain(|e| !(e.id == id && e.str == str))` — dedup is per-id AND
      per-exact-string; at most one can exist);
   3. **then** evict front entries while `used + new_len > HISTORY_SIZE`;
   4. push `(id, str)` to the back.
   Dedup-before-evict is required: freeing the duplicate's bytes before the
   budget check ensures re-adding an existing string never spuriously evicts a
   different entry. (C++ does `deleteString` in the dup scan, then
   `insertString` does the evict-loop — same order.)

4. **`clear_history()` empties the whole store** (all ids). Consumers call it.

## API (free functions + a private thread-local store)

```rust
pub fn history_add(id: u8, str: &str);
pub fn history_count(id: u8) -> usize;          // entries for this id
pub fn history_str(id: u8, index: usize) -> Option<String>;  // None if out of range
pub fn clear_history();
```

- **Holder:** `thread_local! { static HISTORY: RefCell<Vec<HistRec>> = ... }`
  where `struct HistRec { id: u8, str: String }` (private). Thread-local is the
  right call: libtest runs each `#[test]` on its own thread, so each test gets a
  pristine store automatically — no `Mutex`, no `Sync` gymnastics, faithful to
  the single-threaded C++ design. (Mirrors the process-global `ViewId` minter's
  spirit in `src/view/id.rs`, adapted for mutable state.) Access via
  `HISTORY.with(|h| ...)`.
- `history_count` / `history_str` filter the global `Vec` by `id`, preserving
  insertion order (front→back = oldest→newest).

## Deliberate documented deviation (put this in a code comment AND note it for review)

C++ keeps a hidden front **sentinel** record (`clearHistory` writes
`HistRec(0, "")` at the front) and `advanceStringPointer` **always skips the
front-most record**. Two halves of byte-block bookkeeping. The artifact: once
>1024 bytes accumulate and the sentinel is evicted, the front-skip starts
hiding a real globally-oldest entry from `count`/`str`.

**We model the clean contract:** no sentinel, no front-skip — **every
non-evicted entry is readable.** Pre-overflow behavior is byte-for-byte
identical to C++; the only divergence is a single hidden globally-oldest entry
that C++ would lose *after* the budget is first exceeded — a byte-block
artifact, not designed behavior. In the clean `Vec` model both the sentinel and
the skip simply vanish. This is intentional; document it so the spec reviewer
does not flag it as a missing behavior.

## Omit (MOOT — do NOT stub, per the project's no-dead-stubs rule)

- `initHistory` / `doneHistory` — alloc/free the global block. A thread-local
  `Vec` is born empty and dropped at thread exit. **Omit entirely.**
- `historySize` as a *mutable* global (C++ lets apps override it before
  `initHistory`). Port as `const HISTORY_SIZE: usize = 1024`; mention it as the
  one provisional simplification. No consumer overrides it.
- The `HistRec` byte layout, `operator new` placement, `movmem`/`memcpy`,
  `advance`/`backup`/`next`/`prev` pointer arithmetic — all subsumed by `Vec`.

## Wiring

- New `mod history;` in `src/widgets/mod.rs` (keep the file private; re-export
  the four free fns).
- `pub use history::{history_add, history_count, history_str, clear_history};`
  in `src/widgets/mod.rs`, then add them to the `pub use widgets::{...}` line in
  `src/lib.rs:120` (alphabetical-ish, matching the existing style).

## Verification (no snapshot — pure data, renders nothing)

Unit tests in the module (each starts with `clear_history()` for belt-and-
suspenders isolation, independent of the thread-local model). Cover, each
bite-checked (a wrong impl must fail):

- **add + count + str round-trip** per id; ordering is oldest→newest
  (`history_str(id,0)` = first added).
- **empty string is ignored** (`history_add(id, "")` → count unchanged).
- **per-id isolation**: ids 1 and 2 don't see each other's entries; `count`
  and `str` filter correctly.
- **dedup moves to newest**: add "a","b","a" under one id → count==2, order is
  "b","a" (the re-added "a" is now last). Bite: a no-dedup impl gives count 3.
- **out-of-range index** → `None`.
- **global byte-budget eviction**: add enough bytes across MULTIPLE ids to
  exceed 1024, assert the globally-oldest entry (possibly a *different* id) is
  evicted first. Bite: a per-id budget would not evict across ids — construct
  the test so a per-id model gives a different surviving set.
- **dedup-before-evict**: near-full store, re-add an existing string; assert no
  unrelated entry was evicted (the dup's bytes were freed first).
- **clear_history** empties all ids.

## Definition of done

`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all --check` all clean (set
`CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`). English-only
comments/identifiers. The four functions exported from `tvision::`.
