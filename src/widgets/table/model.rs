//! The table's data source: the [`TableModel`] trait, its row identity
//! [`RowKey`], the sort currency [`SortKey`], and the ready-made [`VecModel`].

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

use crate::theme::Role;

/// The stable identity of a table row.
///
/// Focus and the multi-selection follow the key, not the row position, so a
/// reload ([`Table::set_model`](super::Table::set_model)) or a re-sort keeps the
/// user on the same record. Keys must be unique within one model.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowKey(pub String);

impl RowKey {
    /// The key as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RowKey {
    fn from(s: &str) -> Self {
        RowKey(s.to_string())
    }
}

impl From<String> for RowKey {
    fn from(s: String) -> Self {
        RowKey(s)
    }
}

impl fmt::Display for RowKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The value a column sorts by.
///
/// Numbers sort before text in **both** directions (a descending sort
/// reverses numbers among themselves and texts among themselves, never the
/// two groups). Two numbers compare numerically, two texts as strings. Only
/// finite numbers come out of [`parse`](Self::parse); a NaN that a model
/// supplies itself sorts after every other number.
#[derive(Clone, Debug, PartialEq)]
pub enum SortKey {
    /// A numeric key — `"10.2"` sorts after `"9.9"`.
    Num(f64),
    /// A text key, compared as a plain string.
    Text(String),
}

impl SortKey {
    /// Parse `cell` (trimmed) as a finite number; anything else, including
    /// `"nan"`, `"inf"` and `"infinity"`, stays text.
    pub fn parse(cell: &str) -> SortKey {
        match cell.trim().parse::<f64>() {
            Ok(n) if n.is_finite() => SortKey::Num(n),
            _ => SortKey::Text(cell.to_string()),
        }
    }

    /// Whether both keys are of the same kind (both numbers or both text).
    pub fn same_kind(&self, other: &SortKey) -> bool {
        matches!(
            (self, other),
            (SortKey::Num(_), SortKey::Num(_)) | (SortKey::Text(_), SortKey::Text(_))
        )
    }

    /// The total sort order: numbers before text, numbers by value, text by
    /// string order.
    pub fn compare(&self, other: &SortKey) -> Ordering {
        match (self, other) {
            (SortKey::Num(a), SortKey::Num(b)) => a.total_cmp(b),
            (SortKey::Num(_), SortKey::Text(_)) => Ordering::Less,
            (SortKey::Text(_), SortKey::Num(_)) => Ordering::Greater,
            (SortKey::Text(a), SortKey::Text(b)) => a.cmp(b),
        }
    }
}

/// The rows a [`Table`](super::Table) shows.
///
/// The table never reorders the model: it keeps its own row permutation for
/// the current sort. Implement this over your own records to avoid copying
/// them into strings up front; [`VecModel`] covers the plain-strings case.
pub trait TableModel {
    /// Number of rows.
    fn row_count(&self) -> usize;

    /// The text of cell (`row`, `col`). An out-of-range column yields `""`.
    fn cell(&self, row: usize, col: usize) -> Cow<'_, str>;

    /// Stable identity of a row; focus and selection follow the key across
    /// reloads.
    fn row_key(&self, row: usize) -> RowKey;

    /// Sort key of cell (`row`, `col`). The default parses the cell as a
    /// number and falls back to its text; override it when the displayed
    /// text is not what should sort (`"10.2%"`, `"3 days ago"`).
    fn sort_key(&self, row: usize, col: usize) -> SortKey {
        SortKey::parse(&self.cell(row, col))
    }

    /// Optional theme role for the whole row (e.g. a dimmed or an error row).
    /// It replaces only the normal row colour: the focused and the selected
    /// colours still win.
    fn row_role(&self, _row: usize) -> Option<Role> {
        None
    }
}

