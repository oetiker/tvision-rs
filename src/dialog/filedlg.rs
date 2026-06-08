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
/// `attr`, `time`, and `size` are populated by the filesystem-reading layer in
/// `TFileList`/`TFileDialog` (deferred to those rows).
/// `TODO(filedlg fs-read): populate attr/time/size from std::fs.`
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
        }
    }

    /// The current item collection (`TDirListBox::list()`).
    pub fn list(&self) -> &[DirEntry] {
        &self.items
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

    /// `TDirListBox::selectItem` — sends cmChangeDir carrying the chosen
    /// [`DirEntry`] payload to the owner `TChDirDialog`.
    ///
    /// TODO(row 80 TChDirDialog): selectItem sends cmChangeDir carrying the
    /// chosen DirEntry payload to the owner. rstv's payload-less
    /// Event::Broadcast can't carry it; design the typed-payload command seam
    /// when TChDirDialog (the only consumer) lands.
    fn select_item(&mut self, _item: i32, _ctx: &mut crate::view::Context) {
        // Intentionally empty — see TODO above.
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

    /// `TDirListBox::setState` — delegates to `list_viewer::set_state`, then (on
    /// `sfFocused` change) would call `makeDefault(enable)` on the owner's
    /// `chDirButton`.
    ///
    /// TODO(row 80 TChDirDialog): on sfFocused change, also makeDefault(enable)
    /// the owner's chDirButton. Needs the owner-downcast + button seam; deferred
    /// to TChDirDialog.
    fn set_state(
        &mut self,
        flag: crate::view::StateFlag,
        enable: bool,
        ctx: &mut crate::view::Context,
    ) {
        crate::widgets::list_viewer::set_state(self, flag, enable, ctx);
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
}
