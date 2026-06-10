# rstv — implementation log

> The per-session implementation narrative + the git-commit changelog, **newest
> first**. This is the historical record; the forward-looking "where things stand
> / what's next" lives in [`docs/HANDOVER.md`](file:///home/oetiker/checkouts/rstv/docs/HANDOVER.md).
> Add a new section at the top each session; do not rewrite history.

## Session addendum — the CommandSet denylist flip (A1)

**`faabc78`** — **row A1** (subagent-built, two-stage reviewed): command
enablement flipped from the unfaithful allowlist to the faithful
`tview.cpp::initCommands` **denylist** — everything enabled by default,
exactly {ZOOM, CLOSE, RESIZE, NEXT, PREV} seeded disabled. The allowlist
`default_command_set()` + its file-dialog BANDAID are deleted; the "OK does
nothing" bug class (a new feature's command silently dropped by the central
list) is structurally gone. `update_menu_commands`' argument is now
contractually the **disabled set**; `StatusLine`'s cache renamed to match.
The C++ ">255 always enabled" rule is *subsumed* (open string space, every
command maskable) — documented in the new
`docs/design/command-enablement.md` + PORTING-GUIDE D1. **New seam:**
`Context::command_enabled(cmd)` backed by a per-pump snapshot (refreshed at
all three dispatch sites) — unblocks B1 (button/inputline graying); the six
"no command-set query" deferral comments retagged `TODO(B1)`. `CommandSet`
gained polarity-neutral `insert`/`remove` aliases (quality-review finding:
seeding a *disabled* set via `enable_cmd` was a polarity trap; the faithful
`enableCmd`/`disableCmd` names stay for enabled-set use). **Zero snapshot
changes** — the 5-command seed reproduces old observable behavior everywhere
it was correct (the spec reviewer verified no code path relied on
unknown-command filtering). 995 lib tests (+6); clippy + fmt clean.

## Session — the backlog run begins: TODO audit, BACKLOG.md, theme chain (A4)

The post-port backlog run started with a **full TODO audit** (every marker in
the tree cross-checked against PORT-ORDER/HANDOVER), producing:

- **`7efd683`** — comments-only hygiene: ~10 stale TODO breadcrumbs retired
  (validator `error()` claims, the history outside-click "(C) DEFERRED",
  frame.rs row-33 drag TODOs that `Window::start_drag`/`DragCapture` had long
  satisfied, backend row-31 tags, the input-line row-59 transfer hedge).
- **`fb99048`** — **`docs/BACKLOG.md`**, the PORT-ORDER successor: Phase A
  FOUNDATION seams → Phase B mechanical fan-out → Phase C backlogged features.
  Two user directives recorded: **OS clipboard by default** (A6; internal
  buffer demoted to fallback) and **no hand-rolled terminal setup in app
  code** (B7; `hello.rs` currently compensates — C++ `TApplication`/`TScreen`
  does it in the ctor).
- **`66e7527`** — **row A4, the theme-trust-the-chain pass** (subagent-built,
  two-stage reviewed; spec reviewer independently re-derived 13 chains): every
  `theme.rs` value now carries its literal `cpX → cpOwner → cpAppColor` chain
  inline; 19 roles corrected (InputNormal 0x1F, the cluster 0x30/0x3F/0x3E
  cyan-strip family, MenuSelected 0x20, the list matrix resolved for a
  gray-dialog owner, indicator 0x1F/0x1A, label shortcuts 0x7E, outline-in-
  blue-window, FrameDragging 0x1A). New roles (ROLE_COUNT 67→75): the
  `FrameCyan*` quartet (cyan window scheme wired; all six `TODO(row 34 cyan
  theming)` sites retired), `HistoryArrow`/`HistorySides`,
  `HistoryViewerNormal/Focused` via the new **`ListRoles` quintet +
  `ListViewer::list_roles()` hook** (the D7 successor of the
  `THistoryViewer::getPalette` virtual; `HistoryViewer::LIST_ROLES` const;
  `tv::ListRoles` re-exported). `HistoryWindow` keeps the blue family with two
  documented unobservable deviations (passive frame fg; sb controls quirk).
  31 snapshots regenerated, every changed cell chain-attributed. 989 lib
  tests; clippy + fmt clean.

## Session addendum — topmost pre-inserted window unfocusable (currency foundation)

Follow-up user report on `examples/hello.rs`: the three pre-inserted windows
start with **no window focused**, and clicking the topmost (Window 3) does
nothing — only clicking a lower window focuses it.

**Root cause** (`e8d82f2`): C++ maintains the invariant *topmost ofTopSelect
window == current* via insert-time `show() → owner->resetCurrent()`; two
faithful pieces depend on it — `putInFrontOf`'s `already_in_place` no-op
(which skips its `resetCurrent` tail) and `select()`'s makeFirst-only path.
rstv's ctx-less `Group::insert` (deliberate, D3) skips currency and broke the
invariant: `Desktop::insert_view` left `current == None`, so a click on the
already-topmost window ran `focus_child → make_first → already_in_place
no-op` and **never reached `set_current`**. `Desktop::insert_and_focus` had
already documented this exact trap as a local DEVIATION workaround — the
second compensation site (after `exec_view`), so this was fixed at the
foundation instead of adding a third:

- **`Group::focus_child` self-heal**: after `make_first`, re-assert currency
  when `current != id` — a no-op whenever the C++ invariant holds.
- **`Program::new` startup currency**: desktop-internal `reset_current` after
  the root `set_current(desktop)` (the C++ insert-time cascade collapsed to
  one startup call); the queued `EnableCommand` deferreds drain on the first
  pump (out_events guaranteed non-empty by the desktop focus broadcast).
- **`insert_and_focus` DEVIATION retired** → plain `focus_child` (also more
  faithful: the `focus()` validate gate now applies on the runtime open path).

One existing test adapted (it relied on nothing-focused-at-startup, i.e. on
the bug). Verified live in tmux (startup active frame + clicks both ways) and
by 3 new tests. 988 lib tests green; clippy + fmt clean. Spec-reviewed.

## Session — button shadow color, mouse hold-tracking, gray dialog surface

User report, two button bugs from live testing: **(a)** the cells around every
button render solid dark grey regardless of the surface it sits on; **(b)** a
button fires on mouse-*down* instead of the modern (and faithful C++) press-
down / track / fire-on-release-inside behavior. Mid-session follow-up: after
fixing (a), the dialog surface itself turned out to be **blue** while every
dialog widget is colored for the C++ *gray* dialog — the real (a) culprit.

- **`36188f4` fix(theme): ButtonShadow follows the gray-dialog palette chain.**
  `classic_blue` had `ButtonShadow` hardcoded darkgray-on-black, with a comment
  claiming the literal chain value "is not shadow-like". The chain
  `cpButton[8]=0x0F → cpGrayDialog[15]=0x2E → cpAppColor[46]=0x70` is
  black-on-**lightgray**: the ▄█▀ half-block glyphs paint black while the
  background half shows the dialog surface — that *is* the adaptive classic
  shadow. (Every other Button role already matched its chain.) 13 snapshots.
- **`9aa291d` feat(button): mouse press-and-hold tracking (D9 capture).**
  Button deferral 3 implemented now that the capture stack exists:
  `ButtonTrackCapture` (the `DragCapture`/`ColorDragCapture` shape) + two new
  `Deferred` variants (`ButtonTrackDown`, `ButtonTrackRelease`) + pump brokers
  (the `MakeButtonDefault` shape). Faithful details: tracking rect is
  `clickRect.b.x++` (one wider than the press gate); the press decision uses
  the **last move's** containment, not the mouse-up position (the C++ loop body
  never re-evaluates the up event); all non-mouse events are swallowed during
  the hold (`mouseEvent`'s discard — the hold is modal). `abs_origin` cached at
  draw time converts the capture's absolute coords to button-local (the
  ColorPicker `body_origin` pattern). Id-less (uninserted) buttons keep the
  immediate press as a documented degenerate fallback.
- **`14544f6` feat(theme,frame): gray dialog surface (the row-34 gray chunk).**
  `Frame` hardcoded the blue `Frame*` roles for windows *and* dialogs; in C++
  `TFrame::draw` resolves through the **owner's** palette, so a `TDialog` gets
  the lightgray surface its widgets are colored for. Added
  `FrameGrayActive/Passive/Dragging/Icon` (literal chain
  `cpFrame → cpGrayDialog → cpAppColor`: 0x70 / 0x7F / 0x7A / 0x7A);
  `Frame.palette: WindowPalette` selects the role family in `draw`;
  `Window::set_palette` now propagates to the frame child (it was
  "recorded only" since row 34). Cyan still falls back to blue
  (`TODO(row 34 cyan theming)`). 6 dialog snapshots flip frame/interior attrs
  only; blue-window snapshots unchanged. Noted: blue `FrameDragging` 0x1E is a
  pre-existing deviation from its literal chain value 0x1A (gray follows the
  chain).

**Verification:** 985 lib tests green; clippy + fmt clean. Snapshot diffs
verified cell-by-cell (glyphs identical, only the targeted attrs flip).

Process: subagent-driven (Sonnet implementers for the two button fixes, main-
model implementer for the frame seam; spec-compliance review on each chunk +
code-quality review on the tracking feature → 2 test fixes applied).

## Session — the D8 window-shadow pass lands (`d5d9354`)

User report: **no shadows rendered** in `examples/hello.rs` — windows sat flat
on the desktop. Diagnosis: the shadow plumbing was half-built since row 33.
`Window::new` and `MenuBox` set `state.shadow` (sfShadow), the per-cell
`Modifiers::no_shadow` marker (slNoShadow) existed in the cell format, and
comments referenced "the D8 shadow pass" — but nothing ever *read* either flag.
`Group::draw` carried `// TODO(row 33): shadow casting (no infra yet)`.

**Port** (tview.cpp `shadowSize={2,1}`/`shadowAttr=0x08`, tvwrite.cpp
`applyShadow`, modern non-Borland branch):

- **`Role::Shadow`** (ROLE_COUNT 62→63) — the C++ global `shadowAttr` (a
  file-scope constant there, not a palette entry; themed per D7).
  `classic_blue`: darkgray on black.
- **`DrawCtx::cast_shadow(area_local)`** + `SHADOW_SIZE` (`view/context.rs`):
  region = `(area + shadowSize) \ area` — right strip 2 cols (top+1 to
  bottom+1) + bottom strip 1 row (from left+2); clipped to the ctx clip.
  Per-cell faithful `applyShadow`: glyph preserved, `no_shadow`-marked cells
  skipped (no double-shadow), **reversed** shadow attr on black backgrounds
  (incl. `Default` — `toBIOS(false)` maps it to 0; `bg_is_black` is a
  documented D6 simplification, no quantization ladder), original modifiers
  survive (`setStyle(attr, style|slNoShadow)`), `no_shadow` set.
- **`Group::draw` hook**: after each visible sfShadow child, `cast_shadow` —
  back-to-front painter's order makes higher siblings overwrite occluded
  shadow cells (the D8 equivalent of the TVWrite occlusion walk). One hook
  covers Window/Dialog/EditWindow/MenuBox (all draw through `Group::draw`;
  Desktop/Window delegate `draw` to their inner Group). Buffer resets each
  frame, so `no_shadow` never goes stale.

**Verification:** 8 `cast_shadow` unit tests + a window-over-desktop snapshot
(exact offset-L, glyphs preserved, `+no_shadow`). Zero existing snapshots
changed — investigated, not assumed: every prior Window/MenuBox snapshot
renders the caster as the **root** view (no owning Group → no shadow pass),
faithfully so since the shadow composites over the *owner's* other subviews.
Live tmux check of `hello.rs` shows the darkgray ░ strips beside/below each
window. 985 lib tests green; clippy + fmt clean.

Process: subagent-driven (Opus implementer → spec review → quality review →
fix round: +zero-size-view no-op test, two style nits → re-approved).

## Session — inserted EditWindow opened keyboard-dead (focus-on-insert)

Live testing of the editor wiring surfaced a third pump-only bug: **opening a
file or a new editor window worked, but typing and Save did nothing.** Both
symptoms share one root cause.

**Root cause.** C++ `TView::setState(sfVisible)` calls `owner->resetCurrent()`
for an `ofSelectable` view, so inserting the `FileEditor` into an `EditWindow`
makes it the window's `current` at construction; focusing the window then
cascades focus into the editor. rstv's ctx-less `Group::insert` deliberately
**skips** that cascade, and `Desktop::insert_and_focus` (the `desktop_insert`
path, unlike the modal `exec_view`) never called `reset_current` — so the
inserted window arrived with `current == None`, the editor was never focused, and
both KeyDown inserts and `cmSave` (which `FileEditor::handle_event` does handle)
routed to nothing.

**Fix** (`src/desktop/desktop.rs::insert_and_focus`): call `reset_current` on the
inserted view to establish its internal currency, then `set_current(Some(id),
Normal)` to focus it. Note: the obvious `focus_child` does **not** work for a
freshly-inserted `ofTopSelect` window — it routes through `make_first →
put_in_front_of`, which short-circuits with an `already_in_place` no-op when the
new window is already top of the Z-order, so `set_current` never runs and focus
never cascades. `set_current` directly is also the faithful C++ insert→show→
resetCurrent→setCurrent path. Symptom-level regression test
(`inserted_edit_window_receives_typed_characters`) drives the real pump and
asserts a typed `'X'` lands in the editor buffer. 967 lib tests green.

This is the **third** site compensating for the missing `show()→resetCurrent`
cascade (after `exec_view` and `HistoryWindow::select_child`); logged as a
standing deferral (HANDOVER + memory `show-resetcurrent-cascade-gap`) for the
post-port architecture pass alongside the CommandSet denylist flip.

## Session — two runtime bugs from the live editor wiring (`63cbc32`)

Wiring a live `FileEditor`/`FileDialog` into `hello.rs` surfaced two bugs that
unit tests (which bypass the pump) never hit:

1. **FileDialog OK did nothing.** The OK/Open button fires `cmFileOpen` (C++
   `stddlg.h` 1001 — a `> 255` ALWAYS-enabled command). The D1 allowlist dropped
   C++'s ">255 always enabled" rule, so `pump_once`'s command filter dropped
   `cmFileOpen` before `FileDialog::handle_event` could `end_modal`. **Bandaid:**
   added the file-dialog result commands to `default_command_set()`. The real fix
   (allowlist→denylist flip, matching C++ `initCommands`) is recorded in HANDOVER
   "Standing deferrals" + memory `command-set-allowlist-smell` for the post-port
   architecture pass.
2. **New/Open editor windows rendered as a flat green field.** `Role::ScrollerNormal`
   held the degenerate "no-window-remap" color (`0x28` green). Corrected to the
   faithful in-window palette-chain resolution `0x1E` (yellow on blue) / `0x71`
   (blue on lightgray) — `cpScroller→cpBlueWindow→cpAppColor`. Regenerated 10
   scroller/editor/memo/terminal/color-picker snapshots.

Two regression tests added (cmFileOpen survives the pump filter; default set
enables the file commands). 966 lib tests green.

## Session — hello.rs: EditWindow + FileDialog wired into the demo app

Enhanced `examples/hello.rs` to be a real editor application: File → Open (F3),
File → New (F4), File → Save (F2), File → Save As… all work. New windows fill the
desktop extent and get keyboard focus immediately. This validates the `FileEditor`
brokers (`SyncEditorDelta`, `OpenSaveAsDialog`, `SaveAsPick`, `ModalFrame` outside-
click, modified-prompt) end-to-end in a running app, not just unit tests.

### New `Program` API (`a947004`)

- **`desktop_rect()`** — desktop local extent (mirrors C++ `deskTop→getExtent()`).
- **`desktop_insert(view)`** — insert a window into the desktop and focus it at
  runtime. Uses `Desktop::insert_and_focus(view, ctx)` reached via a new
  `as_any_mut → downcast_mut::<Desktop>()` hatch (same `as_any_mut` pattern as
  `FileEditor`, `Button`; `as_any_mut` removed from `Desktop`'s `#[delegate]` skip
  list and overridden to `Some(self)`).
- **`open_file_dialog(title, wild)`** — runs a `FileDialog` via `exec_view`, reads
  the `resolved_name` via the new `gather_self = true` branch (see below).
- **`exec_view_with_completion` `gather_self: bool` parameter** — when `true`,
  pre-mints the modal's ViewId with `ViewId::next()`, inserts via `insert_with_id`,
  and automatically passes that id as the gather target so `FileDialog::value()`
  (the resolved path) is readable after the modal closes. All 6 existing callers
  unchanged (pass `false`).

### `lib.rs`

Re-exports: `EditWindow`, `FileEditor`, `FileDialog`, `FD_OPEN_BUTTON`.

---

## Session — FileEditor::saveAs (view-triggered FileDialog seam)

Wired **`TFileEditor::saveAs`** (`tfiledtr.cpp`) — the last unported editor
behavior after all 92 PORT-ORDER rows landed. `cmSaveAs` / `cmSave`-on-untitled
now open a `FileDialog`, read the chosen filename, save, and refresh the hosting
`EditWindow`'s frame title. Modelled on the existing async-modal-from-a-view
seams (`OpenHistory` / `OpenMessageBox`).

### The seam (HistoryPick shape + a re-inject twist)

- **`Deferred::OpenSaveAsDialog { editor_id }`** + `Context::request_save_as_dialog`
  (`context.rs`): a `FileEditor` leaf holds only `&mut Context` and cannot run a
  nested modal inline, so it requests one.
- **Pump arm** (`program.rs` `pump_once` deferred drain): builds the C++ `edSaveAs`
  dialog — `FileDialog::new("*.*", "Save file as", "~N~ame", FD_OK_BUTTON, 101)`,
  pre-filled with the editor's current filename — and stashes it into
  `pending_modal` with `ModalCompletion::SaveAsPick { editor_id }` (self-centering,
  no bounds / no initial-focus).
- **`apply_modal_completion(SaveAsPick, …)`**: the accept test is **`!= CANCEL`**,
  NOT `== OK` — the `FileDialog`'s `FD_OK_BUTTON` ends the modal with `cmFileOpen`,
  not `cmOK` (faithful to C++ `editorDialog(edSaveAs,…) != cmCancel`). On accept it
  reads `value()` (the `resolved_name` cache, kept current by
  `validate_modal_close → valid()`), sets `file_name` + `pending_title_update` on
  the editor, and **re-injects `Command::SAVE`** so the normal `cmSave` path runs
  `save_file` with a full `ctx`.

### Editor + window wiring (`editor.rs`)

- `FileEditor::pending_title_update` flag; `save()` untitled branch requests the
  dialog; `handle_event` adds a `cmSaveAs` arm and, after a flagged `cmSave`,
  broadcasts `cmUpdateTitle` (C++ `message(owner, …, cmUpdateTitle)`).
- `EditWindow::handle_event` override listens for the `cmUpdateTitle` broadcast and
  recomputes its frame title from the editor's current `file_name` via the new
  `Window::set_title` (`window.rs`, mirrors `set_flags`). The event is **not
  cleared** (unlike C++): rstv's `broadcast` fans out to every window, so clearing
  would starve non-first windows; each refreshes its own title idempotently.

### The `as_any_mut` foundation fix (load-bearing)

The seam needs to downcast a group child back to `&mut FileEditor`, but
`#[delegate(to = editor)]` forwarded `as_any_mut` to the inner `Editor` — so the
downcast silently missed. Added an `as_any_mut → self` override to `FileEditor`,
which would have **regressed the editor scroll-sync/paste brokers**
(`SyncEditorDelta` / `EditorPaste` target the inserted view's id = a `FileEditor`
in an `EditWindow`, yet need the inner `Editor`). Introduced
`widgets::editor_mut(&mut dyn View) -> Option<&mut Editor>` (FileEditor-first,
else Editor/Memo) and routed both broker arms + two close-test helpers through it.

### Tests

7 new (957 → 964): `save_as_requests_dialog`, `untitled_save_requests_dialog`,
`save_as_then_save_writes_and_broadcasts_title`,
`edit_window_updates_title_on_broadcast` (editor.rs);
`save_as_pick_sets_filename_and_reinjects_save`, `save_as_pick_cancel_is_noop`,
`open_save_as_dialog_deferred_stashes_pending_modal` (program.rs).

### Known limitation (breadcrumbed in `save()`)

The `valid()` modified-close path (cmClose → Yes → untitled `save()`) returns
`false` and so VETOES the close, then the dialog opens separately. A full fix
requires `validate_modal_close` to drive an `OpenSaveAsDialog` inline (the §6
modal-close twin of this seam). No consumer exercises the untitled-close+Yes path.

---

## Session — ModalFrame outside-click delivery seam (row 56/57)

Cleared the long-standing **row 57 modal-loop breadcrumb**: outside-bounds
positional events were swallowed by `ModalFrame` before reaching the modal view,
so `THistoryWindow`'s `evMouseDown && !mouseInView → endModal(cmCancel)`
(`thistwin.cpp`) could never fire. Commits `af109fc` (feature) + `95ba912`
(review fixes).

### The seam

- **`CaptureHandler::is_modal_gate()`** (`capture.rs`, default `false`;
  `ModalFrame` overrides `true`): distinguishes a true modal-bounds gate from drag
  / menu-box handlers that also carry a `view()`. **`CaptureStack::top_modal_view()`**
  returns the top handler's `ViewId` only when it is a modal gate.
- **Pump pre-dispatch redirect** (`program.rs` `pump_once`, before
  `captures.dispatch`): when the top capture is a `ModalFrame` and the event is a
  positional event outside the modal's bounds, the pump **localizes** it
  (`position -= modal_bounds.a`, the makeLocal) and delivers it directly to the
  modal view — skipping `captures.dispatch` + `program_handle_event`. The root
  group sits at `(0,0)`, so `modal_bounds.a` is the absolute origin (same
  coordinate contract as the existing `ModalFrame` gate).
- **`HistoryWindow::handle_event` part (C)** now implements the C++ check: after
  the base `TWindow::handleEvent`, an uncleared `MouseDown` whose (localized)
  position is outside `get_extent()` → `ctx.end_modal(cmCancel)` + clear. A plain
  `Dialog` has no such override, so it **ignores** outside clicks (faithful — C++
  `TDialog` does not cancel on outside click).

### Tests

4 new (953 → 957): `outside_modal_click_delivered_to_modal_view`,
`inside_modal_click_uses_normal_dispatch`, `plain_dialog_modal_ignores_outside_click`
(program.rs); `history_window_cancels_on_outside_click` (history.rs). The existing
`modal_frame_gates_events` test was updated to the new "deliver, don't swallow"
contract. Review fix `95ba912` removed an unused `CaptureStack::top_view()`
(dangerous API surface — `top_modal_view()` is the only correct entry point) and
added the plain-Dialog regression guard.

---

## Session — terminal family (rows 91–92)

Ported **`TTextDevice`** (row 91) and **`TTerminal`** (row 92) from
`textview.cpp`/`ttprvlns.cpp` into a new `src/widgets/terminal.rs`. These are the
last two rows of the 92-class porting checklist — **all rows now done**.

### Key design decisions

- **`TextDevice` (row 91):** a plain `pub trait` with a single method `write_bytes(&mut self, data: &[u8], ctx: &mut Context) -> usize`. The C++ `streambuf` inheritance and `otstream` wrapper are dropped entirely (D11/D12); users call `write_bytes` directly.

- **`Terminal` (row 92):** embeds a `Scroller` with `#[delegate(to = scroller)]` on the `impl View for Terminal` block, providing only `draw` and `as_any_mut` overrides. This means the macro auto-generates all the boilerplate `View` forwarders. `as_any_mut` explicitly forwards to `self.scroller.as_any_mut()`, returning `Some(&mut self.scroller)` — enabling the existing `SyncScrollerDelta` pump arm to downcast and call `apply_delta` without any new `Deferred` variant.

