//! `TFileDialog` data-support classes (rows 71–75): [`DirEntry`],
//! [`SearchRec`], [`DirCollection`], [`FileCollection`], [`DirListBox`].
//!
//! Rows 71–74 are pure-data types. Row 75 (`TDirListBox`) is the first view
//! type here — a concrete [`ListViewer`](crate::widgets::list_viewer::ListViewer)
//! impl over a [`Vec<DirEntry>`] that renders a tree-indented directory listing.
//!
//! Per the rstv "collections → `Vec`" deviation (no `TCollection`; cf.
//! `ListBox`'s `Vec<String>`), `TDirCollection` is a plain `Vec<DirEntry>`
//! alias and `TFileCollection` is a `Vec<SearchRec>` carrying only the one
//! piece of real logic — the sorted insert and its comparator.  The unused C++
//! collection API (`indexOf`/`remove`/`atPut`/`firstThat`/…) is dropped; no
//! consumer exists.
//!
//! ## D14 — native Linux paths
//! `TDirListBox::newDirectory` had a DOS `showDrives` branch (A:–Z: drive scan)
//! and a DOS `showDirs` branch (`\`-separated). Per D14 only `showDirs` is
//! ported, with `/`-separated paths and `std::fs::read_dir` for enumeration.
//! The `showDrives` branch and all drive-related helpers are dropped.

use core::cmp::Ordering;

// ---------------------------------------------------------------------------
// DirEntry — row 71
// ---------------------------------------------------------------------------

/// `TDirEntry` (row 71) — a (display-text, directory-path) pair for the
/// directory tree pane.
///
/// The C++ type heap-allocates two `char*` fields (`displayText`,
/// `directory`).  In Rust they are plain `String`s on the same allocation as
/// the struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// `displayText` — what the dir-list box draws (may carry tree-glyph
    /// prefixes added by the populator).
    pub display_text: String,
    /// `directory` — the path this entry navigates to when selected.
    pub directory: String,
}

impl DirEntry {
    /// `TDirEntry::TDirEntry(txt, dir)` — construct from any `Into<String>`.
    pub fn new(display_text: impl Into<String>, directory: impl Into<String>) -> Self {
        DirEntry {
            display_text: display_text.into(),
            directory: directory.into(),
        }
    }

    /// `TDirEntry::text()` — the display string.
    pub fn text(&self) -> &str {
        &self.display_text
    }

    /// `TDirEntry::dir()` — the navigation path.
    pub fn dir(&self) -> &str {
        &self.directory
    }
}

// ---------------------------------------------------------------------------
// DirCollection — row 72
// ---------------------------------------------------------------------------

/// `TDirCollection` (row 72) — an ordered list of [`DirEntry`] items.
///
/// The C++ type is a `TCollection` of `TDirEntry*`.  Per the
/// collections→`Vec` deviation this collapses to a bare type alias: row 75
/// (`TDirListBox`) only needs `push`, index, and `len`; the full `TCollection`
/// API is dropped.
pub type DirCollection = Vec<DirEntry>;

// ---------------------------------------------------------------------------
// SearchRec — row 73
// ---------------------------------------------------------------------------

/// The directory-attribute bit of [`SearchRec::attr`] (`FA_DIREC = 0x10`).
pub const FA_DIREC: u8 = 0x10;

/// `TSearchRec` (row 73) — a directory-listing file-metadata record.
///
/// The C++ struct uses a fixed-length `char name[MAXFILE+MAXEXT-1]` to keep
/// it POD-copyable for the collection.  In Rust, `name` is a `String` and the
/// struct derives `Clone`.
///
/// `attr`, `time`, and `size` are populated by [`FileList::raw_from_fs`] /
/// [`FileList::build_listing`]: `size` is `meta.len()` (saturated to `i32`),
/// `time` is the `modified()` mtime packed into the DOS `ftime` bitfield by
/// [`pack_dos_time`], and `attr` carries [`FA_DIREC`] for directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRec {
    /// `attr` — DOS file-attribute byte; only [`FA_DIREC`] is examined here.
    pub attr: u8,
    /// `time` — packed DOS timestamp.
    pub time: i32,
    /// `size` — file size in bytes.
    pub size: i32,
    /// `name` — the file or directory name (no path component).
    pub name: String,
}

// ---------------------------------------------------------------------------
// FileCollection — row 74
// ---------------------------------------------------------------------------

/// `TFileCollection::compare` (row 74) — the sort order for a directory
/// listing: `".."` last, directories after plain files, then case-sensitive
/// byte-order by name.
///
/// Ported verbatim from the C++ (the sign of every branch matters — do not
/// "tidy" it).
///
/// ```
/// use tvision::dialog::{SearchRec, search_rec_compare, FA_DIREC};
/// use core::cmp::Ordering;
///
/// let a = SearchRec { attr: 0,        time: 0, size: 0, name: "..".into() };
/// let b = SearchRec { attr: 0,        time: 0, size: 0, name: "foo".into() };
/// assert_eq!(search_rec_compare(&a, &b), Ordering::Greater); // ".." sorts last
/// ```
pub fn search_rec_compare(a: &SearchRec, b: &SearchRec) -> Ordering {
    // Equal names → Equal (mirrors the first strcmp returning 0).
    if a.name == b.name {
        return Ordering::Equal;
    }
    // key1 == ".." → positive (Greater means *after* in ascending order).
    if a.name == ".." {
        return Ordering::Greater;
    }
    // key2 == ".." → negative (Less).
    if b.name == ".." {
        return Ordering::Less;
    }
    let a_dir = a.attr & FA_DIREC != 0;
    let b_dir = b.attr & FA_DIREC != 0;
    // a is a directory, b is a plain file → a sorts after b.
    if a_dir && !b_dir {
        return Ordering::Greater;
    }
    // b is a directory, a is a plain file → a sorts before b.
    if b_dir && !a_dir {
        return Ordering::Less;
    }
    // Same kind — case-sensitive byte order (faithful to C++ `strcmp`).
    a.name.cmp(&b.name)
}

/// `TFileCollection` (row 74) — a name-sorted list of [`SearchRec`] items.
///
/// The C++ type is a `TSortedCollection` of `TSearchRec*`.  Per the
/// collections→`Vec` deviation the only transported behaviour is the sorted
/// insert and its comparator ([`search_rec_compare`]); the rest of the C++
/// `TSortedCollection` API is dropped (no consumer).
///
/// The sort order is: plain files alphabetically (case-sensitive), then
/// directories alphabetically, then `".."` last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileCollection {
    items: Vec<SearchRec>,
}

impl FileCollection {
    /// Create an empty `FileCollection`.
    pub fn new() -> Self {
        FileCollection { items: Vec::new() }
    }

    /// `TSortedCollection::insert` — insert `rec` while keeping the list
    /// sorted by [`search_rec_compare`].  Duplicate names do not occur in a
    /// real directory listing.
    pub fn insert(&mut self, rec: SearchRec) {
        let pos = self
            .items
            .partition_point(|x| search_rec_compare(x, &rec) == Ordering::Less);
        self.items.insert(pos, rec);
    }

    /// `at(index)` — borrow the record at `index`, or `None` when out of
    /// bounds.
    pub fn at(&self, index: usize) -> Option<&SearchRec> {
        self.items.get(index)
    }

    /// `getCount()` — number of records in the collection.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the collection contains no records.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Read-only slice of the sorted records.
    pub fn items(&self) -> &[SearchRec] {
        &self.items
    }

    /// Consume the collection, returning its sorted records by value (avoids a
    /// clone when the `FileCollection` is a temporary).
    pub fn into_items(self) -> Vec<SearchRec> {
        self.items
    }
}

// ---------------------------------------------------------------------------
// DirListBox — row 75
// ---------------------------------------------------------------------------

/// Tree-glyph connector: root entry and each ancestor — `pathDir` in the C++.
const PATH_DIR: &str = "└─┬"; // U+2514 U+2500 U+252C
/// Tree-glyph connector: first subdirectory — `firstDir` in the C++.
const FIRST_DIR: &str = "└┬─"; // U+2514 U+252C U+2500
/// Tree-glyph connector: subsequent subdirectories — `middleDir` in the C++.
const MIDDLE_DIR: &str = " ├─"; // SPACE U+251C U+2500
/// How many extra spaces are added per depth level.
const INDENT_STEP: usize = 2;

/// `TDirListBox` (row 75) — a concrete [`ListViewer`] over a
/// [`Vec<DirEntry>`] that renders the current working directory as a
/// tree-indented listing of its ancestors and immediate subdirectories.
///
/// ## How it differs from `ListBox`
///
/// `TDirListBox` is a **C++ subclass of `TListBox`**. In rstv it is a *second,
/// parallel, direct* [`ListViewer`] impl — exactly like
/// [`ListBox`](crate::widgets::ListBox) is — over its own `Vec<DirEntry>`
/// storage. It does **not** embed or delegate through a
/// `ListBox`: if it delegated [`View::draw`](crate::view::View::draw), draw
/// would run with the inner `ListBox` as `self` and call its `get_text` over
/// `Vec<String>`, never consulting the `Vec<DirEntry>`. See the D2
/// embed-and-delegate note in PORTING-GUIDE.md.
///
/// ## D14 — native Linux paths
///
/// The C++ `newDirectory` has a DOS `showDrives` branch (A:–Z: drive scan).
/// Per D14 only `showDirs` is ported, re-imagined for Linux `/`-separated
/// paths. No `showDrives`, no drive letters, no backslashes.
///
/// ## Drops / deferrals
///
/// - `showDrives` / drive-letter scan — D14 (native Linux).
/// - `~TDirListBox` / `destroy` — Vec ownership; no manual destroy needed.
/// - `write`/`read`/`build`/`streamableName`/`name` — D12 streaming dropped.
/// - `select_item` cmChangeDir payload — deferred to row 80 (`TChDirDialog`).
///
/// [`ListViewer`]: crate::widgets::list_viewer::ListViewer
pub struct DirListBox {
    lv: crate::widgets::list_viewer::ListViewerState,
    /// The `TDirCollection` — the rendered tree of [`DirEntry`] items.
    items: Vec<DirEntry>,
    /// Index of the *current* directory entry (the highlighted ancestor).
    cur: usize,
    /// The current directory path (native `/`-separated, with trailing `/`).
    ///
    /// Retained for the row-80 `TChDirDialog` consumer (`select_item` /
    /// `cmChangeDir` reads the current directory). Currently **unread in the
    /// base port** — like `SortedListBox::shift_state`, it is captured but has
    /// no reader until its consumer lands.
    dir: String,
    /// The owner `TChDirDialog`'s `chDirButton` id, wired by
    /// [`set_chdir_button`](DirListBox::set_chdir_button) after assembly. On an
    /// `sfFocused` change, [`set_state`](DirListBox::set_state) requests the pump
    /// make this button (un-)default — the C++ `setState`'s
    /// `((TChDirDialog*)owner)->chDirButton->makeDefault(enable)`. `None` outside a
    /// `TChDirDialog` (e.g. a `TFileDialog` never wires one).
    chdir_button: Option<crate::view::ViewId>,
}

impl DirListBox {
    /// `TDirListBox::TDirListBox` — construct an empty dir list box.
    ///
    /// Faithful: `TListBox(bounds, 1, aScrollBar)` → `ListViewerState::new(bounds,
    /// 1, h, v)` (num_cols always 1, only the vertical scrollbar is used in the
    /// C++ ctor). `h` is kept for **ctor parity with
    /// [`ListBox::new`](crate::widgets::ListBox::new)** — the C++ `TListBox` ctor
    /// takes a single scrollbar and `TDirListBox` only ever wires the vertical
    /// one, so `h` is typically `None` here.
    pub fn new(
        bounds: crate::view::Rect,
        h: Option<crate::view::ViewId>,
        v: Option<crate::view::ViewId>,
    ) -> Self {
        DirListBox {
            lv: crate::widgets::list_viewer::ListViewerState::new(bounds, 1, h, v),
            items: Vec::new(),
            cur: 0,
            dir: String::new(),
            chdir_button: None,
        }
    }

    /// The current item collection (`TDirListBox::list()`).
    pub fn list(&self) -> &[DirEntry] {
        &self.items
    }

    /// Wire the owner `TChDirDialog`'s `chDirButton` id (set after assembly, once
    /// the button's id is known). Read only by [`set_state`](DirListBox::set_state)
    /// to drive the C++ `chDirButton->makeDefault(enable)` on a focus change.
    pub fn set_chdir_button(&mut self, id: crate::view::ViewId) {
        self.chdir_button = Some(id);
    }

    /// The focused [`DirEntry`] (`list()->at(focused)`), or `None` when the list is
    /// empty / `focused` is out of range. Read by `TChDirDialog::handleEvent`'s
    /// `cmChangeDir` arm (the C++ reads `dirList->list()->at(dirList->focused)`).
    pub fn focused_entry(&self) -> Option<&DirEntry> {
        self.items.get(self.lv.focused as usize)
    }

    /// Pure tree-builder — ports the `showDirs` block of `TDirListBox::newDirectory`.
    ///
    /// Given `dir` (a `/`-terminated absolute path, e.g. `"/home/oetiker/"`) and
    /// an already-sorted list of immediate subdirectory names `subdirs`, returns
    /// `(entries, cur)` where `cur` is the index of the current-directory entry
    /// (the deepest ancestor, highlighted by [`ListViewer::is_selected`]).
    ///
    /// ## Layout (D14)
    ///
    /// ```text
    /// └─┬/             ← root, indent 0 (PATH_DIR)
    ///   └─┬home        ← indent 2 (PATH_DIR)
    ///     └─┬oetiker   ← indent 4 (PATH_DIR) ← cur
    ///       └┬─projects  ← indent 6 (FIRST_DIR, fixed up → └── if last)
    ///        ├─scratch    ← indent 6 (MIDDLE_DIR)
    ///        └─tmp        ← indent 6 (last; ├ → └)
    /// ```
    ///
    /// For `dir = "/"` (only the root): `cur = 0`, subdirs at indent 2.
    fn build_tree(dir: &str, subdirs: &[String]) -> (Vec<DirEntry>, usize) {
        let mut entries: Vec<DirEntry> = Vec::new();

        // --- Step 1: root entry -------------------------------------------
        entries.push(DirEntry::new(format!("{PATH_DIR}/"), "/".to_string()));

        // --- Step 2: ancestor entries ---------------------------------------
        // Split `dir` on `/`; the meaningful segments are the non-empty parts.
        // For `dir = "/home/oetiker/"` → segments ["home", "oetiker"].
        let segments: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();

        for (i, &seg) in segments.iter().enumerate() {
            let indent = (i + 1) * INDENT_STEP;
            // Build the absolute path through this segment (no trailing slash).
            let abs_path = format!("/{}", segments[..=i].join("/"));
            entries.push(DirEntry::new(
                format!("{}{}{}", " ".repeat(indent), PATH_DIR, seg),
                abs_path,
            ));
        }

        // `cur` is the index of the deepest ancestor = last entry pushed so far.
        let cur = entries.len() - 1;

        // Indent for subdirs = (depth of cur) + INDENT_STEP.
        // For root-only (`/`, segments empty, cur = 0): sub_indent = 0 + 2 = 2.
        // For `/home/oetiker/` (cur at index 2, depth = 2*INDENT_STEP = 4):
        //   sub_indent = 4 + 2 = 6.
        let sub_indent = cur * INDENT_STEP + INDENT_STEP;

        // --- Step 3: immediate subdirectories --------------------------------
        for (i, name) in subdirs.iter().enumerate() {
            let connector = if i == 0 { FIRST_DIR } else { MIDDLE_DIR };
            // `directory = dir + name` (dir ends with `/`).
            let directory = format!("{}{}", dir, name);
            entries.push(DirEntry::new(
                format!("{}{}{}", " ".repeat(sub_indent), connector, name),
                directory,
            ));
        }

        // --- Step 4: last-entry glyph fix-up --------------------------------
        // Faithful to the C++ pointer surgery on `dirs->at(getCount()-1)`,
        // applied UNCONDITIONALLY to the last entry (the deepest visible node
        // has no sibling/child below it, so its connector becomes a corner):
        //   - has '└' (PATH_DIR "└─┬" or FIRST_DIR "└┬─"): turn the two chars
        //     after '└' into "──"  →  "└──".
        //   - else has '├' (MIDDLE_DIR " ├─"): turn '├' into '└'  →  " └─".
        // When subdirs exist this hits the last subdir; with no subdirs it hits
        // the deepest ancestor ("└─┬name" → "└──name"). `entries` is never empty
        // (the root is always present).
        let last = entries.last_mut().unwrap();
        let mut c: Vec<char> = last.display_text.chars().collect();
        if let Some(i) = c.iter().position(|&ch| ch == '└') {
            if i + 1 < c.len() {
                c[i + 1] = '─';
            }
            if i + 2 < c.len() {
                c[i + 2] = '─';
            }
            last.display_text = c.into_iter().collect();
        } else if let Some(i) = c.iter().position(|&ch| ch == '├') {
            c[i] = '└';
            last.display_text = c.into_iter().collect();
        }

        (entries, cur)
    }

    /// `TDirListBox::newDirectory` — read `dir`'s subdirectories from the
    /// filesystem, build the tree via the private `build_tree`, and
    /// publish the result to the list-viewer machinery.
    ///
    /// Faithful: `newList(dirs); focusItem(cur)`.
    ///
    /// The only impure operation (filesystem read) is isolated here; all tree
    /// construction is in the pure `build_tree`.
    pub fn new_directory(&mut self, dir: &str, ctx: &mut crate::view::Context) {
        // Normalize to a trailing `/` (build_tree's precondition). The row-80
        // callers (`reset_current`/`cmRevert`) pass `std::env::current_dir()`,
        // which has NO trailing slash; without this, `build_tree`'s segment
        // joins and the subdir `dir + name` concatenation would mis-form paths.
        let dir = if dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{dir}/")
        };
        let dir = dir.as_str();
        self.dir = dir.to_string();

        // Read immediate subdirectories from the filesystem.
        let mut subdirs: Vec<String> = match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(|e| {
                    let e = e.ok()?;
                    // stat-follows-symlinks (std::fs::metadata), matching
                    // magiblot's findfirst (cvtAttr in source/platform/findfrst.cpp
                    // uses stat()). DirEntry::file_type() is lstat-based and would
                    // wrongly exclude a symlink pointing at a directory — wrong for
                    // a directory navigator. A broken symlink → metadata errs → the
                    // `?` skips it, which is the desired behavior.
                    let meta = std::fs::metadata(e.path()).ok()?;
                    if !meta.is_dir() {
                        return None;
                    }
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        return None;
                    }
                    Some(name)
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        // Sort case-insensitively (row-70 ordering — identical to `ci_cmp` in
        // list_box.rs; inlined here to avoid cross-module coupling).
        subdirs.sort_by(|a, b| {
            a.chars()
                .map(|c| c.to_ascii_lowercase())
                .cmp(b.chars().map(|c| c.to_ascii_lowercase()))
        });

        let (items, cur) = Self::build_tree(dir, &subdirs);
        self.items = items;
        self.cur = cur;

        let len = self.items.len() as i32;
        crate::widgets::list_viewer::set_range(self, len, ctx);
        if self.lv.range > 0 {
            crate::widgets::list_viewer::focus_item(self, self.cur as i32, ctx);
        }
    }
}

impl crate::widgets::list_viewer::ListViewer for DirListBox {
    fn lv(&self) -> &crate::widgets::list_viewer::ListViewerState {
        &self.lv
    }

    fn lv_mut(&mut self) -> &mut crate::widgets::list_viewer::ListViewerState {
        &mut self.lv
    }

    /// `TDirListBox::getText` — the display text for `item`.
    ///
    /// Faithful: `strnzcpy(text, list()->at(item)->text(), …)`.
    fn get_text(&self, item: i32) -> String {
        self.items
            .get(item as usize)
            .map(|e| e.text().to_string())
            .unwrap_or_default()
    }

    /// `TDirListBox::isSelected` — `item == cur` (the current directory ancestor).
    ///
    /// Faithful override of the base (base is `item == focused`; dir list
    /// highlights the *current directory* entry, not just the cursor position).
    fn is_selected(&self, item: i32) -> bool {
        item as usize == self.cur
    }

    /// `TDirListBox::selectItem` — `message(owner, evCommand, cmChangeDir,
    /// list()->at(item))`: post `cmChangeDir` to the owner `TChDirDialog`.
    ///
    /// The C++ carries the chosen [`DirEntry`] as the command's `infoPtr`, but the
    /// dialog's `cmChangeDir` handler ignores it and re-reads
    /// `dirList->list()->at(dirList->focused)` itself — so the faithful rstv path
    /// is just a payload-less posted command. `selectItem` runs on a
    /// double-click/Enter, by which point `focused == item` (the list focuses an
    /// item before selecting it), so the dialog reads the same entry.
    fn select_item(&mut self, _item: i32, ctx: &mut crate::view::Context) {
        ctx.post(crate::command::Command::CHANGE_DIR);
    }
}

impl crate::view::View for DirListBox {
    fn state(&self) -> &crate::view::ViewState {
        &self.lv.state
    }

    fn state_mut(&mut self) -> &mut crate::view::ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut crate::view::DrawCtx) {
        crate::widgets::list_viewer::draw(self, ctx);
    }

    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        crate::widgets::list_viewer::handle_event(self, ev, ctx);
    }

    /// `TDirListBox::setState` — `TListBox::setState(...)` then, on an `sfFocused`
    /// change, `((TChDirDialog*)owner)->chDirButton->makeDefault(enable)`: the dir
    /// list grabs the dialog's default button when focused and releases it when
    /// not.
    ///
    /// rstv (D3): the leaf list cannot poke its sibling button inline, so it
    /// requests the change through the pump's
    /// [`MakeButtonDefault`](crate::view::Deferred::MakeButtonDefault) broker via
    /// [`Context::make_button_default`]. `chdir_button` is `None` outside a
    /// `TChDirDialog`, so this is a no-op for any other owner.
    fn set_state(
        &mut self,
        flag: crate::view::StateFlag,
        enable: bool,
        ctx: &mut crate::view::Context,
    ) {
        crate::widgets::list_viewer::set_state(self, flag, enable, ctx);
        if flag == crate::view::StateFlag::Focused
            && let Some(btn) = self.chdir_button
        {
            ctx.make_button_default(btn, enable);
        }
    }

    fn cursor_request(&self) -> Option<crate::view::Point> {
        crate::widgets::list_viewer::focused_cursor(self)
    }

    fn apply_list_scroll(
        &mut self,
        h: Option<i32>,
        v: Option<i32>,
        ctx: &mut crate::view::Context,
    ) {
        crate::widgets::list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `TDirListBox` has no `getData` override in the C++ — the focused item
    /// index (same as `ListBox`) is the natural value.
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Int(self.lv.focused))
    }
}

// ---------------------------------------------------------------------------
// FileList — row 76
// ---------------------------------------------------------------------------

