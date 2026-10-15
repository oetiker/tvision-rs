//! Column specs and the pure layout/format/sort helpers behind the table's
//! header and body. Nothing here touches a view; everything is unit-tested.
//!
//! The helpers are adapted from tvdocker's table (MIT, by supertorpe):
//! widest-first column shrinking, ellipsis truncation, the header sort marker,
//! and the stable sort.

use unicode_segmentation::UnicodeSegmentation;

use super::model::TableModel;
use crate::text;

/// Column alignment inside its cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Text starts at the left edge of the cell (the default).
    #[default]
    Left,
    /// Text ends at the right edge of the cell — the usual choice for numbers.
    Right,
}

/// A sort direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    /// Smallest first; the header shows `▲`.
    Asc,
    /// Largest first; the header shows `▼`.
    Desc,
}

impl SortDir {
    /// The other direction.
    pub fn flip(self) -> SortDir {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }

    /// The header marker glyph.
    pub fn marker(self) -> char {
        match self {
            SortDir::Asc => '▲',
            SortDir::Desc => '▼',
        }
    }
}

/// One column of a [`Table`](super::Table).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSpec {
    /// The header label.
    pub label: String,
    /// Preferred width in cells.
    pub width: i32,
    /// The narrowest the layout may shrink this column to (at least 1).
    pub min_width: i32,
    /// Alignment of the label and the cells.
    pub align: Align,
    /// A hidden column takes no space at all (no cell, no separator).
    pub hidden: bool,
}

impl ColumnSpec {
    /// A left-aligned, visible column of preferred `width`, shrinkable to 1.
    pub fn new(label: impl Into<String>, width: i32) -> Self {
        ColumnSpec {
            label: label.into(),
            width,
            min_width: 1,
            align: Align::Left,
            hidden: false,
        }
    }

    /// Builder: the narrowest this column may be shrunk to.
    pub fn min_width(mut self, min_width: i32) -> Self {
        self.min_width = min_width;
        self
    }

    /// Builder: right-align the label and the cells.
    pub fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }

    /// Builder: set the alignment.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Builder: start hidden.
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
}

/// Width of the multi-select check column: the `[ ]` box of
/// [`CheckBoxes`](crate::widgets::CheckBoxes).
pub const CHECK_WIDTH: i32 = 3;

/// The fixed, non-shrinkable check column a multi-select table lays out in
/// front of its own columns (blank label).
pub fn check_column() -> ColumnSpec {
    ColumnSpec::new("", CHECK_WIDTH).min_width(CHECK_WIDTH)
}

/// Prefix `rest` (an already joined line) with the check-column cell `check`
/// of width `w0`, using the same one-space separator as the other columns.
/// A zero-width check column (no room at all) adds nothing.
pub fn with_check_cell(w0: i32, check: &str, rest: String) -> String {
    if w0 <= 0 {
        return rest;
    }
    let cell = pad_cell(check, w0, Align::Left);
    if rest.is_empty() {
        cell
    } else {
        format!("{cell} {rest}")
    }
}

/// Distribute `avail` cells over `cols`: hidden columns get 0; the visible ones
/// keep their preferred width while everything (plus one separator between
/// neighbouring visible columns) fits, else the currently widest shrinkable
/// column loses one cell per pass until it fits or every column sits at its
/// `min_width`. A layout that still does not fit is clipped by the view. With
/// no room at all (`avail` at or below the separators) every width is 0.
pub fn layout_widths(cols: &[ColumnSpec], avail: i32) -> Vec<i32> {
    let mut widths: Vec<i32> = cols
        .iter()
        .map(|c| if c.hidden { 0 } else { c.width.max(0) })
        .collect();
    // A visible column of width 0 renders nothing and gets no separator
    // (as in `join_cells` / `column_at`), so it does not count here either.
    let visible = cols.iter().filter(|c| !c.hidden && c.width > 0).count() as i32;
    let budget = avail - (visible - 1).max(0);
    if visible == 0 || budget <= 0 {
        return vec![0; cols.len()];
    }
    let mut over = widths.iter().sum::<i32>() - budget;
    while over > 0 {
        let widest = widths
            .iter()
            .enumerate()
            .filter(|(i, w)| !cols[*i].hidden && **w > cols[*i].min_width.max(1))
            .max_by_key(|(_, w)| **w)
            .map(|(i, _)| i);
        let Some(i) = widest else { break };
        widths[i] -= 1;
        over -= 1;
    }
    widths
}

