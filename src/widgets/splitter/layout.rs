//! Pure layout math for the splitter: constraint types + the flexbox solver.
//! No view, no rendering — fully unit-testable.

/// Which axis a [`Splitter`](super::Splitter) divides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    /// Side-by-side panes, **vertical** dividers, split along **x** (`cols`).
    Cols,
    /// Stacked panes, **horizontal** dividers, split along **y** (`rows`).
    Rows,
}

/// How a pane claims space along the splitter axis. `min == max` ⇒ a fixed pane.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Constraints {
    /// Share of *free* space (after every pane has its `min`). Authoring unit.
    pub weight: u16,
    /// Hard minimum size in cells along the axis.
    pub min: i32,
    /// Hard maximum size in cells along the axis.
    pub max: i32,
}

impl Constraints {
    /// Fully elastic: weight 1, `min 0`, `max i32::MAX`.
    pub fn flex() -> Self {
        Constraints {
            weight: 1,
            min: 0,
            max: i32::MAX,
        }
    }
    /// Elastic with a custom weight.
    pub fn weight(w: u16) -> Self {
        Constraints {
            weight: w.max(1),
            min: 0,
            max: i32::MAX,
        }
    }
    /// Pinned to exactly `n` cells (`min == max == n`).
    pub fn fixed(n: i32) -> Self {
        Constraints {
            weight: 1,
            min: n,
            max: n,
        }
    }
    /// Builder: set the minimum.
    pub fn min(mut self, n: i32) -> Self {
        self.min = n;
        self
    }
    /// Builder: set the maximum.
    pub fn max(mut self, n: i32) -> Self {
        self.max = n;
        self
    }
    /// A fixed pane has no free-space share.
    pub fn is_fixed(&self) -> bool {
        self.min == self.max
    }
}

/// One pane's solver input: its hard bounds plus a mutable *effective weight*
/// (seeded from `Constraints.weight`, mutated by drags/relax). Fixed panes
/// (`min == max`) carry weight 0 and take no free space.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub min: i32,
    pub max: i32,
    pub weight: f64,
}

impl Slot {
    pub fn from_constraints(c: Constraints) -> Self {
        Slot {
            min: c.min,
            max: c.max,
            weight: if c.is_fixed() {
                0.0
            } else {
                c.weight.max(1) as f64
            },
        }
    }
    #[allow(dead_code)]
    fn is_fixed(&self) -> bool {
        self.min == self.max
    }
}

/// Apportion `total` cells across `slots`, honoring each `[min, max]` and
/// distributing the remaining free space by `weight`. Deterministic
/// (largest-remainder rounding); the returned sizes always sum to
/// `total.max(sum_of_mins-clamped)` and never violate a slot's bounds.
///
/// `total` here is the **content** length (axis length already minus the
/// dividers' reserved cells — the caller subtracts those).
pub fn solve(slots: &[Slot], total: i32) -> Vec<i32> {
    if slots.is_empty() {
        return Vec::new();
    }
    // 1. Everyone starts at their min.
    let mut sizes: Vec<i32> = slots.iter().map(|s| s.min).collect();
    let used: i32 = sizes.iter().sum();
    let mut free = total - used;

    if free <= 0 {
        // Not enough room even for the mins: shrink proportionally to mins,
        // largest-remainder, never below 0. (Degenerate small-terminal case.)
        return shrink_to_fit(&sizes, total.max(0));
    }

    // 2. Iteratively hand free space to non-saturated, weighted slots.
    //    A slot is "open" if weight > 0 and size < max.
    loop {
        let open: Vec<usize> = (0..slots.len())
            .filter(|&i| slots[i].weight > 0.0 && sizes[i] < slots[i].max)
            .collect();
        let wsum: f64 = open.iter().map(|&i| slots[i].weight).sum();
        if open.is_empty() || wsum == 0.0 || free == 0 {
            break;
        }
        // Ideal real allocation per open slot this round.
        let mut alloc: Vec<(usize, i32, f64)> = open
            .iter()
            .map(|&i| {
                let ideal = free as f64 * slots[i].weight / wsum;
                let floor = ideal.floor() as i32;
                // Clamp to headroom so we never exceed max in one shot.
                let headroom = slots[i].max - sizes[i];
                let give = floor.min(headroom);
                (i, give, ideal - give as f64)
            })
            .collect();
        let mut given: i32 = alloc.iter().map(|(_, g, _)| *g).sum();
        // Largest-remainder: hand out the leftover cells one-by-one to the
        // open slots with the biggest fractional remainder and remaining headroom.
        let mut leftover = free - given;
        alloc.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
        let mut idx = 0;
        while leftover > 0 && !alloc.is_empty() {
            let len = alloc.len();
            let (i, ref mut g, _) = alloc[idx % len];
            if sizes[i] + *g < slots[i].max {
                *g += 1;
                given += 1;
                leftover -= 1;
            }
            idx += 1;
            // Safety: if a full sweep gave nothing, everyone is saturated.
            if idx > len * 2 && leftover == free - given {
                break;
            }
        }
        for (i, g, _) in &alloc {
            sizes[*i] += *g;
        }
        let progressed = given > 0;
        free -= given;
        if !progressed {
            break;
        }
    }
    sizes
}