/// `TFileList` (row 76) — the file-listing pane of `TFileDialog`. A concrete
/// [`SortedSearch`](crate::widgets::list_viewer::SortedSearch) (hence
/// [`ListViewer`]) over a name-sorted [`Vec<SearchRec>`] (== a
/// [`FileCollection`]'s contents), with two columns and incremental
/// type-to-search.
///
/// ## Structural shape
///
/// `TFileList` is a C++ subclass of `TSortedListBox`. In rstv it is a *direct*
/// [`SortedSearch`] impl — the same shape as
/// [`SortedListBox`](crate::widgets::SortedListBox), NOT
/// [`DirListBox`](crate::dialog::DirListBox) (which is a plain [`ListViewer`]).
/// It therefore wires `handle_event`→`sorted_handle_event` and
/// `cursor_request`→`sorted_cursor`, inherits the base `is_selected`
/// (`item == focused`), and contributes no dialog data (`value() == None`).
///
/// ## The `search` key (getKey + list()->search, fused)
///
/// `search` is the one method whose comparator is **non-obvious**: it must
/// compare via [`search_rec_compare`] over raw [`SearchRec`]s, **not** over
/// [`get_text`](ListViewer::get_text) (which appends `/` to dir names and would
/// mis-order). The key carries an `attr` of [`FA_DIREC`] when the
/// [`shift_state`](crate::widgets::list_viewer::SortedSearch::shift_state)
/// holds [`KB_SHIFT`](crate::widgets::list_viewer::KB_SHIFT) or the typed prefix
/// starts with `.`, routing the search into the directory section of the
/// collection. That routing exists only in `search_rec_compare`'s file/dir
/// ordering — hence this is a per-impl `search`, not the shared `get_text` one.
///
/// ## D14 — native Linux paths
///
/// `getText` appends `/` (not the DOS `\`) to directory names; `getKey`'s
/// `strupr` (DOS-only, `#ifndef __FLAT__`) is **not** ported — the Linux build
/// is case-sensitive.
///
/// ## Owner broadcasts (row 77)
///
/// `TFileList::focusItem` broadcasts `cmFileFocused` (payload = the focused
/// `SearchRec`) on every focus change, and `selectItem` broadcasts
/// `cmFileDoubleClicked`. Both are wired here via the `on_focus_changed`
/// virtual-`focusItem` hook ([`ListViewer::on_focus_changed`]) and the
/// `select_item` override; the payload travels through the pump's
/// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) broker (D4:
/// payload-less broadcast + resolvable `source`).
///
/// ## Drops / deferrals (faithful, breadcrumbed)
///
/// - `getData`/`setData`/`dataSize` no-op → `value() == None` (no dialog data).
/// - `write`/`read`/`build`/`streamableName`/`name` — D12 streaming dropped.
/// - the `tooManyFiles` messageBox OOM guard — dropped (`Vec` is infallible).
/// - `DirSearchRec::operator new` safety-pool check — dropped.
/// - `fexpand`/`squeeze`/path canonicalization — DOS path machinery, not needed
///   under D14 (the caller passes an absolute `/`-path).
///
/// [`ListViewer`]: crate::widgets::list_viewer::ListViewer
/// [`SortedSearch`]: crate::widgets::list_viewer::SortedSearch
pub struct FileList {
    lv: crate::widgets::list_viewer::ListViewerState,
    /// The sorted listing — the contents of the C++ `TFileCollection`.
    items: Vec<SearchRec>,
    /// `searchPos` — index of the last matched char in the focused item's text;
    /// -1 = no active search.
    search_pos: i32,
    /// `shiftState` — `kbShift` bits captured at the searchPos -1↔0 transition;
    /// read by [`search`](FileList::search) to route the key into the dir
    /// section (`getKey`: `(shiftState & kbShift) != 0` → `attr = FA_DIREC`).
    shift_state: u8,
}

impl FileList {
    /// `TFileList::TFileList(bounds, aScrollBar)` — `TSortedListBox(bounds, 2,
    /// aScrollBar)`, so **num_cols = 2**. Like the `TSortedListBox` ctor it shows
    /// the cursor at column 1. `h` is kept for ctor parity (the C++ ctor takes a
    /// single scrollbar — typically the vertical one).
    pub fn new(
        bounds: crate::view::Rect,
        h: Option<crate::view::ViewId>,
        v: Option<crate::view::ViewId>,
    ) -> Self {
        let mut lv = crate::widgets::list_viewer::ListViewerState::new(bounds, 2, h, v);
        lv.state.show_cursor();
        lv.state.set_cursor(1, 0);
        FileList {
            lv,
            items: Vec::new(),
            search_pos: -1,
            shift_state: 0,
        }
    }

    /// The current item collection (`TFileList::list()`).
    pub fn list(&self) -> &[SearchRec] {
        &self.items
    }

    /// The focused entry (`list()->at(focused)`), or `None` when the list is empty
    /// (or `focused` is out of range). Read by the pump's
    /// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) broker to
    /// deliver the `cmFileFocused` payload to a `TFileInputLine`/`TFileInfoPane`.
    pub fn focused_rec(&self) -> Option<SearchRec> {
        self.items.get(self.lv.focused as usize).cloned()
    }