/// Fit `cell` into exactly `width` display columns: pad with spaces per
/// `align`, or cut the tail and mark the cut with `…` (`width 1 → "…"`).
/// Measured in display columns, so wide characters are never split.
pub fn pad_cell(cell: &str, width: i32, align: Align) -> String {
    let width = width.max(0) as usize;
    if width == 0 {
        return String::new();
    }
    let len = text::width(cell);
    if len <= width {
        let pad = " ".repeat(width - len);
        return match align {
            Align::Left => format!("{cell}{pad}"),
            Align::Right => format!("{pad}{cell}"),
        };
    }
    // Truncation cuts the tail regardless of alignment (the ellipsis marks the
    // cut); a wide glyph that would straddle the cut is dropped and the gap
    // padded, so the result is always exactly `width` columns.
    let mut head = String::new();
    let mut used = 0;
    for g in cell.graphemes(true) {
        let gw = text::width(g);
        if used + gw > width - 1 {
            break;
        }
        head.push_str(g);
        used += gw;
    }
    format!("{head}…{}", " ".repeat(width - 1 - used))
}

/// Join the cells of the visible, non-zero-width columns with one space.
fn join_cells<'a>(widths: &[i32], cells: impl Iterator<Item = (usize, String)> + 'a) -> String {
    let mut line = String::new();
    for (i, cell) in cells {
        if widths.get(i).copied().unwrap_or(0) <= 0 {
            continue;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&cell);
    }
    line
}

/// The rendered text of model row `row`: each visible column padded or
/// truncated to its width, separated by one space.
pub fn format_line(
    model: &dyn TableModel,
    row: usize,
    widths: &[i32],
    cols: &[ColumnSpec],
) -> String {
    join_cells(
        widths,
        cols.iter().enumerate().map(|(i, c)| {
            let w = widths.get(i).copied().unwrap_or(0);
            (i, pad_cell(&model.cell(row, i), w, c.align))
        }),
    )
}

/// The header text: the labels laid out like [`format_line`], the sorted
/// column's label carrying `▲`/`▼`. A label too long for its column is cut so
/// the marker stays visible.
pub fn header_line(cols: &[ColumnSpec], widths: &[i32], sort: Option<(usize, SortDir)>) -> String {
    join_cells(
        widths,
        cols.iter().enumerate().map(|(i, c)| {
            let w = widths.get(i).copied().unwrap_or(0);
            let cell = match sort {
                Some((s, dir)) if s == i => {
                    let m = dir.marker();
                    let full = format!("{} {m}", c.label);
                    if text::width(&full) as i32 <= w {
                        pad_cell(&full, w, c.align)
                    } else if w >= 2 {
                        format!("{} {m}", pad_cell(&c.label, w - 2, Align::Left))
                    } else {
                        m.to_string()
                    }
                }
                _ => pad_cell(&c.label, w, c.align),
            };
            (i, cell)
        }),
    )
}

/// The column whose cell spans text-relative column `x`, or `None` (a
/// separator, past the last column, or before the first).
pub fn column_at(widths: &[i32], x: i32) -> Option<usize> {
    let mut start = 0;
    for (i, &w) in widths.iter().enumerate() {
        if w <= 0 {
            continue;
        }
        if x >= start && x < start + w {
            return Some(i);
        }
        start += w + 1;
    }
    None
}

