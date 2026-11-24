use super::analysis::{MeasuredCurve, VibrationsParams, VibrationsResult, derive};
use super::grid::Grid;

/// The angles every chip measured with the same recorded speeds, each with the element-wise
/// maximum of the chips' energy. Equal grid axes are not enough: two speed lists can share their
/// ends and their count.
fn common_measured(chips: &[(&Grid, &[MeasuredCurve])]) -> Vec<MeasuredCurve> {
    let Some(((_, first), rest)) = chips.split_first() else {
        return Vec::new();
    };
    first
        .iter()
        .filter_map(|curve| {
            let mut combined = curve.clone();
            for (_, other) in rest {
                let same = other
                    .iter()
                    .find(|o| o.angle == curve.angle && o.speeds == curve.speeds)?;
                for (value, other_value) in combined.energy.iter_mut().zip(&same.energy) {
                    *value = value.max(*other_value);
                }
            }
            Some(combined)
        })
        .collect()
}

/// The profile of the element-wise maximum of the chips' grids, with the same derivation as one
/// chip. It has no motor profile and no steps, which belong to each chip. Each chip is its grid
/// and its measured curves; the result lists the angles all of them measured at the same speeds.
/// One grid gives that chip's own figures. No grids, grids that differ in shape, or a grid of
/// fewer than two speeds give `None`.
pub fn worst_case(
    chips: &[(&Grid, &[MeasuredCurve])],
    p: &VibrationsParams,
) -> Option<VibrationsResult> {
    let ((first, _), rest) = chips.split_first()?;
    if first.speeds.len() < 2 {
        return None;
    }
    let same_shape = |g: &Grid| {
        g.data.len() == first.data.len()
            && g.speeds == first.speeds
            && g.angles == first.angles
            && g.data
                .iter()
                .zip(&first.data)
                .all(|(a, b)| a.len() == b.len())
    };
    if !rest.iter().all(|(g, _)| same_shape(g)) {
        return None;
    }
    let mut worst = (*first).clone();
    for (grid, _) in rest {
        for (row, other) in worst.data.iter_mut().zip(&grid.data) {
            for (cell, value) in row.iter_mut().zip(other) {
                *cell = cell.max(*value);
            }
        }
    }
    let mut result = derive(&worst, p.kinematics);
    result.measured = common_measured(chips);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_angles_give_no_worst_case() {
        let grid = |angles: Vec<f64>| Grid {
            angles,
            speeds: vec![20.0, 25.0],
            data: vec![vec![1.0, 2.0], vec![2.0, 3.0]],
        };
        let a = grid(vec![0.0, 90.0]);
        let b = grid(vec![0.0, 180.0]);
        assert!(worst_case(&[(&a, &[]), (&b, &[])], &VibrationsParams::default()).is_none());
    }

    #[test]
    fn a_grid_with_fewer_than_two_speeds_gives_none() {
        let grid = Grid {
            angles: vec![0.0, 90.0],
            speeds: vec![20.0],
            data: vec![vec![1.0], vec![2.0]],
        };
        assert!(worst_case(&[(&grid, &[])], &VibrationsParams::default()).is_none());
    }
}