    /// Match `name` against a `*`/`?` glob (case-sensitive, like the Linux
    /// build). `*` = any run (including empty); `?` = exactly one char. No
    /// `[...]` classes or escaping (a Unix `fnmatch` subset). `"*"` matches
    /// everything. Classic two-pointer scan with `*`-backtracking.
    fn wildcard_match(pattern: &str, name: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let s: Vec<char> = name.chars().collect();
        let (mut pi, mut si) = (0usize, 0usize);
        // Backtrack anchors: the last `*` position in `p` and the `s` position
        // it was matched against (so a failed match can extend the `*` run).
        let mut star: Option<usize> = None;
        let mut star_si = 0usize;
        while si < s.len() {
            if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
                pi += 1;
                si += 1;
            } else if pi < p.len() && p[pi] == '*' {
                star = Some(pi);
                star_si = si;
                pi += 1;
            } else if let Some(sp) = star {
                // Extend the last `*` to consume one more char of `s`.
                pi = sp + 1;
                star_si += 1;
                si = star_si;
            } else {
                return false;
            }
        }
        // Trailing `*`s in the pattern match the empty remainder.
        while pi < p.len() && p[pi] == '*' {
            pi += 1;
        }
        pi == p.len()
    }

    /// Build the sorted listing from a directory's raw entries — the pure,
    /// unit-testable core of [`read_directory`](FileList::read_directory). `dir`
    /// is a `/`-terminated absolute path; each raw entry is `(name, is_dir,
    /// size, mtime)` where `mtime` is the entry's modification time (`None` when
    /// the filesystem could not report one).
    ///
    /// Faithful to the two-pass C++ `readDirectory`:
    /// - **Pass 1 (files):** a non-directory is kept iff it matches `wildcard`.
    /// - **Pass 2 (dirs):** a directory is kept iff `name[0] != '.'` — the
    ///   wildcard does NOT apply (C++ resets the pattern to `"*.*"`). This drops
    ///   `.`, `..`, AND hidden dirs (faithful to `ff_name[0] != '.'`).
    /// - **`".."`:** appended iff `dir != "/"` (C++ `strlen(dir) > 1`).
    ///
    /// `time` carries the DOS-packed mtime ([`pack_dos_time`]) on every real
    /// entry — the value the [`FileInfoPane`] (row 78) unpacks to render the
    /// size/date line. A real entry with no reportable mtime gets `time = 0`
    /// (an empty `name` would suppress its date line; a real name would draw a
    /// blank date, an acceptable degenerate case).
    ///
    /// **D-time deviation on the `".."` row.** rstv synthesizes `".."` *without*
    /// statting the parent, so it uses [`DOTDOT_TIME`] (`0x210000`)
    /// unconditionally — a cosmetic difference in the displayed date on the
    /// `".."` row only. C++ `readDirectory` (tfillist.cpp) instead stats the real
    /// parent via `findfirst("..", FA_DIREC)` and uses the parent dir's real
    /// mtime, falling back to this constant only when that findfirst fails.
    ///
    /// Returns the [`search_rec_compare`]-sorted `Vec`, built via
    /// [`FileCollection::insert`].
    fn build_listing(
        dir: &str,
        wildcard: &str,
        raw: &[(String, bool, i32, Option<std::time::SystemTime>)],
    ) -> Vec<SearchRec> {
        let mut fc = FileCollection::new();
        for (name, is_dir, size, mtime) in raw {
            let time = mtime.as_ref().map(pack_dos_time).unwrap_or(0);
            if *is_dir {
                if !name.starts_with('.') {
                    fc.insert(SearchRec {
                        attr: FA_DIREC,
                        time,
                        size: 0,
                        name: name.clone(),
                    });
                }
            } else if Self::wildcard_match(wildcard, name) {
                fc.insert(SearchRec {
                    attr: 0,
                    time,
                    size: *size,
                    name: name.clone(),
                });
            }
        }
        if dir != "/" {
            fc.insert(SearchRec {
                attr: FA_DIREC,
                time: DOTDOT_TIME,
                size: 0,
                name: "..".into(),
            });
        }
        fc.into_items()
    }

    /// `TFileList::readDirectory` — read `dir`'s entries from the filesystem,
    /// build the sorted listing via the pure [`build_listing`](FileList::build_listing),
    /// and publish it to the list-viewer machinery.
    ///
    /// The only impure operation (the filesystem read) is isolated here. Like
    /// [`DirListBox::new_directory`], it uses `std::fs::metadata` (which follows
    /// symlinks — matching magiblot's `findfirst`/`stat`, not `lstat`); a broken
    /// symlink errs and is skipped via `.ok()`. `size` saturates into `i32`.
    ///
    /// Faithful to the C++ `readDirectory` tail: after `newList(fileList)`, it
    /// broadcasts `cmFileFocused` for item 0 (or an all-zero `DirSearchRec noFile`
    /// sentinel when the listing is empty). Here the non-empty case is FREE — the
    /// `focus_item(self, 0, …)` below fires `on_focus_changed`, which broadcasts
    /// `FILE_FOCUSED { source = self }`; the broker reads `focused_rec()`. The
    /// empty case has no focusable item, so it broadcasts `FILE_FOCUSED` directly
    /// (the broker then reads `focused_rec() == None` → a blank field, the noFile
    /// sentinel's effect).
    /// Read `dir`'s raw entries from the filesystem — the impure half shared by
    /// [`read_directory`](FileList::read_directory) and the ctx-free
    /// [`read_directory_listing`](FileList::read_directory_listing). Each entry is
    /// `(name, is_dir, size, mtime)`. Uses `std::fs::metadata` (follows symlinks,
    /// matching magiblot's `findfirst`/`stat`); a broken symlink errs and is
    /// skipped. `size` saturates into `i32`.
    fn raw_from_fs(dir: &str) -> Vec<(String, bool, i32, Option<std::time::SystemTime>)> {
        match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(|e| {
                    let e = e.ok()?;
                    let meta = std::fs::metadata(e.path()).ok()?;
                    let name = e.file_name().to_string_lossy().into_owned();
                    let is_dir = meta.is_dir();
                    let size = meta.len().min(i32::MAX as u64) as i32;
                    // `.modified()` is unsupported on some platforms; a None
                    // mtime packs to `time = 0` in build_listing.
                    let mtime = meta.modified().ok();
                    Some((name, is_dir, size, mtime))
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// `TFileList::readDirectory`, ctx-free — a sibling of
    /// [`read_directory`](FileList::read_directory) for the `TFileDialog` ctor
    /// path and deterministic tests. Builds + publishes the listing to the
    /// list-viewer state (`range`/`focused`/`top_item`) but does **NOT** sync the
    /// scrollbar or broadcast `cmFileFocused` — both of those need a `Context`,
    /// which the C++ ctor's trailing `readDirectory()` also lacks. The ctx-ful
    /// `read_directory` (driven from `reset_current`) is the path that does the
    /// scrollbar sync + broadcast.
    pub fn read_directory_listing(&mut self, dir: &str, wildcard: &str) {
        let raw = Self::raw_from_fs(dir);
        self.items = Self::build_listing(dir, wildcard, &raw);
        self.lv.range = self.items.len() as i32;
        self.lv.focused = 0;
        self.lv.top_item = 0;
    }

    pub fn read_directory(&mut self, dir: &str, wildcard: &str, ctx: &mut crate::view::Context) {
        let raw = Self::raw_from_fs(dir);

        self.items = Self::build_listing(dir, wildcard, &raw);

        let len = self.items.len() as i32;
        crate::widgets::list_viewer::set_range(self, len, ctx);
        if self.lv.range > 0 {
            // focus_item → on_focus_changed → FILE_FOCUSED broadcast (item 0).
            crate::widgets::list_viewer::focus_item(self, 0, ctx);
        } else if let Some(id) = self.lv.state.id() {
            // Empty listing — no focusable item, so the `focus_item` path can't
            // fire. Broadcast FILE_FOCUSED directly: the broker reads
            // `focused_rec() == None` → a blank field (the C++ noFile sentinel).
            ctx.broadcast(crate::command::Command::FILE_FOCUSED, Some(id));
        }
    }
}

impl crate::widgets::list_viewer::ListViewer for FileList {
    fn lv(&self) -> &crate::widgets::list_viewer::ListViewerState {
        &self.lv
    }

    fn lv_mut(&mut self) -> &mut crate::widgets::list_viewer::ListViewerState {
        &mut self.lv
    }

    /// `TFileList::getText` — the file/dir name, with a trailing `/` (D14, not
    /// the DOS `\`) appended to directories. OOB → empty.
    fn get_text(&self, item: i32) -> String {
        match self.items.get(item as usize) {
            Some(rec) => {
                if rec.attr & FA_DIREC != 0 {
                    format!("{}/", rec.name)
                } else {
                    rec.name.clone()
                }
            }
            None => String::new(),
        }
    }

    // `is_selected` is INHERITED (base: `item == focused`). TFileList does not
    // override it — do NOT add one here.

    /// `TFileList::focusItem` — the virtual tail, fired on EVERY focus change.
    ///
    /// Faithful: `message(owner, evBroadcast, cmFileFocused, list()->at(item))`.
    /// rstv's broadcast is payload-less (D4), so we broadcast `FILE_FOCUSED` with
    /// this view as the resolvable `source`; the pump's
    /// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) broker
    /// reads [`focused_rec`](FileList::focused_rec) and delivers it to the
    /// consumer. (The C++ calls `TSortedListBox::focusItem` first — that base step
    /// is the shared `focus_item` free fn that invokes this hook, so it has already
    /// run.)
    fn on_focus_changed(&mut self, ctx: &mut crate::view::Context) {
        if let Some(id) = self.lv.state.id() {
            ctx.broadcast(crate::command::Command::FILE_FOCUSED, Some(id));
        }
    }

    /// `TFileList::selectItem` — broadcast `cmFileDoubleClicked`.
    ///
    /// Faithful: `message(owner, evBroadcast, cmFileDoubleClicked, list()->at(item))`.
    /// The C++ payload is the focused `SearchRec`, but the only consumer
    /// (`TFileDialog::handleEvent`) merely turns `cmFileDoubleClicked` into `cmOK`
    /// and never reads the record — so it is faithfully **payload-less** here (D4):
    /// just `FILE_DOUBLE_CLICKED { source = self }`. Does NOT call the base, so it
    /// does NOT also broadcast `cmListItemSelected`.
    ///
    /// TODO(row 79 TFileDialog): the dialog's `cmFileDoubleClicked → cmOK`
    /// conversion is row 79's job.
    fn select_item(&mut self, _item: i32, ctx: &mut crate::view::Context) {
        if let Some(id) = self.lv.state.id() {
            ctx.broadcast(crate::command::Command::FILE_DOUBLE_CLICKED, Some(id));
        }
    }
}

impl crate::widgets::list_viewer::SortedSearch for FileList {
    fn search_pos(&self) -> i32 {
        self.search_pos
    }

    fn set_search_pos(&mut self, pos: i32) {
        self.search_pos = pos;
    }

    fn shift_state(&self) -> u8 {
        self.shift_state
    }

    fn set_shift_state(&mut self, s: u8) {
        self.shift_state = s;
    }

    /// `TFileList::getKey` + `list()->search`, fused.
    ///
    /// `getKey`: the key's `attr` is [`FA_DIREC`] when `shift_state & KB_SHIFT`
    /// is set OR the typed prefix starts with `.`, else 0. The name is the typed
    /// prefix verbatim — NO `strupr` (the `__FLAT__`/Linux branch).
    ///
    /// `list()->search`: the first index `i` in `0..range` where
    /// `search_rec_compare(items[i], key) != Less` (the insertion point);
    /// `range` if none. **Compares via [`search_rec_compare`] over raw
    /// [`SearchRec`]s** (the `attr` routes the search into the file or dir
    /// section) — NOT over `get_text`.
    fn search(&self, cur: &[char]) -> i32 {
        use crate::widgets::list_viewer::KB_SHIFT;
        let attr = if (self.shift_state & KB_SHIFT) != 0 || cur.first() == Some(&'.') {
            FA_DIREC
        } else {
            0
        };
        let name: String = cur.iter().collect();
        let key = SearchRec {
            attr,
            time: 0,
            size: 0,
            name,
        };
        // Insertion point over the sorted items, bounded by `range` (which the
        // tests set independently of `items.len()`), via search_rec_compare.
        let range = self.lv.range;
        let (mut lo, mut hi) = (0i32, range);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if search_rec_compare(&self.items[mid as usize], &key) == Ordering::Less {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

impl crate::view::View for FileList {
    fn state(&self) -> &crate::view::ViewState {
        &self.lv.state
    }

    fn state_mut(&mut self) -> &mut crate::view::ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut crate::view::DrawCtx) {
        crate::widgets::list_viewer::draw(self, ctx);
    }

    /// `TSortedListBox::handleEvent` — the incremental type-to-search machine.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        crate::widgets::list_viewer::sorted_handle_event(self, ev, ctx);
    }

    fn cursor_request(&self) -> Option<crate::view::Point> {
        crate::widgets::list_viewer::sorted_cursor(self)
    }

    /// `TFileList` has no `setState` override (its only owner poke is in the
    /// `focusItem` override, deferred to row 79) — the plain base.
    fn set_state(
        &mut self,
        flag: crate::view::StateFlag,
        enable: bool,
        ctx: &mut crate::view::Context,
    ) {
        crate::widgets::list_viewer::set_state(self, flag, enable, ctx);
    }

    fn apply_list_scroll(
        &mut self,
        h: Option<i32>,
        v: Option<i32>,
        ctx: &mut crate::view::Context,
    ) {
        crate::widgets::list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `None` — C++ overrides `getData`/`setData`/`dataSize` to no-op/0, so
    /// `TFileList` contributes nothing to dialog data transfer.
    fn value(&self) -> Option<crate::data::FieldValue> {
        None
    }
}

// ---------------------------------------------------------------------------
// FileInputLine — row 77
// ---------------------------------------------------------------------------

/// `TFileInputLine` (stddlg.cpp) — a [`TInputLine`](crate::widgets::InputLine)
/// filename field that mirrors the [`FileList`]'s focused entry.
///
/// On a `cmFileFocused` broadcast (and only while the user is **not** typing in
/// it — the `!(state & sfSelected)` guard) it copies the focused entry's name
/// into the field, appending `/<wildCard>` (D14 `/`, not the DOS `\`) when the
/// entry is a directory.
///
/// ## Structural shape (D2 embed-and-delegate)
///
/// `TFileInputLine` is a C++ subclass of `TInputLine`. In rstv it **embeds** an
/// [`InputLine`] and forwards the un-overridden [`View`](crate::view::View)
/// methods via `#[crate::delegate(to = inner)]` — only `handle_event` and
/// `as_any_mut` differ. `value`/`set_value` (D10) are NOT overridden (the C++
/// `TFileInputLine` has no `getData`/`setData`), so they forward to the inner
/// `InputLine`.
///
/// ## The payload broker (D3/D4)
///
/// The broadcast is payload-less in rstv, so `handle_event` does not read the
/// record inline; it requests
/// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) and the pump
/// resolves the producer's [`focused_rec`](FileList::focused_rec), then calls
/// [`on_file_focused`](FileInputLine::on_file_focused) on this view.
///
/// ## Drops / deferrals
/// - **D8:** the C++ `drawView()` after the copy is dropped (whole-tree redraw).
/// - **D12:** `write`/`read`/`build`/`streamableName`/`name` streaming dropped.
pub struct FileInputLine {
    /// The embedded `TInputLine` — the D2 delegation target.
    inner: crate::widgets::InputLine,
    /// Cached `((TFileDialog*)owner)->wildCard` (D3: a child can't read its owner
    /// inline). Set at ctor; row 79 pushes updates when the dialog re-reads with a
    /// new mask. Appended after a `/` when the focused entry is a directory.
    wild_card: String,
}

impl FileInputLine {
    /// `TFileInputLine::TFileInputLine(bounds, aMaxLen)` — build a filename field.
    ///
    /// Faithful: `TInputLine(bounds, aMaxLen)` — `aMaxLen` is the byte cap
    /// ([`LimitMode::MaxBytes`], the C++ default), so `maxLen = aMaxLen - 1`. The
    /// C++ `eventMask |= evBroadcast` is implicit (the group delivers every
    /// broadcast unconditionally), so no event mask is wired. `wild_card` caches
    /// the owner's `wildCard` (which the C++ reads via `owner` on each dir entry).
    pub fn new(bounds: crate::view::Rect, max_len: i32, wild_card: impl Into<String>) -> Self {
        FileInputLine {
            inner: crate::widgets::InputLine::new(
                bounds,
                max_len,
                None,
                crate::widgets::LimitMode::MaxBytes,
            ),
            wild_card: wild_card.into(),
        }
    }

    /// Write the focused record into the field — the body of the C++
    /// `handleEvent`'s `cmFileFocused` block, called by the pump's
    /// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) broker
    /// once it has resolved the producer's focused [`SearchRec`].
    ///
    /// Faithful: `strcpy(data, rec->name)`; if `rec->attr & FA_DIREC`, then
    /// `strcat(data, "/"); strcat(data, wildCard)` (D14 `/`, not `\`); then
    /// `selectAll(False)`. `None` is the C++ all-zero `noFile` sentinel (empty
    /// name) → a blank field. `drawView()` is dropped (D8).
    pub fn on_file_focused(&mut self, rec: Option<SearchRec>) {
        let text = match rec {
            Some(r) if r.attr & FA_DIREC != 0 => format!("{}/{}", r.name, self.wild_card),
            Some(r) => r.name,
            None => String::new(),
        };
        // C++ `strcpy(data, …)` does not clamp in this path; assign directly
        // (InputLine exposes no clamping text-setter, and `data` is `pub`).
        self.inner.data = text;
        // selectAll(False) — the C++ shorthand `selectAll(Boolean)` defaults
        // `scroll = True`, so this is `select_all(false, true)`.
        self.inner.select_all(false, true);
        // drawView() dropped (D8 — whole-tree redraw + diff).
    }

    /// Update the cached owner `wildCard` ([`FileDialog::valid`]'s isWild branch
    /// drives this when the dialog re-reads the directory with a new mask, so the
    /// next dir-focus appends the fresh mask rather than the stale one).
    pub fn set_wild_card(&mut self, w: impl Into<String>) {
        self.wild_card = w.into();
    }

    /// The current field text (the C++ `fileName->data`) — the filename the
    /// dialog resolves in [`FileDialog::get_file_name`].
    pub fn text(&self) -> &str {
        &self.inner.data
    }
}

#[crate::delegate(to = inner)]
impl crate::view::View for FileInputLine {
    /// `TFileInputLine::handleEvent` — delegate to `TInputLine::handleEvent`
    /// first, then, on a `cmFileFocused` broadcast while NOT selected (so the copy
    /// never clobbers the field the user is typing in), request the payload broker.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        // TInputLine::handleEvent — the base call (the C++ runs it first).
        self.inner.handle_event(ev, ctx);

        if let crate::event::Event::Broadcast {
            command,
            source: Some(src),
        } = *ev
            && command == crate::command::Command::FILE_FOCUSED
            // !(state & sfSelected) — do NOT clobber the field while the user types.
            && !self.inner.state().state.selected
            && let Some(my_id) = self.inner.state().id()
        {
            ctx.request_resolve_focused_file(my_id, src);
        }
    }

    /// **Override** (the OPPOSITE of [`Memo`](crate::widgets::Memo), which
    /// forwards `as_any_mut` to its editor): the pump's broker downcasts the
    /// resolved subscriber to `FileInputLine`, so `as_any_mut` MUST return
    /// `self`, NOT the inner `InputLine`.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// FileInfoPane — row 78
// ---------------------------------------------------------------------------

/// Month names indexed by the DOS `ft_month` field (1–12); index 0 is the empty
/// string for a noFile/blank rec — `months[]` in stddlg.cpp.
const MONTHS: [&str; 13] = [
    "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// `amText` — the AM-hours suffix on the date line.
const AM: &str = "a";
/// `pmText` — the PM-hours suffix on the date line.
const PM: &str = "p";

/// The DOS-packed time rstv's synthesized `".."` rec carries (`0x210000`):
/// date byte `0x21` → day 1, month 1, year-1980 0 → Jan 01 1980 00:00. Using it
/// (rather than a literal `0`, which would unpack to month 0 / day 0 — a blank
/// month name + a `00` day) keeps the `".."` row's date well-formed.
///
/// **D-time deviation.** rstv synthesizes `".."` without statting the parent, so
/// it uses this constant unconditionally. C++ `readDirectory` only falls back to
/// `0x210000` when its `findfirst("..", FA_DIREC)` fails; in the normal path it
/// shows the real parent dir's mtime. The difference is cosmetic (the `".."`
/// row's displayed date).
const DOTDOT_TIME: i32 = 0x0021_0000;

/// Pack a [`SystemTime`](std::time::SystemTime) into the DOS `ftime` `u32`
/// layout the C++ `findfirst` produced, so the [`FileInfoPane`] draw can unpack
/// it verbatim.
///
/// Layout (high 16 bits = date, low 16 bits = time):
/// - date: `year-1980` in bits 9–15, `month` (1–12) in bits 5–8, `day` (1–31)
///   in bits 0–4.
/// - time: `hour` (0–23) in bits 11–15, `min` (0–59) in bits 5–10, `sec/2` in
///   bits 0–4.
///
/// **D-time deviation.** The C++ time came from DOS `findfirst` in **local**
/// time. rstv reads `std::fs` mtime and packs the civil Y/M/D H:M:S in **UTC**
/// (computed via Howard Hinnant's days-from-civil algorithm — no timezone crate
/// dependency). The displayed clock is therefore UTC, not local. Times before
/// the 1980 DOS epoch clamp to Jan 01 1980 00:00 (DOS cannot represent earlier).
fn pack_dos_time(t: &std::time::SystemTime) -> i32 {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        // A pre-epoch mtime is well before the 1980 DOS epoch → clamp.
        Err(_) => return DOTDOT_TIME,
    };
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;

    // Howard Hinnant's civil-from-days (days are relative to 1970-01-01).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    // Clamp to the DOS 1980 epoch (DOS cannot represent earlier dates).
    if y < 1980 {
        return DOTDOT_TIME;
    }

    let date = (((y - 1980) as u32) << 9) | ((m as u32) << 5) | (d as u32);
    let time = ((hour as u32) << 11) | ((min as u32) << 5) | ((sec / 2) as u32);
    ((date << 16) | time) as i32
}

/// `TFileInfoPane` (stddlg.cpp) — a plain `TView` that displays the focused
/// file's path on line 0 and its name + size + date on line 1.
///
/// ## Structural shape
///
/// `TFileInfoPane` is a C++ subclass of `TView` (not `TInputLine`/`TListViewer`),
/// so in rstv it is a **direct [`View`](crate::view::View) impl** over a
/// [`ViewState`](crate::view::ViewState) — *not* a D2 delegate.
///
/// ## The payload broker (D3/D4) — shared with [`FileInputLine`]
///
/// Like `FileInputLine` it subscribes to `cmFileFocused`: on the broadcast it
/// requests [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile)
/// and the pump resolves the producer's
/// [`focused_rec`](FileList::focused_rec), then calls
/// [`on_file_focused`](FileInfoPane::on_file_focused) on this view. Unlike the
/// input line there is **no `!sfSelected` guard** — the pane always updates.
///
/// ## D3 — cached owner fields
///
/// `TFileInfoPane::draw` reads `owner->directory` and `owner->wildCard`. A leaf
/// view cannot read its owner inline (and `draw` has no `Context` at all), so
/// both are **cached** (`directory` / `wild_card`), set at ctor and refreshed by
/// [`set_dir_info`](FileInfoPane::set_dir_info) (row 79 drives that via a
/// deferred push when the dialog re-reads with a new mask). The focused record
/// is cached in `file_block`.
///
/// ## Deviations
/// - **D7:** no palette chain — the text is drawn in [`Role::InfoPane`].
/// - **D8:** the C++ `drawView()` after a focus change is dropped (whole-tree
///   redraw).
/// - **D14:** `fexpand` is dropped — the path line is `directory` concatenated
///   with `wild_card` (the dialog guarantees `directory` ends with `/`); no
///   `\`↔`/` translation.
/// - **D12:** `getPalette`/`write`/`read`/`build`/`streamableName`/`name`
///   streaming dropped.
pub struct FileInfoPane {
    /// The base-view state (bounds, flags, id, …).
    state: crate::view::ViewState,
    /// Cached `((TFileDialog*)owner)->directory` (D3), `/`-terminated.
    directory: String,
    /// Cached `((TFileDialog*)owner)->wildCard` (D3).
    wild_card: String,
    /// The focused record (`file_block`); `None` = the all-zero `noFile`
    /// sentinel → a blank name → no size/date line.
    file_block: Option<SearchRec>,
}

impl FileInfoPane {
    /// `TFileInfoPane::TFileInfoPane(bounds)` — build the pane. `directory` /
    /// `wild_card` cache the owner fields the draw needs (D3); `file_block`
    /// starts `None` (blank) until the first `cmFileFocused`.
    ///
    /// The C++ `eventMask |= evBroadcast` is implicit (the group delivers every
    /// broadcast unconditionally), so no event mask is wired.
    pub fn new(
        bounds: crate::view::Rect,
        directory: impl Into<String>,
        wild_card: impl Into<String>,
    ) -> Self {
        FileInfoPane {
            state: crate::view::ViewState::new(bounds),
            directory: directory.into(),
            wild_card: wild_card.into(),
            file_block: None,
        }
    }

    /// Cache the focused record — the body of the C++ `handleEvent`'s
    /// `cmFileFocused` block (`file_block = *infoPtr`), called by the pump's
    /// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) broker.
    /// `None` is the all-zero `noFile` sentinel → a blank name. `drawView()` is
    /// dropped (D8).
    pub fn on_file_focused(&mut self, rec: Option<SearchRec>) {
        self.file_block = rec;
    }

    /// Update the cached owner `directory` / `wildCard` ([`FileDialog`]'s
    /// `reset_current` and [`FileDialog::valid`]'s navigate branches drive this
    /// when the dialog (re-)reads the directory).
    pub fn set_dir_info(&mut self, directory: impl Into<String>, wild_card: impl Into<String>) {
        self.directory = directory.into();
        self.wild_card = wild_card.into();
    }
}

impl crate::view::View for FileInfoPane {
    fn state(&self) -> &crate::view::ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut crate::view::ViewState {
        &mut self.state
    }

    /// `TFileInfoPane::draw` — line 0 = the `directory`+`wildCard` path; line 1 =
    /// the focused name + (when named) its size left-aligned at `size.x-38` and
    /// its date as `Mon DD, YYYY HH:MMa/p` in the right columns; remaining rows
    /// cleared. All text in [`Role::InfoPane`].
    fn draw(&mut self, ctx: &mut crate::view::DrawCtx) {
        let color = ctx.style(crate::theme::Role::InfoPane);
        let w = self.state.size.x;
        let h = self.state.size.y;

        // --- line 0: the path (directory + wildCard, D14 — no fexpand) ------
        // `Rect::new` is `TRect(ax, ay, bx, by)` — corners, not (x, y, w, h).
        ctx.fill(crate::view::Rect::new(0, 0, w, 1), ' ', color);
        let path = format!("{}{}", self.directory, self.wild_card);
        ctx.put_str(1, 0, &path, color);

        // --- line 1: name + size + date -------------------------------------
        ctx.fill(crate::view::Rect::new(0, 1, w, 2), ' ', color);
        if let Some(rec) = &self.file_block
            && !rec.name.is_empty()
        {
            ctx.put_str(1, 1, &rec.name, color);

            // size, left-aligned at column size.x - 38.
            let size_str = rec.size.to_string();
            ctx.put_str(w - 38, 1, &size_str, color);

            // Unpack the DOS ftime bitfield (see `pack_dos_time` for the layout).
            let t = rec.time as u32;
            let date = t >> 16;
            let time = t & 0xFFFF;
            let month = ((date >> 5) & 0xF) as usize;
            let day = date & 0x1F;
            let year = (date >> 9) + 1980;
            let mut hour = time >> 11;
            let minute = (time >> 5) & 0x3F;

            // month name at size.x - 22 (index guarded — 0..=12).
            let month_name = MONTHS.get(month).copied().unwrap_or("");
            ctx.put_str(w - 22, 1, month_name, color);

            // zero-padded day at size.x - 18.
            ctx.put_str(w - 18, 1, &format!("{day:02}"), color);
            ctx.put_char(w - 16, 1, ',', color);
            // year (already +1980) at size.x - 15.
            ctx.put_str(w - 15, 1, &year.to_string(), color);

            // 12-hour clock with a/p suffix. The C++ `time->ft_hour %= 12`
            // mutates `file_block` through an aliased pointer (a latent C++ bug);
            // rstv computes into the local `hour`, correctly NOT reproducing it
            // (required under D8 — draw must not mutate cached state).
            let pm = hour >= 12;
            hour %= 12;
            if hour == 0 {
                hour = 12;
            }
            ctx.put_str(w - 9, 1, &format!("{hour:02}"), color);
            ctx.put_char(w - 7, 1, ':', color);
            ctx.put_str(w - 6, 1, &format!("{minute:02}"), color);
            ctx.put_str(w - 4, 1, if pm { PM } else { AM }, color);
        }

        // --- clear the rest (rows 2..h) -------------------------------------
        if h > 2 {
            ctx.fill(crate::view::Rect::new(0, 2, w, h), ' ', color);
        }
    }

    /// `TFileInfoPane::handleEvent` — on a `cmFileFocused` broadcast (with a
    /// resolvable source) request the payload broker. No `!sfSelected` guard
    /// (unlike the input line) — the pane always reflects the focused file.
    ///
    /// The C++ `TView::handleEvent(event)` base call is dropped (inert — the base
    /// only does the `ofSelectable` mouse-select the pane lacks).
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        if let crate::event::Event::Broadcast {
            command,
            source: Some(src),
        } = *ev
            && command == crate::command::Command::FILE_FOCUSED
            && let Some(my_id) = self.state.id()
        {
            ctx.request_resolve_focused_file(my_id, src);
        }
    }

    /// The pump's broker downcasts the resolved subscriber to `FileInfoPane`, so
    /// `as_any_mut` MUST return `self`.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// FileDialog — row 79 (skeleton: B1)
// ---------------------------------------------------------------------------

use crate::dialog::Dialog;

/// Display width of `s`, skipping `~` hotkey markers — the C++ `cstrlen` loop
/// used to size the `inputName` label (`3 + cstrlen(inputName)`).
fn cstrlen(s: &str) -> i32 {
    s.chars()
        .filter(|&c| c != '~')
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as i32)
        .sum()
}

/// `MAXPATH` — the filename field's byte cap (DOS `MAXPATH`; rstv uses 255).
const MAXPATH: i32 = 255;

// ---------------------------------------------------------------------------
// Path helpers (D14 — native `/` paths). Pure, FS-independent unless noted.
// ---------------------------------------------------------------------------

/// `fexpand(buf, directory)` (D14) — resolve `input` against `dir`, lexically.
///
/// If `input` is absolute (starts with `/`) it stands alone; otherwise it is
/// joined onto `dir`. The result is then **lexically normalized** — `.` is
/// dropped, `..` pops the previous component, and `//` collapses — by walking
/// [`Path::components`]. This is **not** [`std::fs::canonicalize`]: it does no
/// FS access and does not resolve symlinks (faithful `fexpand` is purely
/// textual). A trailing `/` on the input is preserved in the returned string so
/// the bare-directory test ([`split_dir_file`] / [`FileDialog::get_file_name`])
/// can detect a directory-only path.
///
/// An **empty `dir`** with a relative `input` yields a *relative* path (the join
/// is just `input`). That is only reachable before a caller sets `directory`
/// (e.g. under `FD_NO_LOAD_DIR`, where `reset_current`'s initial read is
/// skipped) — matching C++'s empty initial `directory`.
fn expand_path(dir: &str, input: &str) -> String {
    use std::path::{Component, Path, PathBuf};

    let joined: PathBuf = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        Path::new(dir).join(input)
    };

    let mut out = PathBuf::new();
    for comp in joined.components() {
        match comp {
            Component::Prefix(_) => {} // no Windows prefixes under D14
            Component::RootDir => out.push("/"),
            Component::CurDir => {} // "." — drop
            Component::ParentDir => {
                // Pop the previous Normal component. At/above an absolute root,
                // `..` is absorbed (a `/..`-walk stays at `/`), matching fexpand.
                match out.components().next_back() {
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    Some(Component::RootDir) => {} // `/..` → `/`
                    _ => out.push(".."),           // relative leading `..`
                }
            }
            Component::Normal(seg) => out.push(seg),
        }
    }

    let mut s = out.to_string_lossy().into_owned();
    if s.is_empty() {
        s.push('/');
    }
    // Preserve a trailing slash from the input (a directory-only path) so the
    // wildcard-append in get_file_name fires. `components()` strips it.
    if (input.ends_with('/') || input.is_empty()) && !s.ends_with('/') {
        s.push('/');
    }
    s
}

/// `isWild(s)` — `s` contains a `*` or `?` glob metacharacter.
fn is_wild(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}

/// Whether `path` is **directory-only** — has no filename part (the C++ `name`
/// AND `ext` both empty). True when `path` ends with `/`, is `/`, is empty, or
/// its final component is `.`/`..`.
///
/// NOTE: [`Path::file_name`] alone is **wrong** here — it strips a trailing
/// slash, so `Path::new("/a/b/").file_name()` is `Some("b")`. We test the
/// trailing slash explicitly first.
fn is_dir_only(path: &str) -> bool {
    use std::path::{Component, Path};
    if path.is_empty() || path.ends_with('/') {
        return true;
    }
    matches!(
        Path::new(path).components().next_back(),
        Some(Component::RootDir | Component::CurDir | Component::ParentDir) | None
    )
}

/// `fnsplit` (the dir-vs-file split, D14) — split an absolute, normalized path
/// into its directory part (with a trailing `/`) and its filename part.
///
/// A path that ends with `/` (or whose final component is `.`/`..`) has an empty
/// filename — equivalently [`Path::file_name`] is `None`. The dir part always
/// ends with `/` so it satisfies [`FileList::read_directory`]'s precondition.
fn split_dir_file(path: &str) -> (String, String) {
    use std::path::Path;
    if is_dir_only(path) {
        // Directory-only path — the whole thing is the dir, empty file.
        let mut d = path.to_string();
        if !d.ends_with('/') {
            d.push('/');
        }
        return (d, String::new());
    }
    let p = Path::new(path);
    let file = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = match p.parent() {
        Some(par) => {
            let mut d = par.to_string_lossy().into_owned();
            if !d.ends_with('/') {
                d.push('/');
            }
            d
        }
        None => "/".to_string(),
    };
    (dir, file)
}

/// `isDir(s)` — `s` names an existing directory (stat-follows-symlinks, like the
/// C++ `stat`). FS-dependent.
fn is_dir(s: &str) -> bool {
    std::fs::metadata(s).map(|m| m.is_dir()).unwrap_or(false)
}

/// `pathValid(str)` — the directory `str` exists and is a directory. FS-dependent.
/// (D14 simplification of the C++ `pathValid`, which validated DOS drive letters.)
fn path_valid(s: &str) -> bool {
    std::fs::metadata(s).map(|m| m.is_dir()).unwrap_or(false)
}

/// `validFileName(s)` (D14 simplification) — `s` has a non-empty filename
/// component and no interior NUL. The C++ `validFileName` enforced DOS 8.3 /
/// charset rules we do not need on Linux; a non-empty name with a real parent is
/// plausible enough, and the OS rejects a truly invalid name at open time.
fn valid_file_name(s: &str) -> bool {
    if s.is_empty() || s.contains('\0') {
        return false;
    }
    // A directory-only path (trailing `/`, or final `.`/`..`) has no filename.
    !is_dir_only(s)
}

// Validation message text (tfildlg.cpp).
const INVALID_DRIVE_TEXT: &str = "Invalid drive or directory";
const INVALID_FILE_TEXT: &str = "Invalid file name";

/// `fdOKButton` — insert an "OK" button (`cmFileOpen`).
pub const FD_OK_BUTTON: u16 = 0x0001;
/// `fdOpenButton` — insert an "Open" button (`cmFileOpen`).
pub const FD_OPEN_BUTTON: u16 = 0x0002;
/// `fdReplaceButton` — insert a "Replace" button (`cmFileReplace`).
pub const FD_REPLACE_BUTTON: u16 = 0x0004;
/// `fdClearButton` — insert a "Clear" button (`cmFileClear`).
pub const FD_CLEAR_BUTTON: u16 = 0x0008;
/// `fdHelpButton` — insert a "Help" button (`cmHelp`).
pub const FD_HELP_BUTTON: u16 = 0x0010;
/// `fdNoLoadDir` — skip the initial `readDirectory` on open.
pub const FD_NO_LOAD_DIR: u16 = 0x0100;

// --- TChDirDialog options (row 80) -----------------------------------------

/// `cdNormal` — no extra buttons, load the directory on open.
pub const CD_NORMAL: u16 = 0x0000;
/// `cdNoLoadDir` — skip the initial `setUpDialog` (directory read) on open.
pub const CD_NO_LOAD_DIR: u16 = 0x0001;
/// `cdHelpButton` — insert a "Help" button (`cmHelp`).
pub const CD_HELP_BUTTON: u16 = 0x0002;

// `TChDirDialog` text (tvtext2.cpp). `drivesText` is DROPPED (D14 — no drives).
const CHANGE_DIR_TITLE: &str = "Change Directory";
const DIR_NAME_TEXT: &str = "Directory ~n~ame";
const DIR_TREE_TEXT: &str = "Directory ~t~ree";
const CHDIR_TEXT: &str = "~C~hdir";
const REVERT_TEXT: &str = "~R~evert";
const INVALID_DIR_TEXT: &str = "Invalid directory";

// Button / label text. `~X~` is rstv's hotkey markup (the widgets parse it).
const FILES_TEXT: &str = "~F~iles";
const OPEN_TEXT: &str = "~O~pen";
const OK_TEXT: &str = "O~K~";
const REPLACE_TEXT: &str = "~R~eplace";
const CLEAR_TEXT: &str = "~C~lear";
const CANCEL_TEXT: &str = "Cancel";
const HELP_TEXT: &str = "~H~elp";

/// The action-button layout for a given `options` mask — the pure decision the
/// ctor realizes into `TButton`s. Each tuple is `(text, command, is_default,
/// y_top)`.
///
/// Faithful to the C++ ctor loop (tfildlg.cpp): `opt` starts `bfDefault`; each
/// *option* button (Open/OK/Replace/Clear, in that order) consumes the default
/// then resets `opt` to `bfNormal`, so only the **first** option button is the
/// default. **Cancel is always inserted, and Cancel/Help are hardcoded
/// `bfNormal`** — they never participate in the default chain (so a dialog with
/// no option buttons has no default button at all). `r` starts at `(35,3,46,5)`
/// and steps `+3` in y on every button (`y_top` = 3, 6, 9, …).
fn button_specs(options: u16) -> Vec<(&'static str, crate::command::Command, bool, i32)> {
    use crate::command::Command;
    let mut specs: Vec<(&'static str, Command, bool, i32)> = Vec::new();
    let mut y = 3;
    let mut default = true;
    // `is_opt` = participates in the bfDefault chain (the four option buttons).
    let push = |specs: &mut Vec<_>, y: &mut i32, default: &mut bool, text, cmd, is_opt: bool| {
        let is_default = is_opt && *default;
        specs.push((text, cmd, is_default, *y));
        if is_opt {
            *default = false;
        }
        *y += 3;
    };

    if options & FD_OPEN_BUTTON != 0 {
        push(
            &mut specs,
            &mut y,
            &mut default,
            OPEN_TEXT,
            Command::FILE_OPEN,
            true,
        );
    }
    if options & FD_OK_BUTTON != 0 {
        push(
            &mut specs,
            &mut y,
            &mut default,
            OK_TEXT,
            Command::FILE_OPEN,
            true,
        );
    }
    if options & FD_REPLACE_BUTTON != 0 {
        push(
            &mut specs,
            &mut y,
            &mut default,
            REPLACE_TEXT,
            Command::FILE_REPLACE,
            true,
        );
    }
    if options & FD_CLEAR_BUTTON != 0 {
        push(
            &mut specs,
            &mut y,
            &mut default,
            CLEAR_TEXT,
            Command::FILE_CLEAR,
            true,
        );
    }
    // Cancel: always inserted, hardcoded bfNormal (is_opt = false).
    push(
        &mut specs,
        &mut y,
        &mut default,
        CANCEL_TEXT,
        Command::CANCEL,
        false,
    );
    if options & FD_HELP_BUTTON != 0 {
        push(
            &mut specs,
            &mut y,
            &mut default,
            HELP_TEXT,
            Command::HELP,
            false,
        );
    }
    specs
}

/// `TFileDialog` (tfildlg.cpp) — a [`Dialog`] that assembles the row-76/77/78
/// widgets ([`FileList`], [`FileInputLine`], [`FileInfoPane`]) plus a filename
/// label, a history icon, a scroll bar, and the action buttons into a working
/// file picker.
///
/// ## Structural shape (D2 embed-and-delegate)
///
/// `TFileDialog` is a C++ subclass of `TDialog`. In rstv it **embeds** a
/// [`Dialog`] and forwards the un-overridden [`View`](crate::view::View) methods
/// via `#[crate::delegate(to = dialog)]`. It overrides only `handle_event`,
/// `size_limits`, `reset_current`, and `as_any_mut` (so the modal loop / the
/// owner-downcast target is the `FileDialog`, not the inner `Dialog`).
/// `calc_bounds` is **skip-listed** (left at the trait default) so an
/// owner-driven resize routes through this type's `size_limits` 49×19 floor —
/// mirroring the `EditWindow` precedent.
///
/// ## D14 — native Linux paths
///
/// The initial directory comes from [`std::env::current_dir`] (not DOS
/// `getCurDir`), normalized to end with `/` (the [`FileList::read_directory`]
/// trailing-slash precondition). No drive letters, no `\`.
///
/// ## Deferrals
///
/// - The "21st-century percentages" screen-relative resize block — applied at
///   the first `handle_event` call (when `ctx.owner_size()` is available), not
///   at ctor time (D3: no `Context` in ctor). See
///   [`needs_screen_resize`](FileDialog::needs_screen_resize) + the deviation
///   note in `handle_event`.
/// - **D12:** `TStreamable` (`write`/`read`/`build`/`streamableName`) dropped.
pub struct FileDialog {
    /// The embedded `TDialog` — the D2 delegation target.
    dialog: Dialog,
    /// `wildCard` — the active file mask (cached; pushed to the children).
    wild_card: String,
    /// `directory` — set by `reset_current`'s initial `readDirectory` (D14,
    /// `/`-terminated).
    directory: String,
    /// The [`FileInputLine`] child's id — read by
    /// [`get_file_name`](FileDialog::get_file_name)/[`valid`](FileDialog::valid)
    /// (the filename the dialog returns).
    file_name_id: crate::view::ViewId,
    /// The [`FileList`] child's id.
    file_list_id: crate::view::ViewId,
    /// The [`FileInfoPane`] child's id.
    info_pane_id: crate::view::ViewId,
    /// One-time guard for the `reset_current` initial `readDirectory`.
    needs_read_directory: bool,
    /// One-time guard for the "21st-century percentages" screen-relative resize.
    ///
    /// The C++ ctor applies this immediately (it has `TProgram::application->size`).
    /// In rstv a `Context` is required to get the screen size via `ctx.owner_size()`
    /// and to queue the `Deferred::ChangeBounds` — neither is available at ctor
    /// time (D3: no `Context` in ctor). **D-deviation from C++**: the resize fires
    /// at the first `handle_event` call where `ctx.owner_size()` is non-zero,
    /// before the first dispatch to children — observably identical to C++ (both
    /// happen before the first draw).
    needs_screen_resize: bool,
    /// Cache of the last [`get_file_name`](FileDialog::get_file_name) result, so
    /// the `&self` [`value`](FileDialog::value) (D10 `getData`) can return the
    /// resolved filename. `get_file_name` needs `&mut self` (it reads the input
    /// line via `child_mut`), and an immutable `child` accessor would live
    /// outside this module — so `valid()` (the gate the modal gather runs right
    /// before reading `value()`) refreshes this cache. Invariant: the cache is
    /// current after any `valid()` call.
    resolved_name: String,
}

impl FileDialog {
    /// `TFileDialog::TFileDialog(wildCard, title, inputName, aOptions, histId)`.
    ///
    /// Assembles the children in the **exact C++ insertion order** (so the
    /// labels/history can link to the already-captured input-line / file-list
    /// ids, and `reset_current` focuses the first selectable = the input line).
    /// Bounds are verbatim from the C++; `grow_mode` is set per the C++ table on
    /// each child before insert (faithful completeness — resize is not exercised
    /// in a fixed-size snapshot).
    pub fn new(
        wild_card: impl Into<String>,
        title: impl Into<String>,
        input_name: impl Into<String>,
        options: u16,
        history_id: u8,
    ) -> Self {
        use crate::view::{GrowMode, Rect, View};
        use crate::widgets::{Button, ButtonFlags, Label, ScrollBar, THistory};

        let wild_card = wild_card.into();
        let input_name = input_name.into();

        // TDialog(TRect(15,1,64,20), title); options |= ofCentered; flags |= wfGrow.
        let mut dialog = Dialog::new(Rect::new(15, 1, 64, 20), Some(title.into()));
        // ofCentered — reachable via the public View::state_mut() → ViewState.
        {
            let opts = &mut dialog.state_mut().options;
            opts.center_x = true;
            opts.center_y = true;
        }
        // flags |= wfGrow — faithful C++ port; Dialog::set_flags/flags are
        // pub(crate) accessors added on Dialog for this row (row 79 B6).
        {
            let mut f = dialog.flags();
            f.grow = true;
            dialog.set_flags(f);
        }

        // --- fileName: TFileInputLine(TRect(3,3,31,4), MAXPATH) --------------
        // strnzcpy(fileName->data, wildCard) — initial text = the wildcard.
        // growMode = gfGrowHiX.
        let mut fil = FileInputLine::new(Rect::new(3, 3, 31, 4), MAXPATH, wild_card.clone());
        fil.inner.data = wild_card.clone();
        fil.inner.state.grow_mode = GrowMode {
            hi_x: true,
            ..Default::default()
        };
        let file_name_id = dialog.insert_child(Box::new(fil));

        // --- TLabel(TRect(2,2,3+len(inputName),3), inputName, fileName) ------
        // growMode = 0.
        let label_w = 3 + cstrlen(&input_name);
        dialog.insert_child(Box::new(Label::new(
            Rect::new(2, 2, label_w, 3),
            input_name,
            Some(file_name_id),
        )));

        // --- THistory(TRect(31,3,34,4), fileName, histId) -------------------
        // growMode = gfGrowLoX | gfGrowHiX.
        let mut hist = THistory::new(Rect::new(31, 3, 34, 4), file_name_id, history_id);
        hist.state_mut().grow_mode = GrowMode {
            lo_x: true,
            hi_x: true,
            ..Default::default()
        };
        dialog.insert_child(Box::new(hist));

        // --- TScrollBar(TRect(3,14,34,15)) ----------------------------------
        let sb_id = dialog.insert_child(Box::new(ScrollBar::new(Rect::new(3, 14, 34, 15))));

        // --- fileList: TFileList(TRect(3,6,34,14), sb) ----------------------
        // growMode = gfGrowHiX | gfGrowHiY.
        let mut fl = FileList::new(Rect::new(3, 6, 34, 14), None, Some(sb_id));
        fl.lv.state.grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        let file_list_id = dialog.insert_child(Box::new(fl));

        // --- TLabel(TRect(2,5,8,6), filesText, fileList) --------------------
        // growMode = 0.
        dialog.insert_child(Box::new(Label::new(
            Rect::new(2, 5, 8, 6),
            FILES_TEXT,
            Some(file_list_id),
        )));

        // --- the action buttons ---------------------------------------------
        // The which/order/default/y decision is the pure `button_specs` helper;
        // here we just realize each spec into a TButton at `r(35, y, 46, y+2)`
        // (growMode = gfGrowLoX | gfGrowHiX). r steps +3 in y per button.
        let grow_lo_hi_x = GrowMode {
            lo_x: true,
            hi_x: true,
            ..Default::default()
        };
        for (text, cmd, is_default, y) in button_specs(options) {
            let flags = if is_default {
                ButtonFlags {
                    default: true,
                    ..Default::default()
                }
            } else {
                ButtonFlags::new()
            };
            let mut b = Button::new(Rect::new(35, y, 46, y + 2), text, cmd, flags);
            b.state.grow_mode = grow_lo_hi_x;
            dialog.insert_child(Box::new(b));
        }

        // --- infoPane: TFileInfoPane(TRect(1,16,48,18)) ---------------------
        // growMode = gfGrowAll & ~gfGrowLoX = { lo_y, hi_x, hi_y }.
        let mut fip = FileInfoPane::new(Rect::new(1, 16, 48, 18), "", wild_card.clone());
        fip.state_mut().grow_mode = GrowMode {
            lo_y: true,
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        let info_pane_id = dialog.insert_child(Box::new(fip));

        // selectNext(False): the modal loop's reset_current establishes currency
        // (focuses the first selectable child = the input line, inserted first),
        // so no explicit selectNext is needed here (see View::reset_current).

        // The "21st-century percentages" screen-relative resize (C++ ctor lines
        // 141-167) is deferred to the first handle_event (needs ctx.owner_size()).
        // See the `needs_screen_resize` field doc for the full deviation note.

        FileDialog {
            dialog,
            wild_card,
            directory: String::new(),
            file_name_id,
            file_list_id,
            info_pane_id,
            needs_read_directory: options & FD_NO_LOAD_DIR == 0,
            needs_screen_resize: true,
            resolved_name: String::new(),
        }
    }

    /// `TFileDialog::getFileName` — resolve the input-line text into an absolute,
    /// normalized filename relative to `directory`.
    ///
    /// Faithful: `trim` (a no-op under `__FLAT__`) → `fexpand(buf, directory)` →
    /// `fnsplit`; when the resolved path is a **bare directory** (no filename
    /// part) the wildcard's filename is appended. Under D14 the wildcard *is* its
    /// own filename (no drive/ext split), so we append `self.wild_card` directly.
    ///
    /// `&mut self` (not `&self`): it reads the [`FileInputLine`] via `child_mut`.
    /// It is only called from [`valid`](FileDialog::valid) (which has `&mut self`)
    /// and refreshes [`resolved_name`](FileDialog::resolved_name) so the `&self`
    /// [`value`](FileDialog::value) can return the result without `child_mut`.
    pub fn get_file_name(&mut self) -> String {
        let field_text = self
            .dialog
            .child_mut(self.file_name_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInputLine>())
            .map(|fil| fil.text().to_string())
            .unwrap_or_default();

        let expanded = expand_path(&self.directory, &field_text);

        // fnsplit name+ext-empty check → append the wildcard's filename when the
        // resolved path is directory-only (see `is_dir_only` — `Path::file_name`
        // alone is wrong, it strips a trailing slash).
        let resolved = if is_dir_only(&expanded) {
            let mut s = expanded;
            if !s.ends_with('/') {
                s.push('/');
            }
            s.push_str(&self.wild_card);
            s
        } else {
            expanded
        };

        self.resolved_name = resolved.clone();
        resolved
    }

    /// `TFileDialog::checkDirectory` — `pathValid(path)` ? true : (pop the
    /// "invalid drive/dir" box + refocus the filename field + false).
    ///
    /// The box is informational (`mfError|mfOKButton`, no answer routing) via the
    /// async-modal-from-a-view seam: `valid()` requests it and returns false;
    /// `validate_modal_close` drives it inline and keeps the (false) valid.
    fn check_directory(&mut self, path: &str, ctx: &mut crate::view::Context) -> bool {
        if path_valid(path) {
            true
        } else {
            ctx.request_message_box(
                format!("{INVALID_DRIVE_TEXT}: '{path}'"),
                crate::dialog::MessageBoxKind::Error,
                crate::dialog::MessageBoxButtons::ok(),
                None,
                None,
            );
            ctx.request_focus(self.file_name_id);
            false
        }
    }

    /// Re-read `directory` into the [`FileList`] and refresh the info pane /
    /// input-line caches — the navigation tail shared by the isWild and isDir
    /// branches of [`valid`](FileDialog::valid). Direct child mutation (the dialog
    /// owns the group), like `reset_current`.
    fn navigate(&mut self, ctx: &mut crate::view::Context) {
        let dir = self.directory.clone();
        let wild = self.wild_card.clone();
        if let Some(fl) = self
            .dialog
            .child_mut(self.file_list_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileList>())
        {
            fl.read_directory(&dir, &wild, ctx);
        }
        if let Some(fip) = self
            .dialog
            .child_mut(self.info_pane_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInfoPane>())
        {
            fip.set_dir_info(&dir, &wild);
        }
        // Refresh the input line's cached wildCard (the C++ reads owner->wildCard
        // live on each dir-focus; without this the next dir-focus would append
        // the stale mask). The isDir branch leaves wild_card unchanged, so this is
        // a no-op there.
        if let Some(fil) = self
            .dialog
            .child_mut(self.file_name_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInputLine>())
        {
            fil.set_wild_card(&wild);
        }
    }
}

#[crate::delegate(
    to = dialog,
    skip(
        apply_list_scroll,
        as_any_mut,
        calc_bounds,
        grabs_focus_on_click,
        select_window_num,
        set_value,
        size_limits,
        value
    )
)]
impl crate::view::View for FileDialog {
    /// `TFileDialog::handleEvent` — delegate to `TDialog::handleEvent` first (the
    /// faithful base call), then:
    /// - `cmFileOpen`/`cmFileReplace`/`cmFileClear` → `endModal(command)` + clear.
    ///   (B2 inserts the `valid()` path-check gate before accepting.)
    /// - `cmFileDoubleClicked` broadcast → re-inject as `cmOK` (`putEvent`) +
    ///   clear. The base `TDialog::handleEvent` then turns `cmOK` into
    ///   `endModal(cmOK)` on the next cycle (the dialog is modal).
    ///
    /// **One-time pre-delegate work** (before the base call): if
    /// `needs_screen_resize` is true and the screen size is available from
    /// `ctx.owner_size()`, applies the C++ ctor's "21st-century percentages"
    /// resize formula (C++ tfildlg.cpp lines 141-167). D-deviation: the C++
    /// ctor runs this immediately against `TProgram::application->size`; rstv
    /// defers it to the first handle_event where `ctx.owner_size()` is non-zero
    /// (set by the owning group's handle_event bracket). Before-first-draw
    /// ordering is preserved.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        use crate::command::Command;
        use crate::event::Event;

        // "21st-century percentages" screen-relative resize (C++ ctor lines
        // 141-167 in tfildlg.cpp). D-deviation: the C++ ctor runs this
        // immediately against `TProgram::application->size`; rstv defers it to
        // the first `handle_event` where `ctx.owner_size()` (set by the owning
        // group's `handle_event` bracket) is non-zero. Before-first-draw
        // ordering is preserved: no event is dispatched to children before this
        // fires.
        if self.needs_screen_resize {
            let screen_size = ctx.owner_size();
            if screen_size.x > 0 {
                self.needs_screen_resize = false;
                let original = crate::view::View::state(self).get_bounds();
                let mut bounds = original;
                let screen_bounds = crate::view::Rect::new(0, 0, screen_size.x, screen_size.y);
                if screen_size.x > 90 {
                    bounds.grow(15, 0); // new width 79
                } else if screen_size.x > 63 {
                    let mut sb = screen_bounds;
                    sb.grow(-7, 0);
                    bounds.a.x = sb.a.x;
                    bounds.b.x = sb.b.x;
                }
                if screen_size.y > 34 {
                    bounds.grow(0, 5); // new height 29
                } else if screen_size.y > 25 {
                    let mut sb = screen_bounds;
                    sb.grow(0, -3);
                    bounds.a.y = sb.a.y;
                    bounds.b.y = sb.b.y;
                }
                // Apply the size floor (49x19 — FileDialog::size_limits min).
                let w = (bounds.b.x - bounds.a.x).max(49);
                let h = (bounds.b.y - bounds.a.y).max(19);
                bounds.b.x = bounds.a.x + w;
                bounds.b.y = bounds.a.y + h;
                // Only queue if bounds actually changed (mirrors C++ `locate`'s
                // `if (bounds != getBounds())` guard).
                if bounds != original
                    && let Some(id) = crate::view::View::state(self).id()
                {
                    ctx.request_bounds(id, bounds);
                }
            }
        }

        // TDialog::handleEvent FIRST (faithful order).
        self.dialog.handle_event(ev, ctx);

        match *ev {
            // The path-check gate is `valid()`, NOT a manual call here: the modal
            // loop runs `validate_modal_close → valid(endState)` on this
            // `end_modal`, so implementing `valid()` *is* the gate. A manual
            // pre-check would double-validate.
            Event::Command(c)
                if matches!(
                    c,
                    Command::FILE_OPEN | Command::FILE_REPLACE | Command::FILE_CLEAR
                ) =>
            {
                ctx.end_modal(c);
                ev.clear();
            }
            // cmFileDoubleClicked -> re-inject cmOK (putEvent == ctx.post), clear.
            Event::Broadcast {
                command: Command::FILE_DOUBLE_CLICKED,
                ..
            } => {
                ctx.post(Command::OK);
                ev.clear();
            }
            _ => {}
        }
    }

    /// `TFileDialog::sizeLimits` — `min = {49, 19}` (the base dialog size);
    /// `max` from the embedded dialog. `calc_bounds` is in the skip list so an
    /// owner-driven resize routes through this floor (the `EditWindow` pattern).
    fn size_limits(
        &self,
        owner_size: crate::view::Point,
    ) -> (crate::view::Point, crate::view::Point) {
        let (_min, max) = crate::view::View::size_limits(&self.dialog, owner_size);
        (crate::view::Point::new(49, 19), max)
    }

    /// The ctx-bearing init hook (the C++ ctor's trailing `readDirectory()` maps
    /// here — the ctor has no `Context`). Establishes the dialog's internal
    /// currency first (focuses the input line), then, once, performs the initial
    /// `readDirectory`: the current dir (D14, `/`-terminated) is read into the
    /// [`FileList`] (ctx-ful — scrollbar sync + `cmFileFocused` broadcast) and the
    /// [`FileInfoPane`]'s cached dir/wildcard are refreshed (direct owner-state
    /// push, not a cross-view broker — the dialog owns the group).
    fn reset_current(&mut self, ctx: &mut crate::view::Context) {
        self.dialog.reset_current(ctx);

        if self.needs_read_directory {
            self.needs_read_directory = false;
            // D14: the initial directory from std::env::current_dir, not getCurDir.
            let dir = std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(String::from))
                .unwrap_or_else(|| "/".into());
            // D14 trailing-slash precondition for FileList::read_directory.
            let dir = if dir.ends_with('/') {
                dir
            } else {
                format!("{dir}/")
            };
            self.directory = dir.clone();
            let wild = self.wild_card.clone();

            // FileList ctx-ful read: builds + scrollbar sync + cmFileFocused
            // broadcast (the broker updates the info pane / input line).
            if let Some(fl) = self
                .dialog
                .child_mut(self.file_list_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileList>())
            {
                fl.read_directory(&dir, &wild, ctx);
            }
            // Owner-state push to the info pane (direct child mutation — the dialog
            // owns the group, so this is NOT a cross-view broker).
            if let Some(fip) = self
                .dialog
                .child_mut(self.info_pane_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileInfoPane>())
            {
                fip.set_dir_info(&dir, &wild);
            }
        }
    }

    /// The modal loop and any owner-downcast target must reach the `FileDialog`,
    /// so `as_any_mut` returns `self`, NOT the inner `Dialog`.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `TFileDialog::valid` — the navigate/accept gate, ported faithfully.
    ///
    /// The nested structure mirrors the C++:
    /// - `cmValid` (rstv `Command::VALID`, the C++ `command == 0`) → `true`
    ///   immediately, *before* the base call.
    /// - else run the base `TDialog::valid` (group children) first; on its `false`
    ///   → `false` (keep open).
    /// - on base-valid, for the accept commands (NOT `cmCancel`/`cmFileClear`):
    ///   resolve the filename, then
    ///   - **isWild** → NAVIGATE: split into dir+wildcard, `checkDirectory`, on
    ///     success set `directory`/`wild_card` + re-read; always fall through to
    ///     `false` (keep open).
    ///   - **isDir** → NAVIGATE into it: `checkDirectory`, on success append `/`,
    ///     set `directory`, re-read; `false`.
    ///   - **validFileName** → ACCEPT (`true`).
    ///   - else → "invalid file name" box + `false`.
    /// - `cmCancel`/`cmFileClear` → `true` (always valid, *after* the base call —
    ///   `cmFileClear` must still pass the group).
    ///
    /// Refreshes [`resolved_name`](FileDialog::resolved_name) via `get_file_name`
    /// for every non-VALID command (faithful to C++ `getData`'s unconditional
    /// `getFileName`), so the `&self` `value()` is current after any such call —
    /// including the cmCancel/cmFileClear path that returns before the branches.
    fn valid(&mut self, cmd: crate::command::Command, ctx: &mut crate::view::Context) -> bool {
        use crate::command::Command;

        // cmValid → True immediately (the C++ `command == 0`), before the base.
        if cmd == Command::VALID {
            return true;
        }

        // TDialog::valid (group children) first.
        if !self.dialog.valid(cmd, ctx) {
            return false;
        }

        // Resolve the filename UNCONDITIONALLY (faithful to C++ `getData`, which
        // calls `getFileName` for every command). Its side effect — refreshing
        // `self.resolved_name` — is what keeps the `&self` `value()` current even
        // on the cmCancel/cmFileClear path that returns before the navigate/accept
        // branches. Harmless for those commands (it only reads the field + caches).
        let f_name = self.get_file_name();

        // cmCancel / cmFileClear are always valid (after the base group-valid).
        if cmd == Command::CANCEL || cmd == Command::FILE_CLEAR {
            return true;
        }

        if is_wild(&f_name) {
            // NAVIGATE: change the wildcard + directory, re-read. Falls through
            // to `false` (keep the dialog open) regardless of checkDirectory.
            let (dir, file) = split_dir_file(&f_name);
            if self.check_directory(&dir, ctx) {
                self.directory = dir;
                self.wild_card = file;
                if cmd != Command::FILE_INIT {
                    ctx.request_focus(self.file_list_id);
                }
                self.navigate(ctx);
            }
            false
        } else if is_dir(&f_name) {
            // NAVIGATE into an existing directory.
            if self.check_directory(&f_name, ctx) {
                let mut dir = f_name;
                if !dir.ends_with('/') {
                    dir.push('/'); // D14: append '/' (the C++ '\\')
                }
                self.directory = dir;
                if cmd != Command::FILE_INIT {
                    ctx.request_focus(self.file_list_id);
                }
                self.navigate(ctx);
            }
            false
        } else if valid_file_name(&f_name) {
            // ACCEPT — a real filename.
            true
        } else {
            ctx.request_message_box(
                format!("{INVALID_FILE_TEXT}: '{f_name}'"),
                crate::dialog::MessageBoxKind::Error,
                crate::dialog::MessageBoxButtons::ok(),
                None,
                None,
            );
            false
        }
    }

    /// `TFileDialog::getData` → `getFileName` (D10): the resolved filename. Reads
    /// the [`resolved_name`](FileDialog::resolved_name) cache, which any non-VALID
    /// `valid()` call refreshes unconditionally (it runs `get_file_name` before
    /// any early-return), so the cache is current after the modal gather's
    /// `valid(endState)` — including the cmCancel/cmFileClear paths.
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Text(self.resolved_name.clone()))
    }

    /// `TFileDialog::setData` (D10) — load the field text from a `FieldValue::Text`.
    ///
    /// The C++ `setData` also runs `valid(cmFileInit)` when the input is wild (to
    /// navigate into the new mask on open) and then `fileName->select()`. Neither
    /// is done here: `set_value` has **no `&mut Context`**, so it cannot drive the
    /// navigate (which needs `read_directory(…, ctx)`) nor request focus.
    /// BREADCRUMB(row 79): the on-wild navigate-on-load + select are deferred until
    /// a ctx-bearing setData seam exists (no current consumer needs them — the
    /// dialog opens with the ctor's wildcard and `reset_current` does the initial
    /// read).
    fn set_value(&mut self, v: crate::data::FieldValue) {
        // Forward to the FileInputLine, whose View::set_value delegates (D2) to
        // the embedded InputLine::set_value — the faithful `TInputLine::setData`
        // (copy text + `selectAll(True)`). We resolve by id rather than
        // downcasting to a concrete type so the delegation runs.
        if let Some(fil) = self.dialog.child_mut(self.file_name_id) {
            crate::view::View::set_value(fil, v);
        }
    }
}

