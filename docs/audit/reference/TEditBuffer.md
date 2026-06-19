# TEditBuffer type  (guide p. 421)

Rust module(s): src/widgets/editor.rs   |   magiblot: include/tvision/editors.h (`typedef char TEditBuffer[...]` / `PEditBuffer`)

> `TEditBuffer = array[0..65519] of Char;` — the flat character array that
> `TEditor`/`TMemo` use to hold their edit buffer. In the C++ port it is
> `char*`/`PEditBuffer` (a heap allocation). It is a storage *type*, not a class:
> it has no fields or methods of its own.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TEditBuffer` (array type) | 421 | EQUIVALENT | OK | `Editor.buffer: Vec<u8>` (private field) | N/A | The flat fixed-array edit buffer becomes a `Vec<u8>` of physical capacity `buf_size`, split by a movable gap of `gap_len` at `cur_ptr` (gap-buffer storage). The 64K (`0..65519`) DOS array bound is a memory-era artifact and is **not** reproduced — the `Vec` grows in file-editor mode (`set_buf_size` rounds up to a 0x1000 boundary). The DOS-segment cap is intentionally dropped (idiomatic mapping: DOS/memory-manager machinery → no analog). The gap semantics (logical offset → physical index via `buf_ptr`, gap sits at the cursor) are faithfully ported. Private field, so rustdoc N/A; the module-level doc-comment explains the gap representation thoroughly (score would be 3 if it were public). |
| `PEditBuffer` (pointer-to-buffer) | 421 | NOT-PORTED | — | — | — | The Pascal `PEditBuffer` / C++ `PEditBuffer` pointer alias has no analog: ownership is the `Vec<u8>` itself (idiomatic mapping: raw pointers → owned storage / handles). No gap. |

## Summary

- PORTED: 0   EQUIVALENT: 1   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: `TEditBuffer` is a flat `char` array in the original; the
  port replaces it with a `Vec<u8>` gap buffer (`buffer`/`gap_len`/`cur_ptr`).
  The 64K array bound is deliberately dropped (DOS-era), and file-editor mode
  grows the `Vec`. No gaps; faithful gap-buffer storage with a documented,
  modern (unbounded, growable) deviation. The type carries no behavior, so there
  is nothing to flag.