/// The display order of `model` sorted by column `col`: model row indices,
/// stable for ties (equal keys keep model order in both directions). Numbers
/// come before text in both directions.
pub fn sort_order(model: &dyn TableModel, col: usize, dir: SortDir) -> Vec<usize> {
    let keys: Vec<_> = (0..model.row_count())
        .map(|r| model.sort_key(r, col))
        .collect();
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| {
        let ord = keys[a].compare(&keys[b]);
        // Descending reverses within a kind only: numbers stay before text.
        match dir {
            SortDir::Desc if keys[a].same_kind(&keys[b]) => ord.reverse(),
            _ => ord,
        }
    });
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::table::model::{RowKey, SortKey, VecModel};
    use std::borrow::Cow;

    fn col(label: &str, width: i32) -> ColumnSpec {
        ColumnSpec::new(label, width)
    }

    fn model(rows: &[&[&str]]) -> VecModel {
        VecModel::new(
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    #[test]
    fn layout_widths_shrink_widest_until_it_fits() {
        // avail 30 − 1 separator = 29; wants 60 → shave the widest per pass.
        let w = layout_widths(&[col("A", 40), col("B", 20)], 30);
        assert_eq!(w, vec![15, 14]);
        assert_eq!(w.iter().sum::<i32>() + 1, 30, "total + separator == avail");
        // Total already fits → untouched.
        assert_eq!(layout_widths(&[col("A", 5), col("B", 5)], 30), vec![5, 5]);
    }

    #[test]
    fn layout_widths_respects_min_width() {
        // avail 12 − 1 = 11; A may not go below 8, so B absorbs the rest.
        let w = layout_widths(&[col("A", 10).min_width(8), col("B", 10)], 12);
        assert_eq!(w, vec![8, 3]);
        // Nothing left to shave: stays over budget (the view clips).
        let w = layout_widths(&[col("A", 10).min_width(10), col("B", 3).min_width(3)], 8);
        assert_eq!(w, vec![10, 3]);
    }

    #[test]
    fn layout_widths_nonpositive_avail_yields_zeros() {
        assert_eq!(layout_widths(&[col("A", 40)], 0), vec![0]);
        assert_eq!(layout_widths(&[col("A", 4), col("B", 4)], 1), vec![0, 0]);
    }

    #[test]
    fn hidden_columns_take_no_width_and_no_separator() {
        let cols = [col("A", 10), col("B", 12).hidden(true), col("C", 8)];
        // 10 + 8 + one separator = 19 fits exactly.
        assert_eq!(layout_widths(&cols, 19), vec![10, 0, 8]);
        let m = model(&[&["a", "b", "c"]]);
        let line = format_line(&m, 0, &[10, 0, 8], &cols);
        assert_eq!(line, "a          c       ");
        assert_eq!(text::width(&line), 19);
        assert!(layout_widths(&[col("A", 3).hidden(true)], 10) == vec![0]);
    }

    #[test]
    fn pad_cell_uses_ellipsis_truncation() {
        assert_eq!(pad_cell("abc", 0, Align::Left), "");
        assert_eq!(pad_cell("abc", 1, Align::Left), "…");
        assert_eq!(pad_cell("abc", 2, Align::Left), "a…");
        assert_eq!(pad_cell("abc", 3, Align::Left), "abc", "exact fit");
        assert_eq!(pad_cell("abc", 5, Align::Left), "abc  ");
        assert_eq!(pad_cell("abc", 5, Align::Right), "  abc");
        assert_eq!(
            pad_cell("abcdef", 4, Align::Right),
            "abc…",
            "tail cut keeps ellipsis at the cut"
        );
        assert_eq!(pad_cell("ééé", 2, Align::Left), "é…");
    }

    #[test]
    fn pad_cell_measures_display_columns() {
        // Each CJK glyph is two columns wide; never split one.
        assert_eq!(pad_cell("日本語", 6, Align::Left), "日本語");
        assert_eq!(pad_cell("日本語テキスト", 5, Align::Left), "日本…");
        assert_eq!(pad_cell("日本語テキスト", 4, Align::Left), "日… ");
        assert_eq!(text::width(&pad_cell("日本語テキスト", 4, Align::Left)), 4);
    }

    #[test]
    fn format_line_pads_and_truncates_to_the_cell() {
        let cols = [col("C1", 5), col("C2", 8), col("C3", 6)];
        let m = model(&[&["c1", "very-long-name-here", "running"]]);
        assert_eq!(format_line(&m, 0, &[4, 7, 5], &cols), "c1   very-l… runn…");
        let cols = [col("N", 3).right()];
        assert_eq!(format_line(&model(&[&["7"]]), 0, &[3], &cols), "  7");
    }

    #[test]
    fn header_line_marks_sort_direction() {
        let cols = [col("ID", 10), col("NAME", 12), col("STATE", 8)];
        let widths = [10, 12, 8];
        assert!(header_line(&cols, &widths, Some((1, SortDir::Asc))).contains("NAME ▲"));
        assert!(header_line(&cols, &widths, Some((1, SortDir::Desc))).contains("NAME ▼"));
        assert!(!header_line(&cols, &widths, None).contains('▲'));
    }

    #[test]
    fn header_line_keeps_the_marker_when_the_label_is_cut() {
        let cols = [col("STATUS", 5)];
        assert_eq!(header_line(&cols, &[5], Some((0, SortDir::Asc))), "ST… ▲");
        assert_eq!(header_line(&cols, &[1], Some((0, SortDir::Desc))), "▼");
        assert_eq!(header_line(&cols, &[5], None), "STAT…");
    }

    #[test]
    fn column_at_maps_x_to_columns_and_skips_separators() {
        let widths = [3, 0, 4];
        assert_eq!(column_at(&widths, 0), Some(0));
        assert_eq!(column_at(&widths, 2), Some(0));
        assert_eq!(column_at(&widths, 3), None, "separator");
        assert_eq!(column_at(&widths, 4), Some(2), "hidden column skipped");
        assert_eq!(column_at(&widths, 7), Some(2));
        assert_eq!(column_at(&widths, 8), None, "past the end");
        assert_eq!(column_at(&widths, -1), None);
    }

    #[test]
    fn sort_order_orders_strings_and_is_stable_for_ties() {
        let m = model(&[&["1", "b"], &["2", "a"], &["3", "b"]]);
        assert_eq!(sort_order(&m, 1, SortDir::Asc), [1, 0, 2]);
        assert_eq!(
            sort_order(&m, 1, SortDir::Desc),
            [0, 2, 1],
            "ties keep model order in both directions"
        );
    }

    #[test]
    fn sort_order_compares_numbers_numerically() {
        let m = model(&[&["9.9"], &["10.2"], &["1.5"]]);
        assert_eq!(
            sort_order(&m, 0, SortDir::Asc),
            [2, 0, 1],
            "numeric: 1.5 < 9.9 < 10.2 — string order would say 1.5 < 10.2 < 9.9"
        );
        assert_eq!(sort_order(&m, 0, SortDir::Desc), [1, 0, 2]);
    }

    #[test]
    fn numbers_sort_before_text_in_both_directions() {
        let m = model(&[&["n/a"], &["10"], &["9"], &["abc"]]);
        assert_eq!(sort_order(&m, 0, SortDir::Asc), [2, 1, 3, 0]);
        assert_eq!(
            sort_order(&m, 0, SortDir::Desc),
            [1, 2, 0, 3],
            "descending reverses within each kind, numbers still first"
        );
    }

    #[test]
    fn check_column_is_fixed_on_a_narrow_table() {
        // avail 12: check 3 + separators 2 leaves 7 for A and B (wanted 20).
        let cols = [check_column(), col("A", 10), col("B", 10)];
        let w = layout_widths(&cols, 12);
        assert_eq!(w, vec![3, 4, 3], "the check column never shrinks");
        let m = model(&[&["alpha", "beta"]]);
        let line = with_check_cell(w[0], "[X]", format_line(&m, 0, &w[1..], &cols[1..]));
        assert_eq!(line, "[X] alp… be…");
        assert_eq!(text::width(&line), 12, "fills avail exactly");
        assert_eq!(with_check_cell(0, "[X]", "rest".into()), "rest");
        assert_eq!(with_check_cell(3, "[ ]", String::new()), "[ ]");
    }

    #[test]
    fn zero_width_visible_columns_get_no_separator() {
        // 5 + 5 + one separator = 11: the zero-width B must not eat a cell.
        let cols = [col("A", 5), col("B", 0), col("C", 5)];
        assert_eq!(layout_widths(&cols, 11), vec![5, 0, 5]);
        let m = model(&[&["a", "b", "c"]]);
        assert_eq!(text::width(&format_line(&m, 0, &[5, 0, 5], &cols)), 11);
    }

    /// A model whose display text is not its sort key (the tvdocker
    /// `sort_keys` case, here via [`TableModel::sort_key`]).
    struct Percent(Vec<f64>);

    impl TableModel for Percent {
        fn row_count(&self) -> usize {
            self.0.len()
        }
        fn cell(&self, row: usize, _col: usize) -> Cow<'_, str> {
            Cow::Owned(format!("{}%", self.0[row]))
        }
        fn row_key(&self, row: usize) -> RowKey {
            RowKey(row.to_string())
        }
        fn sort_key(&self, row: usize, _col: usize) -> SortKey {
            SortKey::Num(self.0[row])
        }
    }

    #[test]
    fn sort_order_uses_the_model_sort_key() {
        let m = Percent(vec![9.9, 10.2, 1.5]);
        assert_eq!(sort_order(&m, 0, SortDir::Asc), [2, 0, 1]);
    }

    #[test]
    fn nan_sort_keys_tie_and_stay_stable() {
        let m = Percent(vec![f64::NAN, f64::NAN]);
        assert_eq!(sort_order(&m, 0, SortDir::Asc), [0, 1]);
    }
}