// ---------------------------------------------------------------------------
// ChDirDialog — row 80
// ---------------------------------------------------------------------------

/// `TChDirDialog` (tchdrdlg.cpp) — a [`Dialog`] that lets the user change the
/// process current directory, assembling a path input line, a directory-tree
/// pane ([`DirListBox`]), a history icon, and the OK / Chdir / Revert (and
/// optional Help) buttons.
///
/// ## Structural shape (D2 embed-and-delegate)
///
/// Like [`FileDialog`], `TChDirDialog` is a C++ subclass of `TDialog`; in rstv it
/// **embeds** a [`Dialog`] and forwards the un-overridden
/// [`View`](crate::view::View) methods via `#[crate::delegate(to = dialog)]`. It
/// overrides only `handle_event`, `size_limits`, `reset_current`, and
/// `as_any_mut`. `value`/`set_value` are **skip-listed** (left at the trait
/// default — `None` / no-op) because the C++ `dataSize()` returns 0 (the dialog
/// carries no transfer data); skipping them stops the macro forwarding to the
/// inner `Dialog`'s gather/scatter. `calc_bounds` is skip-listed so an
/// owner-driven resize routes through this type's `size_limits` 48×18 floor.
///
/// ## D14 — native Linux paths
///
/// `/`-separated, root `/`, no drives / `\` / "Drives" entry. The initial
/// directory comes from [`std::env::current_dir`]; `cmRevert` re-reads the
/// **live** cwd (not a saved baseline — there is no `directory` field); `valid`'s
/// accept does the real `chdir` via [`std::env::set_current_dir`].
///
/// ## Deferrals
///
/// - **D12:** `TStreamable` (`write`/`read`/`build`/`streamableName`) dropped.
pub struct ChDirDialog {
    /// The embedded `TDialog` — the D2 delegation target.
    dialog: Dialog,
    /// The path [`InputLine`](crate::widgets::InputLine) child's id (`dirInput`).
    dir_input_id: crate::view::ViewId,
    /// The [`DirListBox`] child's id (`dirList`).
    dir_list_id: crate::view::ViewId,
    /// The Chdir [`Button`](crate::widgets::Button)'s id (`chDirButton`) — wired
    /// into the dir list so its focus changes (un-)default it.
    chdir_button_id: crate::view::ViewId,
    /// One-time guard for the `reset_current` initial `setUpDialog`
    /// (`(opts & cdNoLoadDir) == 0`).
    needs_setup: bool,
}