- **Ctor / `init` split:** the C++ ctor calls `setLimit`/`setCursor`/`showCursor` which need a `Context`. Following the `TOutline` pattern, `Terminal::new` takes no `Context`; consumers call `Terminal::init(&mut self, ctx)` after insertion.

- **Ring-buffer helpers:** `buf_dec`/`buf_inc` wrap at `buf_size` (safe usize arithmetic, no overflow); `can_insert` faithfully translates the C++ signed-comparison trick to usize; `prev_lines`/`find_lf_backwards` (from `ttprvlns.cpp`) are verbatim ports handling wrap-around.

- **`draw()`:** uses `put_str` (UTF-8-width-aware) + `fill` for padding. UTF-8 truncation (`discardPossiblyTruncatedCharsAtEnd`) is `str::from_utf8` with `valid_up_to` trim (D13). `setCursor(-1,-1)` dropped (D8).

- **`write_bytes`:** reads `limit.y` *before* calling `set_limit` (critical ordering); eviction loop faithfully mirrors C++ `do_sputn`; `drawLock`/`drawView` dropped (D8).

### Tests

11 unit tests: ring-buffer helper round-trips (`buf_inc`/`buf_dec`/`can_insert`), `prev_lines` (linear + wrap-around), `write_bytes` behavior (newline counting, eviction), and two insta snapshots (`draw_empty_terminal`, `draw_with_lines`).

### Commits

- **feat(terminal): port TTextDevice/TTerminal (rows 91–92)** (`0288317`) — `TextDevice` trait, `Terminal` ring-buffer view, 11 tests, 2 snapshots; all 92 PORT-ORDER rows complete.
- **fix(terminal): faithful prev_lines wrap-scan + draw scratch-per-chunk** (`7a987de`) — `find_lf_backwards` returns `(bool, usize)` so the outer loop continues across the ring-buffer wrap boundary (was returning `que_back` early); draw inner loop uses a fresh scratch buffer per 256-byte chunk (was cumulative, corrupting long lines).
- **fix(terminal): advance line_pos by raw bytes; guard draw loop on size.y==0** (`62625e8`) — `line_pos` now advances by raw `copy_len`/`fst_len+snd_len` (not trimmed `slen`) so invalid/continuation UTF-8 bytes never cause an infinite loop; outer draw loop is `while y >= 0` (was `loop{…if y==0 break}`, hanging when `size.y==0`). Added ring-wrap draw snapshot and tighter eviction test assertions.

---

## Session — outline family (rows 88–90)

Ported the **outline cluster** (`TNode`/`TOutlineViewer`/`TOutline`) into a new
`src/widgets/outline.rs`. This is the faithful-port resumption after the
color-picker extension; rows 81–87 stay dropped (see the next session below).

`TOutlineViewer` is a `TScroller` subclass whose abstract virtuals
(`getRoot`/`getNext`/`getChild`/`getText`/`isExpanded`/`hasChildren`/`adjust`) are
called from *inside* the base's own `draw`/`handleEvent`/`update`. As with
`TListViewer` (row 28), a concrete-struct embed can't dispatch from the base's draw
back into the embedder, so it is modeled as the **`OutlineViewer` trait + free
functions generic over `<L: OutlineViewer + ?Sized>`** (`traverse`, `ov_draw`,
`ov_handle_event`, `ov_set_state`, `ov_update`, `ov_expand_all`, `adjust_focus`,
`ov_get_node_info`, `ov_get_graph`, `create_graph`) + an `OutlineViewerState` for
the non-virtual data (`delta`/`limit`/`foc`/scrollbar ids).

### Commit

- **`7472343` feat(outline): port TNode/TOutlineViewer/TOutline (rows 88–90)** —
  - **`Node` (row 88):** owned `Box<Node>` tree (`text`/`child_list`/`next`/`expanded`)
    with a builder API (`new`/`with_children`/`with_next`/`with_expanded`); Rust's
    recursive `Box` drop replaces C++ `disposeNode` (D12 streaming dropped).
  - **`OutlineViewer` (row 89, FOUNDATION):** `traverse` ports `iterate`+`traverseTree`
    (DFS, 0-based pre-incremented positions, `ovExpanded`/`ovChildren`/`ovLast` flags,
    `lines` continuation-bar bitset, root-level sibling walk). `ov_draw` ports
    `drawTree` (focus/select/normal color matrix, `delta.x` horizontal skip via
    `put_str_part`, the `(flags & ovExpanded) ? color : color>>8` not-expanded text
    color, trailing blank-fill). `create_graph`/`ov_get_graph` port the box-drawing
    graph string (`levWidth=endWidth=3`, glyphs from `ctx.glyphs()`). Keyboard nav,
    `+`/`-`/`*` expand/collapse, `adjust_focus` scroll-into-view all faithful;
    **mouse drag-loop deferred** (`TODO(row 31, D9)`, single-click only).
  - **`Outline` (row 90):** concrete impl over the owned tree; `adjust(pos, expand)`
    resolves a DFS position to the owned node via the **safe recursive**
    `set_expanded_at_pos` (no `unsafe`). The ctor does **not** call `update()`
    (needs a `Context`) — consumers call `ov_update` after insertion, like the
    scroller/list-viewer.
  - **Seams added:** `Role::Outline{Normal,Focused,Selected,NotExpanded}` (D7,
    `cpOutlineViewer "\x6\x7\x3\x8"`; indices 58–61, `ROLE_COUNT` 58→62);
    `Command::OUTLINE_ITEM_SELECTED` (`cmOutlineItemSelected = 301`);
    `Deferred::SyncOutlineViewerDelta` + `Context::request_sync_outline_viewer_delta`
    + a pump apply-arm (the scrollbar→delta read-broker, downcast to `Outline`,
    mirroring `SyncScrollerDelta`).
  - Two snapshot tests (expanded-tree draw, focused-collapsed text color). No new
    `View` trait methods → `tvision-macros/src/specs.rs` unchanged.
  - **Two-stage review caught two issues, both fixed:** spec reviewer found
    `ov_expand_all`'s subtree-boundary guard was `level == start_level` (would leak
    into an ancestor's siblings at a shallower level) → fixed to `level <= start_level`;
    quality reviewer found an unsound `*const Node → *mut Node` write in `adjust`
    (UB under Stacked Borrows — provenance traced to a `&Node`) → replaced with the
    safe recursive traversal. **941 lib tests green; clippy + fmt clean.**

## Session — truecolor color-picker extension (rows 81–87 dropped)

Built the **truecolor color-picker** (`src/dialog/colorpick/`) as an
rstv-original extension replacing the faithful `TColorDialog` cluster (rows
81–87). The faithful cluster edited a flat BIOS `TPalette` that rstv deleted
under D7 — dead code by construction. The picker is reusable (not locked to a
specific palette) and produces any `Color` variant (`Default`/`Bios`/`Indexed`/`Rgb`).

### Commits in order

- **`9aa8e12` revert: drop faithful color rows 81–82** — deleted `src/dialog/colordlg.rs`
  + 3 snapshots, removed the `colordlg` exports from `dialog/mod.rs`, and
  removed the 3 unused `COLOR_*` commands from `command.rs` (921 lib tests after
  the revert; was 924 with the now-deleted colordlg tests).

- **`c66a705` feat(colorpick): ColorModel + rgb\<->hsv + BIOS display table** —
  `src/dialog/colorpick/model.rs`: `ColorModel { color, hsv }` (the picker's
  shared single-source-of-truth); `Hsv { h, s, v: f32 }` with deterministic
  round-half-up to u8; `rgb_to_hsv`/`hsv_to_rgb` (standard sextant formula);
  the 16-entry `BIOS_RGB` display palette (distinct from `quantize.rs` which
  leaves indices 0..15 = 0); `color_to_display_rgb`. HSV is retained so hue
  survives brightness→0 and saturation→0 round-trips. Pure logic, unit-tested.

- **`9f3bad1` feat(colorpick): Surface trait + PresetsSurface** —
  `Surface` trait (`draw`/`handle_event`/`drag_region_at`/`apply_drag`) + shared
  layout constants (`TAB_BAR_Y`, `INFO_COL_X`, `BODY_TOP`); `PresetsSurface` = a
  scrolling Default + 16 BIOS + 12 curated `Rgb` preset list with arrow nav,
  click select, per-row swatches. Snapshot + event tests.

- **`adc4676` feat(colorpick): RgbSurface (R/G/B gauges + hex field)** —
  Three proportional gauge bars + a `#RRGGBB` hex field + live swatch. Up/Down
  move field focus (R/G/B/Hex), Left/Right adjust ±1, PgUp/PgDn ±16, typed hex
  commits on 6 digits, click+drag scrubs a bar. Every edit → `m.set_rgb`.

- **`1303630` feat(colorpick): PlaneSurface (hue strip + half-block SV box)** —
  A vertical hue strip + a Saturation×Value box in the current hue. Half-blocks
  (`▀`) double vertical resolution. Cursor derives from `m.hsv` (no local state).
  Arrows move sat/val, `[`/`]` change hue, click+drag scrubs. Every edit →
  `m.set_hsv`, retaining hue across value→0.

- **`987c6d0` feat(colorpick): Xterm256Surface (true 16×16 grid)** —
  A true 16×16 grid of the xterm-256 palette (2 cols/cell), cursor-marked.
  Arrows move the cursor (clamped, no wrap), click selects. Every move →
  `m.set_indexed`. Cursor seeds from `Indexed(n)` or `rgb→nearest-256` on entry.

- **`c9f0642` feat(colorpick): ColorPicker view — tabs, info column, color()** —
  The reusable `ColorPicker: View` assembling the four surfaces under a tab bar +
  info column. `Ctrl+←/→` cycle tabs, `Alt+hotkey` jumps, tab-label click
  switches; plain `Tab` passes to the dialog for focus nav. Switching never
  converts/commits. `color()` is the result contract; `as_any_mut → Some(self)`
  for the drag broker. `body_origin` cached each draw for the drag-handler
  coordinate conversion. Per-tab snapshots.

- **`2b0751f` feat(colorpick): mouse drag broker (Deferred::ColorPickerDrag)** —
  The `window.rs DragCapture` pattern reused for the picker: `MouseDown` in a
  draggable region sets `active_drag` + pushes a `ColorDragCapture`; each
  `MouseMove` posts `Deferred::ColorPickerDrag { picker, pos }`; the pump
  downcasts to `ColorPicker::apply_drag`; `MouseUp` pops. **Coordinate contract:**
  ONE frame (picker-local) everywhere — `body_origin` converts absolute→picker-
  local once in the handler; each surface subtracts `body.a` exactly once; nothing
  pre-subtracts `BODY_TOP`. The prior draft mixed three frames — that was a real
  bug, now fixed. One new `Deferred` variant + `Context::request_color_drag` +
  pump arm (deferred `ColorPickerDrag` arm, after `MakeButtonDefault`).
  Integration test places the picker at non-zero absolute origin (10,5) to lock
  the frame: wrong-frame path gives a detectably different color.

- **`5b1fabf` feat(colorpick): color_dialog modal shell + result extraction** —
  `Program::color_dialog(initial) -> Option<Color>`: a 60×23 "Select Color"
  `Dialog` embedding `ColorPicker` + OK + Cancel, run on the existing modal
  machinery. Result extracted via a new `ModalCompletion::ColorPick { picker,
  sink: Rc<Cell<Option<Color>>> }` — on `cmOK` the pump arm downcasts the in-tree
  modal to `ColorPicker`, reads `color()`, writes to the sink. **No
  `FieldValue::Color`** (spec non-goal; `color()` is the contract). `Some(color)`
  on OK, `None` on Cancel/Esc. `ColorPicker` + `Tab` re-exported from `lib.rs`.
  Three pump-level integration tests. 924 lib tests.

### Key seams reused / established

- **`Deferred` broker shape** (D3/D9): `ColorPickerDrag` is the fourth broker
  after the scroller/indicator/`MakeButtonDefault` shape — ViewId + primitive
  position in the deferred enum; the pump downcasts; widget state stays in the
  widget. No widget-layer types in the FOUNDATION `Deferred` enum.
- **`ModalCompletion` result extraction**: `ColorPick { picker, sink }` is the
  same shape as `HistoryPick { link }` — downcast the in-tree modal while it
  still exists, read a concrete accessor, write the result to a caller-owned sink.
- **`window.rs DragCapture` pattern**: `ColorDragCapture` is a third drag capture
  (window frame + window move were the first two). The pattern is proven and
  directly reusable.

## Session — row 82 `TColorSelector` (the 16-color grid view)

Landed **row 82 `TColorSelector`** (`ColorSelector`) — the BIOS-color picker grid,
the second view in the color-selection cluster. Appended to `src/dialog/colordlg.rs`;
exported (`ColorSel`, `ColorSelector`) from `dialog/mod.rs`. One fresh-implementer
(Sonnet) → one spec+quality review → one fix pass → integrate → commit. 892 →
**924 lib tests** (+32: nav arithmetic for both `selType`s, mouse pick, broadcast
emission with `source`; +3 snapshots).

### First raw-BIOS-color widget (a new draw pattern)
Unlike every prior widget (which draws through theme `Role`s), the color selector
draws the 16 BIOS colors **literally** — its whole job is showing the palette. It
builds `Style`s directly: cell color `c` → `Style::new(Color::Bios(c&0xF),
Color::Bios((c>>4)&0xF))`; the row fill + the `c==0` marker use attr `0x70`
(`Bios(0)` on `Bios(7)`, so the marker is visible on the black-on-black cell).
Glyphs: `icon='\u{2588}'` (█, CP437 0xDB), marker `'\u{25D8}'` (◘, CP437 0x08) at
the middle cell of the selected color. The C++ `for i in 0..=size.y` inclusive
loop + `TDrawBuffer` clipping is ported as `for i in 0..4` + `DrawCtx` clip — the
faithful equivalent (a `Background` selector of height 2 shows colors 0–7; a
`Foreground` of height 4 shows 0–15), verified by 3 snapshots.