/// A [`TableModel`] over `Vec<Vec<String>>`.
///
/// Rows are keyed by their index as text unless keys are supplied with
/// [`with_keys`](Self::with_keys); give real keys whenever the rows can be
/// reloaded in a different order, or focus cannot follow the record.
#[derive(Clone, Debug, Default)]
pub struct VecModel {
    rows: Vec<Vec<String>>,
    keys: Option<Vec<RowKey>>,
    roles: Vec<Option<Role>>,
}

impl VecModel {
    /// A model over `rows`, keyed by row index.
    pub fn new(rows: Vec<Vec<String>>) -> Self {
        VecModel {
            rows,
            keys: None,
            roles: Vec::new(),
        }
    }

    /// Builder: one key per row, index-aligned with the rows. A row without a
    /// key (the vector is too short) falls back to its index.
    pub fn with_keys(mut self, keys: Vec<RowKey>) -> Self {
        self.keys = Some(keys);
        self
    }

    /// Builder: one optional row role per row, index-aligned with the rows
    /// (see [`TableModel::row_role`]). A shorter vector leaves the remaining
    /// rows plain.
    pub fn with_row_roles(mut self, roles: Vec<Option<Role>>) -> Self {
        self.roles = roles;
        self
    }

    /// The rows.
    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }
}

impl TableModel for VecModel {
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn cell(&self, row: usize, col: usize) -> Cow<'_, str> {
        self.rows
            .get(row)
            .and_then(|r| r.get(col))
            .map_or(Cow::Borrowed(""), |s| Cow::Borrowed(s.as_str()))
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.keys
            .as_ref()
            .and_then(|k| k.get(row))
            .cloned()
            .unwrap_or_else(|| RowKey(row.to_string()))
    }

    fn row_role(&self, row: usize) -> Option<Role> {
        self.roles.get(row).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_key_parses_numbers_and_falls_back_to_text() {
        assert_eq!(SortKey::parse(" 10.5 "), SortKey::Num(10.5));
        assert_eq!(SortKey::parse("-3"), SortKey::Num(-3.0));
        assert_eq!(SortKey::parse("10.2%"), SortKey::Text("10.2%".into()));
        assert_eq!(SortKey::parse(""), SortKey::Text(String::new()));
        for t in ["nan", "NaN", "inf", "-inf", "infinity", "Infinity"] {
            assert_eq!(SortKey::parse(t), SortKey::Text(t.into()), "{t} is text");
        }
    }

    #[test]
    fn numbers_sort_before_text_and_by_value() {
        let n = |v| SortKey::Num(v);
        let t = |s: &str| SortKey::Text(s.into());
        assert_eq!(n(9.9).compare(&n(10.2)), Ordering::Less);
        assert_eq!(n(1e9).compare(&t("a")), Ordering::Less);
        assert_eq!(t("a").compare(&n(0.0)), Ordering::Greater);
        assert_eq!(t("b").compare(&t("a")), Ordering::Greater);
        assert_eq!(
            n(f64::NAN).compare(&n(f64::NAN)),
            Ordering::Equal,
            "two NaNs tie, so a stable sort keeps their order"
        );
    }

    #[test]
    fn vec_model_defaults_and_keys() {
        let m = VecModel::new(vec![vec!["a".into(), "b".into()], vec!["c".into()]]);
        assert_eq!(m.row_count(), 2);
        assert_eq!(m.cell(0, 1), "b");
        assert_eq!(m.cell(1, 1), "", "missing cell is empty");
        assert_eq!(m.cell(5, 0), "", "missing row is empty");
        assert_eq!(m.row_key(1), RowKey::from("1"), "index key by default");
        let m = m.with_keys(vec!["x".into()]);
        assert_eq!(m.row_key(0), RowKey::from("x"));
        assert_eq!(m.row_key(1), RowKey::from("1"), "short key list falls back");
        let m = m.with_row_roles(vec![None, Some(Role::Error)]);
        assert_eq!(m.row_role(0), None);
        assert_eq!(m.row_role(1), Some(Role::Error));
        assert_eq!(m.row_role(2), None);
    }
}