/// Degenerate path: even the mins don't fit. Shrink proportionally to the mins.
fn shrink_to_fit(mins: &[i32], total: i32) -> Vec<i32> {
    let sum: i32 = mins.iter().sum();
    if sum <= 0 {
        return vec![0; mins.len()];
    }
    let mut sizes: Vec<i32> = mins
        .iter()
        .map(|&m| (m as i64 * total as i64 / sum as i64) as i32)
        .collect();
    // Largest-remainder fix so they sum to `total`.
    let mut deficit = total - sizes.iter().sum::<i32>();
    let mut order: Vec<usize> = (0..mins.len()).collect();
    order.sort_by(|&a, &b| mins[b].cmp(&mins[a]));
    let mut k = 0;
    while deficit > 0 && !order.is_empty() {
        sizes[order[k % order.len()]] += 1;
        deficit -= 1;
        k += 1;
    }
    sizes
}

/// The closed form for [`Splitter::relax`](super::Splitter::relax): the weight to
/// give a pane currently sized `pane_size` so that making it flexible does not
/// move any divider. `other_weights` = sum of the *currently flexible* panes'
/// weights; `free_space` = current free cells (total content minus the sum of
/// mins). Returns the weight; if `free_space <= 0` falls back to `1.0`.
pub fn relax_weight(other_weights: f64, pane_size: i32, free_space: i32) -> f64 {
    if free_space <= 0 || pane_size <= 0 {
        return 1.0;
    }
    (other_weights * pane_size as f64 / free_space as f64).max(0.000_001)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(cs: &[Constraints]) -> Vec<Slot> {
        cs.iter().map(|&c| Slot::from_constraints(c)).collect()
    }

    #[test]
    fn three_equal_columns() {
        let s = slots(&[
            Constraints::flex(),
            Constraints::flex(),
            Constraints::flex(),
        ]);
        // 30 content cells, 3 equal panes.
        assert_eq!(solve(&s, 30), vec![10, 10, 10]);
    }

    #[test]
    fn equal_columns_largest_remainder_is_deterministic() {
        let s = slots(&[
            Constraints::flex(),
            Constraints::flex(),
            Constraints::flex(),
        ]);
        // 31 doesn't divide by 3: extras go to earliest slots, total == 31.
        let out = solve(&s, 31);
        assert_eq!(out.iter().sum::<i32>(), 31);
        assert_eq!(out, vec![11, 10, 10]);
    }

    #[test]
    fn fixed_sidebar_keeps_width_others_take_rest() {
        let s = slots(&[
            Constraints::fixed(20),
            Constraints::flex(),
            Constraints::weight(2),
        ]);
        // 20 fixed + (80 free split 1:2) => 20, ~27, ~53; sum == 100.
        let out = solve(&s, 100);
        assert_eq!(out[0], 20);
        assert_eq!(out.iter().sum::<i32>(), 100);
        assert!(out[2] > out[1], "weight-2 pane is wider");
    }

    #[test]
    fn weight_one_respects_min() {
        let s = slots(&[Constraints::flex().min(10), Constraints::flex()]);
        // Total 12: pane0 floored at min 10, pane1 gets the rest (but min 0).
        // free after mins = 12-10 = 2, split by weight 1:1 => pane0 11, pane1 1.
        let out = solve(&s, 12);
        assert!(out[0] >= 10);
        assert_eq!(out.iter().sum::<i32>(), 12);
    }

    #[test]
    fn max_saturates_and_redistributes() {
        let s = slots(&[Constraints::flex().max(15), Constraints::flex()]);
        // 100 cells: pane0 capped at 15, pane1 absorbs the remaining 85.
        assert_eq!(solve(&s, 100), vec![15, 85]);
    }

    #[test]
    fn relax_weight_preserves_position() {
        // pane currently 20 cells, other flexible weights sum 3, free space 80.
        let w = relax_weight(3.0, 20, 80);
        assert!((w - 0.75).abs() < 1e-9);
    }
}
