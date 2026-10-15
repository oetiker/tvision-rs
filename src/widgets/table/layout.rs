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
    /// Stretch weight (0 = fixed, the default). When the visible columns are
    /// narrower than the table, the spare cells go to the flex columns in
    /// proportion to their weights.
    pub flex: u16,
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
            flex: 0,
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

    /// Builder: let this column stretch into spare width with `weight`
    /// (0 = fixed). `width` is the width it starts from; a flex column is at
    /// least `min_width` wide, so `ColumnSpec::new("Notes", 0).flex(1)` simply
    /// fills the rest.
    pub fn flex(mut self, weight: u16) -> Self {
        self.flex = weight;
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
/// `min_width`. A layout that still does not fit is wider than `avail`; the
/// table scrolls it horizontally. Spare cells go to the
/// [`flex`](ColumnSpec::flex) columns by weight (see [`stretch`]); without one
/// they stay empty. With no room at all (`avail` at or below the separators)
/// every width is 0.
pub fn layout_widths(cols: &[ColumnSpec], avail: i32) -> Vec<i32> {
    let mut widths: Vec<i32> = cols.iter().map(preferred_width).collect();
    // A visible column of width 0 renders nothing and gets no separator
    // (as in `join_cells` / `column_at`), so it does not count here either.
    let visible = widths.iter().filter(|&&w| w > 0).count() as i32;
    let budget = avail - (visible - 1).max(0);
    if visible == 0 || budget <= 0 {
        return vec![0; cols.len()];
    }
    let mut over = sum_widths(&widths).saturating_sub(budget);
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
    if over < 0 {
        stretch(cols, &mut widths, -over);
    }
    widths
}

/// The width a column starts the layout from: 0 when hidden; a flex column
/// at least its `min_width` (and at least 1), so `ColumnSpec::new("X",
/// 0).flex(1)` is the "fill the rest" column.
fn preferred_width(c: &ColumnSpec) -> i32 {
    match () {
        _ if c.hidden => 0,
        _ if c.flex > 0 => c.width.max(c.min_width).max(1),
        _ => c.width.max(0),
    }
}

/// The sum of `widths`, saturating instead of overflowing on absurd widths.
fn sum_widths(widths: &[i32]) -> i32 {
    widths
        .iter()
        .filter(|&&w| w > 0)
        .fold(0i32, |acc, &w| acc.saturating_add(w))
}

/// Give `spare` cells to the visible, non-zero-width flex columns: each gets
/// `spare * weight / total_weight` (rounded down), and the cells the rounding
/// leaves over go one each to the leftmost flex columns. Without a flex column
/// nothing changes.
pub fn stretch(cols: &[ColumnSpec], widths: &mut [i32], spare: i32) {
    let flex: Vec<usize> = (0..cols.len())
        .filter(|&i| !cols[i].hidden && cols[i].flex > 0 && widths[i] > 0)
        .collect();
    let total: i64 = flex.iter().map(|&i| i64::from(cols[i].flex)).sum();
    if total == 0 || spare <= 0 {
        return;
    }
    let mut left = spare;
    for &i in &flex {
        let share = (i64::from(spare) * i64::from(cols[i].flex) / total) as i32;
        widths[i] += share;
        left -= share;
    }
    // The rounding remainder is below the number of flex columns.
    for &i in flex.iter().take(left as usize) {
        widths[i] += 1;
    }
}

/// The width of a line of `widths` joined with one-cell separators (zero-width
/// columns take no cell and no separator).
pub fn line_width(widths: &[i32]) -> i32 {
    let shown = widths.iter().filter(|&&w| w > 0).count() as i32;
    sum_widths(widths).saturating_add((shown - 1).max(0))
}

/// The horizontal geometry of a table row for one view width: the leading
/// `frozen` layout columns (the check column) stay put, the rest scroll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowLayout {
    /// The width of every layout column, frozen ones first.
    pub widths: Vec<i32>,
    /// How many leading layout columns are frozen.
    pub frozen: usize,
    /// Cells the frozen part takes, its trailing separator included.
    pub frozen_width: i32,
    /// The full width of the scrolling part.
    pub content: i32,
    /// The cells the scrolling part has on screen.
    pub view: i32,
}