impl ChDirDialog {
    /// `TChDirDialog::TChDirDialog(opts, histId)`.
    ///
    /// Assembles the children in the **exact C++ insertion order** (so the labels
    /// link to the already-captured input-line / dir-list ids, and `reset_current`
    /// focuses the first selectable = the input line). Bounds are verbatim from the
    /// C++; `grow_mode` is set per the C++ table on each child before insert.
    pub fn new(opts: u16, history_id: u8) -> Self {
        use crate::view::{GrowMode, Rect, View};
        use crate::widgets::{
            Button, ButtonFlags, InputLine, Label, LimitMode, ScrollBar, THistory,
        };

        // TDialog(TRect(16,2,64,20), changeDirTitle); options |= ofCentered;
        // flags |= wfGrow.
        let mut dialog = Dialog::new(Rect::new(16, 2, 64, 20), Some(CHANGE_DIR_TITLE.into()));
        {
            let opts = &mut dialog.state_mut().options;
            opts.center_x = true;
            opts.center_y = true;
        }
        // flags |= wfGrow — faithful C++ port; Dialog::set_flags/flags are
        // pub(crate) accessors added on Dialog for row 79 B6 (mirrors FileDialog).
        {
            let mut f = dialog.flags();
            f.grow = true;
            dialog.set_flags(f);
        }

        // --- dirInput: TInputLine(TRect(3,3,42,4), MAXPATH-1) ----------------
        // growMode = gfGrowHiX. C++ TInputLine(bounds, aMaxLen) → maxLen =
        // aMaxLen-1, and InputLine::new(MaxBytes) does the same `limit-1`, so
        // pass MAXPATH-1 to get the C++ maxLen = MAXPATH-2.
        let mut dir_input = InputLine::new(
            Rect::new(3, 3, 42, 4),
            MAXPATH - 1,
            None,
            LimitMode::MaxBytes,
        );
        dir_input.state_mut().grow_mode = GrowMode {
            hi_x: true,
            ..Default::default()
        };
        let dir_input_id = dialog.insert_child(Box::new(dir_input));

        // --- TLabel(TRect(2,2,17,3), dirNameText, dirInput) ------------------
        // growMode = 0.
        dialog.insert_child(Box::new(Label::new(
            Rect::new(2, 2, 17, 3),
            DIR_NAME_TEXT,
            Some(dir_input_id),
        )));

        // --- THistory(TRect(42,3,45,4), dirInput, histId) --------------------
        // growMode = gfGrowLoX | gfGrowHiX.
        let mut hist = THistory::new(Rect::new(42, 3, 45, 4), dir_input_id, history_id);
        hist.state_mut().grow_mode = GrowMode {
            lo_x: true,
            hi_x: true,
            ..Default::default()
        };
        dialog.insert_child(Box::new(hist));

        // --- TScrollBar(TRect(32,6,33,16)) -----------------------------------
        let sb_id = dialog.insert_child(Box::new(ScrollBar::new(Rect::new(32, 6, 33, 16))));

        // --- dirList: TDirListBox(TRect(3,6,32,16), sb) ----------------------
        // growMode = gfGrowHiX | gfGrowHiY.
        let mut dir_list = DirListBox::new(Rect::new(3, 6, 32, 16), None, Some(sb_id));
        dir_list.lv.state.grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        let dir_list_id = dialog.insert_child(Box::new(dir_list));

        // --- TLabel(TRect(2,5,17,6), dirTreeText, dirList) -------------------
        // growMode = 0.
        dialog.insert_child(Box::new(Label::new(
            Rect::new(2, 5, 17, 6),
            DIR_TREE_TEXT,
            Some(dir_list_id),
        )));

        let grow_lo_hi_x = GrowMode {
            lo_x: true,
            hi_x: true,
            ..Default::default()
        };

        // --- okButton: TButton(TRect(35,6,45,8), okText, cmOK, bfDefault) ----
        let mut ok_button = Button::new(
            Rect::new(35, 6, 45, 8),
            OK_TEXT,
            crate::command::Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        ok_button.state.grow_mode = grow_lo_hi_x;
        dialog.insert_child(Box::new(ok_button));

        // --- chDirButton: TButton(TRect(35,9,45,11), chdirText, cmChangeDir) -
        let mut chdir_button = Button::new(
            Rect::new(35, 9, 45, 11),
            CHDIR_TEXT,
            crate::command::Command::CHANGE_DIR,
            ButtonFlags::new(),
        );
        chdir_button.state.grow_mode = grow_lo_hi_x;
        let chdir_button_id = dialog.insert_child(Box::new(chdir_button));

        // --- revertButton: TButton(TRect(35,12,45,14), revertText, cmRevert) -
        let mut revert_button = Button::new(
            Rect::new(35, 12, 45, 14),
            REVERT_TEXT,
            crate::command::Command::REVERT,
            ButtonFlags::new(),
        );
        revert_button.state.grow_mode = grow_lo_hi_x;
        dialog.insert_child(Box::new(revert_button));

        // --- helpButton: TButton(TRect(35,15,45,17), helpText, cmHelp) -------
        // Inserted only when cdHelpButton is set.
        if opts & CD_HELP_BUTTON != 0 {
            let mut help_button = Button::new(
                Rect::new(35, 15, 45, 17),
                HELP_TEXT,
                crate::command::Command::HELP,
                ButtonFlags::new(),
            );
            help_button.state.grow_mode = grow_lo_hi_x;
            dialog.insert_child(Box::new(help_button));
        }

        // selectNext(False): reset_current establishes currency (focuses the first
        // selectable child = dirInput, inserted first) — see View::reset_current.

        let mut cd = ChDirDialog {
            dialog,
            dir_input_id,
            dir_list_id,
            chdir_button_id,
            needs_setup: opts & CD_NO_LOAD_DIR == 0,
        };
        // Wire the chdir button into the dir list so its focus (un-)defaults it
        // (C++ `TDirListBox::setState` → `owner->chDirButton->makeDefault`). Both
        // ids are now known, so this is an after-insert child_mut reading the
        // stored `chdir_button_id`.
        cd.wire_chdir_button();
        cd
    }

    /// Hand the dir list the `chDirButton` id so its focus changes (un-)default
    /// the button (the C++ `TDirListBox::setState` →
    /// `owner->chDirButton->makeDefault`). Called once from the ctor.
    fn wire_chdir_button(&mut self) {
        let btn = self.chdir_button_id;
        if let Some(dl) = self
            .dialog
            .child_mut(self.dir_list_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<DirListBox>())
        {
            dl.set_chdir_button(btn);
        }
    }

    /// Trim a single trailing `/` from `path`, **keeping the root `/`**
    /// (`trimEndSeparator`). D14 adaptation of the DOS `if (len > 3 &&
    /// isSeparator(path[len-1]))` guard — the DOS `len > 3` protected `"C:\"`; here
    /// `len > 1` protects the bare root `"/"`.
    fn trim_end_separator(path: &str) -> String {
        if path.len() > 1 && path.ends_with('/') {
            path[..path.len() - 1].to_string()
        } else {
            path.to_string()
        }
    }

    /// Read the current process directory (`getCurrentDir`/`getCurDir`) as a
    /// `/`-terminated absolute path (D14, `std::env::current_dir`), falling back to
    /// `/` when it cannot be read.
    fn current_dir_normalized() -> String {
        let dir = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| "/".into());
        if dir.ends_with('/') {
            dir
        } else {
            format!("{dir}/")
        }
    }

    /// The navigation tail shared by `cmRevert` and a successful `cmChangeDir`
    /// (the C++ `handleEvent` body after the switch): re-read `dir` into the
    /// [`DirListBox`], reflect the trimmed path in `dirInput`, and focus the dir
    /// list (`dirList->select()`). Direct child mutation (the dialog owns the
    /// group), sequenced like [`FileDialog::navigate`] (one `child_mut` borrow at a
    /// time).
    fn navigate_to(&mut self, dir: &str, ctx: &mut crate::view::Context) {
        if let Some(dl) = self
            .dialog
            .child_mut(self.dir_list_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<DirListBox>())
        {
            dl.new_directory(dir, ctx);
        }
        let trimmed = Self::trim_end_separator(dir);
        if let Some(input) = self.dialog.child_mut(self.dir_input_id) {
            crate::view::View::set_value(input, crate::data::FieldValue::Text(trimmed));
        }
        // dirList->select() — make the dir list the current view.
        ctx.request_focus(self.dir_list_id);
    }
}

#[crate::delegate(
    to = dialog,
    skip(
        apply_list_scroll,
        as_any_mut,
        calc_bounds,
        grabs_focus_on_click,
        select_window_num,
        set_value,
        size_limits,
        value
    )
)]
impl crate::view::View for ChDirDialog {
    /// `TChDirDialog::handleEvent` — delegate to `TDialog::handleEvent` first (the
    /// faithful base call), then handle `cmRevert` / `cmChangeDir`:
    /// - `cmRevert` → re-read the **live** cwd (`getCurrentDir`).
    /// - `cmChangeDir` → read the **focused** dir-list entry's path; under D14, if
    ///   it starts with `/` ((`isSeparator`)) append a trailing `/` (the C++
    ///   `\\`); otherwise `return` leaving the event uncleared (passes through —
    ///   faithful to the C++ `else return;`, NOT a `clearEvent`). The C++
    ///   `drivesText` compare is dropped (D14).
    ///
    /// Both feed the shared navigate tail ([`navigate_to`](ChDirDialog::navigate_to)):
    /// `newDirectory` → reflect in `dirInput` → focus the dir list, then clear.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        use crate::command::Command;
        use crate::event::Event;

        // TDialog::handleEvent FIRST (faithful order).
        self.dialog.handle_event(ev, ctx);

