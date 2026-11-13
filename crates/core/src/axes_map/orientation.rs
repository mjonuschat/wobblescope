//! Orientation from three sweeps: ports of Shake&Tune's `_detect_noise_only_axis`,
//! `_extrapolate_missing_axis`, `_orthonormalize_rotation_matrix` and `_extract_euler_xyz`.

use crate::error::CoreError;

/// Confidence below which an axis with weak motion counts as noise-only.
const NOISE_CONFIDENCE_THRESHOLD: f64 = 0.3;
/// Smallest cross product length treated as a usable third axis.
const MIN_CROSS_NORM: f64 = 1e-12;
/// Jacobi sweeps allowed before the SVD gives up refining.
const MAX_SWEEPS: usize = 60;

type Matrix = [[f64; 3]; 3];

/// Find the one axis that carries no signal, if any.
///
/// An axis is noise-only when its confidence is below 0.3 and its peak is below a quarter of
/// the largest peak. `peaks` are the largest absolute peak velocities per segment.
///
/// # Errors
/// `InvalidParameter` when two or more axes have no signal.
pub(crate) fn relative_noise_only_axis(
    confidences: [f64; 3],
    peaks: [f64; 3],
) -> Result<Option<usize>, CoreError> {
    let threshold = peaks.iter().copied().fold(f64::NEG_INFINITY, f64::max) / 4.0;
    let noise: Vec<usize> = (0..3)
        .filter(|&i| confidences[i] < NOISE_CONFIDENCE_THRESHOLD && peaks[i] < threshold)
        .collect();
    match noise.as_slice() {
        [] => Ok(None),
        [i] => Ok(Some(*i)),
        many => Err(CoreError::InvalidParameter(format!(
            "Multiple axes ({}) have no signal. Ensure accelerometer is properly mounted and \
             moves with toolhead.",
            many.len()
        ))),
    }
}