impl RowLayout {
    /// Lay `cols` out over `avail` cells with the first `frozen` columns fixed.
    pub fn new(cols: &[ColumnSpec], frozen: usize, avail: i32) -> Self {
        let widths = layout_widths(cols, avail);
        let frozen = frozen.min(widths.len());
        let content = line_width(&widths[frozen..]);
        let mut frozen_width = line_width(&widths[..frozen]);
        if frozen_width > 0 && content > 0 {
            frozen_width = frozen_width.saturating_add(1);
        }
        RowLayout {
            view: (avail.max(0) - frozen_width).max(0),
            widths,
            frozen,
            frozen_width,
            content,
        }
    }

    /// The largest horizontal offset: how far the scrolling part overflows.
    pub fn max_offset(&self) -> i32 {
        (self.content - self.view).max(0)
    }

    /// `offset` clamped into `0..=max_offset()`.
    pub fn clamp(&self, offset: i32) -> i32 {
        offset.clamp(0, self.max_offset())
    }

    /// The on-screen text of a row: the `frozen` cell text (the check cell),
    /// then the scrolling `rest` (a full-width line over
    /// `widths[frozen..]`) cut to the view from `offset`.
    pub fn compose(&self, frozen: &str, rest: &str, offset: i32) -> String {
        let shown = slice_cols(rest, self.clamp(offset), self.view);
        match self.frozen {
            0 => shown,
            _ => with_check_cell(self.widths[0], frozen, shown),
        }
    }

    /// The layout column under text-relative column `x` at `offset`, or
    /// `None` (a separator, or no column there).
    pub fn column_at(&self, x: i32, offset: i32) -> Option<usize> {
        if x < 0 {
            return None;
        }
        if x < self.frozen_width {
            return column_at(&self.widths[..self.frozen], x);
        }
        let sx = x - self.frozen_width;
        if sx >= self.view {
            return None;
        }
        column_at(
            &self.widths[self.frozen..],
            sx.saturating_add(self.clamp(offset)),
        )
        .map(|c| c + self.frozen)
    }
}