### The cluster's color-changed seam (emit now, broker at the consumer row)
`colorChanged` is a **payload-less broadcast** (D4): it emits
`COLOR_FOREGROUND_CHANGED`/`COLOR_BACKGROUND_CHANGED` with `source = Some(self_id)`
and NO color payload. Future consumers (rows 83/84) resolve the color via a new
`color()` accessor + `as_any_mut`→`Some(self)` (the `FileList::focused_rec`
reachability precedent) — NOT D10 `value`/`set_value` (`TColorSelector` has no
`getData`/`setData`). The inbound `cmColorSet` (which carries the row-83
`TColorDisplay`'s attr) is **breadcrumbed inert** `TODO(row 83)` — its resolvable
source doesn't exist yet. New `Command`s: `COLOR_FOREGROUND_CHANGED` (71),
`COLOR_BACKGROUND_CHANGED` (72), `COLOR_SET` (73). Mouse is single-shot (the C++
`do{}while(mouseEvent…evMouseMove)` drag loop deferred `TODO(row 31)`, like
`button.rs`); coords are already view-local (D3). Nav wrap arithmetic ported
verbatim (incl. the underflow-safe `>width-1` / `==0` / `==max_col` guards).

## Session — row 81 color-selection data classes (`TColorItem`/`TColorGroup`/`TColorIndex`)

Landed **row 81** — the three pure data classes that open the color-selection
cluster (81–87, `colordlg`). New file `src/dialog/colordlg.rs`; `ColorItem`,
`ColorGroup`, `ColorIndex` exported from `dialog/mod.rs`. One fresh-implementer
(Sonnet, MECHANICAL) → one focused spec+quality review → two fix passes →
integrate → commit. 882 → **892 lib tests** (+10 unit tests; +1 doctest). Pure
data, no rendering → unit tests only (no snapshot), per the HANDOVER rule.

### The collections→`Vec` shape (the cluster's data foundation)
The C++ types are `next`-pointer singly-linked lists chained with `operator+`.
Per the rstv collections→`Vec` deviation that machinery is dropped:
- `ColorItem { name: String, index: u8 }` — `index` is an **immutable palette
  index** (ctor param). `char*`→`String` (the `newStr` heap copy).
- `ColorGroup { name: String, index: u8, items: Vec<ColorItem> }` — the
  `TColorItem* items` list → owned `Vec`. **Naming trap captured in the docs:**
  `ColorGroup::index` is *not* a palette index — it is **mutable focus state**
  (the focused-item *position* within the group), left uninitialized by the C++
  ctor and written later by `setGroupIndex` (row 85). So it is **not** a ctor
  parameter; it defaults to `0` and is set via `set_index`. A `with_item(name,
  idx)` fluent builder is the sanctioned replacement for the C++ `group + item`
  chaining (just a `Vec` push — no `std::ops::Add`).
- `ColorIndex { group_index: u8, color_index: Vec<u8> }` — the C++
  `TColorIndex` is a variable-length struct (`new uchar[numGroups+2]`; the
  header's `colorIndex[256]` is a sentinel, never the real size). `colorSize`
  becomes `color_index.len()` (`color_size()` derives it); no separate field.

The groups list is a bare `Vec<ColorGroup>` — **no `ColorGroupList` newtype**;
`Vec` indexing replaces every linked-list walk the later rows (85/87) would do
(`getGroup`/`getNumGroups`/`setGroupIndex` all become O(1)/`.len()`).

### Scope discipline
Structs + ctors + read/focus accessors only. Row-85/87 logic
(`setGroupIndex`/`getNumGroups`/`focusItem`/`getText`/`setIndexes`/`getIndexes`)
deliberately **not** pulled forward. Review also trimmed three speculative
write-paths (`push_item`/`items_mut`/`color_index_mut`) that had no consumer yet
— YAGNI; rows 85/87 add exactly what they need when they land. D12
(`TStreamable` read/write/build for the groups tree) dropped.

## Session — row 80 `TChDirDialog` (the last filedlg row)

Landed **row 80 `TChDirDialog`** (`ChDirDialog`) — the change-directory dialog —
completing the file-dialog family. One fresh-implementer (Opus) → two-stage
review (spec ✅ then quality ✅) → fix → integrate → commit cycle. 868 → **882 lib
tests** (+14, incl. a pump-level integration test exercising the real
`MakeButtonDefault` broker arm end-to-end). The orchestrator owned the shared-file
foundation edits directly
(per CLAUDE.md), the implementer owned `filedlg.rs`.

### The new FOUNDATION seam: the makeDefault broker (D3)
C++ `TDirListBox::setState` does `((TChDirDialog*)owner)->chDirButton->makeDefault(enable)`
on every `sfFocused` change — focus the dir tree and the **Chdir** button becomes
the default (Enter triggers it). The dir list is a leaf holding only `&mut Context`
(D3), so it cannot reach its sibling button inline. New broker, the same shape as
the row-77 `ResolveFocusedFile`:
- **`Deferred::MakeButtonDefault { button, enable }`** (`view/context.rs`) +
  `Context::make_button_default(button, enable)` convenience (mirrors
  `request_focus`). The pump arm (`app/program.rs`) resolves `button`, downcasts to
  `Button`, calls `make_default(enable, ctx)` — whose `cmGrabDefault`/`cmReleaseDefault`
  re-broadcast settles next pump (like `EditorPaste`).
- **`Button::make_default` is now `pub(crate)`** and **`Button::as_any_mut` returns
  `Some(self)`** (so the broker can downcast a sibling button). Both behavior-
  preserving additions to the leaf.

### The two row-75 `DirListBox` breadcrumbs, resolved (row 80 is their only consumer)
1. **`select_item`** (the dir-tree double-click/Enter) → `ctx.post(Command::CHANGE_DIR)`.
   Faithful to C++ `message(owner, evCommand, cmChangeDir, …)`: a posted **command**,
   not a broadcast — it unifies with the **Chdir** button press (also a `cmChangeDir`
   command) into ONE `Event::Command(CHANGE_DIR)` handler arm. The dialog reads the
   **focused** entry itself (`DirListBox::focused_entry()`, like `FileList::focused_rec()`),
   ignoring any payload — exactly as the C++ dialog reads `dirList->focused`.
2. **`set_state`** → on the `sfFocused` flag change, `ctx.make_button_default(chdir_button, enable)`
   via the new broker. `DirListBox` gained a `chdir_button: Option<ViewId>` field (wired
   after assembly by `set_chdir_button`); `None` outside a `TChDirDialog`, so it is a
   no-op for any other owner.

### `ChDirDialog` (D2 embed-and-delegate, the `FileDialog` precedent)
Embeds a `Dialog`, `#[delegate(to = dialog, skip(…))]`, overrides only
`handle_event`/`size_limits`/`reset_current`/`as_any_mut`/`valid`. Assembly verbatim
from the C++ ctor (`TRect(16,2,64,20)`, `ofCentered`, dirInput + label + history +
scrollbar + dirList + label + OK/Chdir/Revert/[Help] buttons in exact insertion
order with grow modes). Key faithfulness points:
- **`valid` does NOT chain to the base** `TDialog::valid` (unlike `FileDialog`) —
  the C++ `TChDirDialog::valid` goes straight to the `cmOK` check: `fexpand` →
  `trimEndSeparator` → **real** `chdir` (`std::env::set_current_dir`); on error an
  informational "Invalid directory: '…'." box + keep-open.
- **`handle_event`**: base first, then `cmRevert` (re-read **live** cwd) / `cmChangeDir`
  (focused entry's path); the shared tail passes the **untrimmed** (trailing-`/`) path
  to `new_directory` and the **trimmed** path to `dirInput`, then focuses the dir list
  (`dirList->select()`) and clears.
- **`reset_current`** = `setUpDialog` (one-time `needs_setup` guard, gated by
  `(opts & cdNoLoadDir)==0`) + `selectNext(False)` (the base `reset_current` focuses
  dirInput first).
- **D10**: `dataSize()==0` → `value`/`set_value` skip-listed to the trait default
  (`None`/no-op), NOT the inner Dialog's gather.
- **D14**: native Linux `/` paths throughout — the `drivesText`/`driveValid`/`\\`
  branches dropped; `trimEndSeparator`'s DOS `len>3` guard becomes `len>1` (protect
  root `/`); `new_directory` trailing-`/`-normalizes its input at the top (protects the
  new cwd-derived callers — the HANDOVER footgun).

### Test isolation (process-global cwd)
`valid`/`reset_current` touch the process cwd, and tests run multi-threaded in one
binary. So: the `valid` test exercises ONLY the failure path (nonexistent absolute
dir → `set_current_dir` errs, cwd untouched, one box queued); the snapshot test seeds
a deterministic `build_tree` listing and sets the input by hand, never calling the
cwd-reading `reset_current` — mirroring how row 79's `FileDialog` tests stayed
deterministic.

## Session — the filedlg consumer cluster: rows 77 `TFileInputLine`, 78 `TFileInfoPane`, 79 `TFileDialog`

Landed the **interlocked file-dialog cluster** (77 + 78 + 79) on top of row 76,
each as a fresh-implementer → two-stage review (spec ✅ then quality ✅) → fix →
integrate → commit cycle. 822 → 867 lib tests (+45). Five commits:
`4f325ca` (77), `15e2ca0` (78), `5270a78` (79 B1), `2342e3e` (79 B2), plus this
docs commit.

**Why a cluster, not three independent leaves.** Both 77 (`TFileInputLine`) and
78 (`TFileInfoPane`) react to a `cmFileFocused` broadcast **carrying a
`TSearchRec`**, and 79 (`TFileDialog`) is its producer/assembler. rstv's
`Event::Broadcast { command, source }` is **payload-less** (D4 — `source` is the
resolvable subject, not a value carrier), so the cluster shares one new
FOUNDATION piece: the **payload-carrying-broadcast seam**, designed once at its
first consumer.

### The payload-broadcast seam (FOUNDATION, row 77)
The faithful translation of C++ `message(owner, evBroadcast, cmFileFocused,
list()->at(item))` under D4/D3:
- **Producer.** C++ `TFileList::focusItem` is **virtual** and fires on *every*
  focus change (keyboard, mouse, scrollbar-at-apply-time, readDirectory). rstv's
  `focus_item` is a shared free fn, not virtual — so a defaulted no-op
  `ListViewer::on_focus_changed(&mut ctx)` hook is called at the **tail of
  `focus_item`** (the sole funnel all focus changes pass through), and `FileList`
  overrides it to `ctx.broadcast(FILE_FOCUSED, Some(self_id))`. Behaviour-preserving
  for every other `ListViewer` impl. (The advisor initially suggested a
  `handle_event` before/after diff; we rejected it — the **scrollbar** path
  changes focus at *deferred-apply* time, which a synchronous `handle_event` diff
  structurally cannot see. `focus_item` is the only correct single point.)
- **Broker.** `Deferred::ResolveFocusedFile { subscriber, source }` — the pump
  reads `FileList::focused_rec()` from `source` (own `find_mut`, borrow dropped),
  then concrete-downcasts `subscriber` to `FileInputLine` (row 77) or
  `FileInfoPane` (row 78, an `else if` arm) and calls `on_file_focused(rec)`. The
  consumer holds only `&mut Context` (D3) so it can't read its sibling — it
  filters the broadcast in `handle_event` and requests the broker by its own id +
  the broadcast `source`. Same shape as the `cmScrollBarChanged`→`SyncScrollerDelta`
  broker. **`as_any_mut`→`self`** on both consumers (the opposite of `Memo`, which
  forwards to its inner) so the downcast targets the consumer, not its embed.
- `cmFileDoubleClicked` is faithfully **payload-less** (the only consumer,
  `TFileDialog::handleEvent`, turns it into cmOK and never reads the record).

### Row 78 — the draw-time owner-state problem (D-time + Role::InfoPane)
`TFileInfoPane::draw` needs `directory`/`wildCard`/`file_block` but `draw()` has
**no `Context`** — so caching on the consumer is *forced*, not a choice:
`file_block` flows via the broker; `directory`/`wild_card` are owner-state the
dialog pushes. The **date** introduced a **D-time deviation**: C++ read DOS
local time from `findfirst`; rstv packs `std::fs` mtime into the same DOS `ftime`
u32 (so the bitfield unpack ports verbatim), computed in **UTC** via Hinnant's
days-from-civil (no tz crate); pre-1980 clamps to the DOS epoch; ≥2044 sets the
`i32` sign bit and round-trips through `as u32`. `Role::InfoPane` traces the
classic palette chain to BIOS `0x13` (cyan on blue).

### Row 79 — the assembly (B1 skeleton + B2 valid/path-logic)
- **The ctor-has-no-ctx problem.** C++ calls `readDirectory()` at the end of the
  ctor; rstv's `FileList::read_directory` needs `ctx` (it broadcasts). The
  ctx-bearing hook the modal loop runs **once, right after insert, before the
  first draw** is `View::reset_current` — so the initial `readDirectory` maps
  there (guarded by a one-time flag). The owner-state push to children is **not**
  a cross-view broker (that's for leaf views): the dialog **owns** the group, so
  it mutates its children directly via the new `pub(crate) Dialog::child_mut`
  (`set_dir_info` on the info pane, `read_directory` on the file list).
- **`valid()` is the gate, not `handle_event`.** `handle_event` just
  `end_modal(cmFileOpen)`; the pump's `validate_modal_close` calls
  `valid(endState, ctx)` before accepting. The faithful 4-branch `valid`
  **navigates** (isWild/isDir → re-read, return *false* = keep open) or
  **accepts** (validFileName → true); the two error boxes (`invalidDrive`,
  `invalidFile`) are `mfError|mfOKButton` → **Informational** consumers of the
  async-modal-from-view seam (request + return false; `validate_modal_close`
  drives the box inline — no pump change).
- **D14 lexical path helpers** (`expand_path`/`is_wild`/`is_dir_only`/
  `split_dir_file`/`path_valid`/`valid_file_name`) over `std::path` — no
  `canonicalize` (faithful fexpand is purely lexical). The bug worth recording:
  `Path::file_name()` returns `Some` for a trailing-slash path, so it can't
  detect a bare directory — hence the explicit `is_dir_only`.

**Still breadcrumbed (row 79):** the "21st-century percentages" screen-resize
block, `wfGrow`, and the `FileEditor::saveAs` consumer (now **unblocked** by
`FileDialog::value()`).

## Session — row 76 `TFileList`

Ported **row 76 `TFileList`** as `FileList` in `src/dialog/filedlg.rs` (on top of
the sorted-search seam below). 810→822 lib tests (+12). Two-stage subagent review
(spec ✅ then quality ✅) before integration; two cosmetic NITs fixed.

**Shape (the row-70/75 item-source seam, now fully due).** `TFileList` is a C++
subclass of `TSortedListBox` but stores a `TFileCollection` (`Vec<SearchRec>`) with
an overridden `getText` — so, exactly like `DirListBox` (row 75), it **cannot** be a
D2 embed-delegate (a delegated `View::draw` would consult the inner list's
`Vec<String>`). It is a **direct `ListViewer` impl** over `Vec<SearchRec>`. What made
this row *not* a routine leaf: it also needs `TSortedListBox`'s incremental
type-to-search — which is why the FOUNDATION seam (next section) was extracted first,
so `FileList` gets the search machine for free by implementing `SortedSearch` and
overriding only `search`.

**`search` — the load-bearing override (fuses C++ `getKey` + `list()->search`).** The
key is a `SearchRec` whose `attr = FA_DIREC` iff `(shift_state & KB_SHIFT) != 0` OR
the typed prefix starts with `.`, else 0; the name is the typed prefix **verbatim —
no `strupr`** (the C++ `strupr` is under `#ifndef __FLAT__`, i.e. skipped on the
Linux/flat build → case-sensitive, matching `search_rec_compare`'s `strcmp`). It
binary-searches `self.items` via **`search_rec_compare` over the raw recs, NOT over
`get_text`** — because the `attr=FA_DIREC` key must route the search into the
*directory* section of the collection, an ordering that exists only in
`search_rec_compare` (and `get_text` carries a `/` suffix that would mis-order). A
discriminating test (`search_attr_routes_into_file_vs_dir_section`) pins this: with
`shift_state = KB_SHIFT` the same prefix lands in the dir section, not the file
section. The shared confirm step stays `ci_prefix_eq` (= C++ `equal`/`strnicmp`,
case-insensitive — faithful even against the case-sensitive collection).

**`read_directory` (D14, native `/`).** Pure `build_listing(dir, wildcard, raw)`
split from the `std::fs` read for testability (row-75 precedent): files kept iff a
minimal `*`/`?` `wildcard_match` matches; **directories always kept** (the wildcard
does NOT apply to dirs — C++ pass 2 resets the pattern to `*.*`) iff `name[0] != '.'`
(drops `.`/`..`/hidden); `..` appended iff `dir != "/"`; sorted via
`FileCollection::insert`. The fs read uses `std::fs::metadata` (follows symlinks like
magiblot's `findfirst`/`stat`; broken symlink → skip), `size` saturated to i32,
`time = 0` (DOS date packing → row 78). `num_cols = 2`; `get_text` appends `/` (not
`\`) for dirs; `value() = None` (C++ getData/setData/dataSize are no-op/0).

**Deferred (breadcrumbed).** All three owner broadcasts are payload-carrying, which
rstv's payload-less `Event::Broadcast` can't express → **row 79 `TFileDialog`**:
`focusItem` → `cmFileFocused` on *every* focus change (a focus-change *observation*
seam, broader than row 75's commit-only `select_item`; row 79 can build it on the
`old_value != focused` diff already computed in `sorted_handle_event`), the post-
`newList` item-0/noFile `cmFileFocused`, and `selectItem` → `cmFileDoubleClicked`
(`select_item` is a true no-op that does NOT call the base, so no stray
`cmListItemSelected`). The `tooManyFiles` OOM box, `DirSearchRec::operator new`
safety pool, and `fexpand`/`squeeze` DOS path canonicalization are dropped.

## Session — sorted-search seam extraction (FOUNDATION sub-step for row 76)

The HANDOVER flagged **row 76 `TFileList`** as *not* a routine leaf: like
`DirListBox` (row 75) it stores its own collection (`Vec<SearchRec>`) with an
overridden `getText`, so it **cannot** be a D2 delegate over `SortedListBox` — it
must be a *direct* `ListViewer` impl. But unlike `DirListBox` it *also* needs
`TSortedListBox`'s incremental type-to-search, which until now lived **inside**
`SortedListBox` operating on its embedded `ListBox`'s `Vec<String>`. A direct impl
would have to duplicate that machine. This commit performs the deferred
"list-viewer item-source seam refactor" so row 76 gets search **for free**.

**The extraction.** `SortedListBox::handle_event`/`cursor_request` (the verbatim
`TSortedListBox::handleEvent` state machine) moved into two free functions in
`src/widgets/list_viewer.rs` — `sorted_handle_event` / `sorted_cursor` — generic
over a new sub-trait `SortedSearch: ListViewer` (mirroring how `draw`/`handle_event`
are free functions over `ListViewer`). `SortedSearch` exposes the polymorphic parts:
`search_pos`/`set_search_pos`, `shift_state`/`set_shift_state`, and a single
`search(&self, cur: &[char]) -> i32` that **fuses C++ `getKey` + `list()->search`**
— the base SortedListBox makes it identity-getKey + a case-insensitive binary
search; `FileList` (row 76) will override it to build a key `SearchRec` (attr from
shift/dot) and binary-search via `search_rec_compare`. The base call inside the
machine is now `list_viewer::handle_event(this, …)` (faithful: `TListBox` does not
override `handleEvent`, so it == `TListViewer::handleEvent`). Every documented trap
is preserved (cur re-seeded from the focused item each keystroke; dot-branch
doesn't truncate; `ci_prefix_eq`=`strnicmp` confirm over `prefix_len = searchPos+1`;
consume-iff `searchPos != oldPos || isalpha`).

**The one intentional delta (safe).** At the `searchPos -1↔0` transition the base
previously stored `shift_state = 0` (a breadcrumb — C++ captured `controlKeyState`
there but the base never read it). It now captures the real bit
(`if ke.modifiers.shift { KB_SHIFT } else { 0 }`, `KB_SHIFT = kbLeftShift|kbRightShift
= 0x03`) so the future `FileList::search` can read it. Unobserved by every existing
test (the base never reads `shift_state`).

**SortedListBox → direct `ListViewer`.** With the search extracted, the embedded
`ListBox` was pure indirection, so `SortedListBox` became a direct `ListViewer`
impl over its own `Vec<String>` (parallel to `ListBox`/`DirListBox`) + a
`SortedSearch` impl. The `#[delegate(to = inner)]` is gone; `handle_event`/
`cursor_request` delegate to the new free functions. **Behavior-preserving:** all
existing `SortedListBox` tests keep their assertions verbatim (only `slb.inner.X` →
`slb.X` access-path rewrites). Verified zero production consumers of `SortedListBox`
before converting. 810 lib tests green, clippy + fmt clean.

## Session — row 75 `TDirListBox` + deviation D14 (native Linux `/` paths)

Ported **row 75 `TDirListBox`** as `DirListBox` in `src/dialog/filedlg.rs`.
802→810 lib tests (+8). The HANDOVER flagged this row as *not* mechanical despite
its tag — three real problems, resolved as follows.

**The design decision — deviation D14 (native Linux `/` paths).** The C++
file-dialog cluster (rows 75–80) is DOS-flavored: `\` separators, A:–Z: drive
letters, a "Drives" tree entry, `getdisk`/`driveValid`. magiblot keeps `\` in the
TV layer and translates at the syscall boundary (`path_dos2unix`), emulating a
single disk on UNIX. rstv is a **native-Linux** port → **D14** (new, documented in
PORTING-GUIDE Baseline→Deviation→Integration): paths are `/`-separated, the tree
root is `/`, `showDrives`/drive-letters/the "Drives" entry are **dropped**, subdirs
come from `std::fs::read_dir`. D14 is inherited by all of rows 75–80 — one
`/`-native path model, no `\`↔`/` translation seam anywhere. *(User-confirmed the
native-`/` direction over a faithful-DOS port, which would be unusable on Linux.)*

**Problem 1 — owner-coupling to the unported `TChDirDialog` (row 80).** C++
`selectItem` does `message(owner, cmChangeDir, list()->at(item))` — a command
**carrying a `DirEntry` payload**, which rstv's payload-less `Event::Broadcast {
command, source }` cannot carry; `setState` downcasts the owner to poke
`chDirButton->makeDefault`. Both **breadcrumbed as no-ops with a row-80 TODO** (the
typed-payload-command seam is designed at its first consumer — row 80 — not
speculatively now). `set_state` still performs the base `list_viewer::set_state`.

**Problem 2 — the `get_text`-over-`Vec<DirEntry>` seam.** `TDirListBox` subclasses
`TListBox` but holds a `Vec<DirEntry>` (not `Vec<String>`) and overrides `getText`.
In rstv this **cannot** be a D2 embed-delegate: a delegated `View::draw` would run
with the inner `ListBox` as `self` and call *its* `get_text` over `Vec<String>`,
never consulting the `Vec<DirEntry>`. So `DirListBox` is a **second, parallel
direct `ListViewer` impl** over its own storage (exactly as `ListBox` is over
`Vec<String>`) — `lv`/`lv_mut` + overridden `get_text`/`is_selected`/`select_item`,
delegating all `View` methods to the `list_viewer` free fns. The row-70 "fixed to
`Vec<String>`" breadcrumb was about *delegation*; a direct impl sidesteps it.

**The tree builder.** The DOS `showDirs` (pointer-arithmetic over a space-filled
buffer) is ported to a **pure `build_tree(dir, subdirs) -> (Vec<DirEntry>, cur)`**
split from the `read_dir` FS read (the editor "ctx-threading split" pattern → the
tree is snapshot-testable without the filesystem). `dir` arrives with a trailing
`/`; entries = root `└─┬/` (indent 0) + each `/`-segment ancestor (indent 2,4,…,
`directory` = absolute path, no trailing slash) + the immediate subdirs (`└┬─`
first, ` ├─` rest, indent = cur-depth+2). The **last-entry glyph fix-up** is
faithful to the C++ byte surgery on `dirs->at(getCount()-1)` and runs
**unconditionally**: `└…`→the two chars after `└` become `──`, else `├`→`└`. The
non-obvious case (a first pass got wrong): with **no subdirs** the last entry is
the deepest ancestor, so `└─┬name`→`└──name` (a leaf corner). Subdir enumeration
**follows symlinks** (`std::fs::metadata`, not lstat-based `file_type()`) to match
magiblot's `findfirst`/`cvtAttr` `stat()`. Two snapshot tests (focused==cur and
focused≠cur) lock the rendered tree and prove `is_selected` (the *current dir*,
not the cursor) is wired through draw.

**Process.** Subagent-driven: Sonnet implementer → fresh Opus spec-compliance +
code-quality reviewers (parallel) → fix round → integrate. Spec review found one
faithfulness bug (the unconditional fix-up — the brief's error) and confirmed one
subtle divergence (symlink-following), both fixed; quality review trimmed
`build_tree` to private, doc-breadcrumbed the write-only `dir` field, and applied
minor doc/format tidies.

## Session — the async-modal-from-a-view seam (FOUNDATION detour, before row 75)

A deliberate PORT-ORDER detour (decided in HANDOVER): build the
**async-modal-from-a-view** seam once to retire **three** inert consumers at
once. 795→802 lib tests (+7). Design note:
[`docs/design/async-modal-from-view.md`](file:///home/oetiker/checkouts/rstv/docs/design/async-modal-from-view.md).

**The problem.** C++ `valid()` *blocks* and does I/O: `TInputLine::valid` →
`validator->valid()` → `error()` pops a `messageBox`; `TFileEditor::valid` pops a
Yes/No/Cancel box and **uses its answer** to decide the bool. rstv's single loop
(D9) forbids a downward-borrowed `&mut View` from running a nested modal inline.

**The non-obvious trap (the whole reason it's FOUNDATION).** `valid()` is called
from sites that are **not symmetric**:
- **handle_event paths** — focus-leave `valid(cmReleasedFocus)` (`group.rs`
  `focus_child`) and window-close `valid(cmClose)` (`window.rs`
  `Window::handle_event`): event in flight → the **deferred queue drains normally**
  via the pump.
- **modal-close path** — `valid(endState)` at `program.rs` `exec_view`'s loop:
  **between** pump iterations, where the deferred drain is **event-gated
  (`!ev.is_nothing()`)** and would NEVER fire (esp. headlessly). This site must
  **drive the modal INLINE**, holding `&mut self` (mirrors the `pending_modal`
  explicit drive) — the new `validate_modal_close` re-validate loop.

**The seam.**
- `View::valid` signature → `(&mut self, cmd, &mut Context)` (blessed in HANDOVER);
  ripples to every impl + the `specs.rs` `valid` forwarder + `Group::valid`
  (manual `iter_mut`, **first-invalid short-circuit kept**) + all call sites.
- `Deferred::OpenMessageBox { text, kind, buttons, answer_to, then_command }` +
  `Context::request_message_box` (the **ADD-A-VARIANT** pattern, like
  `OpenHistory`).
- `View::set_modal_answer` trait hook (default no-op, `specs.rs` forwarder) —
  `FileEditor` overrides it to stash into `pending_save_answer`.
- `ModalCompletion::{RouteModalAnswer, Informational}`; `apply_modal_completion`
  now returns `Option<Event>` re-injected into `out_events` (the re-post channel —
  `pump_once` pops `out_events` before polling). `pending_modal` extended to carry
  the box's first-button **initial-focus** id (so the close prompt defaults to
  **Yes**, not Cancel — matching C++ `selectNext(False)`).

**Consumers retired (the three breadcrumb clusters):**
1. **All 5 validator `error()` boxes** (`Validator::error(&self, ctx)` — NOT a
   `View` method, so **no `specs.rs` forwarder**) — exact C++ strings, `mfError|
   mfOKButton` → Error kind + OK; fires on cmReleasedFocus + cmOK.
2. **`FileEditor::valid` modified-save prompt** (`edSaveModify`/`edSaveUntitled`,
   Yes→`save`, No→clear-modified+true, Cancel→false) — via the window-close
   handle_event path (queue → `pending_modal` → `RouteModalAnswer` → re-post
   `cmClose` re-validates with the cached answer).
3. **`FileEditor` save-error boxes** (`save`/`save_file` thread ctx; write/create
   failure → `Error writing file …`; create-vs-write merged — `std::fs::write` is
   atomic). `edReadError` on **load** stays deferred (the ctor has no ctx).

**Reviewed** two-stage (spec PASS, quality PASS). **Two documented interims:**
(a) the `valid_end`/**cmQuit** path *vetoes* close of a modified editor without a
prompt (the whole-tree quit-drive is deferred — latent: no runnable app wires a
FileEditor yet); (b) `saveAs`/`edReadError`/`TFileDialog` paths still breadcrumbed.

**Verification.** 802 lib tests green (+7: FileEditor close Yes/No/Cancel, the
first-button-focus regression guard, validator-error inline + deferred, the
quit-veto no-leak guard); `clippy --all-targets -D warnings` (forced) + `fmt
--check` clean.

## Session — file-dialog data classes (rows 71–74) — batched, collections→Vec

Rows 71–74 (`TDirEntry`/`TDirCollection`/`TSearchRec`/`TFileCollection`,
`stddlg.h`/`tfilecol.cpp`). 781→795 lib tests (+14). **Rows 71–74 ✅.** The four
`TFileDialog` data-support classes — pure data (no draw/events), so **batched into
one cycle** (`src/dialog/filedlg.rs`) with a single combined review.

- **They collapse hard under the "collections → `Vec`" deviation** (rstv has no
  `TCollection`). The batch is really *two structs + one comparator + one sorted
  insert*:
  - `DirEntry { display_text, directory }` (71) + `text()`/`dir()` accessors.
  - `SearchRec { attr:u8, time:i32, size:i32, name:String }` (73) — the DOS
    metadata record; `attr`/`time`/`size` are populated by the (deferred)
    filesystem-reading layer in `TFileList`/`TFileDialog` (breadcrumbed).
  - `DirCollection = Vec<DirEntry>` (72) — a bare **type alias**; the C++
    type-safe `TCollection` wrapper API is dropped (row 75 needs only
    push/index/len).
  - `FileCollection` (74) — a `Vec<SearchRec>` newtype holding the one piece of
    real logic: a **verbatim** `search_rec_compare` (`".."` last, directories
    after files, else case-SENSITIVE `strcmp`/byte order) + a sorted `insert`
    (`partition_point` by the comparator). The unused `TSortedCollection` API
    (indexOf/remove/atPut/firstThat/…) is dropped — no consumer.
- **The comparator is the only non-obvious bit** (the `".."` and dir-vs-file
  tiebreaks). Tests assert the **sign of each branch in isolation** (not a
  reconstructed display order), plus the sorted-insert invariant and
  case-sensitivity (`'Z'` < `'a'`). A doctest demonstrates `".."` sorting last.
- **Row 75 (`TDirListBox`) deliberately NOT batched** — it is a design cycle, not
  mechanical: owner-coupled to the unported `TChDirDialog` (`setState` downcasts
  the owner to poke its `chDirButton`; `selectItem` messages the owner `cmChangeDir`
  **carrying a `DirEntry` payload** — which rstv's payload-less `Broadcast` can't
  carry directly), DOS-drive-specific (`showDrives` walks A:–Z:; Linux has no drive
  letters → the root behavior must be *designed*), and it holds `Vec<DirEntry>` (not
  `Vec<String>`) overriding `get_text` — exactly the row-70 "make `get_key`/`get_text`
  overridable" breadcrumb coming due.
- **Verification.** 795 lib tests green (+ the doctest); `clippy --all-targets -D
  warnings` (forced) + `fmt --check` clean. Unit tests only (pure data, nothing draws).

## Session — `TSortedListBox` (row 70) — type-to-search list, no collection

Row 70 (`TSortedListBox`, `stddlg.cpp`). 773→781 lib tests (+8). **Row 70 ✅.**
Begins the standard/file-dialog family (70–75). A `ListBox` with incremental
type-to-search; the design interest is two C++ subtleties + the collection cut.

- **`SortedListBox`** is a D2 embed-delegate over `ListBox`
  (`#[delegate(to = inner)]`, overriding only `handle_event` + `cursor_request`).
- **No generic `TSortedCollection`.** rstv already replaced `TCollection` with a
  `Vec<String>` inside `ListBox`; so `SortedListBox::new_list` keeps that Vec
  **case-insensitively sorted** and the search is a binary search (`partition_point`
  by `ci_cmp`) over `0..range` via `get_text(i)`. Rows 72/74 (`TDirCollection`/
  `TFileCollection`) will each hold their own typed sorted Vec with their own
  comparator — there likely never is one generic collection. Breadcrumbed.
- **Trap 1 — `curString` is the FOCUSED item's text, re-seeded every keystroke;
  `search_pos` indexes into it** (NOT an accumulated typed buffer). The
  state machine re-reads `get_text(focused)` each event, then the Backspace/'.'/char
  branch mutates it. This is load-bearing for the `'.'` branch: pressing '.' does
  `strchr` over the focused item's *full* text to jump to its extension separator.
  Ported as a `Vec<char>` (UTF-8-safe; C++'s 256-byte `curString` cap dropped).
- **Trap 2 — the sequence:** save `old_value = focused` → delegate to
  `inner.handle_event` FIRST → reset `search_pos = -1` on focus-change OR a
  `cmReleasedFocus` broadcast → THEN gate on `ev` still being `KeyDown`. The base
  `ListBox` clears the keys it handles (Space/arrows/Page/Home/End), so only
  passed-through keys (letters/'.'/Backspace) drive search, and arrow-nav cancels an
  in-progress search by moving `focused`.
- **Comparator coherence (deviation).** Sort, binary search, and the prefix-confirm
  (`ci_prefix_eq`) are all case-insensitive so the search lands where the confirm
  accepts. C++ leaves ordering to the injected collection's `compare`; this is a
  deliberate rstv choice, documented.
- **Spec-review blocker, caught + fixed (TDD):** the `'.'` branch must pass the
  **full** focused-item text to `search` (only the *confirm* uses `search_pos+1`).
  The first cut truncated the key for all branches, so on items sharing a basename
  (`file.bak`/`file.txt`) pressing '.' wrongly jumped to the sibling. Fix: pass the
  whole `cur` to `get_key` (the char/back branches already truncate `cur` in place,
  mirroring C++'s in-place `curString[searchPos+1]=EOS`; the dot branch leaves it
  full). The dot test was strengthened with a same-basename sibling and shown to
  fail-then-pass.
- **Deferrals (breadcrumbed):** `get_key` identity (C++ virtual; file/dir subclasses
  override at row 75), `shift_state` stored-but-unused (C++ captures
  `controlKeyState`), the `curString` cap, `TStreamable` (D12).
- **Verification.** 781 lib tests green; `clippy --all-targets -D warnings` (forced)
  + `fmt --check` clean. No snapshot (draws identically to `ListBox` — `draw`
  delegates).

## Session — `TEditWindow` (row 69) — the editor window (assembly row)

Row 69 (`TEditWindow`, `teditwnd.cpp`). 768→773 lib tests (+5). **Row 69 ✅.** A
pure **integration** row — it assembles already-ported pieces (`Window`,
`ScrollBar`, `Indicator`, `FileEditor`) into a window; no shared-type surgery. The
only subtleties are rstv-model mechanics, not C++ behavior.

- **`EditWindow`** is a D2 embed-delegate over `Window` (like `Dialog`), holding the
  window + the four child ids (`editor_id` + the three aux ids, mirroring C++
  `TEditWindow`'s public `hScrollBar`/`vScrollBar`/`indicator` members).
- **ViewId-at-insertion drove the wiring order.** rstv assigns a view's `ViewId` at
  **insertion**, not construction (`Group::insert` stamps it). The C++ ctor wires the
  editor to the scrollbars/indicator by pointer; here we must **insert the three
  (hidden) aux views first to obtain their ids**, then construct
  `FileEditor::new(r, Some(h), Some(v), Some(ind), file)` with those ids and insert
  it. Child bounds, `ofTileable`, and the inner extent (`get_extent().grow(-1,-1)`)
  are verbatim from C++.
- **Hidden aux children are load-bearing twice.** `state.hide()` on the two
  scrollbars + indicator before insert is (a) faithful to C++ `hide()`, and (b) the
  reason the **editor** becomes the window's current view: `reset_current` selects
  the first *visible*-and-selectable child, so the hidden bars are skipped and the
  visible+selectable `FileEditor` wins — and `Editor::set_state(Active)` (row 66)
  then shows the bars/indicator when the window activates. A regression assertion
  pins all three aux children hidden (a visible-but-empty bar would pass the
  snapshot yet silently steal currency).
- **Manual `ScrollBar::new` (not `Window::standard_scroll_bar`).** `ScrollBar::new`
  already sets the exact C++ `TScrollBar` growMode and `selectable`; C++ `TEditWindow`
  builds bare bars (no `ofPostProcess`), so manual construct + `state.hide()` +
  `insert_child` is the faithful path (`standard_scroll_bar` would add keyboard
  postprocess the edit-window bars don't have).
- **`size_limits` min {24,6} + the `calc_bounds` skip.** Overriding `size_limits`
  is not enough: `calc_bounds` must also be in the delegate `skip(...)` (exactly as
  `Window` does for its own 16×6) so an owner-driven resize routes through the
  virtual `size_limits` minimum instead of the group's 0×0 floor — a silent-failure
  trap the spec review specifically checked.
- **Forced deferrals (breadcrumbed).** The dynamic `getTitle`/`cmUpdateTitle`
  frame-refresh (C++ `getTitle` is a virtual the frame calls each draw; rstv stores
  the title string at construction) is deferred because its only trigger,
  `FileEditor::saveAs`, is itself deferred on `TFileDialog` — so the title is derived
  once at construction (`file_name` / "Untitled"), with **no** `handle_event`
  override yet. `close()`'s `isClipboard→hide` branch is breadcrumbed (no rstv
  `close()` View method; the internal-clipboard editor is unported). `TStreamable`
  dropped (D12).
- **Verification.** 773 lib tests green; `clippy --all-targets -D warnings` (forced
  re-lint) + `fmt --check` clean. One snapshot of the assembled framed "Untitled"
  window (clean frame glyphs confirm the three aux children render hidden).

## Session — `TFileEditor` (row 68) — file-backed editor + growable-buffer seam

Row 68 (`TFileEditor`, `tfiledtr.cpp`). 758→768 lib tests (+10). **Row 68 ✅
core** — the dialog-dependent branches are *forced*-deferred (their substrate
isn't ported yet), not chosen-out. Tagged MECHANICAL, but it carries one genuine
**FOUNDATION** change to the shared `Editor`, so it was Opus-implemented.

- **The FOUNDATION problem.** C++ `TFileEditor` overrides `setBufSize` (virtual)
  so the *base* editor's insert path grows the buffer; in our D2 embed-delegate
  model a wrapper cannot override the inner `Editor`'s concrete internal methods.
  Resolution: add the growth **into `Editor` itself, gated by a `file_editor`
  flag** the file editor sets. The base/`Memo` stay fixed-buffer; only a file
  editor grows. Same pattern for `updateCommands` (enable `cmSave`/`cmSaveAs`).
- **Growable buffer.** `set_buf_size` changed `&self`→`&mut self` (both callers
  were already `&mut`): when `file_editor` and over capacity, round `new_size` up
  to a `0x1000` boundary, `Vec::resize`, then `copy_within` the post-gap tail
  (`n = buf_len - cur_ptr + del_count` bytes) from `[old_size-n..old_size]` to
  `[rounded-n..]` — the C++ `memmove(&buffer[newSize-n], &temp[bufSize-n], n)`
  translated; `copy_within` is memmove (overlap-safe). `gap_len` recomputed after.
  `new_file_editor` builds an `Editor` at `buf_size 0` with `is_valid = true` (a
  growable empty buffer is valid, unlike a fixed 0-size one). `load_file` reuses
  the row-67 `set_text` (grow + `setBufLen` — same placement math as C++ loadFile).
- **Default-off is the load-bearing review check.** Touching shared `Editor` risks
  silently changing `Memo`/base. Growth is strictly opt-in: `set_buf_size` with
  `file_editor == false` returns the exact old `new_size <= buf_size`. Two
  regression tests (`base_editor_buffer_does_not_grow`, `memo_buffer_does_not_grow`)
  pin this; the spec review verified fixed-buffer behavior is unchanged.
- **File I/O (real `std::fs`).** `load_file` (NotFound ⇒ empty+valid, like C++
  can't-open; other `Err` ⇒ invalid, the `edReadError` dialog deferred), `save_file`
  (writes `editor.text()` — the gap-skipping logical text = C++'s two `writeBlock`s;
  then `clear_modified` = `modified=False; update(ufUpdate)`), `save` (existing file
  ⇒ saveFile; untitled ⇒ deferred saveAs no-op). `handle_event` runs the base editor
  first, then `cmSave` (with a follow-up `flush_if_unlocked` so the indicator/command
  state publish that frame — the inner editor had already flushed with `modified`
  still true). `valid(cmValid)` reflects `is_valid`; the modified-save prompt is
  deferred to allow-close.
- **Forced deferrals (breadcrumbed).** saveAs/`SAVE_AS`/untitled-save → needs
  `TFileDialog` (unported); all `editorDialog` error/confirm popups + the `valid()`
  modified prompt → need the **async-modal-from-a-view seam** (the shared unblocker
  — same blocker as validator `error()`); `efBackupFiles`; `shutDown` (no rstv View
  analogue); DOS 16-bit/OOM ceilings (Vec growth is infallible — documented
  deviation); `setBufSize` shrink (memory-reclaim only); `TStreamable` (D12).
- **Verification.** 768 lib tests green; `clippy --all-targets -D warnings` (forced
  re-lint) + `fmt --check` clean. File-I/O tests use `std::env::temp_dir()` +
  per-test unique names (no `tempfile` dev-dep), cleaning up after themselves.

## Session — `TMemo` (row 67) — single-field dialog editor + a latent-bug fix

Row 67 (`TMemo`, `tmemo.cpp`). 752→758 lib tests (+6: 4 Memo + 2 Shift+Tab
regression). **Row 67 ✅.** A genuinely thin row — TMemo adds almost nothing over
TEditor — so the interest is in how the deviations collapse the C++ surface.

- **Scope.** `Memo { pub editor: Editor }`, a **D2 embed-delegate** wrapper:
  `#[crate::delegate(to = editor)]` with **no `skip(...)` list**, so every
  un-overridden `View` method forwards to the inner `Editor`. Three overrides:
  - `handle_event` — C++ `TMemo::handleEvent` swallows only a *plain* `kbTab`
    KeyDown (so Tab bubbles to the dialog's focus navigation) and forwards
    everything else to the editor. In the rstv decomposed `Key` model that is
    `Key::Tab` with `!shift && !ctrl && !alt`; the swallow `return`s **without
    `ev.clear()`** so the event survives to the dialog.
  - `value` / `set_value` — the **D10** successors to `getData`/`setData`:
    `FieldValue::Text` of the whole logical buffer. `set_value` drives a new
    inherent `Editor::set_text(&[u8])` that mirrors C++ `setData`
    (`setBufSize`-gated, all-or-nothing, places bytes at `buffer[bufSize-len..]`,
    `setBufLen`; **no** `selectAll`, unlike `TInputLine::setData`).
- **Deviations that erased C++ surface.** `dataSize()` is **dropped** (D10 — no
  untyped byte-size in the typed-value model); `getPalette`/`cpMemo "\x1A\x1B"` is
  **dropped** (D7 — the Role model abstracts the palette-index chain; Memo reuses
  the editor's `draw`, i.e. `Role::ScrollerNormal`/`ScrollerSelected`). A distinct
  dialog-context palette (`MemoNormal`/`MemoSelected` roles) is a noted, deferred
  theme refinement — not worth expanding `theme.rs` for this row.
- **`as_any_mut` must delegate (the load-bearing non-obvious bit).** The pump's
  `SyncEditorDelta`/`EditorPaste`/`IndicatorSetValue` brokers resolve a view by id
  and do `find_mut(id).as_any_mut().downcast_mut::<Editor>()`. With a `Memo`
  inserted at that id, `find_mut` returns the `Memo`; the no-skip delegation makes
  `Memo::as_any_mut` forward to the inner editor, so the downcast still reaches the
  `Editor`. Skipping it (the cluster-wrapper instinct) would silently kill
  scrollbar/indicator sync — and no snapshot test would catch it.
- **Latent row-66 `Editor` bug, found by the spec review + fixed (TDD).** The
  editor's insertable-char gate treated `Key::Tab` as insertable on `!ctrl &&
  !alt` only — so **Shift+Tab** inserted a stray `\t`. Faithful to C++ this is
  wrong: `kbTab` (charCode 9) inserts, but `kbShiftTab` (charCode 0) does not — it
  falls through and bubbles to the dialog for *backward* focus navigation. Added a
  `!shift` guard to the Tab arm (the `Char` arm keeps inserting Shift+Char =
  uppercase). Two regression tests (bare `Editor` + `Memo`) assert both no-insert
  **and** event-survives-uncleared; they were written failing-first.
- **Verification.** 758 lib tests green; `clippy --workspace --all-targets -D
  warnings` (forced re-lint) + `fmt --check` clean. One snapshot
  (`memo_snapshot`) covers the `set_value`→`draw` path.

## Session — `TEditor` core (row 66) — gap-buffer text editor faithful port

Row 66 (`TEditor`, `teditor1.cpp` + `teditor2.cpp` + `edits.cpp`). 712→752 lib
tests (40 new in the editor module). **Row 66 core is ◑** — see deferrals.

- **Scope.** Full gap-buffer core: `bufChar`/`bufPtr`/`getText`, `setBufLen`/
  `setBufSize`, `insertBuffer` (the load-bearing method: line-ending conversion,
  undo `delCount`/`insCount` accounting, gap arithmetic via signed deltas to avoid
  usize underflow on net deletion, the gap memmove), `insertText`/`deleteSelect`/
  `deleteRange`. All navigation: `nextChar`/`prevChar`/`nextWord`/`prevWord`/
  `lineStart`/`lineEnd`/`nextLine`/`prevLine`/`lineMove`/`indentedLineStart`/
  `charPos`/`charPtr`/`getMousePtr`/`nextCharAndPos`. `setCurPtr`/`setSelect` (gap
  memmove on cursor move), `newLine`+auto-indent, single-level `undo`.
  `draw`/`drawLines`/`formatLine` (selection highlighting), `doUpdate`/`update`/
  `lock`/`unlock`, `updateCommands`/`setCmdState`, `setState`, `changeBounds`,
  `scrollTo`/`trackCursor`/`cursorVisible`, `toggleInsMode`/`toggleEncoding`,
  `startSelect`/`hideSelect`/`hasSelection`, `detectLineEndingType` + conversion.
  `search`/`scan`/`iScan`/`countLines` fully ported + unit-tested. Keyboard
  `handleEvent` + `convertEvent` (the Ctrl-K/Ctrl-Q two-key prefix machine, mapped
  to the decomposed `Key`+`KeyModifiers` model). Single-click mouse cursor
  positioning. **System-clipboard cut/copy/paste** via the D11 backend
  (`clipboard==0` branch).
- **New seams (reusable substrate).**
  - **D3 broker for a non-Scroller view.** `TEditor` is not a `TScroller`, so
    reading scrollbar deltas needed a new variant: `Deferred::SyncEditorDelta {
    editor, h, v }` + `Editor::apply_scroll_delta` (mirrors
    `SyncScrollerDelta`/`SyncListViewer` but downcasts to the concrete `Editor`
    type). Writes scrollbar params via the existing scrollbar-params helper. This
    establishes the pattern for future non-Scroller views that need scrollbar
    siblings.
  - **`Deferred::IndicatorSetValue { indicator, location, modified }`** — the
    editor drives its `TIndicator` sibling (row/col + modified flag) through the
    pump (downcast to `Indicator`, `set_value`). A new deferred variant, not a
    `Context` param.
  - **Clipboard broker: `Deferred::SetClipboard(String)` and
    `Deferred::EditorPaste(ViewId)`** — applied in `program.rs` via
    `renderer.backend_mut().set_clipboard()/get_clipboard()`. The backend IS
    reachable in the deferred-apply scope; paste's re-queued scrollbar-param ops
    settle on the next pump (the one-pass drain is expected).
  - **`Role::ScrollerSelected`** filled (idx 2 of `cpScroller`≡`cpEditor`
    `"\x06\x07"`, app color `0x24`) — clears the `theme.rs` breadcrumb that
    explicitly deferred it "to TEditor row 66". Editor normal text reuses
    `Role::ScrollerNormal`.
  - **31 new `Command` consts** in `command.rs`: `FIND`/`REPLACE`/`SEARCH_AGAIN` +
    the `CHAR_LEFT`…`ENCODING` movement/edit family. The `ef*`/`sm*`/`uf*` flag
    families are module-private in `editor.rs` (`EF_*` is `pub(crate)`).
  - **Ctx-threading split.** Core editing methods are `Context`-free (they OR into
    `update_flags`); `&mut Context` threads only into `do_update`/`unlock`/
    `handle_event`/`set_state` and the public ctx-taking entries. This makes the
    whole gap buffer unit-testable in isolation and dodges the ctor's missing-
    `Context` problem. `change_bounds` is geometry-only (mirrors the `TScroller`
    seam). No new `View` trait method was added (so no `specs.rs` forwarder needed).
- **Deferrals (breadcrumbed with TODOs in `editor.rs`).**
  1. **Find/Replace dialogs** (`editorDialog`, the dialog-driven `find()`/`replace()`
     /`efPromptOnReplace` prompt) — `search()` itself is fully live; `cmFind`/
     `cmReplace` are no-ops; `cmSearchAgain` runs with an empty `find_str`.
  2. **Mouse drag-select/edge-scroll/wheel/middle-button pan** — single-click
     positioning kept; the inner `while(mouseEvent)` loops become a future
     `DragCapture` handler (precedent: `window.rs DragCapture`).
  3. **Right-click context menu** (`initContextMenu` + `popupMenu`).
  4. **Internal-clipboard `TEditor` branch** (`insertFrom` from a sibling editor) —
     deferred to row 69 (`TEditWindow` wires the clipboard editor).
  5. `TStreamable` write/read/build (D12).
- **Review outcome.** Two-stage review (spec-compliance + code-quality, fresh
  subagents). Spec reviewer traced the high-risk arithmetic (insertBuffer net-
  deletion, setSelect gap-memmove both directions, undo, lineStart gap-boundary,
  search whole-word, formatLine coloring, the keymap prefix machine, the D3
  brokers). **Two bugs found and fixed:** (1) **CRITICAL keymap bug** — Ctrl-Del
  had mapped to the dead `cmClear` duplicate instead of `cmDelWord` (`scanKeyMap`
  is first-match); (2) **MEDIUM correctness defect** — invalid-UTF-8 over-advance
  in nav helpers via `insert_text(&[u8])` (`from_utf8_lossy`-then-advance replaced
  with raw-byte grapheme helpers that advance an invalid byte by exactly 1). Also:
  dead-code removal, `EF_*`→`pub(crate)`, 6 added regression/coverage tests.

## Session — `StringList` (row 64) — D12 keyed-string-table minimal port

Row 64 (`TStringList`/`TStrListMaker`/`TStrIndexRec`, `tstrlist.cpp`). 704→712 lib
tests. **Row 64 is now ✅.**

- **It is a pure D12 case, not a literal translation.** All three C++ classes exist
  *entirely* to serialize a compressed keyed-string table to/from a resource (`.res`)
  stream via `TStreamable`/`ipstream`/`opstream` — machinery **D12 drops wholesale**.
  The classes have **zero in-framework consumers** (only the streaming/registration
  boilerplate `sstrlst.cpp`/`nmstrlst.cpp` reference them). So only the *observable
  contract* — a keyed lookup `u16 key → string` — was ported; the storage format
  (`TStrIndexRec` key/count/offset index, `MAXKEYS=16` run-length grouping,
  byte-length-prefixed blob, `build`/`read`/`write`) is dropped.
- **One type, not three.** `StringList` in `src/text.rs`, backed by
  `BTreeMap<u16, String>`. The maker/list split existed only for the read/write
  streaming asymmetry (gone under D12), so it collapses to one type. BTreeMap is the
  faithful choice — C++ `get()` linear-scans index records assuming ascending keys —
  and derives serde trivially if D12 persistence is ever revived (more serde-ready
  than the faithful index would have been).
- **API:** `new`/`Default`, `insert(key, impl Into<String>)` (← `TStrListMaker::put`),
  `get(key) -> Option<&str>` (← `TStringList::get`), `len`/`is_empty`,
  `FromIterator<(u16, S: Into<String>)>`. **Deviation noted in doc:** C++ `get()`
  writes an empty-string sentinel (`*dest = EOS`) for a missing key; we return `None`.
- Renders nothing → **unit tests only**, no snapshot (8 tests: round-trip, missing→None,
  overwrite, ordered iteration, len/is_empty, FromIterator borrowed+owned, Default).
  Not re-exported at crate root (matches existing `text` module convention).

## Session — `inputBox` (row 63, PART 2) — the single-input scatter/gather seam

Completes row 63: the `inputBox`/`inputBoxRect` half of msgbox. 698→704 lib tests.
Subagent-driven (Opus implementer → fresh-context two-stage review → fixes →
integrate → commit). **Row 63 is now fully ✅.**

- **Design decision — single-input shortcut, NOT the general D10 group-walk.**
  C++ `inputBoxRect` does `dialog->setData(s)` / `getData(s)`, which in C++ is the
  `TGroup` ordered child-walk. But `inputBox` has exactly **one** transferable field
  (the lone `TInputLine`), so the walk degenerates to "the input line's value". We
  port that degenerate case directly (scatter = `set_value` on the input line,
  gather = `value()` on it) and keep the **general** `Dialog::value`/`set_value`
  group-walk DEFERRED to its first genuine multi-field consumer (Batch E), where the
  typed-record shape (insertion order, which children participate, Int-vs-Text
  mapping) can be pinned against a real dialog rather than guessed. CLAUDE.md /
  `data.rs` already named inputBox as "the first consumer" — it just turned out to
  be a single-field one.
- **The seam (`exec_view_with_completion`).** Added a 4th param `gather:
  Option<ViewId>` and changed the return to `(Command, Option<FieldValue>)`. The
  gather read sits **after** the existing `completion` block and **before**
  `captures.pop()` / `group.remove(id)` — i.e. while the modal is still in the tree
  by id — and is gated on `retval != Command::CANCEL` (faithful to C++ `if (c !=
  cmCancel) getData(s)`). The 3 existing callers (`exec_view`, `message_box_rect`,
  `drive_pending_modal`) pass `None` and take `.0`; semantics unchanged. This is the
  "give exec_view a way to hand back data before the modal is dropped" path the
  handover called for — the dialog is consumed inside exec, so the only hook is
  pre-drop.
- **`build_input_box` + `Program::input_box`/`input_box_rect`.** Pure builder in
  `src/dialog/msgbox.rs` (InputLine first for tab order → first selectable →
  `selectNext(False)` focus target = the input line, NOT a button; Label linked to
  it; OK `bfDefault` cmOK; Cancel `bfNormal` cmCancel; rects ported verbatim,
  `label_size` = C++ `aLabel.size()` byte length). `input_box_rect` scatters
  `initial` then execs with `initial_focus == gather == input_id`; returns
  `(Command, String)` — the gathered text on non-cancel, the unchanged `initial` on
  cancel. `input_box` centers a `(0,0,60,8)` rect on the desktop. A new private
  `Program::desktop_size()` helper de-duplicates the desktop-size lookup shared with
  `message_box`.
- **Verification.** Snapshot of the 60×8 input dialog; behavioral tests for
  Esc→`(CANCEL, initial-unchanged)` and OK→gathered-text. The OK test
  (`input_box_rect_ok_returns_typed_edit`) drives a printable key into the focused
  input line (replacing the select-all'd scattered text) so the gathered value
  **differs** from `initial` — review caught that a "scatter X, assert X back" test
  is tautological (X is also the gather-fails fallback), so this test was added and
  **confirmed to fail when the gather is stubbed to `None`** ("hello" vs "X").

## Session — initial-modal-currency seam + `messageBox` (row 63, PART 1)

Two interlocking pieces: a FOUNDATION currency seam (handover item 2) and the
first half of msgbox (row 63). 688→698 lib tests. Subagent-driven throughout
(advisor-vetted brief → implementer → fresh-context review → integrate → commit);
two implementer runs died from context bloat at ~100+ tool-calls but had finished
all edits first — the orchestrator verified + committed the integrated tree.

- **`View::reset_current` — general initial-modal-currency seam (`957c67e`).**
  Closes handover item 2: a modal opened via `exec_view` was keyboard-dead until a
  nav event because rstv's deliberately ctx-less `Group::insert` (D3) skips the C++
  insert-time cascade `TGroup::insertBefore → p->show() → setState(sfVisible) →
  owner->resetCurrent()` (tview.cpp:723) that establishes a group's `current` =
  first selectable child. The original gap-analysis said this was blocked on
  `Group::insert` taking no `Context`; the fix exploits that **`exec_view` DOES have
  a `Context`** right after insert. New `View::reset_current` trait hook (default
  no-op; `Group` overrides via UFCS to the inherent `Group::reset_current`;
  `Window`/`Dialog` forward via `#[delegate]` + a `specs.rs` forwarder).
  `exec_view` calls it on the freshly-inserted modal BEFORE `set_current(Enter)`,
  so focusing the modal cascades into its now-set current child. The row-57
  `HistoryWindow::select_child` local workaround stays (belt-and-suspenders).
  Discriminating guard: a Group-level trait-dispatch test (`current` None → first
  selectable); the plain-Dialog Esc→CANCEL smoke test is honestly labelled
  non-discriminating (TDialog converts Esc regardless of currency).
- **`messageBox`/`messageBoxRect` (row 63 PART 1, `352c949`).** Faithful port of the
  two synchronous msgbox functions. `src/dialog/msgbox.rs`: D5-typed option API
  (`MessageBoxKind` + `MessageBoxButtons` struct-of-bools replacing the C++ `ushort`
  flag word) + a pure `build_message_box(...) -> (Dialog, Option<ViewId>)` builder
  (faithful centering math, `[Yes,No,OK,Cancel]` order, `bfNormal`, `MsgBoxText`
  titles). `Program::message_box_rect`/`message_box` (the latter ports `makeRect`
  auto-centering on desktop size) own the `exec_view`/destroy tail.
  - **`selectNext(False)` faithfulness fix.** C++ `messageBoxRect` ends on the
    **first** button (Yes), not the firstMatch default (Cancel/last) — traced
    through the C++ ring + `findNext(prev)`. Replicated via a new **additive**
    `initial_focus: Option<ViewId>` on `exec_view_with_completion`: after the modal
    opens, `focus_descendant` moves internal focus to the first button (id returned
    by `build_message_box`). The two pre-existing callers pass `None`, so the
    `reset_current` seam is untouched. The discriminating test drives focused-Space
    on the focused Yes button through the animation timer and asserts
    `end_state == YES` (fails under the old Cancel-focus behavior).
  - **`inputBox`/`inputBoxRect` DEFERRED.** `dialog->setData/getData` is the D10
    dialog-level group-walk gather/scatter, which **does not exist** (`Dialog` has no
    `value`/`set_value`) — net-new FOUNDATION, not the "mechanical" the old handover
    claimed. The five validators' `error()` → `messageBox` wiring is also still a
    TODO: `Validator::error(&self)` has no `Context`, so it cannot reach a deferred
    channel — its own trait-signature seam, a separate follow-up.

## Session — Batch C validators 58–62 + `RegexValidator` (Phase 5)

The whole `tvalidat.cpp` validator family ported in PORT-ORDER sequence
(58→62), each via the standard cycle (advisor-vetted brief → implementer →
fresh-context two-stage SPEC+QUALITY review → fixes → integrate → commit), **plus
a new `RegexValidator`** — an rstv-original extension the user requested to
"bring the picture-mask DSL into the now." 628→688 lib tests.

- **`FilterValidator` / `LookupValidator` / `StringLookupValidator` (58/60/61,
  `ff63715`)** — leaf validators. Filter = every char in the allowed set
  (`strspn==strlen`, per-char vs C++ per-byte documented); Lookup = the abstract
  base realized as accept-all (the `lookup()` virtual collapses into `is_valid`
  under D2); StringLookup = exact membership in an owned `Vec<String>`. `error()`
  bodies are `TODO(row 63)` breadcrumbs preserving the exact C++ messages.
- **`RangeValidator` (59, `b52dc23`) + the `transfer`/D10 hook** — the
  FOUNDATION-ish row. Range embeds a `FilterValidator` (Range IS-A Filter, D2);
  `is_valid` = charset gate → parse → `[min,max]`; `is_valid_input` is INHERITED
  (charset-only while typing). Resolved the deferred `transfer` D10 seam: new
  `Validator::transfer_get`/`transfer_set` (default `None`), gated on
  `transfer_enabled` (C++ `options & voTransfer`, default OFF), wired into
  `InputLine::value`/`set_value` before the text fallback — gate-OFF keeps every
  existing call site at `Text` (row-39 regression guard pinned). `parse_long` =
  `trim().parse::<i32>().ok()` (documented stricter-than-`sscanf` deviation, no
  panic). NOT a `View` method → no `specs.rs` forwarder.
- **`PXPictureValidator` (62, `81a478c`)** — the ~450-line Paradox picture-mask
  recursive state machine. Idiomatic crux: the C++ `index`/`jndex` member cursors
  are per-call scratch, so they move onto a transient `Picture` scanner built
  fresh per `picture()` call (the trait methods are `&self`, object-safe);
  `process`/`scan`/`iteration`/`group`/… map 1:1. Byte-level (`Vec<u8>`); a
  `pic_at`/`input_at` out-of-range→0 helper models the NUL terminator — no index
  panics (the reviewer independently re-traced the backtracking engine + every
  flagged golden vector + confirmed no panic on any input). `is_valid_input`
  mutates the buffer in place (autofill + uppercase). Batch C COMPLETE.
- **`RegexValidator` (NEW rstv extension, `1a7eada`)** — a regex-driven validator
  that reproduces the picture validator's two-phase behavior from one pattern:
  `is_valid` = whole input matches; `is_valid_input` = the input is still a
  **prefix of some complete match** ("could it still become valid"), via a
  `regex-automata` anchored dense-DFA **dead-state** test. Pattern treated as the
  complete value (start-anchored via `StartKind::Anchored`; end-anchored via
  `(?:<pat>)\z`). **SECURITY:** `new` validates the BARE pattern first — a crafted
  unbalanced-`)` pattern (`cat)|(.*`) would otherwise escape the wrap and silently
  validate ANY input (caught by review, fixed + regression-tested). Lives
  ALONGSIDE the picture-mask DSL (not a replacement). Dep: `regex-automata` 0.4
  (`default-features=false` + minimal features → pulls only `regex-syntax`, no
  `aho-corasick`). Design proven by an orchestrator DFA spike before dispatch.
  **This is an extension beyond the C++ port** — worth a PORTING-GUIDE note when
  next touching that file.

---

## Where things stand (git `main`) — as of the prior (THistory 57) session

| commit | what |
|--------|------|
| _(this session)_ | **`THistory` (57) — the view-triggered async-modal seam** — faithful `thistory.cpp`. The dropdown-arrow icon next to a `TInputLine` that opens a modal `HistoryWindow` and writes the pick back. **THE FOUNDATION SEAM (the menu sessions reserved it):** a leaf view holds only the link's `ViewId` (D3) and **cannot call `exec_view`** (top-level only) — so it **requests** the open and the pump drives the modal. New `Deferred::OpenHistory{link, history_id, require_focus}` + `RecordHistory{link, history_id}` (+ `Context::request_*`); the `OpenHistory` apply arm (inside the `pump_once` destructure — split borrow, so it does NOT call exec_view) reads the link's text/bounds/focus via `group`, `history_add`s the **current** text (recordHistory at OPEN, never the pick), builds the `HistoryWindow`, and stashes it into a new **`Program::pending_modal: Option<(Box<dyn View>, ModalCompletion)>`**. The **OUTER driver `pump_and_drive`** (used in **both** `run`'s AND `exec_view`'s inner `while`, since a `THistory` lives in a `Dialog` opened via exec_view → modal-from-modal) takes `pending_modal` after `pump_once` returns — where it holds a whole `&mut self` — and runs the re-entrant `exec_view` at top level. **`exec_view` refactored to `exec_view_with_completion`** with two additions: (1) **`end_state` save/restore** at entry/exit (re-entrancy — else the inner modal's end command leaks to the outer loop; bite-tested; the cmQuit-as-retval deviation still holds); (2) the **`ModalCompletion` runs BEFORE remove/drop** (while the modal is still in the tree by id) as a DIRECT `group` mutation — NOT via the deferred queue (that drain is gated on `!ev.is_nothing()` and would never fire from here). `ModalCompletion::HistoryPick{link}`: on `cmOK`, downcast the modal → `HistoryWindow::get_selection` → `link.set_value(Text)` (= the C++ `strnzcpy`+`selectAll`; `InputLine::set_value` already does `data=s; select_all`). New `View::descendant_global_bounds(&self, id, acc)` trait method (+ Group override + delegate forwarder, spy 25→**26**) for the link-local→absolute geometry the root-insert path needs (clamp-to-screen deviation vs C++'s owner-extent intersect — both documented). `HistoryWindow::as_any_mut` promoted to real `Some(self)` (was skipped→default `None`; the completion downcast needs it). **SPEC review (fresh C++-adversarial Opus) caught a REAL foundational gap:** `Group::insert` has no `ctx` → never `reset_current`, so unlike C++ (`insertView→show→resetCurrent`) an opened modal's internal `current` stays `None` until a nav event — **popup Esc/Enter were dead on open** (the brief's "already work" claim was false). **Fixed locally + faithfully:** `Window::select_child_for_test`→production `pub(crate) select_child`, called in `HistoryWindow`'s first-event setup guard to make the viewer current (deviation: currency at FIRST-EVENT not at open — same class as the viewer `setup()`); new `no_nav_first_event_dismisses_popup_bite` ([mouseDown,Esc]→CANCEL / [mouseDown,Enter]→OK, no prior nav — hangs before the fix). The **general** dialog initial-currency gap is breadcrumbed at `exec_view`'s `set_current` site (foundational follow-on). **QUALITY review (fresh Opus): borrow-soundness clean, both core bites verified by mutation; 3 test-rigor/doc fixes** (the anti-double-record test reworked OK-path → non-vacuous; a `None`-path assert added to `descendant_global_bounds`; 2 stale comments). Deferred (breadcrumbed): the `ModalFrame` outside-click cancel (a `program_handle_event` delivery-path change, NOT a `ModalFrame` tweak — confirmed); `max_len` clamp on `set_value` (row-39 gap); helpCtx propagation. 618→**631** lib tests. FOUNDATION ← THIS session |
| `ad41f05` | **`THistoryWindow` (56) — modal recall window hosting the viewer** — faithful `thistwin.cpp`. `HistoryWindow` is a `TWindow` subtype (D2 embed + `#[delegate(to = window)]`, like `Dialog`) assembling a frame + two `standard_scroll_bar`s (h `sbHorizontal\|sbHandleKeyboard`, v `sbVertical\|…`) + a `HistoryViewer` (55) over an extent grown `(-1,-1)`, with `get_selection` = the viewer's focused `get_text` (by id + `as_any_mut` downcast). `flags = wfClose` only (not movable). **Seam promoted (shared foundation touch, also unblocks msgbox 63 + Batch E):** `Window::insert_child`/`Dialog::insert_child` go `#[cfg(test)]`→real `pub(crate)`, + new `pub(crate) Window::child_mut`. **Viewer `setup()` (the Context-needing ctor tail, row 55/ListBox constraint) runs ONCE at the TOP of `handle_event`, BEFORE delegating to `TWindow::handleEvent`** — so the first event reaches an initialized viewer (the bite: misorder → first Down hits a range=0 viewer → focused wrong; verified empirically to fail with focused==1). **DEFERRED (breadcrumbed, NOT a silent drop):** the C++ `evMouseDown && !mouseInView → endModal(cmCancel)` outside-click cancel — our `ModalFrame` (program.rs) **Consumes outside positional events before they reach the modal view**, so the arm is unreachable; delivering outside clicks to the modal view is a **modal-loop change reserved for row 57 / msgbox 63** (`Deferred::OpenModal`). Esc/Enter/double-click confirm/cancel still work (the viewer, row 55). `cpHistoryWindow` palette → provisional Window/Frame + TODO(row 34). **Two-stage review (fresh SPEC + QUALITY Opus, both PASS on production code, no blockers):** the two converged test-quality SHOULD-FIX items fixed — the setup-guard test rewritten into a TRUE bite (new `#[cfg(test)] Window::select_child_for_test` makes the viewer current; deliver Down; assert focused 1→2; empirically confirmed to fail when misordered) + the negative h-bar-max test made non-vacuous (asserts the exact `-32` queued max AND that it drains through `ScrollBar::set_params` in a live `exec_view` pump without panic — the HANDOVER watch-item, now pinned end-to-end). 613→618 tests. MECHANICAL ← THIS session |
| `6ada1fd` | **`THistoryViewer` (55) — modal recall list over the store** — faithful `thstview.cpp`. A single-column `TListViewer` subclass (mirrors the `TListBox` template: `impl ListViewer` lv/lv_mut + override only `get_text`; `impl View` delegating draw/event/nav to the `list_viewer::` free fns) that reads the row-54 store **live by id**. `get_text(item)` → `history_str(id, item)`; `handle_event`: Enter/double-click → `ctx.end_modal(Command::OK)`, Esc/`cmCancel` → `end_modal(Command::CANCEL)`, **unconditional (no `sfModal` gate** — the viewer only ever lives in a modal `THistoryWindow`, faithful to the C++), else fall through to `list_viewer::handle_event`. `history_width()` = max `text::width` over the channel. The Context-needing ctor tail (`set_range(history_count)`/`focus_item(1)` when range>1/h-bar `setRange(0, historyWidth()-size.x+3)`) moved to a post-insert **`setup(&mut self, ctx)`** (same Context-free-ctor constraint as `ListBox::new`; **does NOT** publish step sizes — the C++ ctor doesn't either). `history_id: u8` throughout (C++ `ushort` truncates to the `uchar` store; explicit `u8` avoids silent aliasing). Palette reuses provisional `Role::List*` + `TODO(row 34): cpHistoryViewer remap` (no new Role variants). **Two-stage review (SPEC + QUALITY, fresh Opus): SPEC PASS no findings; QUALITY PASS + 1 SHOULD-FIX** (added a bite-checked test for the previously-untested h-bar `setRange` branch — exact `max=-12` from `historyWidth-size.x+3`) + 1 cost NIT comment. 601→613 tests (+12 incl. a snapshot proving item-1 focus). MECHANICAL ← THIS session |
| `121ec67` | **history store (54) — `historyAdd`/`Count`/`Str`/`clearHistory`** — faithful `histlist.cpp` as an idiomatic `Vec`: a single **global 1024-byte budget** with **global FIFO eviction** (NOT per-id), `thread_local! { RefCell<Vec<HistRec>> }` (single-threaded like the C++; per-test isolation falls out of libtest's per-test threads). Four free fns in `src/widgets/history.rs`. Cost/entry = `str.len()+3` (UTF-8 bytes, faithful `TStringView::size`). Index order **oldest→newest per id** (row 55's `get_text` reads it directly — no inversion anywhere, faithful to C++). `history_add`: dedup `(id,str)` FIRST → evict front → push back. **Documented deliberate deviation:** the C++ front-sentinel + always-skip-front bookkeeping is NOT replicated, so every non-evicted entry is readable (C++ hides one globally-oldest entry after the first overflow); C++'s in-band sentinel makes its budget 3 bytes tighter (a byte-boundary nuance, not a new divergence). `initHistory`/`doneHistory` moot (thread-local Vec) — omitted, not stubbed. **Two-stage review (SPEC + QUALITY, fresh Opus): both PASS, no blockers; +3 NITs** (`#[must_use]` getters, single `cost_of()`, doc precision note). 593→601 tests. The shared dependency for rows 55–57. MECHANICAL ← THIS session |
| `0fc6a9e` | **`Desktop::tile`/`cascade` geometry + `cmTile`/`cmCascade` WIRED — the row-32 `TApplication` breadcrumb is CLOSED** — faithful port of `TDeskTop::tile`/`cascade` (`tdesktop.cpp`): `i_sqr`/`most_equal_divisors`/`divider_loc`/`calc_tile_rect` ported as pure fns (the C++ file statics threaded as params, no globals; `divider_loc` multiply in `i64`), re-added the `tile_columns_first` field (`favorY = !tile_columns_first`; tile now consumes it). **New seam `view::locate` is a FREE FN, NOT a `View` trait method** — a trait method would be forwarded by `#[delegate]` to a wrapper's inner group, whose `size_limits` is 0×0, bypassing e.g. `Window`'s 16×6 min (the advisor-caught trap; the existing inherent `Window::locate` for zoom is left untouched). `tile`/`cascade` are defaulted no-op `View` trait methods **overridden by `Desktop`** (mirrors `select_window_num`; the program drives the desktop by id through `&mut dyn View`, no downcast) + `Group::tileable_ids` (forEach order = `children.iter().rev()` filtered `tileable && visible`) + `child_mut` per child. **Off-by-one pinned** (`tile_num`/`cascade_num` start `n-1`; cascade error check subtracts the full `n`; `lastView` = `ids.last()`). Wired in `program_handle_event` after `group.handle_event`, beside the QUIT catch (`getTileRect()` = desktop child extent, `ev.clear()` after). `examples/hello.rs` opts its 3 demo windows into `ofTileable` + adds Window→Tile/Cascade items (cmTile/cmCascade are default-enabled, so they route + draw enabled). **Full two-stage review (SPEC + QUALITY, fresh C++-adversarial Opus): no blockers, no should-fix** — SPEC verified line-by-line incl. the end-to-end menu-enable path; QUALITY traced the integer geometry panic-free (`i_sqr(1)=1`, no div-by-zero) + tests discriminating. +3 NIT cleanups (closed a latent **`delegate_view` spy gap**: it never exercised `set_menu_current`, count 24→**25**; + column-first `most_equal_divisors` branch test; + cmCascade pump test). 585→593 tests (FOUNDATION) ← THIS session |
| `e02a4bf` | **Menu bar + status line WIRED INTO `Program` — the drivable-app payoff** — `examples/hello.rs` is now a real running TV app (menu bar row 0, desktop, status line bottom row). `Program` captures the menu-bar/status-line `ViewId`s + **seeds initial command-graying** at construction via `update_menu_commands` (the carried startup-regray gap: `cmCommandSetChanged` does not fire at startup). `pump_once` adds the faithful **`getEvent` status-line pre-routing** (`tprogram.cpp:153`): `evKeyDown` always + over-the-line `evMouseDown` (gated by new **`Group::topmost_child_at`** = `firstThat(viewHasMouse)`), run **BEFORE `captures.dispatch`** so accelerators (F10→cmMenu, Alt-X→cmQuit) fire even while a modal is open (the discriminating placement crux + bite). `StatusLine` keyDown **global-accelerator arm** (`tstatusl.cpp:181`): match keycode over ALL items incl. textless, **transform `ev`→`Command` in place, no clear** (propagates; NOT `ctx.post`+clear). **`MenuBar::update_menu_commands` override closed a latent gap** (graying was silently inert on the real bar — the existing broker test used a test-double). `Desktop::insert_view` → `pub` (production window-insert seam). idle→`update()` help-ctx refresh **deferred (inert under a single `All` `StatusDef`)**. Two-stage review (SPEC faithful, QUALITY no prod blockers; 2 vacuous mouse tests reworked into bite-checked discriminating ones). 576→585 tests (FOUNDATION) ← THIS session |
| `df3b8b9` | **Status line (rows 47 + 53) — `TStatusItem`/`TStatusDef` data + `TStatusLine` draw/data slice** — `src/status/` (`mod.rs` data + builder, `status_line.rs` view). The standalone snapshot-testable view (NOT yet wired into `Program`, mirroring how the menu draw layer landed before the modal/Program wiring). `HelpCtxRange::{All, OneOf(Vec<HelpCtx>)}` replaces C++'s numeric `[min,max]` help-ctx ranges (D1 corollary — string `HelpCtx` has no ordering); `StatusItem.text: Option<String>` (`None` = the hidden global-hotkey item: draws nothing, no width, but the keyDown loop matches it); command-graying via a cached `CommandSet` on the **view** (the `update_menu_commands` broker hook + `cmCommandSetChanged`→`request_update_menu`, NOT a field on `StatusItem` — faithful to C++ computing `commandEnabled` live). 6 `Status*` theme `Role`s. 551→576 tests (FOUNDATION) ← THIS session |
| `add2947` | **Menu MODAL layer Step-2 stage 3 (52) — `TMenuPopup`** — the LAST modal piece: `put_click_event_on_exit` flag on `MenuSession` (gates the bottom-level exit-click re-post; bar/box `true`, popup `false`), popup level starts `current=None` + clears its menu clone's `default` (`menu->deflt=0`), `popup_menu()` free fn + `auto_place_popup` geometry (faithful `popupMenu`/`autoPlacePopup`); `end_session_with` reworked to a kind-keyed (`is_bar`) teardown (a popup's level 0 IS a box). `TMenuPopup::handleEvent` moot/dropped (Ctrl+letter TODO). 545→551 tests (FOUNDATION) ← THIS session |
| `93d6d35` | **Menu MODAL layer Step-2 stage 2 (50–52)** — the **mouse** arms of the flattened `execute()`: `track_mouse`/`mouse_in_view`/`mouse_in_owner`/`mouse_in_menus`, `evMouseDown`/`Up`/`Move` step arms + per-level loop-locals (`last_target_item`/`mouse_active`/`first_event`); stage-1 `handle_key` refactored into one shared `run()` loop (kbd+mouse+cmMenu); `evMouseDown` bar activation (`do_a_select`) + `activate_mouse`; cmMenu routed through `run()` (FOUNDATION) |
| `ed0abfa` | **Menu MODAL layer Step-2 stage 1 (50–52)** — `MenuSession` capture handler = flattened `execute()`; keyboard nav + submenu recursion + the `putEvent`→parent re-apply loop; new `Deferred::OpenMenuBox`/`SetMenuCurrent` + `ctx.put_event` + `Group::insert_with_id` + `View::set_menu_current` (FOUNDATION) |
| `0687530` | **TMenuBar/TMenuBox DRAW/DATA layer (50/51)** — `MenuView` trait + `current` + draw/getItemRect + 6 menu theme roles (FOUNDATION) |
| `dfe66b1` | **TMenuView passive layer (49)** — command-graying broker + hotkey dispatch (FOUNDATION) |
| `c5c061d` | **TMenu data tree (46)** — `MenuItem`/`Menu`/`MenuBuilder` (FOUNDATION) |
| `fc66637` | **TListBox (48)** — first concrete `TListViewer` (MECHANICAL) |
| `3e6645f` | **TApplication (32)** — thin D2 wrapper over `Program` (MECHANICAL) |
| `47894f0…66ab55f` | **`#[delegate]` proc-macro** — `tvision-macros` crate + workspace, then **adopted** across cluster/Window/Dialog/ParamText/Label/Desktop + the hello example (replaces `cluster_wrapper!`) |

**Build state:** 631 lib (was 618; +13 this session: row-57 — 7 program-level
seam tests incl. the no-nav `[mouseDown,Esc/Enter]` currency bite, end-to-end
pick→flowback, cancel-no-write, recordHistory-at-open-OK-path, keyboard-focus
gate, inner-modal-end-no-leak re-entrancy, `descendant_global_bounds` through a
non-zero-origin dialog + its `None` path; + 6 `THistory` widget tests incl. the
`▐↓▌` icon snapshot) + 5 integration (3 `render_pipeline` + 2 `delegate_view`,
the latter exercising **26** macro forwarders — `descendant_global_bounds` added)
+ 2 doctests green;
`cargo build --example hello` builds the drivable app; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo
fmt --all --check` clean (verify clippy with a forced re-lint — a cached run can
mask a fresh warning). **It is a Cargo workspace**
(`tvision` + `tvision-macros`) — use `--workspace` for test/clippy/fmt. (Cargo
artifacts land in `/home/oetiker/scratch/cargo-target` — set `CARGO_TARGET_DIR`.)

**Phase 2 COMPLETE. Batch B (Phase-3 leaves) COMPLETE. Phase-1 row 32 COMPLETE.**
**Phase 4 in progress — Row 46 `TMenu` data tree + Row 49 `TMenuView` passive
layer + Rows 50/51 draw/data + the menu MODAL layer Step-2 stages 1 (keyboard), 2
(mouse) AND 3 (`TMenuPopup` 52) ALL DONE** (a prior session). **The
menu modal layer (rows 46/49/50/51/52) is COMPLETE** — the whole flattened
`TMenuView::execute()` (bar + box + popup, keyboard + mouse) is ported. **Status
line rows 47 (`TStatusItem`/`TStatusDef`) + 53 (`TStatusLine` draw/data slice) are
DONE** (a prior session). **The menu bar + status line are WIRED INTO `Program`**
(`examples/hello.rs` is a drivable TV app), and **`Desktop::tile`/`cascade` +
`cmTile`/`cmCascade` are WIRED** (the row-32 breadcrumb CLOSED) — all prior sessions.
The history store (54) + `THistoryViewer` (55) + `THistoryWindow` (56) landed
prior sessions. **THIS session: `THistory` (57) is DONE** (top git-table row) —
**the FOUNDATION view-triggered async-modal seam is built** (`Deferred::OpenHistory`
+ `Program::pending_modal` + the `pump_and_drive` outer-loop drive + `exec_view`
end_state save/restore + `ModalCompletion` flowback). **The whole history cluster
(54–57) is now COMPLETE.** **Next (lowest-numbered remaining work):**
1. **The `ModalFrame` deliver-outside-to-modal seam** (row 56/57 deferred) — a
   **`program_handle_event` delivery-path change** (NOT a `ModalFrame` return-value
   tweak: `ModalFrame` has no `group`, and `program_handle_event` routes outside
   clicks positionally to the desktop), to un-defer the `HistoryWindow` outside-click
   `endModal(cmCancel)`.
2. **The general initial-modal-currency seam** — `exec_view` should establish a
   freshly-opened modal's internal `current` (first selectable child), so EVERY
   dialog gets keyboard focus on open (C++ `insertView→show→resetCurrent`). Row 57
   worked around it **locally** for the history popup (first-event `select_child`);
   the general fix is blocked on `Group::insert` having no `ctx` (breadcrumbed at
   `exec_view`'s `set_current` site).
3. **msgbox 63** — the co-consumer of the async-modal seam (ADDS a `ModalCompletion`
   variant; uses the row-56 `Window::insert_child` for its children).
Batch C validators 58–62 remain an available parallel fan-out; `cmDosShell` still
needs a backend suspend seam.

> **Worktrees** live under `/scratch/oetiker/claude-worktrees/<project>-<name>`
> (global CLAUDE.md). A `WorktreeCreate` hook (`~/.claude/settings.json` →
> `~/.claude/worktree-create.sh`) redirects the Agent/Workflow
> `isolation:"worktree"` worktrees there, so **isolation IS usable** — BUT the
> hook only activates on a session **restart** (hooks load at startup); until
> then, isolation lands in the project's `.claude/worktrees/` and you should
> create the worktree manually at the `/scratch` path + dispatch a non-isolated
> subagent.

## What landed THIS session — history store (54) + `THistoryViewer` (55) (MECHANICAL)
The first two rows of the **history subsystem**, both Opus-orchestrated with the
standard cycle (advisor-vetted brief → Sonnet implementer → **two-stage review**,
fresh SPEC then QUALITY Opus agents → NIT fixes → integrate → commit). Briefs:
[`row54-history-store.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row54-history-store.md),
[`row55-history-viewer.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row55-history-viewer.md).
Both live in `src/widgets/history.rs` (store + viewer together). Detail is in the
two top git-table rows above; the load-bearing points:

- **Row 54 store** — one **global 1024-byte budget**, **global FIFO eviction**
  (NOT per-id — the trap that kills the obvious `HashMap<id, VecDeque>` port),
  `thread_local! RefCell<Vec<HistRec>>`, oldest→newest index order, dedup-before-
  evict. The advisor caught the C++ **front-sentinel + always-skip-front** byte-
  block artifact; we model the clean contract (every non-evicted entry readable)
  and **document the non-replication** rather than reproduce it.
- **Row 55 viewer** — a `TListViewer` subclass mirroring `ListBox`; the
  Context-needing ctor tail → a post-insert `setup()`; **unconditional** endModal
  (no `sfModal` gate — the viewer only lives in a modal window). The hbar
  `setRange(0, historyWidth()-size.x+3)` can go **negative** (small history, wide
  view) — faithful to C++, published as-is; now covered by an exact-`-12` test.

**The two seams rows 56/57 need (discovered this session — build these FIRST):**
1. **A production `Window::insert` (for row 56).** `Window::insert_child` /
   `Dialog::insert_child` are currently **`#[cfg(test)]`-only** — there is **no
   production path to add child controls to a window/dialog yet** (msgbox 63 and
   all Batch E dialogs were never built, so nothing needed it). `THistoryWindow`
   (56) is the **first** production consumer: its ctor inserts a `THistoryViewer`
   into the window group (after building two `standard_scroll_bar`s). So row 56
   must first promote `Window::insert_child` to a real `pub(crate)` production
   method (it's already ctx-free: `self.group.insert(view)`; same for `Dialog`).
   This is a tiny but genuine foundation touch that **also unblocks msgbox 63 +
   Batch E**. See `tdesktop.cpp`-style factory: `initViewer` grows the extent by
   `(-1,-1)`, builds the two `sbHorizontal|sbHandleKeyboard`/`sbVertical|…`
   bars, constructs the viewer, inserts it; then the window calls the viewer's
   `setup()` (needs a Context — so it lands post-insert, like `ListBox`).
2. **The view-triggered async-modal path (for row 57, shared with msgbox 63).**
   `THistory` (57, the dropdown icon next to a `TInputLine`) `execView`s a
   `THistoryWindow` **from within its own `handle_event`** and writes the picked
   string back into the linked input line. This is the **unbuilt D9 `OpenModal`**
   seam the menu sessions deliberately reserved (a *command* launching a modal,
   not menu nav) — `Deferred::OpenModal` + a posted completion `Command` + a way
   to read the modal's result (`THistoryWindow::getSelection` = the viewer's
   focused `get_text`, reached by id + `as_any_mut` downcast). **Design this with
   the advisor + main-thread care** — it is the FOUNDATION piece of the cluster,
   and msgbox 63 is its co-consumer (build the seam once, wire both).

## Prior session — menu bar + status line WIRED INTO `Program` (FOUNDATION)
The **drivable-app payoff**: the standalone menu-bar + status-line views become a
running app. Brief:
[`docs/briefs/row47-53-program-wiring.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row47-53-program-wiring.md)
(advisor-vetted — the advisor's key call was to **defer the idle→`update()` help-ctx
refresh**: under the single universal `All` `StatusDef` every real app uses,
`find_items` is invariant, so `update()` is observably inert — no consumer ⇒ no new
`View::get_help_ctx`/`TopView` seam this session) → Opus implementer → **full
two-stage review** (SPEC then QUALITY, fresh C++-adversarial Opus agents — SPEC
faithful with no blockers, QUALITY no production blockers) → 2 vacuous mouse tests
reworked into bite-checked discriminating ones → integrate. 576→585 tests.

- **`Program` ids + initial regray.** `Program::new` now captures the menu-bar +
  status-line `ViewId`s and **seeds command-graying directly** via
  `update_menu_commands(&command_set)` at construction — closing the carried gap that
  menus/status are born all-enabled and `cmCommandSetChanged` does **not** fire at
  startup. (No defer: the deferred queue is not drained on an idle first pump anyway.)
- **`getEvent` status-line pre-routing** (`tprogram.cpp:153`) in `pump_once`, at the
  **top of the `Some(ev)` arm, BEFORE `drop_disabled` + `captures.dispatch`** — because
  C++ `getEvent` pre-routes regardless of modal nesting, so accelerators fire **while a
  modal dialog is open** (the discriminating `accelerator_fires_during_a_modal` crux,
  bite-checked). keyDown always; `evMouseDown` only when the line is the topmost view
  under the cursor (new **`Group::topmost_child_at`** = faithful `firstThat(viewHasMouse)`
  over direct children). The pre-route does the `makeLocal` (`m.position -= origin`)
  the group router would normally do, since it bypasses the router.
- **`StatusLine` keyDown global-accelerator arm** (`tstatusl.cpp:181`, the last deferred
  arm): match the keycode over **ALL** items (incl. textless global hotkeys), and if
  `command_enabled`, **transform `ev`→`Event::Command` IN PLACE — no clear, no post** —
  so the same live event propagates into normal dispatch (porting it as `ctx.post`+clear
  would double-handle). The mouseDown arm (post+clear) was already there.
- **`MenuBar::update_menu_commands` override — closed a LATENT FOUNDATION GAP.** The bar
  never implemented this hook, so the command-graying broker fell through to the trait
  no-op: graying was **silently inert on the real `MenuBar`** for BOTH the new startup
  regray AND the pre-existing `cmCommandSetChanged` broadcast path (the row-49 broker
  test used a `#[cfg(test)] MenuProbe` test-double, so it never caught this). One-line
  delegate to the shared `menu_view::update_menu_commands`.
- **`examples/hello.rs` → a drivable app.** Faithful init insets (`initDeskTop`
  `r.a.y++`/`r.b.y--`, `initMenuBar` `r.b.y=r.a.y+1`, `initStatusLine` `r.a.y=r.b.y-1`);
  3 demo windows inserted into the desktop (via the now-`pub` `Desktop::insert_view`);
  a File/Window menu bar + the standard status line; `run()` spins the real
  `program.run()` loop. **Known limitation (documented):** menu items can only wire
  commands that already *route* — menu→dialog needs the unbuilt D9 `OpenModal` path
  (row 63), so **no About/Tile/Cascade items** yet. Alt-shortcuts reach the bar via
  `ofPreProcess` (the bar sets it + `Group::handle_event` runs the preProcess phase);
  F10 enters menus via the status-line accelerator → cmMenu.
- **Deferred + breadcrumbed (NOT stubbed):** idle→`statusLine->update()` help-ctx
  refresh (inert under a single `All` def — omit-until-consumer; revisit for a context-
  split `OneOf` line); `cmTile`/`cmCascade` + `Desktop::tile`/`cascade` geometry +
  `cmDosShell` (see NEXT); the status-line press-and-hold drag-highlight (`TODO(row 31,
  D9)`).
- **Verification:** 9 `wiring` tests — F10-enters-menu, Alt-X-quits, the
  accelerator-during-modal placement crux (bite: move pre-route after capture dispatch →
  red), two *reworked* discriminating mouse tests (status-line click past a modal gate;
  desktop click reaches a spy Probe and is NOT eaten by the line's clear — each
  bite-checked against its own production path), initial-regray (no pump), + a
  full-screen layout snapshot (bar row 0 / desktop / line row h-1).

## Prior session — status line (rows 47 + 53) (FOUNDATION)
The **draw/data slice** of the status line — a standalone, snapshot-testable
`TStatusLine` view (the `TProgram` getEvent/idle wiring is a separate next step,
mirroring how the menu draw layer landed before its modal/Program wiring). Brief:
[`docs/briefs/row47-53-status-line.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row47-53-status-line.md)
(advisor-vetted) → Opus implementer → **full two-stage review** (SPEC then QUALITY,
fresh C++-adversarial Opus agents — both PASS, no blockers) → 3 MINOR fixes →
integrate. New `src/status/` module (`mod.rs` + `status_line.rs` + 4 snapshots) +
`lib.rs`/`theme.rs` wiring. 551→576 tests.

- **`StatusItem` / `StatusDef` (row 47, `src/status/mod.rs`)** — pure data + a
  fluent builder (mirrors `MenuBuilder`). **`StatusItem.text: Option<String>`** is
  load-bearing: `None` (C++ `text == 0`) is a **hidden global-hotkey item** — it
  draws nothing AND consumes no horizontal space (the `i += l+2` advance is *inside*
  `if(text != 0)` in both `drawSelect` and `itemMouseIsIn`), but the (deferred)
  keyDown loop still matches it to fire its command. `key_code: Option<KeyEvent>`
  (`None` = `kbNoKey`).
- **`HelpCtxRange::{All, OneOf(Vec<HelpCtx>)}`** — **THE one real deviation** (a D1
  corollary). C++ `TStatusDef(min, max, items)` selects its items by a **numeric**
  help-context range `[min,max]`; our `HelpCtx` (D1) is a namespaced `&'static str`
  with **no ordering**, so contiguous integer ranges become an explicit membership
  set. `All` = the universal `[0,0xFFFF]` def every real app uses; `OneOf(set)` = the
  rare context-split (tvdemo `[0,50]`/`[50,0xffff]`). `find_items` = first-match walk;
  multi-def selection is faithful-but-unexercised this row (nothing sets a non-default
  help ctx yet) — supported in the data model + unit-tested via `set_help_ctx`.
- **Command graying via a cached `CommandSet` on the VIEW, NOT a field on
  `StatusItem`** (the advisor-flagged crux; the menu precedent misleads here).
  `TMenuItem` has a real `disabled` field the menu broker mutates; **`TStatusItem`
  has none** — C++ `drawSelect` calls `commandEnabled(T->command)` **live**. So the
  view caches one `Option<CommandSet>` snapshot (refreshed by the **same**
  `update_menu_commands` broker hook + the `cmCommandSetChanged`→`request_update_menu`
  broadcast arm, reused verbatim from the menu); `draw` tests `cmd_set.has(cmd)`,
  treating an unset cache as all-enabled (the same startup-regray gap menus carry).
  Status items are **flat** — the hook is non-recursive (unlike the menu's tree walk).
- **`draw` = `drawSelect(0)`** (faithful `tstatusl.cpp:62`): bg fill in `cNormal`,
  per-item leading/trailing space + `put_cstr`, the `i+l < size.x` clip, the 2×2
  color matrix (reuses the menu's `(enabled, selected)` shape via a `StatusColors`
  helper that mirrors `MenuColors` but reads the 6 new `Status*` roles), and the
  hint tail (`if i < size.x-2` → `│ ` separator (U+2502) + clipped hint via the
  `hint` closure). **Themes only, no palettes** — colors resolve from `Theme` via
  `Role` (`getPalette`/`getColor`/`cpStatusLine` are NOT ported; the C++ palette
  bytes only seeded the provisional theme colors, `TODO(row 34 gray theming)`).
- **`hint()` virtual → `Box<dyn Fn(HelpCtx) -> Option<String>>` closure** on the view
  (default `|_| None`; `with_hint`/`set_hint` setters) — the idiomatic port of the
  overridable C++ `virtual hint`.
- **`handle_event`:** the **mouse** arm (single-shot, faithful to the C++
  press-and-hold deferral): `item_mouse_is_in` hit-test (`mouse.y!=0→None`; `[i,k)`
  accumulation skipping textless items) → enabled-check → `ctx.post(cmd)` →
  unconditional `ev.clear()`. The **broadcast** arm: `cmCommandSetChanged` →
  `ctx.request_update_menu(self_id)` (the menu pattern).
- **Deferred + breadcrumbed (NOT stubbed):** the **keyDown global-accelerator arm**
  (deferred to the Program-wiring step — its "transform the event into evCommand
  in place and `return` WITHOUT clearing, so it propagates" semantics only make sense
  inside `getEvent`'s pre-routing; it must NOT be ported as `ctx.post`+clear, which
  double-handles); `TProgram` getEvent pre-routing + `idle()→update()`;
  `update()`/`TopView::getHelpCtx` auto-refresh (`find_items`/`set_help_ctx` ARE
  ported + tested; only the auto-trigger is deferred); the press-and-hold
  drag-highlight (`drawSelect(Some)` hover); streaming (D12); `disposeItems`/dtor (moot,
  owned `Vec`s).
- **Verification:** 25 status tests — 4 snapshots (normal+disabled, hint tail,
  narrow overflow-drop, textless-item-no-width) + bite-checked units for
  `find_items` (first-match order bite), `item_mouse_is_in` (textless-neighbour
  unaffected; col-out-of-range→None), the empty-hint skip, and both broker ends
  (broadcast arm queues `Deferred::UpdateMenu(self_id)`; the hook caches + grays).
  A full `pump_once` chain test was substituted with the two end-unit-tests (the
  `Program` test harness is `#[cfg(test)]`-private to the `program` module; the
  `Deferred::UpdateMenu → update_menu_commands` link is already covered there for
  menus) — QUALITY review judged the substitution acceptable.

## Prior session — menu MODAL layer Step-2 stage 3 (`TMenuPopup` 52) (FOUNDATION)
The **last modal piece** of the flattened `TMenuView::execute()` — standalone popup
menus — mapped onto the single `MenuSession` capture handler as three additive deltas
(no new seam). Brief: [`docs/briefs/row52-tmenupopup.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row52-tmenupopup.md)
(advisor-vetted) → Opus implementer → **full two-stage review** (SPEC then QUALITY,
fresh C++-adversarial Opus agents — both clean, no blockers) → 1 fix round →
integrate. `src/menu/menu_session.rs` + re-exports + 6 tests.

- **`put_click_event_on_exit: bool` on `MenuSession`** (THE thing that makes a popup
  not a box): the **bottom level's** `putClickEventOnExit` (`menus.h:222,229` default
  `True`; `TMenuPopup` `False`, `tmenupop.cpp:45`). Gates the single bottom-level
  exit-click re-post in `run()` (`if exit_click && self.put_click_event_on_exit`). A
  click outside a popup closes it WITHOUT re-posting; a bar still re-posts — the
  **mutual-break test pair** (`popup_click_outside_does_not_repost` vs
  `click_outside_closes_and_reposts`) proves the flag is wired, not a no-op. A
  **session-wide** flag is faithful: C++ `putEvent` is a single slot + the tail
  re-post is unconditional for any box (`parentMenu != 0`), so an exit-click in a
  deep submenu collapses to one click that rides up to the bottom frame, whose flag
  alone gates final delivery (SPEC-verified, incl. the popup→submenu→outside path).
- **`menu->deflt = 0`** (`tmenupop.cpp:51`): the popup level starts `current = None`
  AND `popup_menu` clears its **menu clone's** `default` — so the `evMouseUp`-on-margin
  arm (`menu.default.or(Some(0))`) picks the FIRST item, not a default; the box opens
  with no highlight. (A submenu opened *from* the popup keeps its own `default` — only
  the top popup zeroes, matching C++.)
- **`popup_menu(where_, menu, owner_size, ctx)` free fn + `auto_place_popup`** (faithful
  `popupMenu`/`autoPlacePopup`, `popupmnu.cpp`, via `menu_box_rect`): below-right
  placement (top-left `(p.x, p.y+1)`), desktop bottom-right clamp (`min(size, d)`),
  and the contains-`p` shift-up. Re-exported `tv::popup_menu`.
- **`end_session_with` reworked** from `skip(1)`/`first()` to a kind-keyed (`is_bar`)
  teardown loop — a popup's level 0 IS a box (must be closed), not a permanent bar
  (un-highlighted). Behaviorally identical for bar sessions (SPEC-confirmed).
- **`TMenuPopup::handleEvent` (getCtrlChar/hotKey) is MOOT and dropped** (breadcrumb,
  not stubbed): a popup is always `execView`'d, so `execute()` owns the loop and
  `handleEvent` never routes during its modal life; the accelerators are already
  covered by the flattened `step_default_key` (`find_item` on the active level +
  `hot_key(levels[0].menu)`, which for a popup IS its own tree). Only the **Ctrl+letter**
  variant is un-ported — `TODO(TMenuPopup Ctrl+letter accel)`. No persistent-insertion
  path exists in C++ (`popupMenu` is the sole ctor caller; its editor consumer is
  unported). Synchronous return value + `receiver: TGroup*` dropped (D9 async; `ctx.post`
  is the faithful `receiver->putEvent`).
- **Verification:** 6 discriminating, bite-checked tests — 3 `program.rs` `pump_once`
  (popup-opens-no-highlight, the click-outside-no-repost ANCHOR, select-command-posts),
  1 submenu-popup carry-up exit-click (the SPEC-flagged previously-only-reasoned path),
  2 `auto_place_popup` geometry units (below-right; bottom-edge shift-up). 551 lib green.

### NEXT — two modal-loop foundation seams (surfaced by row 57) + **msgbox 63** / **Batch C validators 58–62**
**The history cluster 54–57 is COMPLETE** (54/55/56 prior sessions; **57 THIS
session** — top git-table row). The view-triggered async-modal seam
(`Deferred::OpenHistory` + `Program::pending_modal` + `pump_and_drive` +
`exec_view` end_state save/restore + `ModalCompletion` flowback) is built and
reviewed. Two modal-loop foundation seams were surfaced/deferred and are the
natural next FOUNDATION work; msgbox 63 + Batch C are also available:

- **(1) The `ModalFrame` deliver-outside-to-modal seam** (row 56/57 deferred —
  un-defers the `HistoryWindow` outside-click `endModal(cmCancel)`). **NOT a
  `ModalFrame` return-value tweak** (confirmed this session): `ModalFrame::handle`
  has no `group` so it cannot deliver to the modal by id, and
  `program_handle_event` routes outside positional events **positionally to the
  desktop** (not by `current`). So the fix is a **delivery-path change in
  `program_handle_event`**: while a `ModalFrame` is the top capture, deliver
  positional events to the modal view by id (makeLocal to its bounds) so the
  modal's own routing + the `HistoryWindow` `mouseInView`-cancel override decide;
  verify a plain `Dialog` still IGNORES an outside click under that delivery (C++
  does — no child catches it). The `HistoryWindow::handle_event` outside-cancel
  breadcrumb (`TODO(row 57 modal-loop seam)`) is in place to un-defer.
- **(2) The general initial-modal-currency seam.** SPEC review found that
  `exec_view` opens a modal but never establishes its **internal** `current`
  (first selectable child), so EVERY dialog is keyboard-dead on open until a nav
  event — C++ gets this via `insertView→show→resetCurrent` as children land in a
  visible group. Row 57 worked around it **locally** for the history popup (a
  first-event `Window::select_child` in `HistoryWindow::handle_event`). The general
  fix is blocked on `Group::insert` taking no `ctx` (so it cannot `reset_current`
  at insert); breadcrumbed at `exec_view`'s `set_current(Some(id), Enter)` site.
  Needs its own SPEC pass (does C++ establish currency at construction or at
  execView?) + likely a new `View`-trait hook or a ctx-bearing insert path.
- **msgbox 63** — the co-consumer of the async-modal seam: it **ADDS a
  `ModalCompletion` variant** (`messageBox` → return/post the button command;
  `inputBox` → flow the input line's text back) and uses the row-56 production
  `Window::insert_child` for its `TStaticText`/`TButton`/`TInputLine` children. The
  seam is built — wiring msgbox is now mostly mechanical + its own completion arm.
- **Batch C concrete validators 58–62** (`tvalidat.cpp`) — the clean worktree
  parallel fan-out (see "Available parallel fan-out" below); **59 `TRangeValidator`
  is FOUNDATION-ish** (resolves the deferred `transfer` hook + the `cur_pos`
  re-clamp hazard; `FieldValue::Int` ready). **Available NOW**, independent of the
  modal-loop seams above.
- **`cmDosShell`** is still deferred — needs a backend terminal-suspend seam + SIGTSTP.

Other open follow-ons (lower priority / parallel):
- **idle→`statusLine->update()` help-ctx refresh** — still deferred; only worth doing
  when a **context-split `OneOf` `StatusDef`** lands (under a single `All` def it is
  inert). Would need a `View::get_help_ctx` method (+ a `tvision-macros/specs.rs`
  forwarder) + a `TopView` resolver (nearest `sfModal` view = the top capture's view,
  else the root group).
- **status-line press-and-hold drag-highlight** (`drawSelect(Some)` hover) —
  `TODO(row 31, D9)`.
- **`program_handle_event` modal-isolation** breadcrumb (suppress program-level
  interception while a `MenuSession`/modal is active) and the `ModalFrame`/`DragCapture`
  "(0,0)-desktop absolute-coords" caveats (the bar now shifts the desktop down by 1 —
  re-examine when a dialog must position relative to the desktop, not the screen).

Batch C concrete validators 58–62 (`tvalidat.cpp`) remain an available parallel fan-out.

## Prior session — menu MODAL layer Step-2 stage 2 (MOUSE nav) (FOUNDATION)
The **mouse** arms of the flattened `TMenuView::execute()`, layered onto the stage-1
`MenuSession`. Brief: [`docs/briefs/row50-52-menu-modal-mouse.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-52-menu-modal-mouse.md)
(advisor-vetted) → Opus implementer → **full two-stage review** (SPEC then QUALITY,
fresh C++-adversarial Opus agents) → 2 fix rounds → integrate. `src/menu/menu_session.rs`
+ `src/menu/menu_view.rs` + 10 tests in `src/app/program.rs`.

- **One shared `run()` loop.** Stage-1's `handle_key` is refactored into a single
  `run(ev, ctx)` that dispatches keyboard/mouse/cmMenu **steps** into the *same*
  post-switch tail (set-current → `last_target_item` reset → open-gate →
  doReturn-pop/re-apply). Keyboard behaviour is preserved (verified: all stage-1
  tests still green); the mouse arms reuse the cross-level re-apply unchanged.
- **Coordinate model:** positions + `MenuLevel::bounds` are **root-frame** (the
  session sees events pre-translation via the capture stack — no `makeLocal`);
  `item_rect_global = item_rect_local` shifted by `level.bounds.a`. New helpers
  `track_mouse` (overwrites `current` to the hit item or `None`; sets `mouse_active`
  monotonically), `mouse_in_view`/`mouse_in_owner` (parent's current-item rect) /
  `mouse_in_menus` (any **parent** level's bounds).
- **Per-level loop-locals added** to `MenuLevel`: `last_target_item`, `mouse_active`,
  `first_event` (each C++ `execute()` per-frame, re-init per level — never leak).
- **THE crux** (`tmnuview.cpp:383-386`, advisor-flagged): `lastTargetItem = current`
  **+ `menu->deflt = current` + `firstEvent = False`** are set on the **parent** at
  the **child-pop** point (the flattened "execView returns") — this is what makes
  **clicking an already-open title CLOSE it** (re-applied click hits the bar:
  `autoSelect = !current || lastTargetItem != current` → `File==File` → False → gate
  shut). Bite-tested.
- **Open-gate re-applies the carried event into a freshly-opened child only for
  `MouseDown`/`MouseMove`** (C++ `putEvent` gating `e.what & (evMouseDown|evMouseMove)`);
  keyboard + `MouseUp` open-and-wait. A box **never** sets `autoSelect` (only the bar
  does, `:273`) — so a nested submenu opens only via `MouseUp`-`doSelect`, not
  drag-hover (a SPEC-confirmed brief correction).
- **Click-outside-closes + re-post** happens at the **bar** level only
  (`putClickEventOnExit`, `:217`); a box's exit-click re-applies up the stack and the
  bar does the final `ctx.put_event` so the view under the click recovers focus.
- **`evMouseDown` bar activation** (`do_a_select`, `:505`) in `menu_view::handle_event`
  (replaces the stage-1 breadcrumb): gated `size.y==1 && bounds.contains(position)` →
  `menu_session::activate_mouse` pushes the bar-only session and **re-posts the click**
  (no pre-open); the session's evMouseDown arm + open-gate then open the clicked title.
- **`cmMenu` routed through `run()`** (SPEC fix, `:343-350`): a box-level cmMenu now
  `doReturn`s (closes the box, re-applies up) and the bar resets `autoSelect`/
  `last_target_item` + stays open — was previously a top-only reset that left a box open.
- **Two-stage review earned its keep:** SPEC **independently confirmed 3 brief-was-wrong
  deviations correct vs the C++** (bar-click leaves the dropdown *unhighlighted* until
  the mouse enters it; a box never auto-selects; a test-bite re-target) and caught the
  cmMenu-closes-box divergence (fixed). QUALITY found no blockers and closed **two
  uncovered `evMouseUp` arms** (release-on-parent-title→reset-to-default;
  release-outside-after-activating→close) with bite-checked tests + fixed an inaccurate
  `track_mouse` comment.
- **Verification:** 10 discriminating, bite-checked `pump_once` tests (click-opens-box,
  click-open-title-closes [the crux], drag-to-neighbour-reopens, click-outside-closes+
  reposts, drag-into-submenu, mouseUp-on-command-posts, mouseUp-on-box-margin-resets,
  cmMenu-from-nested-box-closes-to-bar, mouseUp-on-parent-title-resets, mouseUp-outside-
  after-activating-closes). 545 lib tests green.

## Prior session — menu MODAL layer Step-2 stage 1 (keyboard nav) (FOUNDATION)
The interactive `TMenuView::execute()` (`tmnuview.cpp:179`), flattened onto our single
event loop (D9). Brief: [`docs/briefs/row50-52-menu-modal.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-52-menu-modal.md)
(advisor-vetted **twice** — architecture then concrete mechanism) → Opus implementer →
**full two-stage review** (SPEC then QUALITY, fresh C++-adversarial Opus agents) → two
fix rounds → integrate. `src/menu/menu_session.rs`.

- **THE ARCHITECTURE DECISION (settled, do not relitigate).** C++ `execute()` is a STACK
  of nested `execView` modal loops (opening a submenu *recurses* `owner->execView`). Two
  mappings were weighed and **the advisor + C++ evidence killed the re-entrant one:**
  - **REJECTED — re-entrant `exec_view` per level.** The guide's "`exec_view` = the
    `TGroup::execute` shape" ratifies `exec_view`/`OpenModal` for **`TGroup::execute`**
    (`tgroup.cpp:173`, the *dialog* loop). `execView` calls `p->execute()` **virtually**
    (`tgroup.cpp:205`); for a menu, `p` runs the **overridden `TMenuView::execute`**
    (`menus.h:152`) — a *different function*. So the guide reserved `OpenModal` for "a
    menu *command* launches a dialog," and **never licensed it for menu nav.** Also
    `ModalFrame` *swallows* outside clicks (menus must *close* on them) and per-level
    bounds-gating can't express cross-level mouse (`mouseInMenus`/`mouseInOwner` walk the
    whole `parentMenu` chain). **My initial lean conflated the two `execute()`s — caught
    by the advisor.**
  - **CHOSEN — one `MenuSession` capture handler** owning the WHOLE open stack (bar + every
    open box), the flattened `execute()`. Clean Architecture A: while active it **consumes
    every event**; bar + boxes are **display-only** (never focused). `OpenModal`/`exec_view`
    stays reserved for the menu-command→dialog case (msgbox / Batch E).
- **The flattening insight (the "beautiful" part).** C++'s `doReturn` pops a level and
  **re-posts the triggering event** to the parent's `getEvent` **unless that arm cleared
  it** (`tmnuview.cpp:401-405`). Flattened: `MenuSession::handle_key` is a **re-apply-across-
  levels loop** — on a non-cleared `doReturn`, pop the level and re-deliver the SAME event
  to the new top. This one mechanism produces all the cross-level keyboard behaviors:
  one-Esc-closes-the-whole-menu (from a 1st-level box), Esc-closes-one-level (from a 2nd+
  box), and Left/Right unwinding the stack to the bar + walking to the neighbour title.
- **State:** `MenuSession { levels: Vec<MenuLevel>, owner_size }`; each `MenuLevel {
  view_id, menu (clone-at-open), current, bounds, is_bar, auto_select }`. **Clone-at-open
  is FAITHFUL** (execute() has no `evBroadcast` case → `disabled` frozen for the menu's
  lifetime; the session **swallows broadcasts** while active). **`auto_select` is a keyboard
  concern** (not mouse-only): set on bar kbDown/kbEnter/alt-activation, reset by `cmMenu`;
  it drives the Left/Right title-walk re-open. Bounds cached at open (a box never moves →
  no `sync_gate_bounds`); shaped for stage-2 mouse.
- **New seams (all additive — "a new deferred capability ADDS A VARIANT"):**
  `Deferred::OpenMenuBox { id, menu, bounds }` (the session **pre-mints** the id via
  `ViewId::next`, the pump `Group::insert_with_id`s the box into the root group, **no focus
  move**) + `Deferred::SetMenuCurrent(id, Option<usize>)` (write-only highlight cache, via
  the new defaulted `View::set_menu_current` trait hook — no downcast, mirrors
  `update_menu_commands`; forwarder added to `tvision-macros/specs.rs`) + `ctx.put_event`
  (raw-event sibling of `post`, ports `putEvent`).
- **Activation** (replaces the row-49 `_ => {}` breadcrumb in `menu_view::handle_event`):
  bar `cmMenu`/kbF10 → highlight the default title, **no box** (F10 waits — the `autoSelect=
  False` path); bar **alt-shortcut** → open the matched submenu's box (or post directly if
  it's a top-level command). Pushes the session + the first `OpenMenuBox` in the **same
  deferred batch** (no dead first event).
- **Two-stage review earned its keep (twice over):** SPEC caught 3 keyboard-faithfulness
  blockers (F10-wrongly-opens-box; one-Esc should close the whole menu; Left/Right should
  walk+reopen → `autoSelect` is keyboard, not mouse). QUALITY caught a **real bug SPEC
  missed** — a hotKey accelerator pressed while a box is open was *silently dropped* instead
  of closing the menu + firing the command (the unreachable "defensive" branch was the
  tell) — plus a dead/semantically-broken `first_event` field and 3 clean wins (shared
  `matching_item` helper, stale doc, untested `put_event` path).
- **Verification:** 11 discriminating, bite-checked `pump_once` integration tests
  (F10-no-box, arrow-move, submenu-recurse, command-post+close, Esc 1st-vs-2nd-level
  asymmetry, Right-walk-reopen, F10-then-Right-no-box, alt-shortcut-at-matched, hotKey-
  accelerator-closes-whole-menu, foreign-command-close+repost). 535 lib tests green.

## Prior session — Rows 50/51 `TMenuBar`/`TMenuBox` DRAW/DATA layer (FOUNDATION)
The **draw/data slice** of the menu views — drawing + geometry + the polymorphism
seam, **no modal loop**. Brief: [`docs/briefs/row50-51-menu-draw.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-51-menu-draw.md)
(advisor-vetted scope split) → Opus implementer → **full two-stage review** (spec
then quality, fresh C++-adversarial Opus agents, both PASS) → 6 polish items (M1
`MenuColors` unification + M2/N1 clarity + T1/T2/T4 edge-case tests) → integrate.

- **The scope split (advisor-confirmed, overrides the old "50/51/52 land together"
  framing):** `draw`/`getItemRect`/`getRect` read only `menu` + `current` — never the
  modal state — so the draw layer is a clean, snapshot-testable slice. The modal
  `execute()` loop, `TMenuPopup`, navigation, and the D9 async-modal path are a
  **separate Step-2 design session** (see NEXT). Landing tested draw first de-risks
  the modal work (verified substrate to navigate) — the HANDOVER itself conceded this
  ordering ("each menu view needs getItemRect + draw *so* execute()'s nav is testable").
- **The `MenuView` trait** (`src/menu/menu_view.rs`) — row 49's "no trait yet" decision
  **flips** here: `get_item_rect`/`draw` ARE the overridable virtuals (bar ≠ box). Mirrors
  the row-28 `ListViewer` shape: trait `MenuView: View` with `mv()/mv_mut()` accessors +
  defaulted `get_item_rect(index) -> Rect` (base = empty rect, C++ `TRect(0,0,0,0)`); the
  passive `hot_key`/`update_menu_commands`/`handle_event` stay as the row-49 **free fns**.
  `mv()/mv_mut()` are unused now (the Step-2 polymorphism seam; reachable as pub-API trait
  items so no `dead_code`).
- **`MenuViewState.current: Option<usize>`** added (index into `menu.items`; `None` == C++
  `current == 0`; consistent with `Menu::default`). Verified against every Step-2
  `execute()` mutation (it fits all). **`parentMenu` still deferred** — draw/getItemRect
  never read it; only the Step-2 modal-nav methods do.
- **`TMenuBar`** (`src/menu/menu_bar.rs`): `draw` (`tmenubar.cpp:48` — bg fill + left-to-right
  items, the `if x+l<size.x` clip with `x += l+2` advancing even when clipped, the 4-color
  matrix), `get_item_rect` (horizontal accumulator, separators consume no x), ctor sets
  `gfGrowHiX` + `ofPreProcess`. **`handle_event` delegates to the row-49 passive
  `menu_view::handle_event`** (C++ `TMenuBar::handleEvent` *is* `TMenuView::handleEvent`,
  not overridden) — so row 49 finally has a concrete consumer.
- **`TMenuBox`** (`src/menu/menu_box.rs`): the `menu_box_rect` sizing helper (`getRect`,
  `tmenubox.cpp:25`), `frame_line` (the `frameChars` table decoded to single-line box glyphs
  from `Glyphs` — `frame_tl/tr/bl/br/h/v/tee_l/tee_r`; **note the faithful inset: cols 0 and
  size.x-1 are blank**), `draw` (`tmenubox.cpp:80` — top border → one row per item → bottom;
  per-line `color` fill split from `cNormal` borders; submenu `►` at size.x-4; param
  right-aligned at size.x-3-cstrlen), `get_item_rect` (y from 1). Ctor sets `sfShadow` +
  `ofPreProcess`. `handle_event` delegates to the passive layer (TMenuBox inherits it).
- **Theme:** 6 `Role` variants for the `cpMenuView` palette (`MenuNormal`/`…Shortcut`/
  `MenuSelected`/`…Shortcut`/`MenuDisabled`/`MenuSelectedDisabled`, idx 1/3/4/6/2/5).
  Disabled roles use one style for both lo+hi (no shortcut highlight when greyed).
  **Colours provisional** — `TODO(row 34 gray theming)`. Spec review resolved the faithful
  `cpAppColor` bytes (the row-34 realignment target): cNormal lo=`0x70` hi=`0x74`,
  cSelect lo=`0x20` hi=`0x24`, cNormDisabled=`0x78`, cSelDisabled=`0x28` (4 of 6 seeds are
  already exact; the 2 selected-fg seeds are brightened, realign with the other provisional
  Input/Scroller colours as one coherent pass).
- **`MenuColors`** (`menu_view.rs`, pub) — the 4 `(lo,hi)` pairs + `resolve(&DrawCtx)` +
  `.item(disabled, selected)`; shared by bar AND box (killed an 8-arg helper + its
  `#[allow(too_many_arguments)]`).
- **Verification:** 2 snapshots (bar highlight+disabled; box frame+highlight+disabled+
  separator+param+submenu) + a 3rd narrow-bar snapshot (clip-skip branch) + bite-checked
  unit tests for `get_item_rect` (bar+box) and `menu_box_rect` sizing (incl. a discriminating
  submenu-`+3` test) + empty-menu no-panic + a `handle_event` accelerator-delegation smoke.
  `cargo-insta` NOT installed → `.snap`s generated via `INSTA_UPDATE=always`, hand-verified,
  committed.

_(The Step-2 modal-layer plan that previously lived here was **executed this session** —
its capture-stack hypothesis was advisor-refined into the `MenuSession` architecture and
the re-apply loop; see **What landed THIS session** + the **NEXT** section above, and the
brief [`docs/briefs/row50-52-menu-modal.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-52-menu-modal.md).)_

## Prior session — Row 49 `TMenuView` passive layer (FOUNDATION)
The **passive (non-modal) layer** of `TMenuView` (`tmnuview.cpp`): command-graying
+ hotkey-accelerator dispatch, **no drawing / no modal loop** (those are 50–52).
`src/menu/menu_view.rs`. Built main-thread/Opus-orchestrated: advisor-vetted brief
(`docs/briefs/row49-tmenuview.md`) → Opus implementer → **full two-stage review**
(spec then quality, fresh C++-adversarial Opus agents, both PASS) → one MINOR
doc-link fix → integrate. **Scope was deliberately split** (advisor-confirmed): the
interactive `execute()` modal loop maps to the *unbuilt* D9 view-triggered
async-modal path and lands with the drawing subclasses.

- **Command-graying = a BROKER, NOT a `Context` read-accessor** (this **overturned**
  the prior HANDOVER note that said "add a read accessor on `Context`"). The command
  set lives on `Program`; the pump's apply-phase `Context` is alive across a loop
  whose `EnableCommand`/`DisableCommand` arms mutate `command_set` (`&mut`), so a
  `&CommandSet` on `Context` would alias that borrow (+ would add a `Context::new`
  param at every call site). Instead: new **`Deferred::UpdateMenu(ViewId)`** +
  **`Context::request_update_menu`** + defaulted **`View::update_menu_commands(&mut
  self, &CommandSet)`** (no-op default), applied in the pump where `group` and
  `command_set` are **disjoint destructured fields** (no `ctx` needed). The exact
  `Deferred::SyncListViewer` + `View::apply_list_scroll` precedent — *a new deferred
  capability ADDS A VARIANT*. Delegate forwarder added to `tvision-macros/specs.rs`
  + the `delegate_view` spy test (count 21→22).
- **`updateMenu` ported** as `menu_view::update_menu_commands(&mut Menu, &CommandSet)`:
  recurse submenus, `disabled = !cs.has(command)` on command items only (never a
  submenu's own flag), skip separators. The C++ `Boolean` return is **dropped** (D8
  whole-tree redraw makes `if updateMenu drawView` moot; the guarded write collapses
  to an unconditional flip).
- **`hotKey`/`findHotKey` ported** as `menu_view::hot_key(&Menu, KeyEvent) ->
  Option<Command>`: depth-first, skip separators, recurse submenus **regardless of
  the submenu's own `disabled`** (C++ `!disabled` guard is only on the command
  branch), match a command item iff `!disabled && key_code == Some(key)`. The passive
  `evKeyDown` handler posts the matched command. **The C++ `commandEnabled(p->command)`
  re-check is dropped** — safe because (a) the cached `disabled` is kept current by
  the broker and `hot_key` already filters it, and (b) the pump's `drop_disabled`
  boundary filter drops a stale-enabled post; only a one-idle-cycle staleness window
  remains (documented).
- **evBroadcast mask is MOOT** — `Group::handle_event` fans broadcasts to **every**
  child unconditionally (test `broadcast_reaches_all_children_including_disabled`), so
  the C++ `eventMask |= evBroadcast` opt-in needs no port; no gate added.
- **`MenuViewState { state, menu }`** is the embed target for 50/51. **No `MenuView`
  trait yet** and **`current`/`parentMenu` omitted** (omit-until-consumer: only
  `execute()`/`trackMouse`/`getHelpCtx` use them — added with the modal layer at
  50–52). Free functions, not a trait, since the passive layer dispatches into no
  overridable virtual.
- **Deferred + breadcrumbed (NOT stubbed):** `execute()` (the nested modal loop →
  D9 `OpenModal`), `trackMouse`/`trackKey`/`nextItem`/`prevItem` (modal nav),
  `findItem`/`findAltShortcut`, `do_a_select`/`newSubView`/`mouseInOwner`/
  `mouseInMenus`/`topMenu`, `getItemRect`/`draw`/`getPalette` (`cpMenuView`),
  `getHelpCtx`, streaming (D12). The activation branches of `handle_event`
  (`evMouseDown`, `cmMenu`, alt-shortcut) are breadcrumbed (leave the event live).
- **Verification (no snapshot — nothing draws):** 8 unit tests on `hot_key` (submenu
  recursion, disabled-skip bite, separator/no-key, submenu-own-key-no-match) +
  `update_menu_commands` (recursive regray, negated-predicate bite, submenu-flag
  untouched); **2 integration tests** through real `pump_once` — a `#[cfg(test)]
  MenuProbe` (FakeList precedent) proving the broker end-to-end (enable→regray→enabled,
  disable→`cmCommandSetChanged`→request→apply→disabled, bite-checked) + the
  accelerator-post path (enabled posts, regrayed-disabled posts nothing).

_(The Step-2 modal-layer plan that previously lived here is now the **NEXT** section
above — updated with the capture-stack-not-`OpenModal` framing + the carried
initial-regray gap.)_

## Prior session — Row 46 `TMenu` data tree (FOUNDATION)
First Phase-4 row: the **menu data tree** (`TMenuItem`/`TSubMenu`/`TMenu`,
`menus.h`/`menu.cpp`) — pure data + a builder, **no `View`** (that's row 49).
`src/menu/mod.rs`, wired into `lib.rs` (`pub use menu::{Menu, MenuBuilder,
MenuItem}`). Built main-thread/Opus-orchestrated: brief
(`docs/briefs/row46-menu-data-tree.md`, advisor-vetted design) → Opus implementer
→ **full two-stage review** (spec then quality, fresh C++-adversarial Opus agents,
both PASS) → doc-only fixes → integrate.

- **Data model = a 3-variant enum** (`MenuItem::{Separator, Command{…},
  SubMenu{…}}`), the type-safe translation of the C++ `union { param; subMenu }`
  discriminated by `name==0`⇒separator / `command==0`⇒submenu / else command.
  Shared fields (`name`/`key_code`/`help_ctx`/`disabled`) read via or-patterns;
  **no speculative common sub-struct** (advisor: add it later iff 49–52 want it).
  `MenuItem::disabled_mut() -> Option<&mut bool>` (None for `Separator`) for the
  row-49 command-graying loop.
- **`Menu { items: Vec<MenuItem>, default: Option<usize> }`** — C++ linked list
  `next` → `Vec`; `deflt` pointer → an **index**. The builder sets `default =
  Some(0)` on first push (C++ `TMenu(itemList)` head, no separator-skip), `None`
  when empty; both fields are `pub` and the two-arg C++ `TMenu(itemList, deflt)`
  allows a non-head default, so `default` is documented as *any valid index*.
- **`key_code: Option<KeyEvent>`** (None == C++ `kbNoKey`, faithful to the
  decomposed key model = absence of a key event); **`param: Option<String>`**
  (None == C++ `param==0`; empty `""` → `None`).
- **Builder replaces C++ `operator+`** (`MenuBuilder`: `.separator()`,
  `.command(name,cmd)`, `.command_key(name,cmd,key,param)`,
  `.submenu(name,key,|m| …)` closure-nested, `.item(MenuItem)` raw escape hatch).
  Local `fn alt(char) -> KeyEvent` convenience (mirrors `kbAltX`; `key.rs`
  untouched).
- **Verification is NOT a snapshot** (pure data, renders nothing): the lead test
  builds the canonical File/Window menu via the builder and `assert_eq!`s it
  node-for-node against a hand-built literal tree (a *different* code path, so a
  builder bug can't pass silently) + 5 edge-case tests. **6 tests, all pass.**
- **Scope fenced (FOUNDATION-creep guard):** no `View`/draw/event/`execute`/
  `findItem`/`hotKey`/`getItemRect`/streaming — all rows 49–52.

## Prior session — Row 32 `TApplication` (`3e6645f`, MECHANICAL)
The thin D2 embed wrapper over `Program` (row 31): `Application { program: Program }`,
the type a real app constructs. **Genuinely thin by dependency order**
(advisor-confirmed) — all of `TApplication`'s substance is deferred, so the row is
the type + one real body + faithful breadcrumbs, deliberately NOT padded. Built
main-thread/Opus-orchestrated: tight brief (`docs/briefs/row32-tapplication.md`) →
Sonnet implementer (in a `/scratch` worktree) → spec review (fresh C++-adversarial
agent) → fixes → integrate.

- **`Application`** forwards `run`/`pump_once`/`exec_view`/`desktop`/`end_modal`/
  `end_state`/`{enable,disable,command_enabled}_command` + `program()`/`program_mut()`
  escape hatches — hand-written one-liners. **(Note: `#[delegate]` does NOT apply
  here** even though it later landed and was adopted everywhere — that macro
  generates the `View`-trait forwarding impl for D2 embeds; `Application` forwards
  `Program`'s *inherent* loop methods, not the `View` trait, so it stays
  hand-written. It is correct as-is.)
- **`get_tile_rect()` is the one real body** → new **`Program::get_tile_rect`**
  (the desktop child's extent = `deskTop->getExtent()`, local-origin `(0,0,w,h)`,
  `None` if no desktop; `&mut self` because `Group::find_mut` is `&mut`). Placed on
  `Program` (not `Application`) because `Application` can't reach the private `group`,
  and the future command handler — also in `Program` — reuses it.
- **Deferred (NO dead stubs — omit-until-consumer, the row-35/48 rule):**
  `tile`/`cascade` (need `Desktop::tile`/`cascade` geometry [`mostEqualDivisors`/
  `iSqr`/`calcTileRect`/`dividerLoc`/`doCascade`, `tdesktop.cpp`] + a menu to emit
  cmTile/cmCascade + a way to test → Phase 4); `dosShell`/`suspend`/`resume` (need a
  backend terminal-suspend seam + SIGTSTP); `initHistory`/`doneHistory` (history
  subsystem unported); `TAppInit` subsystem init **dropped** (subsumed by the
  `Backend`/`Renderer` construction path).
- **Command handling breadcrumbed, not wired:** `TApplication::handleEvent`'s
  cmTile/cmCascade/cmDosShell are **program-level** → a TODO in `program_handle_event`
  **after** `group.handle_event` (faithful: C++ runs `TProgram::handleEvent` first),
  beside the QUIT catch. Blocked on the deferred bodies. The consts
  `Command::{TILE,CASCADE,DOS_SHELL}` already exist + are enabled in
  `default_command_set`, but **nothing emits them yet (no menus)** — Phase 4 menus
  are the first emitters; when they land, wire this breadcrumb + build the desktop
  geometry together (trigger + body + test in one go).
- **Review caught + fixed a BLOCKER:** the implementer first added empty
  `tile`/`cascade`/`dos_shell` methods on `Application` — dead stubs (the planned
  handler is in `program_handle_event`, which can't reach `Application`); deleted,
  deferral kept in docs + the breadcrumb. Plus 2 MINORs fixed: breadcrumb moved
  post-dispatch; the `get_tile_rect` test made discriminating (inset 80×20 desktop on
  an 80×25 backend pins desktop-extent vs screen-extent — a screen-rect impl fails it).

## Also landed — the `#[delegate]` proc-macro (`47894f0`…`66ab55f`)
The D2 embed-and-delegate pattern (`Wrapper { inner: Inner }` re-implementing the
whole `View` trait by forwarding to `inner`) was hand-written boilerplate in every
wrapper (Dialog→Window, the cluster family, etc.). It is now a proc-macro:
**`#[delegate(to = <field>)]`** in the new **`tvision-macros`** crate (a workspace
member; the repo root is now a Cargo workspace `["tvision-macros"]`). Applied to a
struct, it generates the `View`-trait forwarding `impl` to the named field.

- **Adopted codebase-wide**, replacing the hand-rolled forwards and the
  `cluster_wrapper!` macro: `cluster` (`2a715a0`), `Window` (`c357c3a`, `to=group`),
  `Dialog` (`e4eaad3`, `to=window`), `ParamText` + `Label` (`be70841`), `Desktop`
  (`7e90907`, `to=group`), and the `hello` example's `AboutDialog` (`415edb8`,
  `to=dialog`).
- **Spec + test:** a "full `View` forwarder spec" with a behavioral spy test
  (`4d92646`) → new integration test **`tests/delegate_view.rs`** (the +2 in the
  build-state count); code-review fixes for docs/diagnostics/drift-signposts
  (`375ef03`); a design note + a CLAUDE.md convention (`30cfe1f`).
- **Implication for future D2 wrappers:** prefer `#[delegate(to = inner)]` over
  hand-writing the `View` forwards. It applies when the wrapper forwards the **`View`
  trait** to an embedded `View` field; it does NOT apply to inherent-method forwards
  (e.g. `Application`→`Program` loop methods). When you override a method (the
  wrapper's own `handle_event`/`valid`), keep that method and let the macro forward
  the rest — check the macro's drift-signpost docs for the override pattern.

### Prior session — Row 48 `TListBox` (`fc66637`, MECHANICAL)
The first **concrete** `TListViewer`, proving the row-28 trait seam end to end.
Built main-thread/Opus-orchestrated: tight brief
(`docs/briefs/row48-tlistbox.md`) → Sonnet implementer → full two-stage review
(SPEC then QUALITY, fresh C++-adversarial Opus agents) → integrate.

`ListBox { lv: ListViewerState, items: Vec<String> }` (`src/widgets/list_box.rs`)
reuses **all** of `TListViewer`'s draw/event/nav verbatim via the `ListViewer`
trait, overriding **only `get_text`** (`items.get(item as usize).cloned().
unwrap_or_default()` — collapses the C++ `items==0→EOS` + OOB cases, panic-free);
`is_selected`/`select_item` **inherit the base** (C++ overrides neither). `impl
View` delegates `draw`/`handle_event`/`set_state`/`cursor_request`/
`apply_list_scroll`/`as_any_mut` to the `list_viewer::*` free fns (the `FakeList`
template). Wired into `widgets/mod.rs` + `lib.rs`.

- **D10 value protocol — first consumer beyond `TInputLine`:** `value() →
  FieldValue::Int(focused)` (the `getData` selection half; the collection is
  config `new_list` manages, NOT part of the transferable value — no `List`
  variant, `FieldValue` grows per consumer).
- **`set_value` DEFERRED** (advisor-confirmed): the **`Context`-free**
  `View::set_value` signature can't republish the v-bar (C++ `setData` =
  `newList`+`focusItem`, both need a `Context` in our model), so a partial would
  leave the scroll thumb desynced after a scatter. Lands with the **dialog
  gather/scatter** consumer (inputBox/Batch E), which must itself solve threading
  a `Context` into scatter. `TODO(set_value: dialog gather/scatter)`.
- **Population is post-insert** (the ctor has no `Context`): `new_list(items,
  ctx)` (`set_range` + `focus_item(0)` iff `range>0`) **plus**
  `list_viewer::update_steps(ctx)` for the page/arrow steps — miss either and the
  thumb starts unsynced. Documented on the type.
- **Dropped:** `dataSize`/`TListBoxRec` (→ typed value), streaming (D12),
  `drawView` (D8). The dialog gather/scatter group-walk stays deferred (no
  consumer yet).
- **Process catch — out-of-scope creep reverted:** the implementer also added an
  exported `delegate_view_rest!` macro to `src/view/view.rs` + refactored
  `examples/hello.rs` to use it — unrelated to row 48, unreviewed (both review
  agents were scoped to `list_box.rs`), and touching a FOUNDATION file. Reverted
  before commit. The macro is a genuinely useful D2-embed helper; if wanted, do it
  deliberately as its own reviewed change.

### Prior session — Row 28 `TListViewer` (`c1ad789`, FOUNDATION)
`TListViewer` (base for `TListBox` 48, history, color/file lists) drives two
sibling scrollbars like `TScroller` but **diverges structurally in two ways** the
"reuse the broker verbatim" line glossed over — both confirmed with the advisor
*before* building. Built main-thread/Opus: brief → Opus implementer → two-stage
review (SPEC then QUALITY, fresh C++-adversarial agents) → fixes. Brief:
`docs/briefs/row28-tlistviewer.md`.

**Divergence 1 — `ListViewer` is a TRAIT, not a concrete struct (the `Validator`
pattern, NOT the `Scroller` embed shape).** `TListBox` reuses `TListViewer::draw`
while *overriding* the virtuals `getText`/`isSelected`; a D2 concrete-embed base
physically cannot dispatch back into the embedder's `getText` from the base's own
`draw`. So:
- `ListViewer: View` trait — `lv()`/`lv_mut() -> &ListViewerState` accessor +
  defaulted `get_text`/`is_selected`/`select_item`.
- `ListViewerState` struct holds the data members (`state: ViewState`, `num_cols`,
  `top_item`, `focused`, `range`, `indent`, `h_scroll_bar`/`v_scroll_bar` ids).
- The shared draw/event/nav logic lives as **free functions generic over
  `<L: ListViewer + ?Sized>`** (`list_viewer::draw`/`handle_event`/`focus_item`/
  `focus_item_num`/`set_range`/`update_steps`/`apply_scroll`/`set_state`/
  `focused_cursor`), which a concrete widget's `View` impl calls.
- Object-safety: `ListViewer` is **not** object-safe (`get_text -> String`) — fine,
  it's only ever a generic bound; concrete widgets are still `Box<dyn View>`.
- A `#[cfg(test)] FakeList` (Vec-backed) is the first consumer (a real consumer for
  the draw/nav tests, NOT a dead stub). **Row-48 `TListBox` is the production one.**

**Divergence 2 — the read-sync WRITES BACK (the scroller never did).** C++
`focusItem → vScrollBar->setValue(item)`; in our model the read-sync issues a
deferred `ScrollBarSetParams{value}`. New mechanism, **scroller path untouched**:
- New defaulted-no-op **`View::apply_list_scroll(&mut self, h, v, ctx)`** + new
  **`Deferred::SyncListViewer{list,h,v}`** + a pump apply arm that calls the **trait
  method (NO downcast** — you can't cast `dyn View → dyn ListViewer`, unlike the
  scroller's `as_any_mut` downcast to a single concrete type).
- **TERMINATION (the centerpiece property):** the vbar→sync→setValue cycle
  terminates **only because `ScrollBar::set_params` is change-guarded**
  (`scrollbar.rs:219/224` — broadcasts `SCROLL_BAR_CHANGED` iff `old_value !=
  a_value`), so the write-back of the already-current value is a silent no-op.
  Proven by a discriminating termination test through real `pump_once` drains
  (6 passes asserting quiescence; bite-checked — removing the guard makes it spin).
- **`indent` cached** on `ListViewerState`: draw can't read the sibling hbar live,
  so the hbar `value` is cached and refreshed by the same sync (the hbar
  `cmScrollBarChanged` branch, C++ "just drawView", becomes "update the cache").

**Reused verbatim from row 27:** `Deferred::ScrollBarSetParams` (setRange +
ctor-setStep) and `SetVisible` (setState show/hide), `Broadcast{source}` as the
`source ∈ {h,v}` filter, `View::value() → FieldValue::Int`.
- **`setState`** uses the C++ **`active && visible` AND-condition** for show/hide
  (NOT the scroller's `active || selected` — a spec-review crosshair).
- **`cmScrollBarClicked` from an own bar → `select()`** → `ctx.request_focus(id)`
  (the row-41 `Deferred::FocusById` seam).
- **Theme reconciled** to the 5-entry cpListViewer palette (`Active/Inactive/
  Focused/Selected/Divider`) → roles `ListNormalActive`/`ListNormalInactive`/
  `ListFocused`/`ListSelected`/`ListDivider` (the old guessed `ListNormal`/
  `ListSelectedFocused` were unused; provisional colours, `TODO(window-scheme
  remap)`).
- **Deferred + breadcrumbed:** mouse press-and-hold/auto-scroll `do…while
  (mouseEvent)` loop (`TODO(row 31, D9)`; ship single-shot + double-click select);
  `changeBounds` step republish (`TODO(resize)` — **note the distinct formula**:
  C++ `changeBounds` uses vbar plain `size.y` + **both bars preserve arStep**,
  unlike the ctor's `update_steps`; do NOT call `update_steps` for resize —
  corrected in-doc after a spec catch); `showMarkers` + streaming dropped (D8/D12);
  scroller/listviewer read-sync unification noted optional/out-of-scope.

### Prior session — Row 27 `TScroller` (`543b2c8`, FOUNDATION)
Established THE cross-view scrollbar broker (pump brokers all scroller↔scrollbar
reads/writes at deferred-apply via `group.find_mut(id)` + `as_any_mut`/
`View::value()`; `Broadcast{source}` is the filter, value NOT stuffed into the
message). New `Deferred`: `SyncScrollerDelta` (read → `apply_delta`),
`ScrollBarSetParams` (write, per-field `Option`=preserve), `SetVisible`. New seams
`FieldValue::Int` + `ScrollBar::value()`. Dropped (D8) `drawLock`/`drawFlag`/
`checkDraw`/`drawView`. `Role::ScrollerSelected` + `changeBounds` resize-republish
deferred to `TEditor` 66. Brief: `docs/briefs/row27-tscroller.md`.

## What landed the PRIOR session (validator wave, `43e5c68`)
The full row-35→39 wave + the **D10 typed-value protocol**, built as one Opus
implementer + full two-stage review (SPEC then QUALITY, fresh C++-adversarial
agents). Brief: `docs/briefs/row35-39-validator-inputline.md`.

- **TValidator (35)** → `src/validate.rs`: object-safe abstract `Validator` trait
  (D2) — `is_valid_input(&self,&mut String,bool)` / `is_valid(&self,&str)` /
  `error` / `is_status_ok` (all defaults accept) + provided non-virtual
  `validate`. **`transfer` deliberately omitted** (PORT-ORDER row 35 lists it, but
  it has no overrider until TRangeValidator row 59 → would be a dead stub; the
  row-34 "no dead stubs" rule wins). `tv::Validator`.
- **D10 value protocol** → `src/data.rs`: **`FieldValue`** typed-transfer currency
  — one `Text(String)` variant, **grows per control** (Role/Glyphs convention;
  `Bits(u32)` for cluster + `Int` for range land when those wire their value).
  Defaulted **`View::value(&self)->Option<FieldValue>` / `set_value(&mut self,
  FieldValue)`** (the getData/setData successors). The dialog **gather/scatter
  group-walk is DEFERRED** to its first consumer (inputBox / Batch E) —
  breadcrumbed in `data.rs`.
- **TInputLine (39)** → `src/widgets/input_line.rs`: faithful `tinputli.cpp` port.
  Draw (scrolled `moveStr` + ◄/► arrows + selection redraw + cursor), full
  keyboard (nav / word-nav / edit / Ins-toggle / Shift-block-extend /
  printable-insert with the `maxLen && maxWidth && maxChars` guard / Ctrl-Y),
  single-shot mouse positioning **+ the faithful single edge-click scroll-by-one**,
  validator `save_state`/`restore_state`/`check_valid`, `valid(cmd)` (faithful
  return), `set_state`→`select_all`, `value`/`set_value`.
  **Key correction the implementer caught:** `first_pos` is a display **COLUMN**,
  not a byte offset (the brief mis-stated it; `cur_pos`/`sel_*`/`anchor` ARE byte
  offsets). All `data` indexing steps through grapheme helpers — **D13
  panic-safe** (multi-byte tests over `ä€中` BITE).
- **New seams:** `text::prev` (`TText::prev`), `DrawCtx::put_str_part` (`moveStr`'s
  `begin` column-skip), 3 theme roles `Input{Normal,Selected,Arrow}` (provisional
  gray, `TODO(row 34 gray theming)`) + 2 glyphs (◄ U+25C4 / ► U+25BA), `cmValid`,
  `State::cursor_ins`.
- **End-to-end veto test (`8ea87cb`, advisor-flagged):** the headline
  `InputLine::valid()` behavior — a modal must NOT close on OK while a child's
  validator rejects — lived only in isolated widget tests. The actual veto is in
  `exec_view`'s outer `while !valid(end_state)` loop. New integration test in
  `program.rs`: a `Dialog` + `InputLine` + `RejectAll` validator, driven through
  `exec_view` with pre-queued `[cmOK, cmCancel]`, asserts the result is **cmCancel**
  (cmOK vetoed, modal stayed open) + the `ModalFrame` popped. Bite-verified; **no
  bug in the veto path** (`exec_view` honors `valid()` correctly). The `[OK,
  CANCEL]` shape is deliberate — `[OK]` alone loops forever (a permanently-rejecting
  field can never close, which IS faithful). + a `#[cfg(test)] Dialog::insert_child`
  hook.

### Deferred + breadcrumbed in the validator wave (prior session; grep the TODOs)
- **clipboard** cmCut/cmCopy/cmPaste — no `Context` clipboard seam (backend has
  set/get_clipboard; not surfaced to views). `TODO(clipboard)` in `input_line.rs`.
- **command-graying** `updateCommands`/`canUpdateCommands` (enable/disable cmCut/
  Copy/Paste) — needs the `Context` command-set query that **TButton also
  deferred**. `TODO(button/inputline: command-set query …)`. **Menus (Phase 4)
  force this** — add a read-only command-set accessor to `Context` then.
- **mouse press-and-hold / drag-select loops** — `TODO(row 31, D9)`; single-shot
  positioning + the single edge-click scroll only.
- **`valid()`'s `select()` focus side-effect** — C++ focuses the invalid field
  before returning false; needs `&mut Context` + the **focus-by-ViewId** seam
  (`Deferred::FocusById` / `request_focus`, already built at row 41).
  `TODO(valid-select)`. The **return value is faithful** (gates modal OK).
- **validator `transfer` hook** — `TODO(row 59)` at both `value`/`set_value`
  sites; TRangeValidator will produce a typed non-`Text` value (→ `Int`).
- **`Validator::error`→msgbox** — `TODO(msgbox row 63)`.
- **`cur_pos` re-clamp hazard** — `TODO(row 59/62)`: a future *mutating* validator
  that SHRINKS `data` could leave `cur_pos` past EOS / mid-grapheme → D13 panic.
  Unreachable now (abstract validator never mutates); re-clamp when the first
  auto-fill validator (Range/PXPicture) lands.

## NEXT — follow PORT-ORDER in sequence

Lowest-numbered incomplete rows = the work. Next up:

### Phase-4 breadcrumb from Row 32 `TApplication` (`3e6645f`, done a prior session)
When menus emit cmTile/cmCascade/cmDosShell, the deferred bodies land
**together** — build
`Desktop::tile`/`cascade` geometry (`mostEqualDivisors`/`iSqr`/`calcTileRect`/
`dividerLoc`/`doCascade`, `tdesktop.cpp`) + wire the breadcrumb in
`program_handle_event` (after `group.handle_event`, beside the QUIT catch, calling
`desktop.tile/cascade(get_tile_rect())`) + test it with real tileable windows in
one change. `dosShell` separately needs a backend terminal-suspend seam + SIGTSTP.

### Phase 4 — the immediate next work, in PORT-ORDER order
**Menus 46/49/50/51/52 — the WHOLE menu modal layer (bar+box+popup, keyboard+mouse)
— DONE** (see the per-session sections above). The command-graying "Context
command-set query" was resolved for menus as the row-49 **`Deferred::UpdateMenu`
broker** (NOT a `Context` read-accessor — that earlier framing is obsolete; the
broker is the established pattern). Remaining Phase-4 work, in order:

- **Status line:** `TStatusItem`/`TStatusDef` (47) + `TStatusLine` draw/data slice
  (53) — **DONE** (see "What landed THIS session" above). Same broker pattern,
  cached on the view. Its interactive arms land with the Program wiring below.
- **Wire a real menu bar + status line into `Program`** — lets the
  `examples/hello.rs` demo grow a real menu bar + status line (and shifts the desktop
  down — revisit the `ModalFrame`/`DragCapture` "(0,0)-desktop absolute-coords"
  caveats then). First emitter of `cmTile`/`cmCascade`/`cmDosShell` → wire the row-32
  breadcrumb + build `Desktop::tile`/`cascade` geometry; close the carried
  initial-regray gap (initial `Deferred::UpdateMenu` on menu-bar insert).

### Available parallel fan-out (efficiency, not a competing direction) — Batch C: concrete validators (58–62, MECHANICAL)
Fully unblocked by `TValidator` (35); **fully parallel among themselves** → the
clean worktree fan-out cadence (Sonnet implementers, `isolation:"worktree"`,
orchestrator integrates + pre-seeds any shared files). These are PORT-ORDER's
"Parallelizable batches" — run them concurrently whenever convenient; they don't
displace the in-sequence FOUNDATION work above. C++ all in `tvalidat.cpp`:
- **58 `TFilterValidator`** (char allow-list), **59 `TRangeValidator`** (int range;
  **resolves the deferred `transfer` hook + the `cur_pos` re-clamp hazard** above —
  and now has `FieldValue::Int` ready [added by row 27]; so this one is
  FOUNDATION-ish, do it carefully),
  **60 `TLookupValidator`** (abstract lookup), **61 `TStringLookupValidator`**,
  **62 `TPXPictureValidator`** (Paradox picture-mask state machine — the big one;
  `picture()`/`process()`/`scan()`/`group()`/`iteration()` — sets `status=vsSyntax`,
  which is what `is_status_ok()` and TInputLine `valid(cmValid)` already consult).
Each validator's `is_valid_input` may **mutate** `s` (auto-fill) — that's the
trigger for the TInputLine `cur_pos` re-clamp `TODO(row 59/62)`.

### Then `msgbox` (63) + Batch E fan out
`messageBox`/`inputBox` (`msgbox.cpp`) is buildable now (TButton + TStaticText +
TInputLine exist) but is the **first consumer of the D9 view-triggered async-modal
path** (`Deferred::OpenModal` + posted completion `Command`) — guide D9 "exec_view
— corrected" carries that design; build when a menu/msgbox needs it (Phase 4), not
before. Batch E dialog families (color/file/chdir/editor/outline/textview) fan out
once their leaf prereqs exist.

## Standing process reminders
- **Fan-out cadence is for gap-free MECHANICAL leaves only** (parallel worktree
  implementers, `isolation:"worktree"`, Sonnet, orchestrator integrates shared
  `mod.rs`/`lib.rs` + pre-seeds `theme.rs`). **FOUNDATION rows → per-row, Opus,
  full two-stage review.** Commit completed rows before dispatching worktree
  agents that build on them (worktree branches from the last *commit*).
  **Worktree location:** `isolation:"worktree"` now lands under
  `/scratch/oetiker/claude-worktrees/` via the `WorktreeCreate` hook — but only
  after a session **restart** (hooks load at startup). Before that, isolation goes
  to the project's `.claude/worktrees/`; create the worktree manually at the
  `/scratch` path + dispatch a **non-isolated** subagent instead (the row-32
  cadence). Verify where a probe worktree actually lands before relying on it.
- **Two-stage review stays mandatory** (SPEC then QUALITY, fresh C++-adversarial
  agents against the **C++ + guide, NOT the brief** — the brief can be wrong, as
  the validator wave's `first_pos` mis-statement proved). Make round-trip/unit tests
  **discriminating + bite-checked** (verify a finding fails before/passes after).
  Both stages keep earning their keep: at row 27, **spec** review caught an invented
  active/selected `draw` branch (the base inherits `TView::draw`'s uniform fill) and
  **quality** caught `std::any`-vs-`core::any` + a stale doc; in the validator wave,
  quality caught the untested validator reject/restore path and spec caught a dropped
  double-click scroll.
- **Snapshot workflow** (Appendix B step 4): `cargo-insta` is NOT installed →
  generate a `.snap` with `INSTA_UPDATE=always cargo test <name>`, verify by hand,
  re-run plain, commit the `.snap`.
- Keep per-row briefs **tight + self-contained + inline** (over-long briefs crashed
  a Sonnet implementer's context earlier in Batch B).

## Older standing deferrals (still open, grep the code)
- **`Context` command-set query** (command-graying) — TButton + TInputLine still
  wait on it (to enable/disable cmCut/Copy/Paste etc.). **Menus did NOT need it** —
  row 49 resolved menu graying with the `Deferred::UpdateMenu` broker instead (the
  read-accessor framing is obsolete). A button/inputline consumer would either reuse
  that broker shape or add a read accessor when it lands.
- **phase signal on `Context`** (plain-letter postProcess accelerator) — 3 waiting
  consumers: button, label, cluster (`is_plain_hotkey` exists but is ungated).
- **`Group::remove` release-after-remove ordering** — a removed selectable child
  never gets `RELEASED_FOCUS{source}`; a `TLabel` whose link is removed at runtime
  keeps a stale `light`. C++ `hide()`s before `removeView`. No consumer hits it yet.
- **`cmResize` keyboard sub-mode** (`window.rs`); **scrollbar auto-repeat +
  thumb-drag** + **cluster drag-cursor** (`TODO(row 31, D9)`); **close
  press-and-hold confirm** (`frame.rs`); **sibling tee-walk** (`framelin.cpp`);
  **shadow casting** (`group.rs`); **gray multi-scheme theming**
  (`TODO(row 34 gray theming)` — realign provisional `*` colours, incl. the 3 new
  Input roles); **row-9 glyphs** continue per-widget.
- **ctrlToArrow / accelerator TODOs** in cluster/scrollbar — shared key helpers
  EXIST (`b53c618`); retire opportunistically.
