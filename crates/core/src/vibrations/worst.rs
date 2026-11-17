//! The worst case over several chips.

use super::analysis::{VibrationsParams, VibrationsResult, derive};
use super::grid::Grid;

/// The profile of the element-wise maximum of the chips' grids, with the same derivation as one
/// chip. It has no motor profile and no steps, which belong to each chip. One grid gives that
/// chip's own figures; no grids, or grids that differ in shape, give `None`.
pub fn worst_case(grids: &[&Grid], p: &VibrationsParams) -> Option<VibrationsResult> {
    let (first, rest) = grids.split_first()?;
    let same_shape = |g: &&Grid| {
        g.data.len() == first.data.len()
            && g.speeds == first.speeds
            && g.data
                .iter()
                .zip(&first.data)
                .all(|(a, b)| a.len() == b.len())
    };
    if !rest.iter().all(same_shape) {
        return None;
    }
    let mut worst = (*first).clone();
    for grid in rest {
        for (row, other) in worst.data.iter_mut().zip(&grid.data) {
            for (cell, value) in row.iter_mut().zip(other) {
                *cell = cell.max(*value);
            }
        }
    }
    Some(derive(&worst, p.kinematics))
}