        if let Event::Command(c) = *ev {
            let cur_dir = match c {
                Command::REVERT => Self::current_dir_normalized(),
                Command::CHANGE_DIR => {
                    // Read the focused entry's path directly (C++
                    // `dirList->list()->at(dirList->focused)`).
                    let focused = self
                        .dialog
                        .child_mut(self.dir_list_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<DirListBox>())
                        .and_then(|dl| dl.focused_entry().map(|e| e.dir().to_string()));
                    let Some(mut path) = focused else {
                        return;
                    };
                    // D14: the drivesText compare is dropped; only the isSeparator
                    // branch remains (a `/`-rooted path). Anything else → return
                    // (leave the event uncleared, NOT clearEvent).
                    if path.starts_with('/') {
                        if !path.ends_with('/') {
                            path.push('/');
                        }
                        path
                    } else {
                        return;
                    }
                }
                _ => return,
            };

            self.navigate_to(&cur_dir, ctx);
            ev.clear();
        }
    }

    /// `TChDirDialog::sizeLimits` — `min = {48, 18}`; `max` from the embedded
    /// dialog. `calc_bounds` is skip-listed so an owner-driven resize routes
    /// through this floor (the [`FileDialog`]/`EditWindow` pattern).
    fn size_limits(
        &self,
        owner_size: crate::view::Point,
    ) -> (crate::view::Point, crate::view::Point) {
        let (_min, max) = crate::view::View::size_limits(&self.dialog, owner_size);
        (crate::view::Point::new(48, 18), max)
    }

    /// The ctx-bearing init hook — `setUpDialog()` + `selectNext(False)`. Establish
    /// the dialog's internal currency first (focuses dirInput), then, once, do the
    /// initial directory read: the live cwd (D14, `/`-terminated) is read into the
    /// [`DirListBox`] and reflected (trimmed) into `dirInput`.
    fn reset_current(&mut self, ctx: &mut crate::view::Context) {
        self.dialog.reset_current(ctx);

        if self.needs_setup {
            self.needs_setup = false;
            let dir = Self::current_dir_normalized();
            if let Some(dl) = self
                .dialog
                .child_mut(self.dir_list_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<DirListBox>())
            {
                dl.new_directory(&dir, ctx);
            }
            let trimmed = Self::trim_end_separator(&dir);
            if let Some(input) = self.dialog.child_mut(self.dir_input_id) {
                crate::view::View::set_value(input, crate::data::FieldValue::Text(trimmed));
            }
        }
    }

    /// The modal loop and any owner-downcast target must reach the `ChDirDialog`,
    /// so `as_any_mut` returns `self`, NOT the inner `Dialog`.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `TChDirDialog::valid` — accept gate. `cmOK` reads `dirInput`, `fexpand`s it
    /// (relative to the live cwd, matching C++ `fexpand`), trims the trailing
    /// separator, and attempts the **real** `chdir` ([`std::env::set_current_dir`]).
    /// On error → an informational "Invalid directory" box + `false` (cwd is
    /// untouched — `set_current_dir` does not mutate on error). Any other command
    /// is always valid.
    fn valid(&mut self, cmd: crate::command::Command, ctx: &mut crate::view::Context) -> bool {
        if cmd != crate::command::Command::OK {
            return true;
        }

        // Read dirInput's text (D10 value protocol → FieldValue::Text).
        let field_text = self
            .dialog
            .child_mut(self.dir_input_id)
            .and_then(|v| v.value())
            .and_then(|val| match val {
                crate::data::FieldValue::Text(s) => Some(s),
                _ => None,
            })
            .unwrap_or_default();

        // fexpand(path): expand relative to the live cwd (C++ fexpand uses the
        // current dir as the base), then trim the trailing separator.
        let base = Self::current_dir_normalized();
        let expanded = expand_path(&base, &field_text);
        let path = Self::trim_end_separator(&expanded);

        // changeDir(path) == chdir(path): the REAL process cwd change.
        if std::env::set_current_dir(&path).is_err() {
            ctx.request_message_box(
                format!("{INVALID_DIR_TEXT}: '{path}'."),
                crate::dialog::MessageBoxKind::Error,
                crate::dialog::MessageBoxButtons::ok(),
                None,
                None,
            );
            false
        } else {
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build a SearchRec with a given name and optional dir-flag.
    fn rec(name: &str, is_dir: bool) -> SearchRec {
        SearchRec {
            attr: if is_dir { FA_DIREC } else { 0 },
            time: 0,
            size: 0,
            name: name.into(),
        }
    }

    // --- DirEntry ---

    #[test]
    fn dir_entry_accessors() {
        let e = DirEntry::new("~ disp", "/some/dir");
        assert_eq!(e.text(), "~ disp");
        assert_eq!(e.dir(), "/some/dir");
    }

    #[test]
    fn dir_entry_clone_and_eq() {
        let e = DirEntry::new("a", "b");
        assert_eq!(e.clone(), e);
    }

    // --- search_rec_compare: one assertion per branch ---

    #[test]
    fn compare_equal_names() {
        assert_eq!(
            search_rec_compare(&rec("foo", false), &rec("foo", false)),
            Ordering::Equal
        );
    }

    #[test]
    fn compare_a_is_dotdot() {
        // ".." sorts after everything → Greater.
        assert_eq!(
            search_rec_compare(&rec("..", false), &rec("foo", false)),
            Ordering::Greater
        );
    }

    #[test]
    fn compare_b_is_dotdot() {
        // ".." as key2 → Less.
        assert_eq!(
            search_rec_compare(&rec("foo", false), &rec("..", false)),
            Ordering::Less
        );
    }

    #[test]
    fn compare_dir_after_file() {
        // a is a directory, b is a plain file (different names to avoid the
        // equal-name short-circuit) → a sorts after → Greater.
        assert_eq!(
            search_rec_compare(&rec("src", true), &rec("main.rs", false)),
            Ordering::Greater
        );
    }

    #[test]
    fn compare_file_before_dir() {
        // a is a plain file, b is a directory → a sorts before → Less.
        assert_eq!(
            search_rec_compare(&rec("main.rs", false), &rec("src", true)),
            Ordering::Less
        );
    }

    #[test]
    fn compare_both_files_alpha_order() {
        // "apple" < "banana" in byte order.
        assert_eq!(
            search_rec_compare(&rec("apple", false), &rec("banana", false)),
            Ordering::Less
        );
        assert_eq!(
            search_rec_compare(&rec("banana", false), &rec("apple", false)),
            Ordering::Greater
        );
    }

    #[test]
    fn compare_case_sensitive_byte_order() {
        // 'Z' (0x5A) < 'a' (0x61) → "Zebra" < "apple".
        assert_eq!(
            search_rec_compare(&rec("Zebra", false), &rec("apple", false)),
            Ordering::Less
        );
    }

    #[test]
    fn compare_both_dirs_alpha_order() {
        assert_eq!(
            search_rec_compare(&rec("alpha", true), &rec("beta", true)),
            Ordering::Less
        );
    }

    // --- FileCollection::insert keeps sorted order ---

    #[test]
    fn file_collection_sorted_insert() {
        let mut fc = FileCollection::new();
        // Insert out of order: a file, a directory, "..", another file.
        fc.insert(rec("readme.txt", false));
        fc.insert(rec("..", false));
        fc.insert(rec("src", true));
        fc.insert(rec("main.rs", false));

        // Expected order (by comparator): files alpha, dirs alpha, ".." last.
        let names: Vec<&str> = fc.items().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["main.rs", "readme.txt", "src", ".."]);

        // Also verify adjacent pairs are non-decreasing under the comparator.
        for w in fc.items().windows(2) {
            assert_ne!(
                search_rec_compare(&w[0], &w[1]),
                Ordering::Greater,
                "pair ({}, {}) is out of order",
                w[0].name,
                w[1].name
            );
        }
    }

    #[test]
    fn file_collection_multiple_dirs() {
        let mut fc = FileCollection::new();
        fc.insert(rec("docs", true));
        fc.insert(rec("file.txt", false));
        fc.insert(rec("alpha", true));
        fc.insert(rec("..", false));
        fc.insert(rec("build.rs", false));

        let names: Vec<&str> = fc.items().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["build.rs", "file.txt", "alpha", "docs", ".."]);
    }

    // --- at / len / is_empty ---

    #[test]
    fn file_collection_at_len_is_empty() {
        let mut fc = FileCollection::new();
        assert!(fc.is_empty());
        assert_eq!(fc.len(), 0);
        assert!(fc.at(0).is_none());

        fc.insert(rec("foo.txt", false));
        assert!(!fc.is_empty());
        assert_eq!(fc.len(), 1);
        assert_eq!(fc.at(0).map(|r| r.name.as_str()), Some("foo.txt"));
        assert!(fc.at(1).is_none());
    }

    #[test]
    fn file_collection_default_is_empty() {
        let fc = FileCollection::default();
        assert!(fc.is_empty());
    }

    // =========================================================================
    // DirListBox — row 75
    // =========================================================================

    // -- build_tree: pure deterministic tests ----------------------------------

    /// Verify the deep tree: `/home/oetiker/` with three subdirs.
    #[test]
    fn build_tree_deep_dir_three_subdirs() {
        let subdirs: Vec<String> = vec!["projects".into(), "scratch".into(), "tmp".into()];
        let (entries, cur) = DirListBox::build_tree("/home/oetiker/", &subdirs);

        // Counts: root + home + oetiker + 3 subdirs = 6.
        assert_eq!(entries.len(), 6, "6 entries total");
        assert_eq!(cur, 2, "cur == 2 (oetiker is the 3rd entry, idx 2)");

        // Ancestor entries.
        assert_eq!(entries[0].directory, "/", "root directory");
        assert_eq!(entries[1].directory, "/home", "home directory");
        assert_eq!(entries[2].directory, "/home/oetiker", "oetiker directory");

        // Subdir directory values.
        assert_eq!(entries[3].directory, "/home/oetiker/projects");
        assert_eq!(entries[4].directory, "/home/oetiker/scratch");
        assert_eq!(entries[5].directory, "/home/oetiker/tmp");

        // Connector prefixes on ancestor entries (indent 0, 2, 4).
        assert!(
            entries[0].display_text.contains("└─┬"),
            "root uses PATH_DIR"
        );
        assert!(
            entries[1].display_text.contains("└─┬"),
            "home uses PATH_DIR"
        );
        assert!(
            entries[2].display_text.contains("└─┬"),
            "oetiker uses PATH_DIR"
        );

        // Connector prefixes on subdirs.
        assert!(
            entries[3].display_text.contains("└┬─"),
            "first subdir uses FIRST_DIR"
        );
        assert!(
            entries[4].display_text.contains(" ├─"),
            "middle subdir uses MIDDLE_DIR"
        );

        // Last-entry fix-up: `├` → `└`.
        assert!(
            entries[5].display_text.contains('└'),
            "last subdir has └ after fix-up"
        );
        assert!(
            !entries[5].display_text.contains('├'),
            "last subdir no longer has ├"
        );
    }

    /// Root-only dir (`"/"`) with two subdirs.
    #[test]
    fn build_tree_root_only_two_subdirs() {
        let subdirs: Vec<String> = vec!["etc".into(), "usr".into()];
        let (entries, cur) = DirListBox::build_tree("/", &subdirs);

        // Counts: root + 2 subdirs = 3.
        assert_eq!(entries.len(), 3, "3 entries total");
        assert_eq!(cur, 0, "cur == 0 (root is the only ancestor)");

        assert_eq!(entries[0].directory, "/");
        assert_eq!(entries[1].directory, "/etc");
        assert_eq!(entries[2].directory, "/usr");

        // Subdirs at indent 2.
        assert!(
            entries[1].display_text.starts_with("  "),
            "subdir indent = 2 spaces"
        );
        // Last-entry fix-up: `├` → `└`.
        assert!(
            entries[2].display_text.contains('└'),
            "last subdir has └ after fix-up"
        );
        assert!(!entries[2].display_text.contains('├'));
    }

    /// Single-subdir fix-up: `└┬─` → `└──`.
    #[test]
    fn build_tree_single_subdir_fixup() {
        let subdirs: Vec<String> = vec!["only".into()];
        let (entries, cur) = DirListBox::build_tree("/", &subdirs);

        assert_eq!(entries.len(), 2);
        assert_eq!(cur, 0);

        // The single subdir started as FIRST_DIR "└┬─"; fix-up replaces "┬─" → "──".
        let display = &entries[1].display_text;
        assert!(
            display.contains("└──"),
            "single subdir fix-up: '└┬─' → '└──', got: {:?}",
            display
        );
        assert!(!display.contains("┬─"), "no remaining ┬─ after fix-up");
    }

    /// No subdirs — the fix-up still runs on the deepest ancestor (it is the
    /// last entry), turning its "└─┬" connector into a leaf corner "└──".
    #[test]
    fn build_tree_no_subdirs() {
        let (entries, cur) = DirListBox::build_tree("/home/user/", &[]);
        // root + home + user = 3 entries, no subdirs.
        assert_eq!(entries.len(), 3);
        assert_eq!(cur, 2);
        // The deepest ancestor (last entry) became a leaf corner "└──user".
        assert!(
            entries[2].display_text.ends_with("└──user"),
            "deepest ancestor fix-up: '└─┬user' → '└──user', got: {:?}",
            entries[2].display_text
        );
        assert!(
            !entries[2].display_text.contains('┬'),
            "no remaining ┬ after fix-up"
        );
        // Earlier ancestors keep their "└─┬" connector (they have children).
        assert!(
            entries[1].display_text.contains("└─┬"),
            "home keeps its branch connector"
        );
    }

    /// Root-only, no subdirs — a single entry, fixed up to a leaf corner.
    #[test]
    fn build_tree_root_only_no_subdirs() {
        let (entries, cur) = DirListBox::build_tree("/", &[]);
        assert_eq!(entries.len(), 1, "just the root");
        assert_eq!(cur, 0);
        assert_eq!(entries[0].directory, "/");
        // "└─┬/" → "└──/".
        assert_eq!(
            entries[0].display_text, "└──/",
            "root-only fix-up: '└─┬/' → '└──/'"
        );
    }

    /// `is_selected` returns true only for `cur`, not for `focused`.
    #[test]
    fn dir_list_box_is_selected_returns_cur() {
        use crate::widgets::list_viewer::ListViewer;

        let subdirs: Vec<String> = vec!["a".into(), "b".into()];
        let (items, cur) = DirListBox::build_tree("/home/oetiker/", &subdirs);

        let mut dlb = DirListBox::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        dlb.items = items;
        dlb.cur = cur;
        dlb.lv.range = dlb.items.len() as i32;
        dlb.lv.focused = 0; // cursor is on root, not cur

        // Only `cur` (index 2) is "selected".
        assert!(dlb.is_selected(cur as i32), "cur entry is selected");
        assert!(!dlb.is_selected(0), "root entry (focused) is not selected");
        assert!(!dlb.is_selected(1), "home entry is not selected");
    }

    // -- snapshot test: draw the rendered tree ---------------------------------

    fn render_dlb(dlb: &mut DirListBox, w: u16, h: u16) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::{DrawCtx, View};

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = dlb.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            dlb.draw(&mut dc);
        });
        screen.snapshot()
    }

    /// Snapshot with `focused == cur`: both focused and is_selected land on
    /// the same row so the highlighted row comes from the focused-color branch.
    #[test]
    fn snapshot_dir_list_box_tree() {
        let subdirs: Vec<String> = vec!["projects".into(), "scratch".into(), "tmp".into()];
        let (items, cur) = DirListBox::build_tree("/home/oetiker/", &subdirs);

        let mut dlb = DirListBox::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        // Seed directly (no Context needed for draw test).
        dlb.lv.state.state.selected = true;
        dlb.lv.state.state.active = true;
        dlb.items = items;
        dlb.cur = cur;
        dlb.lv.range = dlb.items.len() as i32;
        dlb.lv.focused = cur as i32;

        insta::assert_snapshot!(render_dlb(&mut dlb, 30, 8));
    }

    /// Snapshot with `focused != cur` — exercises `is_selected` through the
    /// draw path. The cursor sits on the root (row 0, `focused=0`) while
    /// `is_selected` still marks the oetiker ancestor (`cur=2`).
    ///
    /// In `list_viewer::draw`, the color precedence is:
    ///   focused == item  → focused_color   (root row, cursor here)
    ///   is_selected(item) → selected_color (cur row, highlighted here)
    ///   else              → normal_color
    ///
    /// If `is_selected` were broken (always false) the cur row would render in
    /// normal_color and this snapshot would differ — making the check bite.
    #[test]
    fn snapshot_dir_list_box_tree_cursor_off_cur() {
        let subdirs: Vec<String> = vec!["projects".into(), "scratch".into(), "tmp".into()];
        let (items, cur) = DirListBox::build_tree("/home/oetiker/", &subdirs);

        let mut dlb = DirListBox::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        dlb.lv.state.state.selected = true;
        dlb.lv.state.state.active = true;
        dlb.items = items;
        dlb.cur = cur; // cur == 2 (oetiker) remains the "selected" dir.
        dlb.lv.range = dlb.items.len() as i32;
        dlb.lv.focused = 0; // cursor on root — NOT the current dir.

        insta::assert_snapshot!(render_dlb(&mut dlb, 30, 8));
    }

    // =========================================================================
    // FileList — row 76
    // =========================================================================

    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::view::{Context, Deferred, View};
    use crate::widgets::list_viewer::ListViewer;
    use std::collections::VecDeque;

    fn fl_make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    fn fl_key(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(Key::Char(c), KeyModifiers::default()))
    }

    // -- 1. wildcard_match -----------------------------------------------------

    #[test]
    fn wildcard_star_matches_all() {
        assert!(FileList::wildcard_match("*", ""));
        assert!(FileList::wildcard_match("*", "anything.rs"));
        assert!(FileList::wildcard_match("*", "no.extension.here"));
    }

    #[test]
    fn wildcard_extension_filter() {
        assert!(FileList::wildcard_match("*.txt", "a.txt"));
        assert!(FileList::wildcard_match("*.txt", "longer.name.txt"));
        assert!(!FileList::wildcard_match("*.txt", "a.rs"));
        assert!(!FileList::wildcard_match("*.txt", "txt")); // no dot
    }

    #[test]
    fn wildcard_question_mark_single_char() {
        assert!(FileList::wildcard_match("?.rs", "a.rs"));
        assert!(!FileList::wildcard_match("?.rs", "ab.rs")); // ? is exactly one
        assert!(!FileList::wildcard_match("?.rs", ".rs")); // needs one char
    }

    #[test]
    fn wildcard_star_edges() {
        // "a*z" — prefix 'a', suffix 'z', any middle.
        assert!(FileList::wildcard_match("a*z", "az")); // empty middle
        assert!(FileList::wildcard_match("a*z", "abcz"));
        assert!(!FileList::wildcard_match("a*z", "abc")); // no trailing z
        assert!(!FileList::wildcard_match("a*z", "bz")); // no leading a
        // Case-sensitive (Linux build).
        assert!(!FileList::wildcard_match("*.TXT", "a.txt"));
    }

    // -- 2. build_listing ------------------------------------------------------

    // Helper: a raw (name, is_dir, size, mtime) tuple. The tests don't exercise
    // the date display, so `mtime` is always `None` (→ packed `time = 0`).
    fn raw(
        name: &str,
        is_dir: bool,
        size: i32,
    ) -> (String, bool, i32, Option<std::time::SystemTime>) {
        (name.into(), is_dir, size, None)
    }

    #[test]
    fn build_listing_files_filtered_dirs_always() {
        let raw_entries = vec![
            raw("keep.txt", false, 10),
            raw("skip.rs", false, 20),
            raw("src", true, 0),  // dir: kept regardless of wildcard
            raw("docs", true, 0), // dir: kept regardless of wildcard
        ];
        let items = FileList::build_listing("/home/oetiker/", "*.txt", &raw_entries);
        let names: Vec<&str> = items.iter().map(|r| r.name.as_str()).collect();
        // File "skip.rs" filtered out; both dirs kept; ".." appended.
        assert_eq!(names, vec!["keep.txt", "docs", "src", ".."]);
        // "keep.txt" carries its size; dirs/".." carry FA_DIREC.
        assert_eq!(items[0].size, 10);
        assert_eq!(items[0].attr & FA_DIREC, 0, "file has no FA_DIREC");
        assert_eq!(items[1].attr & FA_DIREC, FA_DIREC, "docs is a dir");
        assert_eq!(items[3].attr & FA_DIREC, FA_DIREC, ".. is a dir");
    }

    #[test]
    fn build_listing_drops_hidden_dirs_and_dot_entries() {
        let raw_entries = vec![
            raw(".git", true, 0), // hidden dir -> dropped
            raw(".", true, 0),    // self -> dropped
            raw("..", true, 0),   // parent in raw -> dropped (synthesized below)
            raw("visible", true, 0),
            raw(".hidden", false, 5), // hidden FILE: matched by "*" -> kept
        ];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let names: Vec<&str> = items.iter().map(|r| r.name.as_str()).collect();
        // Hidden file kept (wildcard "*"); hidden/./.. dirs dropped; ".." synthesized last.
        assert_eq!(names, vec![".hidden", "visible", ".."]);
    }

    #[test]
    fn build_listing_dotdot_only_off_root() {
        // Off the root: ".." appended.
        let items = FileList::build_listing("/home/oetiker/", "*", &[raw("a", true, 0)]);
        assert!(
            items.iter().any(|r| r.name == ".."),
            "dotdot present off root"
        );

        // At the root: no "..".
        let items = FileList::build_listing("/", "*", &[raw("a", true, 0)]);
        assert!(
            !items.iter().any(|r| r.name == ".."),
            "no dotdot at the root"
        );
    }

    #[test]
    fn build_listing_order_matches_comparator() {
        let raw_entries = vec![
            raw("zebra.rs", false, 1),
            raw("apple.rs", false, 1),
            raw("src", true, 0),
            raw("bin", true, 0),
        ];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let names: Vec<&str> = items.iter().map(|r| r.name.as_str()).collect();
        // Files alpha, dirs alpha, ".." last.
        assert_eq!(names, vec!["apple.rs", "zebra.rs", "bin", "src", ".."]);

        // get_text appends "/" to dirs and "..".
        let mut fl = FileList::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        assert_eq!(fl.get_text(0), "apple.rs", "file: no slash");
        assert_eq!(fl.get_text(2), "bin/", "dir: trailing slash");
        assert_eq!(fl.get_text(4), "../", "dotdot dir: trailing slash");
    }

    // -- 3. search: the discriminating comparator test -------------------------

    /// The key check that `search` compares via `search_rec_compare` (with the
    /// attr'd key), NOT via `get_text`.
    ///
    /// Seed a FILE "src.rs" and a DIRECTORY "src". Under the collection order:
    /// files first (alpha), then dirs (alpha), then "..". So:
    ///   items = ["src.rs" (file), "src" (dir), ".." (dir)]
    /// Searching "s" with shift_state == 0 → attr 0 → a FILE key → lands at the
    /// first item not-less-than it, which is the file "src.rs" (index 0 — the
    /// file section). Searching "s" with shift_state == KB_SHIFT → attr FA_DIREC
    /// → a DIR key → sorts AFTER every file → lands at the first dir "src"
    /// (index 1 — the dir section). A get_text-based impl ignores attr and would
    /// return the SAME index for both — this test rules that out.
    #[test]
    fn search_attr_routes_into_file_vs_dir_section() {
        use crate::widgets::list_viewer::{KB_SHIFT, SortedSearch};

        let raw_entries = vec![raw("src.rs", false, 1), raw("src", true, 0)];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        // Expected sorted: file "src.rs" (0), dir "src" (1), ".." (2).
        assert_eq!(
            items.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["src.rs", "src", ".."]
        );

        let mut fl = FileList::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;

        // shift_state 0 → file key → lands in the FILE section (index 0).
        fl.shift_state = 0;
        assert_eq!(
            fl.search(&['s']),
            0,
            "file key lands at the first file >= 's' (src.rs, idx 0)"
        );

        // shift_state KB_SHIFT → dir key → lands in the DIR section (index 1).
        fl.shift_state = KB_SHIFT;
        assert_eq!(
            fl.search(&['s']),
            1,
            "dir key sorts after all files -> first dir (src, idx 1)"
        );

        // A leading '.' in the prefix ALSO routes to FA_DIREC (getKey: `*s == '.'`),
        // even with shift_state == 0 — the discriminating proof that the key's
        // attr (not get_text) drives the search. A dir-key "." sorts AMONG the
        // dirs (after all files), and "." < "src" < ".." in byte order, so it
        // lands at the first dir, index 1. (A get_text impl with shift 0 would
        // treat "." as a plain name and land it in the file section at index 0.)
        fl.shift_state = 0;
        assert_eq!(
            fl.search(&['.']),
            1,
            "leading '.' -> dir key -> first dir (src, idx 1), NOT the file section"
        );
    }

    #[test]
    fn search_plain_first_ge() {
        use crate::widgets::list_viewer::SortedSearch;
        // Files only: a search for "s" finds the first file >= "s".
        let raw_entries = vec![
            raw("alpha.rs", false, 1),
            raw("sigma.rs", false, 1),
            raw("zeta.rs", false, 1),
        ];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let mut fl = FileList::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;
        // "alpha.rs"(0) "sigma.rs"(1) "zeta.rs"(2) ".."(3, a dir).
        assert_eq!(
            fl.search(&['s']),
            1,
            "first file >= 's' is sigma.rs (idx 1)"
        );
    }

    // -- 4. type-to-search smoke test through handle_event ---------------------

    #[test]
    fn file_list_type_to_jump_focuses_match() {
        let raw_entries = vec![
            raw("alpha.rs", false, 1),
            raw("sigma.rs", false, 1),
            raw("zeta.rs", false, 1),
        ];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);

        let mut fl = FileList::new(crate::view::Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;
        fl.lv.focused = 0; // start on "alpha.rs"

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Type 's' -> jump to "sigma.rs" (index 1).
        let mut ev = fl_key('s');
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fl.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(fl.lv().focused, 1, "'s' -> focus sigma.rs (idx 1)");
        assert!(ev.is_nothing(), "alphabetic char consumed");

        // After that single char the search machine advanced search_pos to 0
        // (the index of the matched char in the focused item's text), confirming
        // the type-to-search seam is wired through handle_event.
        use crate::widgets::list_viewer::SortedSearch;
        assert_eq!(fl.search_pos(), 0, "search_pos == 0 after one char");
    }

    // -- 5. snapshot of a rendered FileList ------------------------------------

    fn render_fl(fl: &mut FileList, w: u16, h: u16) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::{DrawCtx, View};

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = fl.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            fl.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_file_list() {
        let raw_entries = vec![
            raw("main.rs", false, 100),
            raw("lib.rs", false, 200),
            raw("readme.txt", false, 50),
            raw("src", true, 0),
            raw("tests", true, 0),
        ];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);

        // 2-column layout (num_cols == 2). Width 30 so each column is ~15 wide.
        let mut fl = FileList::new(crate::view::Rect::new(0, 0, 30, 6), None, None);
        fl.lv.state.state.selected = true;
        fl.lv.state.state.active = true;
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;
        fl.lv.focused = 0;

        insta::assert_snapshot!(render_fl(&mut fl, 30, 6));
    }

    // =========================================================================
    // FileInputLine + the cmFileFocused payload broker — row 77
    // =========================================================================

    use crate::command::Command;
    use crate::view::{Group, Rect};

    /// Insert `fl` into a fresh group, returning the group + the stamped id so we
    /// can drive it through `as_any_mut().downcast_mut::<FileList>()` and read the
    /// broadcasts it queues with `self` as `source`.
    fn fl_in_group(fl: FileList) -> (Group, crate::view::ViewId) {
        let mut g = Group::new(Rect::new(0, 0, 40, 12));
        let id = g.insert(Box::new(fl));
        (g, id)
    }

    fn count_broadcasts(out: &VecDeque<Event>, cmd: Command, src: crate::view::ViewId) -> usize {
        out.iter()
            .filter(|e| {
                matches!(e, Event::Broadcast { command, source }
                if *command == cmd && *source == Some(src))
            })
            .count()
    }

    // -- 1. on_focus_changed queues FILE_FOCUSED { source = self } -------------

    #[test]
    fn focus_change_broadcasts_file_focused_with_self_source() {
        let raw_entries = vec![raw("a.rs", false, 1), raw("b.rs", false, 1)];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let mut fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;
        fl.lv.focused = 0;

        let (mut g, id) = fl_in_group(fl);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Drive a focus change through the shared `focus_item` funnel.
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            let fl = g
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileList>())
                .unwrap();
            crate::widgets::list_viewer::focus_item(fl, 1, &mut ctx);
            assert_eq!(fl.lv().focused, 1);
        }
        assert_eq!(
            count_broadcasts(&out, Command::FILE_FOCUSED, id),
            1,
            "focus change broadcasts exactly one FILE_FOCUSED with self as source"
        );
    }

    // -- 2. focused_rec returns the focused entry / None when empty -----------

    #[test]
    fn focused_rec_returns_focused_or_none() {
        let raw_entries = vec![raw("a.rs", false, 1), raw("b.rs", false, 1)];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let mut fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;

        fl.lv.focused = 0;
        assert_eq!(fl.focused_rec().map(|r| r.name), Some("a.rs".to_string()));
        fl.lv.focused = 1;
        assert_eq!(fl.focused_rec().map(|r| r.name), Some("b.rs".to_string()));

        // Empty listing -> None.
        let empty = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        assert_eq!(empty.focused_rec(), None);
    }

    // -- 3. select_item queues FILE_DOUBLE_CLICKED ----------------------------

    #[test]
    fn select_item_broadcasts_file_double_clicked() {
        let raw_entries = vec![raw("a.rs", false, 1)];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let mut fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;

        let (mut g, id) = fl_in_group(fl);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            let fl = g
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileList>())
                .unwrap();
            fl.select_item(0, &mut ctx);
        }
        assert_eq!(
            count_broadcasts(&out, Command::FILE_DOUBLE_CLICKED, id),
            1,
            "selectItem broadcasts FILE_DOUBLE_CLICKED with self as source"
        );
        // Faithful: it does NOT also broadcast cmListItemSelected (no base call).
        assert_eq!(
            count_broadcasts(&out, Command::LIST_ITEM_SELECTED, id),
            0,
            "selectItem does NOT call the base -> no cmListItemSelected"
        );
    }

    // -- 4. read_directory on an empty dir queues a FILE_FOCUSED (noFile) ------

    /// Verifies the `read_directory` noFile-branch **contract by construction**:
    /// an empty listing (range 0) has no focusable item, so it must broadcast
    /// `FILE_FOCUSED` directly rather than via `focus_item`. A genuinely-empty
    /// listing is unreachable end-to-end (off-root always synthesizes `..`; `/`
    /// always has subdirs on a live system), so this cannot drive `read_directory`
    /// for real — it instead exercises the exact `else` arm's logic in isolation.
    #[test]
    fn empty_listing_branch_broadcasts_file_focused_by_contract() {
        let fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        let (mut g, id) = fl_in_group(fl);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            let fl = g
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileList>())
                .unwrap();
            // Mirrors the production `read_directory` `else` arm verbatim (an empty
            // listing can't be produced end-to-end, so we reproduce its logic here):
            // empty items, range 0 -> broadcast FILE_FOCUSED directly (focus_item
            // never runs with range 0).
            fl.items.clear();
            crate::widgets::list_viewer::set_range(fl, 0, &mut ctx);
            assert_eq!(fl.lv().range, 0);
            if fl.lv().range == 0
                && let Some(vid) = fl.lv().state.id()
            {
                ctx.broadcast(Command::FILE_FOCUSED, Some(vid));
            }
        }
        assert_eq!(
            count_broadcasts(&out, Command::FILE_FOCUSED, id),
            1,
            "empty-listing branch broadcasts FILE_FOCUSED (noFile) once"
        );
    }

    /// Non-empty `read_directory` (off-root, so `..` is present) takes the
    /// `focus_item(0)` path -> `on_focus_changed` -> exactly one FILE_FOCUSED.
    #[test]
    fn read_directory_nonempty_broadcasts_file_focused_once() {
        let tmp = std::env::temp_dir().join(format!("rstv_filedlg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("keep.txt"), b"x").unwrap();
        let dir = format!("{}/", tmp.to_string_lossy());

        let fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        let (mut g, id) = fl_in_group(fl);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            let fl = g
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileList>())
                .unwrap();
            fl.read_directory(&dir, "*", &mut ctx);
            assert!(fl.lv().range > 0, "off-root dir has at least '..'");
        }
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(
            count_broadcasts(&out, Command::FILE_FOCUSED, id),
            1,
            "non-empty read_directory broadcasts exactly one FILE_FOCUSED (item 0)"
        );
    }

    // -- 5. on_file_focused: file / dir / None --------------------------------

    #[test]
    fn file_input_line_on_file_focused_file_dir_none() {
        let mut fil = FileInputLine::new(Rect::new(0, 0, 20, 1), 80, "*.rs");

        // A plain file -> just the name, no slash.
        fil.on_file_focused(Some(SearchRec {
            attr: 0,
            time: 0,
            size: 10,
            name: "main.rs".into(),
        }));
        assert_eq!(fil.inner.data, "main.rs");

        // A directory -> "name/<wild_card>" (D14 slash).
        fil.on_file_focused(Some(SearchRec {
            attr: FA_DIREC,
            time: 0,
            size: 0,
            name: "src".into(),
        }));
        assert_eq!(fil.inner.data, "src/*.rs");

        // None (the noFile sentinel) -> blank.
        fil.on_file_focused(None);
        assert_eq!(fil.inner.data, "");
    }

    // -- 6. handle_event requests ResolveFocusedFile (guarded by sfSelected) --

    #[test]
    fn file_input_line_handle_event_requests_broker_when_not_selected() {
        let fil = FileInputLine::new(Rect::new(0, 0, 20, 1), 80, "*.rs");
        let mut g = Group::new(Rect::new(0, 0, 40, 12));
        let fil_id = g.insert(Box::new(fil));
        // A producer id to name as the broadcast source.
        let src_id = crate::view::ViewId::next();

        // NOT selected (default) -> the broker IS requested.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::FILE_FOCUSED,
                source: Some(src_id),
            };
            g.find_mut(fil_id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            deferred
                .iter()
                .filter(|d| matches!(d,
                    Deferred::ResolveFocusedFile { subscriber, source }
                        if *subscriber == fil_id && *source == src_id))
                .count(),
            1,
            "not-selected FILE_FOCUSED -> one ResolveFocusedFile request"
        );

        // SELECTED (user typing) -> the broker is NOT requested (sfSelected guard).
        if let Some(v) = g.find_mut(fil_id) {
            v.state_mut().state.selected = true;
        }
        let mut out2 = VecDeque::new();
        let mut timers2 = crate::timer::TimerQueue::new();
        let mut deferred2: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out2, &mut timers2, &mut deferred2);
            let mut ev = Event::Broadcast {
                command: Command::FILE_FOCUSED,
                source: Some(src_id),
            };
            g.find_mut(fil_id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !deferred2
                .iter()
                .any(|d| matches!(d, Deferred::ResolveFocusedFile { .. })),
            "selected (typing) FILE_FOCUSED -> no broker request (sfSelected guard)"
        );
    }

    // -- 7. full pump-free round trip: broker resolves producer into consumer -

    #[test]
    fn file_focused_round_trip_through_broker() {
        // Producer FileList with a focused dir entry, consumer FileInputLine —
        // both in one group; emulate the pump's ResolveFocusedFile apply.
        let raw_entries = vec![raw("readme.txt", false, 1), raw("src", true, 0)];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        let mut fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        fl.items = items;
        fl.lv.range = fl.items.len() as i32;
        fl.lv.focused = 1; // the "src" directory

        let mut g = Group::new(Rect::new(0, 0, 40, 12));
        let src_id = g.insert(Box::new(fl));
        let sub_id = g.insert(Box::new(FileInputLine::new(
            Rect::new(0, 1, 20, 1),
            80,
            "*.c",
        )));

        // Emulate the pump's broker: read producer's focused rec, write to consumer.
        let rec = g
            .find_mut(src_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileList>())
            .and_then(|fl| fl.focused_rec());
        let fil = g
            .find_mut(sub_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInputLine>())
            .unwrap();
        fil.on_file_focused(rec);

        // "src" is a directory -> "src/" + wildCard "*.c".
        assert_eq!(fil.inner.data, "src/*.c");
    }

    // -- 8. snapshot of a rendered FileInputLine ------------------------------

    fn render_fil(fil: &mut FileInputLine, w: u16, h: u16) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::{DrawCtx, View};

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = fil.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            fil.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_file_input_line() {
        let mut fil = FileInputLine::new(Rect::new(0, 0, 20, 1), 80, "*.rs");
        fil.inner.state.state.selected = true;
        fil.inner.state.state.active = true;
        // A directory focus -> "src/*.rs" in the field.
        fil.on_file_focused(Some(SearchRec {
            attr: FA_DIREC,
            time: 0,
            size: 0,
            name: "src".into(),
        }));
        insta::assert_snapshot!(render_fil(&mut fil, 20, 1));
    }

    // =========================================================================
    // FileInfoPane — row 78
    // =========================================================================

    // -- 1. on_file_focused sets / clears the cached record -------------------

    #[test]
    fn file_info_pane_on_file_focused_sets_and_clears() {
        let mut fip = FileInfoPane::new(Rect::new(0, 0, 47, 3), "/home/oetiker/", "*.rs");
        assert_eq!(fip.file_block, None, "starts blank");

        let r = SearchRec {
            attr: 0,
            time: 0,
            size: 42,
            name: "main.rs".into(),
        };
        fip.on_file_focused(Some(r.clone()));
        assert_eq!(fip.file_block, Some(r));

        // None (the noFile sentinel) clears it back to blank.
        fip.on_file_focused(None);
        assert_eq!(fip.file_block, None);
    }

    // -- 2. handle_event requests ResolveFocusedFile on FILE_FOCUSED ----------

    #[test]
    fn file_info_pane_handle_event_requests_broker() {
        let fip = FileInfoPane::new(Rect::new(0, 0, 47, 3), "/home/oetiker/", "*.rs");
        let mut g = Group::new(Rect::new(0, 0, 60, 20));
        let fip_id = g.insert(Box::new(fip));
        let src_id = crate::view::ViewId::next();

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::FILE_FOCUSED,
                source: Some(src_id),
            };
            g.find_mut(fip_id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            deferred
                .iter()
                .filter(|d| matches!(d,
                    Deferred::ResolveFocusedFile { subscriber, source }
                        if *subscriber == fip_id && *source == src_id))
                .count(),
            1,
            "FILE_FOCUSED -> one ResolveFocusedFile request (no sfSelected guard)"
        );

        // Unlike FileInputLine, the pane has NO sfSelected guard: even when
        // selected it STILL requests the broker.
        if let Some(v) = g.find_mut(fip_id) {
            v.state_mut().state.selected = true;
        }
        let mut out2 = VecDeque::new();
        let mut timers2 = crate::timer::TimerQueue::new();
        let mut deferred2: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out2, &mut timers2, &mut deferred2);
            let mut ev = Event::Broadcast {
                command: Command::FILE_FOCUSED,
                source: Some(src_id),
            };
            g.find_mut(fip_id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            deferred2
                .iter()
                .filter(|d| matches!(d, Deferred::ResolveFocusedFile { .. }))
                .count(),
            1,
            "selected pane STILL requests the broker (no sfSelected guard)"
        );
    }

    // -- 3. the DOS-time pack: known vector + round-trip ----------------------

    #[test]
    fn pack_dos_time_known_vector_and_round_trip() {
        use std::time::{Duration, UNIX_EPOCH};
        // 2021-01-01 00:00:00 UTC = 1609459200.
        let t = UNIX_EPOCH + Duration::from_secs(1_609_459_200);
        let packed = pack_dos_time(&t);
        // year-1980 = 41 (0x29), month 1, day 1, time 0 -> date 0x5221, time 0.
        assert_eq!(
            packed, 0x5221_0000,
            "2021-01-01 00:00 UTC packs to 0x52210000"
        );

        // Unpack the SAME bitfield the draw uses; round-trip to Y/M/D H:M.
        let v = packed as u32;
        let date = v >> 16;
        let time = v & 0xFFFF;
        assert_eq!((date >> 9) + 1980, 2021, "year");
        assert_eq!((date >> 5) & 0xF, 1, "month");
        assert_eq!(date & 0x1F, 1, "day");
        assert_eq!(time >> 11, 0, "hour");
        assert_eq!((time >> 5) & 0x3F, 0, "minute");
    }

    #[test]
    fn pack_dos_time_afternoon_vector() {
        use std::time::{Duration, UNIX_EPOCH};
        // 2021-06-15 14:37:00 UTC = 1623767820.
        let t = UNIX_EPOCH + Duration::from_secs(1_623_767_820);
        let v = pack_dos_time(&t) as u32;
        let date = v >> 16;
        let time = v & 0xFFFF;
        assert_eq!((date >> 9) + 1980, 2021, "year");
        assert_eq!((date >> 5) & 0xF, 6, "month = Jun");
        assert_eq!(date & 0x1F, 15, "day");
        assert_eq!(time >> 11, 14, "hour (24h)");
        assert_eq!((time >> 5) & 0x3F, 37, "minute");
    }

    #[test]
    fn pack_dos_time_pre_1980_clamps() {
        use std::time::{Duration, UNIX_EPOCH};
        // 1970-01-01 00:00:00 UTC is before the DOS 1980 epoch -> clamp.
        assert_eq!(pack_dos_time(&UNIX_EPOCH), DOTDOT_TIME);
        // 1979-12-31 23:59:59 UTC also clamps.
        let t = UNIX_EPOCH + Duration::from_secs(315_532_799);
        assert_eq!(pack_dos_time(&t), DOTDOT_TIME);
    }

    /// A date in 2044+ sets DOS date bit 15 (`year-1980 >= 64`), which lands in
    /// bit 31 of the packed `u32` — so the `i32` `time` is **negative**. This is
    /// intentional: the `draw` recovers the fields via `(time as u32)` (the same
    /// cast `FileInfoPane::draw` uses), so the negative value round-trips
    /// correctly. Pinning it here so nobody "fixes" the sign.
    #[test]
    fn pack_dos_time_far_future_is_negative_and_round_trips() {
        use std::time::{Duration, UNIX_EPOCH};
        // 2050-01-01 00:00:00 UTC = 2524608000 secs since the epoch.
        let t = UNIX_EPOCH + Duration::from_secs(2_524_608_000);
        let packed = pack_dos_time(&t);
        // year-1980 = 70 (0x46); date = (70<<9)|(1<<5)|1 = 0x8C21; time = 0.
        // As a u32 that is 0x8C210000, whose i32 reinterpretation is negative.
        assert!(packed < 0, "year >= 2044 sets bit 31 -> negative i32");
        assert_eq!(packed as u32, 0x8C21_0000, "packed bit pattern");

        // Unpack via the SAME `as u32` recovery the draw uses.
        let v = packed as u32;
        let date = v >> 16;
        let time = v & 0xFFFF;
        assert_eq!((date >> 9) + 1980, 2050, "year round-trips");
        assert_eq!((date >> 5) & 0xF, 1, "month");
        assert_eq!(date & 0x1F, 1, "day");
        assert_eq!(time >> 11, 0, "hour");
        assert_eq!((time >> 5) & 0x3F, 0, "minute");
    }

    #[test]
    fn build_listing_packs_mtime_and_dotdot_constant() {
        use std::time::{Duration, UNIX_EPOCH};
        let t = UNIX_EPOCH + Duration::from_secs(1_609_459_200); // 2021-01-01
        let raw_entries = vec![("a.rs".to_string(), false, 10, Some(t))];
        let items = FileList::build_listing("/home/oetiker/", "*", &raw_entries);
        // items: "a.rs" (file, packed time), ".." (dir, DOTDOT_TIME).
        let a = items.iter().find(|r| r.name == "a.rs").unwrap();
        assert_eq!(a.time, 0x5221_0000, "file carries its packed mtime");
        let dd = items.iter().find(|r| r.name == "..").unwrap();
        assert_eq!(dd.time, DOTDOT_TIME, ".. carries the 0x210000 constant");
    }

    // -- 4. snapshots ---------------------------------------------------------

    fn render_fip(fip: &mut FileInfoPane, w: u16, h: u16) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::{DrawCtx, View};

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = fip.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            fip.draw(&mut dc);
        });
        screen.snapshot()
    }

    /// A file entry with a real size + date. Width 47 (the C++ pane is
    /// `TRect(1,16,48,18)` = 47 wide) so the right-aligned `size.x-38..` columns
    /// fit. Hand-verify the `Mon DD, YYYY HH:MMa/p` layout against the C++
    /// offsets and the fg=cyan(3) bg=blue(1) InfoPane color.
    ///
    /// The frozen `report.t12345` in the snapshot is **faithful** C++
    /// single-buffer behavior, NOT a bug: the name is drawn at col 1, then the
    /// size is drawn at col `size.x - 38` (= 9 here), overwriting the name's tail
    /// (`report.txt` → `report.t` + `12345`). The C++ `TDrawBuffer` does the same
    /// (one shared row buffer, last write wins).
    #[test]
    fn snapshot_file_info_pane_file() {
        use std::time::{Duration, UNIX_EPOCH};
        let mut fip = FileInfoPane::new(Rect::new(0, 0, 47, 3), "/home/oetiker/", "*.rs");
        // 2021-06-15 14:37:00 UTC -> "Jun 15, 2021 02:37p".
        let t = UNIX_EPOCH + Duration::from_secs(1_623_767_820);
        fip.on_file_focused(Some(SearchRec {
            attr: 0,
            time: pack_dos_time(&t),
            size: 12345,
            name: "report.txt".into(),
        }));
        insta::assert_snapshot!(render_fip(&mut fip, 47, 3));
    }

    /// The blank / noFile state: only the path line draws, no name/size/date.
    #[test]
    fn snapshot_file_info_pane_blank() {
        let mut fip = FileInfoPane::new(Rect::new(0, 0, 47, 3), "/home/oetiker/", "*.rs");
        // file_block stays None -> blank line 1.
        insta::assert_snapshot!(render_fip(&mut fip, 47, 3));
    }

    // =========================================================================
    // FileDialog — row 79 (skeleton: B1)
    // =========================================================================

    /// Resolve a `FileList` child by id, panicking if absent (test helper).
    fn fd_file_list(fd: &mut FileDialog) -> &mut FileList {
        fd.dialog
            .child_mut(fd.file_list_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileList>())
            .expect("file_list resolves")
    }

    // -- 1. assembly: ids distinct + the three captured children resolve -------

    #[test]
    fn file_dialog_assembles_children() {
        // Open + Help selected (plus the always-present Cancel).
        let mut fd = FileDialog::new(
            "*.rs",
            "Open a File",
            "Name",
            FD_OPEN_BUTTON | FD_HELP_BUTTON,
            0,
        );

        // The three captured ids are distinct and non-zero.
        assert_ne!(fd.file_name_id, fd.file_list_id);
        assert_ne!(fd.file_list_id, fd.info_pane_id);
        assert_ne!(fd.file_name_id, fd.info_pane_id);

        // Each captured child resolves via child_mut + downcast.
        assert!(
            fd.dialog
                .child_mut(fd.file_name_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileInputLine>())
                .is_some(),
            "file_name resolves to a FileInputLine"
        );
        assert!(
            fd.dialog
                .child_mut(fd.file_list_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileList>())
                .is_some(),
            "file_list resolves to a FileList"
        );
        assert!(
            fd.dialog
                .child_mut(fd.info_pane_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileInfoPane>())
                .is_some(),
            "info_pane resolves to a FileInfoPane"
        );

        // The input line shows the wildcard as its initial text (strnzcpy).
        let fil = fd
            .dialog
            .child_mut(fd.file_name_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInputLine>())
            .unwrap();
        assert_eq!(fil.inner.data, "*.rs", "input line initial text = wildcard");

        // needs_read_directory is armed when fdNoLoadDir is NOT set.
        assert!(fd.needs_read_directory, "armed without fdNoLoadDir");
    }

    /// `fdNoLoadDir` suppresses the initial readDirectory arming.
    #[test]
    fn file_dialog_no_load_dir_disarms_read() {
        let fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        assert!(
            !fd.needs_read_directory,
            "fdNoLoadDir clears needs_read_directory"
        );
    }

    // -- 1a. button_specs: the which/order/default/y chain by option mask ------

    /// The pure button-layout decision (the source of the ctor's button set).
    /// Covers the degenerate no-option case, a single-option default, and the
    /// all-options chain (only the first is default; y steps +3).
    #[test]
    fn file_dialog_button_specs_combinations() {
        use crate::command::Command;

        // No option buttons -> only Cancel, and NO default button at all.
        let none = button_specs(0);
        assert_eq!(none.len(), 1, "only Cancel inserted");
        assert_eq!(none[0].1, Command::CANCEL);
        assert!(!none[0].2, "Cancel is never the default (bfNormal)");
        assert_eq!(none[0].3, 3, "Cancel at the base y = 3");

        // FD_OK_BUTTON alone -> OK (default, cmFileOpen) + Cancel.
        let ok = button_specs(FD_OK_BUTTON);
        assert_eq!(ok.len(), 2, "OK + Cancel");
        assert_eq!(ok[0].0, OK_TEXT);
        assert_eq!(ok[0].1, Command::FILE_OPEN, "OK fires cmFileOpen");
        assert!(ok[0].2, "the lone option button is the default");
        assert_eq!(ok[1].1, Command::CANCEL);
        assert!(!ok[1].2, "Cancel not default");

        // All option buttons + Help -> Open, OK, Replace, Clear, Cancel, Help.
        let all = button_specs(
            FD_OPEN_BUTTON | FD_OK_BUTTON | FD_REPLACE_BUTTON | FD_CLEAR_BUTTON | FD_HELP_BUTTON,
        );
        assert_eq!(all.len(), 6, "4 option + Cancel + Help");
        // Order + commands.
        assert_eq!(
            all.iter().map(|b| b.1).collect::<Vec<_>>(),
            vec![
                Command::FILE_OPEN,    // Open
                Command::FILE_OPEN,    // OK
                Command::FILE_REPLACE, // Replace
                Command::FILE_CLEAR,   // Clear
                Command::CANCEL,       // Cancel
                Command::HELP,         // Help
            ]
        );
        // Only the first (Open) is the default.
        assert!(all[0].2, "Open is the default");
        assert!(
            !all[1..].iter().any(|b| b.2),
            "no button after the first is the default"
        );
        // y steps +3 per button, starting at 3.
        assert!(
            all.iter().enumerate().all(|(i, b)| b.3 == 3 + 3 * i as i32),
            "y_top steps +3 per button: {:?}",
            all.iter().map(|b| b.3).collect::<Vec<_>>()
        );
    }

    // -- 1b. reset_current performs the initial readDirectory (the title task) --

    /// Driving `reset_current` (the ctx-bearing init hook) once flips the guard,
    /// sets the D14 trailing-slash directory, and reads the current dir into the
    /// FileList. Asserts invariants (not the machine-dependent cwd contents): the
    /// guard flips, `directory` ends with `/`, and the FileList got a non-empty
    /// listing (the cwd always has at least `..`). A second call is a no-op.
    #[test]
    fn file_dialog_reset_current_reads_directory() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON, 0);
        assert!(fd.needs_read_directory);

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            View::reset_current(&mut fd, &mut ctx);
        }
        assert!(!fd.needs_read_directory, "guard flips after the first run");
        assert!(
            fd.directory.ends_with('/'),
            "D14 trailing-slash precondition on directory: {:?}",
            fd.directory
        );
        assert!(
            fd_file_list(&mut fd).lv().range > 0,
            "current dir read into the FileList (cwd always has at least '..')"
        );

        // A second reset_current is a no-op (the guard already cleared).
        let range_after_first = fd_file_list(&mut fd).lv().range;
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            View::reset_current(&mut fd, &mut ctx);
        }
        assert_eq!(
            fd_file_list(&mut fd).lv().range,
            range_after_first,
            "second reset_current does not re-read"
        );
    }

    // -- 2. handle_event: result commands end the modal ------------------------

    #[test]
    fn file_dialog_file_open_ends_modal() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON, 0);

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = Event::Command(Command::FILE_OPEN);
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "cmFileOpen consumed");
        assert_eq!(
            deferred
                .iter()
                .filter(|d| matches!(d, Deferred::EndModal(Command::FILE_OPEN)))
                .count(),
            1,
            "cmFileOpen queues EndModal(FILE_OPEN) directly (no modal-flag gate)"
        );
    }

    // -- 3. handle_event: double-click re-injects cmOK -------------------------

    #[test]
    fn file_dialog_double_click_posts_ok() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON, 0);

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = Event::Broadcast {
            command: Command::FILE_DOUBLE_CLICKED,
            source: Some(crate::view::ViewId::next()),
        };
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "cmFileDoubleClicked consumed");
        assert!(
            out.iter().any(|e| *e == Event::Command(Command::OK)),
            "cmFileDoubleClicked re-injects cmOK (putEvent == ctx.post)"
        );
    }

    // -- 4. read_directory_listing (ctx-free) populates the listing ------------

    #[test]
    fn file_list_read_directory_listing_populates() {
        // Deterministic fixture dir under the system temp dir.
        let tmp = std::env::temp_dir().join(format!("rstv_filedlg_b1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("keep.txt"), b"x").unwrap();
        std::fs::write(tmp.join("skip.rs"), b"y").unwrap();
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        let dir = format!("{}/", tmp.to_string_lossy());

        let mut fl = FileList::new(Rect::new(0, 0, 30, 8), None, None);
        fl.read_directory_listing(&dir, "*.txt");
        let _ = std::fs::remove_dir_all(&tmp);

        let names: Vec<&str> = fl.list().iter().map(|r| r.name.as_str()).collect();
        // "keep.txt" matches; "skip.rs" filtered; "sub" dir kept; ".." synthesized.
        assert_eq!(names, vec!["keep.txt", "sub", ".."]);
        // range/focused/top_item published without a Context.
        assert_eq!(fl.lv().range, 3, "range == listing length");
        assert_eq!(fl.lv().focused, 0, "focused reset to 0");
        assert_eq!(fl.lv().top_item, 0, "top_item reset to 0");
    }

    // -- 5. snapshot: a fully-assembled dialog with a deterministic listing ----

    /// Render the assembled FileDialog with a hardcoded listing injected (so the
    /// frame is deterministic — no real-filesystem size/mtime). The info pane's
    /// `file_block` is left `None` (blank size/date line); only its path line
    /// draws. Hand-verify: the frame + title, the input field showing "*.rs",
    /// the "Name" + "Files" labels, the Open/Cancel buttons, the two-column
    /// file list, and the "/fixture/*.rs" path line in the info pane.
    #[test]
    fn snapshot_file_dialog() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::DrawCtx;

        let mut fd = FileDialog::new("*.rs", "Open a File", "Name", FD_OPEN_BUTTON, 0);
        // Mark the dialog selected/active so the frame draws its active style.
        fd.dialog.state_mut().state.selected = true;
        fd.dialog.state_mut().state.active = true;

        // Inject a deterministic listing into the FileList (no fs read).
        {
            let fl = fd_file_list(&mut fd);
            fl.items = vec![
                SearchRec {
                    attr: 0,
                    time: 0,
                    size: 100,
                    name: "lib.rs".into(),
                },
                SearchRec {
                    attr: 0,
                    time: 0,
                    size: 200,
                    name: "main.rs".into(),
                },
                SearchRec {
                    attr: FA_DIREC,
                    time: 0,
                    size: 0,
                    name: "src".into(),
                },
                SearchRec {
                    attr: FA_DIREC,
                    time: DOTDOT_TIME,
                    size: 0,
                    name: "..".into(),
                },
            ];
            fl.lv.range = fl.items.len() as i32;
            fl.lv.focused = 0;
            fl.lv.state.state.selected = true;
            fl.lv.state.state.active = true;
        }
        // Set the info pane's path line deterministically.
        if let Some(fip) = fd
            .dialog
            .child_mut(fd.info_pane_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInfoPane>())
        {
            fip.set_dir_info("/fixture/", "*.rs");
        }

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(64, 20);
        let mut r = Renderer::new(Box::new(backend));
        let mut view: Box<dyn View> = Box::new(fd);
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // =========================================================================
    // FileDialog — row 79 B2: path logic + valid + messageBoxes
    // =========================================================================

    // -- 6a. pure path helpers -------------------------------------------------

    #[test]
    fn expand_path_absolute_passthrough() {
        // Absolute input ignores `dir`.
        assert_eq!(expand_path("/home/oetiker/", "/etc/hosts"), "/etc/hosts");
        assert_eq!(expand_path("/some/where/", "/"), "/");
    }

    #[test]
    fn expand_path_relative_joins_dir() {
        assert_eq!(
            expand_path("/home/oetiker/", "foo.txt"),
            "/home/oetiker/foo.txt"
        );
        assert_eq!(
            expand_path("/home/oetiker/", "sub/bar"),
            "/home/oetiker/sub/bar"
        );
    }

    #[test]
    fn expand_path_dotdot_and_dot_normalize() {
        // `..` pops a component; `.` is dropped.
        assert_eq!(expand_path("/home/oetiker/", "../foo"), "/home/foo");
        assert_eq!(expand_path("/home/oetiker/", "./bar"), "/home/oetiker/bar");
        assert_eq!(expand_path("/a/b/c/", "../../x"), "/a/x");
        // `..` past the root stays at the root (never pops the RootDir).
        assert_eq!(expand_path("/", "../../etc"), "/etc");
    }

    #[test]
    fn expand_path_collapses_double_slash() {
        // `//` collapses (components() ignores empty segments).
        assert_eq!(expand_path("/home//oetiker/", "x"), "/home/oetiker/x");
        assert_eq!(expand_path("/home/oetiker/", "a//b"), "/home/oetiker/a/b");
    }

    #[test]
    fn expand_path_preserves_trailing_slash_for_bare_dir() {
        // A trailing slash (directory-only input) is preserved so get_file_name
        // can detect the bare-dir case.
        assert_eq!(expand_path("/home/oetiker/", "sub/"), "/home/oetiker/sub/");
        // An empty field resolves to the directory itself (trailing slash).
        assert_eq!(expand_path("/home/oetiker/", ""), "/home/oetiker/");
    }

    #[test]
    fn is_wild_detects_glob() {
        assert!(is_wild("*.txt"));
        assert!(is_wild("foo?.rs"));
        assert!(!is_wild("plain.txt"));
        assert!(!is_wild("/a/b/c"));
    }

    #[test]
    fn split_dir_file_splits() {
        assert_eq!(
            split_dir_file("/home/oetiker/foo.txt"),
            ("/home/oetiker/".to_string(), "foo.txt".to_string())
        );
        // Wildcard pattern: dir + the `*.txt` "filename".
        assert_eq!(
            split_dir_file("/home/oetiker/*.txt"),
            ("/home/oetiker/".to_string(), "*.txt".to_string())
        );
        // Bare directory (trailing slash) → empty file part.
        assert_eq!(
            split_dir_file("/home/oetiker/"),
            ("/home/oetiker/".to_string(), String::new())
        );
        // Root.
        assert_eq!(split_dir_file("/"), ("/".to_string(), String::new()));
    }

    #[test]
    fn valid_file_name_basic() {
        assert!(valid_file_name("/home/oetiker/foo.txt"));
        assert!(valid_file_name("foo.txt"));
        assert!(!valid_file_name(""));
        assert!(!valid_file_name("/home/oetiker/")); // bare dir → no filename
        assert!(!valid_file_name("a\0b")); // interior NUL
    }

    // -- 6b. get_file_name: wildcard-append on a bare-dir field ----------------

    /// A no-load dialog with a bare-directory field resolves to
    /// `<dir>/<wildcard>` (the C++ name+ext-empty → fnmerge with the wildcard).
    #[test]
    fn get_file_name_appends_wildcard_for_bare_dir() {
        let mut fd = FileDialog::new("*.rs", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = "/home/oetiker/".into();
        // Field text is a bare subdirectory (trailing slash → no filename part).
        fd_set_field(&mut fd, "sub/");
        assert_eq!(fd.get_file_name(), "/home/oetiker/sub/*.rs");
        // The resolved_name cache mirrors the return value.
        assert_eq!(fd.resolved_name, "/home/oetiker/sub/*.rs");
    }

    /// A plain filename field resolves to the absolute path, no wildcard append.
    #[test]
    fn get_file_name_plain_filename() {
        let mut fd = FileDialog::new("*.rs", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = "/home/oetiker/".into();
        fd_set_field(&mut fd, "main.rs");
        assert_eq!(fd.get_file_name(), "/home/oetiker/main.rs");
    }

    // -- 6c. valid() branches --------------------------------------------------

    /// Set the FileInputLine's text (test helper).
    fn fd_set_field(fd: &mut FileDialog, text: &str) {
        fd.dialog
            .child_mut(fd.file_name_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInputLine>())
            .expect("file_name resolves")
            .inner
            .data = text.to_string();
    }

    fn count_open_boxes(deferred: &[Deferred]) -> usize {
        deferred
            .iter()
            .filter(|d| matches!(d, Deferred::OpenMessageBox { .. }))
            .count()
    }

    /// `cmCancel` and `cmFileClear` are always valid: they return true after the
    /// base group-valid without a navigate/accept branch or a messageBox.
    /// `get_file_name` does run (faithful to C++ `getData`), but for a wildcard
    /// field it is purely lexical — no FS touch, no error box.
    #[test]
    fn valid_cancel_and_file_clear_always_true() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        // A clearly-invalid wildcard path — checkDirectory must NOT run for these.
        fd.directory = "/no/such/dir/at/all/".into();
        fd_set_field(&mut fd, "/no/such/dir/at/all/*.x");

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);

        assert!(fd.valid(Command::CANCEL, &mut ctx), "cmCancel always valid");
        assert!(
            fd.valid(Command::FILE_CLEAR, &mut ctx),
            "cmFileClear always valid"
        );
        assert_eq!(count_open_boxes(&deferred), 0, "no messageBox for these");
    }

    /// `valid(cmFileClear)` returns true AND leaves `value()` reflecting the
    /// resolved field name — pinning the cache-refresh fix: C++ `getData` calls
    /// `getFileName` unconditionally, so the cancel/clear path must still refresh
    /// `resolved_name` (it runs `get_file_name` before the early-return).
    #[test]
    fn valid_file_clear_refreshes_value_cache() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_CLEAR_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = "/home/oetiker/".into();
        fd_set_field(&mut fd, "report.txt");
        // Before any valid() the cache is empty.
        assert_eq!(
            View::value(&fd),
            Some(crate::data::FieldValue::Text(String::new())),
            "cache empty before valid()"
        );

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.valid(Command::FILE_CLEAR, &mut ctx)
        };
        assert!(accepted, "cmFileClear is valid");
        // value() now reflects the resolved field name, not a stale/empty value.
        assert_eq!(
            View::value(&fd),
            Some(crate::data::FieldValue::Text(
                "/home/oetiker/report.txt".into()
            )),
            "resolved_name refreshed on the cmFileClear path"
        );
        assert_eq!(count_open_boxes(&deferred), 0, "no messageBox");
    }

    /// `cmValid` returns true immediately (before the base call).
    #[test]
    fn valid_cmvalid_short_circuits() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
        assert!(fd.valid(Command::VALID, &mut ctx));
    }

    /// A wildcard field over an existing directory → NAVIGATE: returns false,
    /// updates `directory`/`wild_card`, re-reads the FileList, no messageBox.
    #[test]
    fn valid_wildcard_navigates() {
        let tmp = std::env::temp_dir().join(format!("rstv_fd_wild_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("keep.md"), b"x").unwrap();
        let dir = format!("{}/", tmp.to_string_lossy());

        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = dir.clone();
        // A wildcard pattern rooted at the fixture dir.
        fd_set_field(&mut fd, &format!("{dir}*.md"));

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.valid(Command::FILE_OPEN, &mut ctx)
        };
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(!accepted, "a wildcard navigates → not accepted (false)");
        assert_eq!(fd.wild_card, "*.md", "wild_card updated to the new mask");
        assert_eq!(fd.directory, dir, "directory unchanged (same dir)");
        assert_eq!(count_open_boxes(&deferred), 0, "valid dir → no error box");
        // The FileList was re-read with the new mask (only keep.md + "..").
        let names: Vec<String> = fd_file_list(&mut fd)
            .list()
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert!(
            names.contains(&"keep.md".to_string()),
            "matched file present"
        );
    }

    /// An existing directory field → NAVIGATE into it: false, `directory`
    /// updated (with a trailing slash), no messageBox.
    #[test]
    fn valid_existing_dir_navigates_into_it() {
        let tmp = std::env::temp_dir().join(format!("rstv_fd_dir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        let dir = format!("{}/", tmp.to_string_lossy());
        let subdir = format!("{}/sub", tmp.to_string_lossy());

        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = dir;
        fd_set_field(&mut fd, &subdir); // existing dir, no trailing slash

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.valid(Command::FILE_OPEN, &mut ctx)
        };
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(!accepted, "navigating into a dir → not accepted");
        assert_eq!(
            fd.directory,
            format!("{subdir}/"),
            "directory updated to the sub dir with a trailing '/'"
        );
        assert_eq!(count_open_boxes(&deferred), 0);
    }

    /// A plain, valid filename → ACCEPT (true), no messageBox.
    #[test]
    fn valid_filename_accepts() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = "/home/oetiker/".into();
        fd_set_field(&mut fd, "report.txt");

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.valid(Command::FILE_OPEN, &mut ctx)
        };
        assert!(accepted, "a real filename is accepted");
        assert_eq!(count_open_boxes(&deferred), 0);
        // value() now returns the resolved name (cache refreshed by valid()).
        assert_eq!(
            View::value(&fd),
            Some(crate::data::FieldValue::Text(
                "/home/oetiker/report.txt".into()
            ))
        );
    }

    /// A wildcard over a NON-existent directory → checkDirectory fails → an
    /// invalid-drive box is queued + refocus the field + false.
    #[test]
    fn valid_wildcard_bad_dir_queues_error_box() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = "/".into();
        fd_set_field(&mut fd, "/no/such/dir/zzz/*.x");

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.valid(Command::FILE_OPEN, &mut ctx)
        };
        assert!(!accepted, "bad-dir wildcard keeps the dialog open");
        assert_eq!(
            count_open_boxes(&deferred),
            1,
            "one invalid-drive box queued"
        );
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::FocusById(id) if *id == fd.file_name_id)),
            "field is refocused after the error"
        );
    }

    /// The invalidFile branch: not wild, not an existing dir, not a valid
    /// filename → the invalid-file box is queued + false.
    ///
    /// To reach an empty filename component after `get_file_name`, the wildcard
    /// itself must be empty (otherwise the bare-dir case appends a non-empty
    /// name). With an empty wildcard and a bare-directory field over a
    /// non-existent dir, the resolved path stays directory-only → not isDir,
    /// not validFileName → invalidFile.
    #[test]
    fn valid_invalid_filename_queues_error_box() {
        // Empty wildcard so get_file_name does NOT append a filename to a bare dir.
        let mut fd = FileDialog::new("", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        fd.directory = "/no/such/dir/qqq/".into();
        // Field text resolves to a non-existent bare directory (trailing slash).
        fd_set_field(&mut fd, "/no/such/dir/qqq/sub/");

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            fd.valid(Command::FILE_OPEN, &mut ctx)
        };
        assert!(!accepted, "invalid filename keeps the dialog open");
        assert_eq!(
            count_open_boxes(&deferred),
            1,
            "one invalid-file box queued"
        );
    }

    // -- 6d. set_value loads the field -----------------------------------------

    #[test]
    fn set_value_loads_field_text() {
        let mut fd = FileDialog::new("*", "t", "Name", FD_OPEN_BUTTON | FD_NO_LOAD_DIR, 0);
        View::set_value(&mut fd, crate::data::FieldValue::Text("loaded.txt".into()));
        let text = fd
            .dialog
            .child_mut(fd.file_name_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileInputLine>())
            .unwrap()
            .text()
            .to_string();
        assert_eq!(text, "loaded.txt");
    }

    // =========================================================================
    // ChDirDialog — row 80
    // =========================================================================

    /// Resolve the `DirListBox` child by id (for deterministic injection).
    fn cd_dir_list(cd: &mut ChDirDialog) -> &mut DirListBox {
        cd.dialog
            .child_mut(cd.dir_list_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<DirListBox>())
            .expect("dir_list resolves")
    }

    // -- assembly --------------------------------------------------------------

    /// The three captured ids are distinct, resolve to the right types, and the
    /// Chdir button carries `cmChangeDir`. (Mirrors `file_dialog_assembles_children`
    /// — id-distinctness + downcast, no child-count accessor on `Dialog`.)
    #[test]
    fn chdir_dialog_assembles_children() {
        let mut cd = ChDirDialog::new(CD_NORMAL, 0);

        assert_ne!(cd.dir_input_id, cd.dir_list_id);
        assert_ne!(cd.dir_list_id, cd.chdir_button_id);
        assert_ne!(cd.dir_input_id, cd.chdir_button_id);

        // dir_input is a plain InputLine (no as_any_mut override → not
        // downcastable); verify it resolves and exposes a Text value (D10).
        assert!(
            matches!(
                cd.dialog.child_mut(cd.dir_input_id).and_then(|v| v.value()),
                Some(crate::data::FieldValue::Text(_))
            ),
            "dir_input resolves to a Text-valued InputLine"
        );
        assert!(
            cd.dialog
                .child_mut(cd.dir_list_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<DirListBox>())
                .is_some(),
            "dir_list resolves to a DirListBox"
        );
        // The Chdir button carries cmChangeDir (public `command` field).
        let cmd = cd
            .dialog
            .child_mut(cd.chdir_button_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::widgets::Button>())
            .map(|b| b.command);
        assert_eq!(cmd, Some(Command::CHANGE_DIR), "chdir button cmChangeDir");
    }

    /// D10 / `dataSize() == 0`: `value`/`set_value` are skip-listed so they fall to
    /// the `View` trait default (`None` / no-op), NOT the inner `Dialog`'s
    /// group-gather. Proven empirically: `value()` returns `None` (a gather would
    /// return `Some(Record(..))`), and `set_value` is a silent no-op (value stays
    /// `None` after a set).
    #[test]
    fn chdir_dialog_value_is_trait_default_none() {
        let mut cd = ChDirDialog::new(CD_NORMAL, 0);
        assert_eq!(
            crate::view::View::value(&cd),
            None,
            "value() is the trait default None (skip-listed; not the Dialog gather)"
        );
        // set_value is a no-op (trait default) — it does not establish a value.
        crate::view::View::set_value(&mut cd, crate::data::FieldValue::Text("x".into()));
        assert_eq!(
            crate::view::View::value(&cd),
            None,
            "set_value is a no-op; value() stays None"
        );
    }

    /// The Chdir button id is wired into the dir list (so its focus changes
    /// (un-)default it).
    #[test]
    fn chdir_dialog_wires_chdir_button_into_dir_list() {
        let mut cd = ChDirDialog::new(CD_NORMAL, 0);
        let chdir_id = cd.chdir_button_id;
        assert_eq!(
            cd_dir_list(&mut cd).chdir_button,
            Some(chdir_id),
            "dir list knows the chdir button id"
        );
    }

    // -- breadcrumb 1: select_item posts cmChangeDir ---------------------------

    /// `TDirListBox::selectItem` → `message(owner, evCommand, cmChangeDir)`: posts
    /// a `cmChangeDir` command (payload-less; the dialog reads the focused entry).
    #[test]
    fn dir_list_select_item_posts_change_dir() {
        let mut dl = DirListBox::new(Rect::new(0, 0, 30, 10), None, None);
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            dl.select_item(2, &mut ctx);
        }
        assert_eq!(
            out.iter()
                .filter(|e| matches!(e, Event::Command(c) if *c == Command::CHANGE_DIR))
                .count(),
            1,
            "selectItem posts exactly one cmChangeDir"
        );
    }

    // -- breadcrumb 2: set_state -> MakeButtonDefault --------------------------

    /// `TDirListBox::setState` on an `sfFocused` change queues
    /// `MakeButtonDefault { button, enable }` for the wired chdir button.
    #[test]
    fn dir_list_set_state_focus_queues_make_button_default() {
        let mut dl = DirListBox::new(Rect::new(0, 0, 30, 10), None, None);
        let btn = crate::view::ViewId::next();
        dl.set_chdir_button(btn);

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            View::set_state(&mut dl, crate::view::StateFlag::Focused, true, &mut ctx);
        }
        assert!(
            deferred.iter().any(|d| matches!(
                d,
                Deferred::MakeButtonDefault { button, enable: true } if *button == btn
            )),
            "focus-gain queues MakeButtonDefault(enable=true)"
        );
    }

    /// Losing focus queues `MakeButtonDefault { enable: false }`.
    #[test]
    fn dir_list_set_state_unfocus_queues_make_button_default_false() {
        let mut dl = DirListBox::new(Rect::new(0, 0, 30, 10), None, None);
        let btn = crate::view::ViewId::next();
        dl.set_chdir_button(btn);

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            View::set_state(&mut dl, crate::view::StateFlag::Focused, false, &mut ctx);
        }
        assert!(
            deferred.iter().any(|d| matches!(
                d,
                Deferred::MakeButtonDefault { button, enable: false } if *button == btn
            )),
            "focus-loss queues MakeButtonDefault(enable=false)"
        );
    }

    /// A non-`sfFocused` state change, or no wired button, queues nothing.
    #[test]
    fn dir_list_set_state_no_button_queues_nothing() {
        let mut dl = DirListBox::new(Rect::new(0, 0, 30, 10), None, None);
        // chdir_button is None (no TChDirDialog owner).
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            View::set_state(&mut dl, crate::view::StateFlag::Focused, true, &mut ctx);
        }
        assert!(
            !deferred
                .iter()
                .any(|d| matches!(d, Deferred::MakeButtonDefault { .. })),
            "no wired button -> no MakeButtonDefault"
        );
    }

    // -- new_directory trailing-slash normalize --------------------------------

    /// `new_directory` normalizes a no-trailing-slash dir (as `current_dir`
    /// returns) to a trailing `/` before building the tree, so `self.dir` is
    /// always `/`-terminated. `/tmp` (a real dir) is used so the fs read succeeds.
    #[test]
    fn new_directory_normalizes_trailing_slash() {
        let mut dl = DirListBox::new(Rect::new(0, 0, 30, 10), None, None);
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            // No trailing slash on input.
            dl.new_directory("/tmp", &mut ctx);
        }
        assert_eq!(dl.dir, "/tmp/", "stored dir is trailing-slash-normalized");
        // The root entry plus the /tmp ancestor are present.
        assert_eq!(dl.list()[0].dir(), "/", "root entry");
    }

    // -- trim_end_separator (D14 root guard) -----------------------------------

    #[test]
    fn trim_end_separator_keeps_root() {
        assert_eq!(ChDirDialog::trim_end_separator("/"), "/", "root preserved");
        assert_eq!(ChDirDialog::trim_end_separator("/home/"), "/home");
        assert_eq!(ChDirDialog::trim_end_separator("/home"), "/home");
        assert_eq!(ChDirDialog::trim_end_separator("/a/b/c/"), "/a/b/c");
    }

    // -- valid: failure path ONLY (never really chdir's; cwd is process-global) -

    /// `valid(cmOK)` on a guaranteed-nonexistent directory: the real
    /// `set_current_dir` fails, so an "Invalid directory" box is queued and the
    /// dialog stays open (`false`). The cwd is NOT changed (`set_current_dir`
    /// does not mutate on error), so this is safe to run alongside other tests.
    #[test]
    fn chdir_valid_bad_dir_queues_error_box() {
        let mut cd = ChDirDialog::new(CD_NO_LOAD_DIR, 0);
        // Set the dirInput text directly (ChDirDialog has no set_value of its own
        // — value/set_value are skip-listed to the trait default). A path that
        // cannot exist (absolute, so expand_path passes it through verbatim).
        if let Some(input) = cd.dialog.child_mut(cd.dir_input_id) {
            crate::view::View::set_value(
                input,
                crate::data::FieldValue::Text("/nonexistent_rstv_xyz_zzz".into()),
            );
        }

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let accepted = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            cd.valid(Command::OK, &mut ctx)
        };
        assert!(!accepted, "bad dir keeps the dialog open");
        assert_eq!(count_open_boxes(&deferred), 1, "one invalid-dir box queued");
    }

    /// `valid` for any non-`cmOK` command is always true (and queues nothing).
    #[test]
    fn chdir_valid_non_ok_always_true() {
        let mut cd = ChDirDialog::new(CD_NO_LOAD_DIR, 0);
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let ok = {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            cd.valid(Command::CANCEL, &mut ctx)
        };
        assert!(ok, "non-cmOK is always valid");
        assert_eq!(count_open_boxes(&deferred), 0, "no box for non-cmOK");
    }

    // -- snapshot: deterministic, NO reset_current (which reads cwd) -----------

    #[test]
    fn snapshot_chdir_dialog() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::DrawCtx;

        let mut cd = ChDirDialog::new(CD_HELP_BUTTON, 0);
        cd.dialog.state_mut().state.selected = true;
        cd.dialog.state_mut().state.active = true;

        // Inject a deterministic dir tree (no fs read): /home/user/ with two
        // subdirs. build_tree gives a real tree + cur index.
        {
            let (items, cur) =
                DirListBox::build_tree("/home/user/", &["projects".into(), "src".into()]);
            let dl = cd_dir_list(&mut cd);
            dl.items = items;
            dl.cur = cur;
            dl.dir = "/home/user/".into();
            dl.lv.range = dl.items.len() as i32;
            dl.lv.focused = cur as i32;
            dl.lv.state.state.selected = true;
            dl.lv.state.state.active = true;
        }
        // Set the dirInput text deterministically (the trimmed current dir).
        if let Some(input) = cd.dialog.child_mut(cd.dir_input_id) {
            crate::view::View::set_value(input, crate::data::FieldValue::Text("/home/user".into()));
        }

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(64, 20);
        let mut r = Renderer::new(Box::new(backend));
        let mut view: Box<dyn View> = Box::new(cd);
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- reset_current focuses dirInput (currency) without the cwd read --------

    /// `reset_current` with `cdNoLoadDir` set establishes currency (focuses the
    /// first selectable = dirInput) but skips the cwd read, so it is cwd-safe.
    #[test]
    fn chdir_reset_current_no_load_dir_skips_read() {
        let mut cd = ChDirDialog::new(CD_NO_LOAD_DIR, 0);
        assert!(!cd.needs_setup, "cdNoLoadDir clears the setup guard");
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            View::reset_current(&mut cd, &mut ctx);
        }
        // The dir list was never populated (no cwd read).
        assert!(
            cd_dir_list(&mut cd).list().is_empty(),
            "no directory read under cdNoLoadDir"
        );
    }

    // =========================================================================
    // B6 finishers: wfGrow, screen-relative resize, SearchRec fs metadata
    // =========================================================================

    // ---- finisher 1: wfGrow -------------------------------------------------

    /// `TFileDialog::TFileDialog` does `flags |= wfGrow`; verify the flag is set
    /// after construction (C++ tfildlg.cpp line 62).
    #[test]
    fn file_dialog_ctor_sets_wf_grow() {
        let fd = FileDialog::new("*.*", "Test", "Name", FD_OPEN_BUTTON, 0);
        let flags = fd.dialog.flags();
        assert!(flags.grow, "wfGrow must be set on FileDialog");
        assert!(flags.r#move, "wfMove retained");
        assert!(flags.close, "wfClose retained");
        assert!(!flags.zoom, "wfZoom not set (dialog has no zoom)");
    }

    /// `TChDirDialog::TChDirDialog` does `flags |= wfGrow`; verify the flag is
    /// set after construction (C++ tchdrdlg.cpp line 45).
    #[test]
    fn chdir_dialog_ctor_sets_wf_grow() {
        let cd = ChDirDialog::new(CD_NO_LOAD_DIR, 0);
        let flags = cd.dialog.flags();
        assert!(flags.grow, "wfGrow must be set on ChDirDialog");
        assert!(flags.r#move, "wfMove retained");
        assert!(flags.close, "wfClose retained");
        assert!(!flags.zoom, "wfZoom not set");
    }

    // ---- finisher 2: screen-relative resize ---------------------------------

    /// On a wide screen (> 90 cols) the C++ formula fires `grow(15, 0)` —
    /// dialog width goes from 49 to 79.  Verify via handle_event with
    /// `ctx.owner_size` = (100, 25).
    #[test]
    fn file_dialog_screen_resize_wide_screen() {
        use crate::view::Point;

        let mut fd = FileDialog::new("*.*", "T", "N", FD_NO_LOAD_DIR, 0);
        assert!(fd.needs_screen_resize, "flag set by ctor");

        // Assign an id so request_bounds can reference it.
        fd.dialog.state_mut().id = Some(crate::view::ViewId::next());

        let bounds_before = crate::view::View::state(&fd).get_bounds();
        let w_before = bounds_before.b.x - bounds_before.a.x;
        assert_eq!(w_before, 49, "base width 49 before resize");

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            // owner_size = (100, 25): triggers the x > 90 branch (grow 15) but
            // NOT the y > 34 branch, so height stays 19.
            ctx.set_owner_size(Point::new(100, 25));
            let mut ev = crate::event::Event::Nothing;
            crate::view::View::handle_event(&mut fd, &mut ev, &mut ctx);
        }

        assert!(
            !fd.needs_screen_resize,
            "flag cleared after first handle_event"
        );
        // A ChangeBounds deferred should have been queued.
        let bounds_deferred = deferred.iter().find_map(|d| {
            if let Deferred::ChangeBounds(_, r) = d {
                Some(*r)
            } else {
                None
            }
        });
        let new_bounds = bounds_deferred.expect("ChangeBounds deferred must be queued");
        let new_w = new_bounds.b.x - new_bounds.a.x;
        let new_h = new_bounds.b.y - new_bounds.a.y;
        assert_eq!(
            new_w, 79,
            "wide screen: width grows from 49 to 79 (grow 15)"
        );
        assert_eq!(new_h, 19, "height unchanged (y <= 34)");
    }

    /// On a tall screen (> 34 rows) the C++ formula fires `grow(0, 5)` —
    /// dialog height goes from 19 to 29. Width stays 49 (screen width = 64,
    /// neither > 90 nor > 63).
    #[test]
    fn file_dialog_screen_resize_tall_screen() {
        use crate::view::Point;

        let mut fd = FileDialog::new("*.*", "T", "N", FD_NO_LOAD_DIR, 0);
        fd.dialog.state_mut().id = Some(crate::view::ViewId::next());

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            // owner_size = (64, 40): exactly on the x > 63 boundary AND y > 34.
            ctx.set_owner_size(Point::new(64, 40));
            let mut ev = crate::event::Event::Nothing;
            crate::view::View::handle_event(&mut fd, &mut ev, &mut ctx);
        }

        let bounds_deferred = deferred.iter().find_map(|d| {
            if let Deferred::ChangeBounds(_, r) = d {
                Some(*r)
            } else {
                None
            }
        });
        let new_bounds = bounds_deferred.expect("ChangeBounds deferred must be queued");
        let new_h = new_bounds.b.y - new_bounds.a.y;
        assert_eq!(
            new_h, 29,
            "tall screen: height grows from 19 to 29 (grow 5)"
        );
    }

    /// A second handle_event call does NOT re-queue a ChangeBounds — the resize
    /// fires exactly once.
    #[test]
    fn file_dialog_screen_resize_fires_once() {
        use crate::view::Point;

        let mut fd = FileDialog::new("*.*", "T", "N", FD_NO_LOAD_DIR, 0);
        fd.dialog.state_mut().id = Some(crate::view::ViewId::next());

        let dispatch = |fd: &mut FileDialog, deferred: &mut Vec<Deferred>| {
            let mut out: VecDeque<Event> = VecDeque::new();
            let mut timers = crate::timer::TimerQueue::new();
            let mut ctx = fl_make_ctx(&mut out, &mut timers, deferred);
            ctx.set_owner_size(Point::new(100, 25));
            let mut ev = crate::event::Event::Nothing;
            crate::view::View::handle_event(fd, &mut ev, &mut ctx);
        };

        let mut deferred: Vec<Deferred> = vec![];
        dispatch(&mut fd, &mut deferred);
        let count_first: usize = deferred
            .iter()
            .filter(|d| matches!(d, Deferred::ChangeBounds(..)))
            .count();
        assert_eq!(count_first, 1, "exactly one ChangeBounds on first call");

        deferred.clear();
        dispatch(&mut fd, &mut deferred);
        let count_second: usize = deferred
            .iter()
            .filter(|d| matches!(d, Deferred::ChangeBounds(..)))
            .count();
        assert_eq!(
            count_second, 0,
            "no ChangeBounds on second call (fires once)"
        );
    }

    /// On a small screen (<= 63 wide, <= 25 tall) no resize fires: the dialog
    /// keeps its base 49×19 size.
    #[test]
    fn file_dialog_screen_resize_small_screen_no_change() {
        use crate::view::Point;

        let mut fd = FileDialog::new("*.*", "T", "N", FD_NO_LOAD_DIR, 0);
        fd.dialog.state_mut().id = Some(crate::view::ViewId::next());

        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = fl_make_ctx(&mut out, &mut timers, &mut deferred);
            ctx.set_owner_size(Point::new(63, 25)); // at the boundary, not over
            let mut ev = crate::event::Event::Nothing;
            crate::view::View::handle_event(&mut fd, &mut ev, &mut ctx);
        }

        // No ChangeBounds should be queued — the formula branches don't fire.
        let cb_count: usize = deferred
            .iter()
            .filter(|d| matches!(d, Deferred::ChangeBounds(..)))
            .count();
        assert_eq!(cb_count, 0, "small screen: no resize queued");
    }

    // ---- finisher 3: SearchRec fs metadata ----------------------------------

    /// `FileList::raw_from_fs` populates real size, mtime, and is_dir from the
    /// filesystem. This test creates a fixture directory with a known-size file
    /// and a subdirectory, then asserts the metadata is non-stub:
    /// - size > 0 for the regular file
    /// - time != 0 for the regular file (mtime present)
    /// - attr has FA_DIREC for the directory entry
    /// - attr is 0 for the file entry
    #[test]
    fn search_rec_metadata_populated_from_fixture_dir() {
        let tmp = std::env::temp_dir().join(format!("rstv_b6_searchrec_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // Write a file with known content so size > 0.
        std::fs::write(tmp.join("hello.rs"), b"fn main() {}").unwrap();
        // Create a subdirectory.
        std::fs::create_dir_all(tmp.join("subdir")).unwrap();
        let dir = format!("{}/", tmp.to_string_lossy());

        let raw = FileList::raw_from_fs(&dir);
        let _ = std::fs::remove_dir_all(&tmp);

        // Find the file entry.
        let file_entry = raw
            .iter()
            .find(|(name, _, _, _)| name == "hello.rs")
            .expect("hello.rs must appear in the raw listing");
        let (_, is_dir_file, size, mtime) = file_entry;
        assert!(!is_dir_file, "hello.rs is not a directory");
        assert!(*size > 0, "file size must be > 0");
        assert!(
            mtime.is_some(),
            "mtime must be populated for a regular file"
        );

        // Find the subdir entry.
        let dir_entry = raw
            .iter()
            .find(|(name, _, _, _)| name == "subdir")
            .expect("subdir must appear in the raw listing");
        let (_, is_dir_dir, _, _) = dir_entry;
        assert!(*is_dir_dir, "subdir must be flagged as a directory");

        // Verify that build_listing correctly maps these to SearchRec attr/size/time.
        let recs = FileList::build_listing(&dir, "*.rs", &raw);
        let file_rec = recs
            .iter()
            .find(|r| r.name == "hello.rs")
            .expect("hello.rs must be in listing (matches *.rs)");
        assert_eq!(file_rec.attr, 0, "file attr must not have FA_DIREC");
        assert!(file_rec.size > 0, "file size propagated to SearchRec");
        assert_ne!(file_rec.time, 0, "time propagated to SearchRec (non-zero)");

        let dir_rec = recs
            .iter()
            .find(|r| r.name == "subdir")
            .expect("subdir must be in listing (dirs always included)");
        assert_eq!(dir_rec.attr, FA_DIREC, "directory attr must be FA_DIREC");
        assert_eq!(dir_rec.size, 0, "directory size is 0 in listing");
    }
}
