//! `TFileDialog` data-support classes (rows 71–74): [`DirEntry`],
//! [`SearchRec`], [`DirCollection`], [`FileCollection`].
//!
//! These are pure-data types — the file-dialog views (`TDirListBox` row 75,
//! `TFileList`/`TFileDialog` later) consume them; none of them have draw or
//! event logic.
//!
//! Per the rstv "collections → `Vec`" deviation (no `TCollection`; cf.
//! `ListBox`'s `Vec<String>`), `TDirCollection` is a plain `Vec<DirEntry>`
//! alias and `TFileCollection` is a `Vec<SearchRec>` carrying only the one
//! piece of real logic — the sorted insert and its comparator.  The unused C++
//! collection API (`indexOf`/`remove`/`atPut`/`firstThat`/…) is dropped; no
//! consumer exists.

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
}