/// Fill `directions[missing]` with the cross product of the other two, signed for a
/// right-handed system.
///
/// Returns false and leaves `directions` unchanged when the two measured vectors are parallel
/// or equal.
pub(crate) fn complete_axis(directions: &mut [[f64; 3]; 3], missing: usize) -> bool {
    let (i, j) = match missing {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let (a, b) = (directions[i], directions[j]);
    let mut cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    // Rows X and Z give -Y.
    if (i, j) == (0, 2) {
        cross = cross.map(|c| -c);
    }
    let norm = cross.iter().map(|c| c * c).sum::<f64>().sqrt();
    // D22: the reference divides by this norm unchecked and yields NaN for parallel vectors.
    if norm < MIN_CROSS_NORM {
        return false;
    }
    directions[missing] = cross.map(|c| c / norm);
    true
}

/// The closest proper rotation to `m`: `U * Vt` from an SVD, with the last column of `U`
/// negated when the determinant is negative, as the reference does.
pub(crate) fn nearest_rotation(m: Matrix) -> Matrix {
    let (mut u, vt) = svd(m);
    let product = multiply(&u, &vt);
    if determinant(&product) < 0.0 {
        for row in &mut u {
            row[2] = -row[2];
        }
        return multiply(&u, &vt);
    }
    product
}

/// XYZ Euler angles (roll, pitch, yaw) in degrees, with the reference's formulas and its
/// gimbal test `sy > 1e-6`.
pub(crate) fn euler_degrees(r: &Matrix) -> [f64; 3] {
    let sy = (r[2][1] * r[2][1] + r[2][2] * r[2][2]).sqrt();
    let (roll, pitch, yaw) = if sy > 1e-6 {
        (
            r[2][1].atan2(r[2][2]),
            (-r[2][0]).atan2(sy),
            r[1][0].atan2(r[0][0]),
        )
    } else {
        ((-r[1][2]).atan2(r[1][1]), (-r[2][0]).atan2(sy), 0.0)
    };
    [roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees()]
}

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

fn determinant(m: &Matrix) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// One-sided Jacobi SVD of a 3x3 matrix: returns `U` and `Vt` with singular values sorted
/// descending, so the last column of `U` belongs to the smallest.
fn svd(m: Matrix) -> (Matrix, Matrix) {
    // Work on columns: a[k] is column k of the matrix being orthogonalised.
    let mut a: [[f64; 3]; 3] = std::array::from_fn(|k| [m[0][k], m[1][k], m[2][k]]);
    let mut v: [[f64; 3]; 3] = std::array::from_fn(|k| std::array::from_fn(|r| f64::from(r == k)));

    for _ in 0..MAX_SWEEPS {
        let mut rotated = false;
        for p in 0..2 {
            for q in p + 1..3 {
                let alpha: f64 = a[p].iter().map(|x| x * x).sum();
                let beta: f64 = a[q].iter().map(|x| x * x).sum();
                let gamma: f64 = (0..3).map(|r| a[p][r] * a[q][r]).sum();
                if gamma == 0.0 || gamma.abs() <= f64::EPSILON * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for r in 0..3 {
                    let (ap, aq) = (a[p][r], a[q][r]);
                    a[p][r] = c * ap - s * aq;
                    a[q][r] = s * ap + c * aq;
                    let (vp, vq) = (v[p][r], v[q][r]);
                    v[p][r] = c * vp - s * vq;
                    v[q][r] = s * vp + c * vq;
                }
            }
        }
        if !rotated {
            break;
        }
    }

    let sigma: [f64; 3] = std::array::from_fn(|k| a[k].iter().map(|x| x * x).sum::<f64>().sqrt());
    let mut order = [0, 1, 2];
    order.sort_by(|&x, &y| sigma[y].total_cmp(&sigma[x]));

    // Columns of U in sorted order. A zero singular value has no direction in `a`, so the
    // column is completed to an orthonormal set.
    let mut u_cols: Vec<[f64; 3]> = Vec::with_capacity(3);
    for &k in &order {
        if sigma[k] > f64::EPSILON * sigma[order[0]] && sigma[k] > 0.0 {
            u_cols.push(a[k].map(|x| x / sigma[k]));
        } else {
            u_cols.push(orthogonal_completion(&u_cols));
        }
    }

    let u: Matrix = std::array::from_fn(|r| std::array::from_fn(|c| u_cols[c][r]));
    // Vt row c is the sorted column c of V.
    let vt: Matrix = std::array::from_fn(|c| v[order[c]]);
    (u, vt)
}

/// A unit vector orthogonal to every vector in `basis` (at most two orthonormal vectors).
fn orthogonal_completion(basis: &[[f64; 3]]) -> [f64; 3] {
    if let [a, b] = basis {
        let c = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let norm = c.iter().map(|x| x * x).sum::<f64>().sqrt();
        return c.map(|x| x / norm);
    }
    let mut best = [0.0; 3];
    let mut best_norm = -1.0;
    for axis in 0..3 {
        let mut e = [0.0; 3];
        e[axis] = 1.0;
        for b in basis {
            let dot: f64 = (0..3).map(|k| e[k] * b[k]).sum();
            for k in 0..3 {
                e[k] -= dot * b[k];
            }
        }
        let norm = e.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > best_norm {
            best_norm = norm;
            best = e.map(|x| x / norm);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rotation_matches_svd_on_a_mirrored_matrix() {
        // Mirrored X has determinant -1, so the reference rotates it rather than refusing it.
        let m = [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        // numpy returns U = I and Vt = diag(-1, 1, 1); negating the last column of U gives:
        assert_eq!(
            nearest_rotation(m),
            [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]
        );
    }

    #[test]
    fn nearest_rotation_recovers_a_rotation_from_a_noisy_copy() {
        let (c, s) = (0.3_f64.cos(), 0.3_f64.sin());
        let rot = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
        let noisy = [
            [c + 0.01, -s, 0.002],
            [s, c - 0.01, 0.0],
            [0.003, 0.0, 1.02],
        ];
        let r = nearest_rotation(noisy);
        for i in 0..3 {
            for j in 0..3 {
                assert!((r[i][j] - rot[i][j]).abs() < 0.03, "{i}{j}: {}", r[i][j]);
            }
        }
    }

    #[test]
    fn degenerate_cross_product_is_refused_not_nan() {
        // Two measured vectors on the same chip axis, so no third axis exists.
        let mut d = [[1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let before = d;
        assert!(!complete_axis(&mut d, 2));
        assert_eq!(d, before);
        // Opposite vectors are parallel too.
        let mut d = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(!complete_axis(&mut d, 2));
    }

    #[test]
    fn complete_axis_gives_a_right_handed_third_axis() {
        let mut d = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0; 3]];
        assert!(complete_axis(&mut d, 2));
        assert_eq!(d[2], [0.0, 0.0, 1.0]);
        let mut d = [[1.0, 0.0, 0.0], [0.0; 3], [0.0, 0.0, 1.0]];
        assert!(complete_axis(&mut d, 1));
        assert_eq!(d[1], [0.0, 1.0, 0.0]);
        let mut d = [[0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(complete_axis(&mut d, 0));
        assert_eq!(d[0], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn two_noise_axes_are_an_error_with_the_reference_text() {
        let e = relative_noise_only_axis([0.1, 0.1, 1.0], [1.0, 1.0, 100.0]).unwrap_err();
        assert!(e.to_string().contains(
            "Multiple axes (2) have no signal. Ensure accelerometer is properly mounted and \
             moves with toolhead."
        ));
        assert_eq!(
            relative_noise_only_axis([1.0, 0.1, 1.0], [100.0, 1.0, 100.0]).unwrap(),
            Some(1)
        );
        assert_eq!(
            relative_noise_only_axis([1.0; 3], [100.0; 3]).unwrap(),
            None
        );
    }

    #[test]
    fn euler_of_identity_is_zero() {
        let i = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(euler_degrees(&i), [0.0, 0.0, 0.0]);
    }
}