/// The part of `s` from display column `skip` that fits in `width` display
/// columns. A wide glyph cut by either edge turns into spaces for its visible
/// half, so the result never starts mid-glyph.
pub fn slice_cols(s: &str, skip: i32, width: i32) -> String {
    let (skip, end) = (
        skip.max(0) as usize,
        skip.max(0).saturating_add(width.max(0)) as usize,
    );
    let mut out = String::new();
    let mut col = 0;
    for g in s.graphemes(true) {
        if col >= end {
            break;
        }
        let gw = text::width(g);
        let next = col + gw;
        if col >= skip && next <= end {
            out.push_str(g);
        } else if next > skip {
            // Cut by an edge: pad the visible part.
            out.push_str(&" ".repeat(next.min(end) - col.max(skip)));
        }
        col = next;
    }
    out
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
        if x >= start && x < start.saturating_add(w) {
            return Some(i);
        }
        start = start.saturating_add(w).saturating_add(1);
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
    fn flex_columns_take_the_spare_width_by_weight() {
        // avail 30 − 2 separators = 28; wanted 15 → 13 spare, weights 2:1.
        let cols = [col("A", 5).flex(2), col("B", 5), col("C", 5).flex(1)];
        let w = layout_widths(&cols, 30);
        // 13·2/3 = 8, 13·1/3 = 4, remainder 1 → the leftmost flex column.
        assert_eq!(w, vec![5 + 8 + 1, 5, 5 + 4]);
        assert_eq!(line_width(&w), 30, "fills avail exactly");
    }

    #[test]
    fn flex_remainder_goes_one_cell_each_to_the_leftmost() {
        // avail 10 − 2 = 8; wanted 3 → 5 spare over three equal weights.
        let cols = [
            col("A", 1).flex(1),
            col("B", 1).flex(1),
            col("C", 1).flex(1),
        ];
        assert_eq!(layout_widths(&cols, 10), vec![3, 3, 2]);
    }

    #[test]
    fn without_flex_the_spare_stays_empty_and_hidden_flex_gets_none() {
        assert_eq!(layout_widths(&[col("A", 5), col("B", 5)], 30), vec![5, 5]);
        let cols = [col("A", 5).flex(1).hidden(true), col("B", 5)];
        assert_eq!(layout_widths(&cols, 30), vec![0, 5]);
        let cols = [col("A", 5).flex(1), col("B", 5)];
        assert_eq!(
            layout_widths(&cols, 8),
            vec![4, 3],
            "no spare: flex columns shrink like the others"
        );
    }

    #[test]
    fn a_zero_width_flex_column_fills_the_rest() {
        // avail 20 − 1 separator = 19; A keeps 5, X (width 0) takes 14.
        let cols = [col("A", 5), col("X", 0).flex(1)];
        assert_eq!(layout_widths(&cols, 20), vec![5, 14]);
        // No spare: it still shows one cell and its separator counts.
        assert_eq!(layout_widths(&cols, 7), vec![5, 1]);
    }

    #[test]
    fn huge_widths_saturate_instead_of_overflowing() {
        let big = i32::MAX / 2;
        let cols = [col("A", big).min_width(big), col("B", big).min_width(big)];
        let w = layout_widths(&cols, 10);
        assert_eq!(w, vec![big, big], "nothing may shrink");
        assert_eq!(line_width(&w), i32::MAX);
        let l = RowLayout::new(&cols, 0, 10);
        assert_eq!((l.content, l.view), (i32::MAX, 10));
        assert_eq!(l.max_offset(), i32::MAX - 10);
        assert_eq!(l.column_at(3, l.max_offset()), Some(1));
        assert_eq!(slice_cols("abc", i32::MAX - 1, 10), "");
    }

    #[test]
    fn row_layout_overflows_only_past_min_width() {
        let cols = [col("A", 10).min_width(8), col("B", 10).min_width(8)];
        // Shrinkable to fit: no overflow.
        let l = RowLayout::new(&cols, 0, 17);
        assert_eq!((l.content, l.view, l.max_offset()), (17, 17, 0));
        // 8 + 1 + 8 = 17 cells of content in 12: overflow by 5.
        let l = RowLayout::new(&cols, 0, 12);
        assert_eq!((l.content, l.view, l.max_offset()), (17, 12, 5));
        assert_eq!(l.clamp(-3), 0);
        assert_eq!(l.clamp(3), 3);
        assert_eq!(l.clamp(99), 5, "the offset clamps to the overflow");
    }

    #[test]
    fn row_layout_keeps_the_frozen_column_out_of_the_scroll() {
        let cols = [
            check_column(),
            col("A", 6).min_width(6),
            col("B", 6).min_width(6),
        ];
        // 3 + 1 frozen; 13 content cells in 16 − 4 = 12.
        let l = RowLayout::new(&cols, 1, 16);
        assert_eq!((l.frozen_width, l.content, l.view), (4, 13, 12));
        assert_eq!(l.max_offset(), 1);
        let rest = "aaaaaa bbbbbb";
        assert_eq!(l.compose("[X]", rest, 0), "[X] aaaaaa bbbbb");
        assert_eq!(l.compose("[X]", rest, 1), "[X] aaaaa bbbbbb");
        assert_eq!(l.compose("[X]", rest, 9), "[X] aaaaa bbbbbb", "clamped");
        // Hit test: the frozen cell, then the scrolled columns.
        assert_eq!(l.column_at(1, 1), Some(0));
        assert_eq!(l.column_at(3, 1), None, "frozen separator");
        assert_eq!(l.column_at(4, 0), Some(1));
        assert_eq!(l.column_at(9, 0), Some(1));
        assert_eq!(l.column_at(9, 1), None, "scrolled: now the separator");
        assert_eq!(l.column_at(10, 1), Some(2));
        assert_eq!(l.column_at(16, 1), None, "past the view");
    }

    #[test]
    fn slice_cols_cuts_by_display_columns() {
        assert_eq!(slice_cols("abcdef", 2, 3), "cde");
        assert_eq!(slice_cols("abc", 1, 9), "bc");
        assert_eq!(slice_cols("abc", 5, 2), "");
        // A wide glyph cut by the left or the right edge leaves a space.
        assert_eq!(slice_cols("日本語", 1, 4), " 本 ");
        assert_eq!(slice_cols("日本語", 2, 4), "本語");
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
