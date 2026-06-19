# Primitive Types  (guide pp. 390–391, 581–582, 560)

Rust module(s): `src/validate.rs` (TCharSet), `src/screen/buffer.rs` (TVideoBuf),
                `src/screen/draw_buffer.rs` (TByteArray / TWordArray context)
magiblot: `include/tvision/ttypes.h` (TByteArray, TWordArray), `include/tvision/views.h` (TCharSet, TVideoBuf)

> All four are bare type aliases in Pascal — no fields, no methods, no inheritance.
> The guide gives each a declaration, a function description, and a cross-reference.
> Several are purely DOS/memory-manager artifacts with no meaningful analog in Rust.

---

## TCharSet  (guide p. 390)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TCharSet = set of Char` (type declaration) | 390 | EQUIVALENT | OK | `String` field `valid_chars` inside `tv::FilterValidator` | 3 | Guide: a 256-bit Pascal set of `Char`, used by `TFilterValidator.ValidChars` to hold the allowed character set. Rust: `FilterValidator::valid_chars: String` (contains the legal characters). `valid_chars` is private; membership is tested with `valid_chars.contains(c)`. The module doc notes the byte-vs-char difference (ASCII-safe in practice). There is no free-standing public `CharSet` type — the set is embedded in `FilterValidator`. `FilterValidator::new` rustdoc raised to 3 (now covers how/when to use it, replacing the TCharSet role). |
| Function ("filter validator objects use a field of type TCharSet to define the legal characters a user can type in a filtered input line") | 390 | EQUIVALENT | OK | `tv::FilterValidator::valid_chars: String` | 3 | The `FilterValidator::new(valid_chars)` constructor doc explains what the set controls. The "ASCII vs. Unicode" deviation is noted in the heritage comment. Constructor rustdoc now directs the reader to `is_valid_input` for the while-typing path. |
| `See also: TFilterValidator.ValidChars` | 390 | EQUIVALENT | OK | `tv::FilterValidator` | 3 | `TFilterValidator` is ported as `tv::FilterValidator`; `ValidChars` is its private `valid_chars` field. The cross-reference is fulfilled by the struct doc. FilterValidator struct doc raised to 3 (now explains when to use it vs other validators). |

### TCharSet summary

- PORTED: 0   EQUIVALENT: 6   NOT-PORTED: 5   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |

---

## TByteArray  (guide p. 390)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TByteArray = array[0..32767] of Byte` (type declaration) | 390 | NOT-PORTED | — | — | — | Guide: a byte array type for general use in typecasts, referenced only by `TStringListMaker`. Neither `TStringListMaker` nor any API that requires a free-standing `TByteArray` type alias has been ported. In Rust, raw byte buffers are `&[u8]` / `Vec<u8>` — the typecast use-case is obsolete in a type-safe language. |
| Function ("a byte array type for general use in typecasts") | 390 | NOT-PORTED | — | — | — | The function is purely to enable Pascal raw-memory casts. No Rust equivalent is needed; `&[u8]` / raw pointer casts cover the same ground natively. |
| `See also: TStringListMaker` | 390 | NOT-PORTED | — | — | — | `TStringListMaker` is not ported (part of the DOS resource/stream machinery). |

### TByteArray summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3: N/A (not ported) |

---

## TWordArray  (guide pp. 581–582)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TWordArray = array[0..16383] of Word` (type declaration) | 581 | NOT-PORTED | — | — | — | Guide: a word array type for general use. magiblot does not define `TWordArray` in the modern headers — it is a DOS-era Pascal artifact. The guide notes only "for general use" with no API cross-reference. No Rust equivalent is needed; `Vec<u16>` or `&[u16]` serve the same purpose natively. |
| Function ("a word array type for general use") | 581 | NOT-PORTED | — | — | — | Same rationale as `TByteArray`. No cross-references from any ported class. |

### TWordArray summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3: N/A (not ported) |

---

## TVideoBuf  (guide p. 560)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TVideoBuf = array[0..3999] of Word` (type declaration) | 560 | EQUIVALENT | OK | `tv::Buffer` (`src/screen/buffer.rs`) | 3 | Guide: a fixed 4000-word (80×25 screen) array of char+attr words, used by `TGroup.Buffer` for a view's video back-buffer. magiblot modernised this to a heap-allocated `TScreenCell *` with explicit width/height. Rust: `tv::Buffer` is a `width × height` grid of `tv::Cell` (Unicode-capable), whole-tree repainted each frame and diffed against the previous frame. `Buffer` struct rustdoc raised to 3: now explains render-target usage + heritage note naming `TVideoBuf`. |
| Function ("this type is used to declare video buffers") | 560 | EQUIVALENT | OK | `tv::Buffer` | 3 | `Buffer`'s module doc explains "in-memory screen grid" and the repaint model. `Buffer` struct rustdoc now has a full `# Turbo Vision heritage` section naming `TVideoBuf` and the fixed-to-dynamic-size evolution. |
| `See also: TGroup.Buffer` | 560 | EQUIVALENT | OK | `tv::Buffer` held by `tv::Program` (the root group) | 3 | In the Rust port, the back-buffer is owned by the root `Program` group (the pump), not stored per-view. The `Buffer` struct rustdoc now makes this clear ("Program root owns two buffers and swaps them each event-loop iteration"). |

### TVideoBuf summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |

---

## Combined Summary

- Total rows: 11
- PORTED: 0   EQUIVALENT: 9   NOT-PORTED: 5 (TByteArray ×3, TWordArray ×2)   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: `TByteArray` and `TWordArray` are correctly NOT-PORTED — they are Pascal typecast helpers with no meaningful analog in Rust. The most actionable gap is that `tv::Buffer`'s rustdoc does not mention it replaces `TVideoBuf`; adding a one-sentence `# Turbo Vision heritage` note would close that traceability gap.
