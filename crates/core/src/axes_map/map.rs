//! Axes-map text parsing, formatting and config comparison.

// Functions are consumed in Task 4; suppress the unused warning until then.
#![allow(dead_code)]

use super::{ChipAxis, ConfigCheck};

/// Return the inverse of the signed-permutation matrix encoded by `map`.
///
/// Returns `None` for `None` input, the identity map `"x,y,z"`, or any text
/// that is not a valid signed permutation of the three printer axes.
pub(crate) fn inverse_matrix(map: Option<&str>) -> Option<[[f64; 3]; 3]> {
    let raw = map?;
    let norm = raw.replace(' ', "").to_lowercase();
    if norm == "x,y,z" {
        return None;
    }
    let parts: Vec<&str> = norm.split(',').collect();
    if parts.len() != 3 {
        return None;
    }

    // Parse each entry into (printer_axis_index, sign).
    let mut forward = [[0.0f64; 3]; 3]; // forward[printer_axis][chip_axis]
    let mut used = [false; 3];
    for (chip_idx, part) in parts.iter().enumerate() {
        let (sign, axis_str) = if let Some(a) = part.strip_prefix('-') {
            (-1.0, a)
        } else {
            (1.0, *part)
        };
        let printer_idx = match axis_str {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            _ => return None,
        };
        if used[printer_idx] {
            return None; // repeated printer axis
        }
        used[printer_idx] = true;
        forward[printer_idx][chip_idx] = sign;
    }

    // Inverse of a signed permutation = its transpose.
    let mut inv = [[0.0f64; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            inv[r][c] = forward[c][r];
        }
    }
    Some(inv)
}

/// Format a detected axis mapping as a Klipper `axes_map` string.
///
/// Returns `None` when two chip axes map to the same printer axis (not a permutation).
pub(crate) fn format_map(axes: &[(ChipAxis, bool); 3]) -> Option<String> {
    // Check that no printer axis is repeated.
    let mut used = [false; 3];
    for (printer_axis, _neg) in axes {
        let idx = chip_axis_index(*printer_axis);
        if used[idx] {
            return None;
        }
        used[idx] = true;
    }

    let parts: Vec<String> = axes
        .iter()
        .map(|(printer_axis, neg)| {
            let name = chip_axis_name(*printer_axis);
            if *neg {
                format!("-{name}")
            } else {
                name.to_owned()
            }
        })
        .collect();
    Some(parts.join(", "))
}

fn chip_axis_index(axis: ChipAxis) -> usize {
    match axis {
        ChipAxis::X => 0,
        ChipAxis::Y => 1,
        ChipAxis::Z => 2,
    }
}

fn chip_axis_name(axis: ChipAxis) -> &'static str {
    match axis {
        ChipAxis::X => "x",
        ChipAxis::Y => "y",
        ChipAxis::Z => "z",
    }
}

/// Compare the currently-configured axes_map with what was detected.
pub(crate) fn config_check(configured: Option<&str>, detected: &str) -> ConfigCheck {
    let Some(raw) = configured else {
        return ConfigCheck::NoneConfigured;
    };
    let norm = raw.replace(' ', "").to_lowercase();
    if norm == "x,y,z" {
        return ConfigCheck::NoneConfigured;
    }
    let det_norm = detected.replace(' ', "").to_lowercase();
    if norm == det_norm {
        ConfigCheck::AlreadyCorrect
    } else {
        ConfigCheck::DoesNotMatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChipAxis::{X, Y, Z};

    #[test]
    fn inverse_of_negz_y_x_undoes_the_forward_map() {
        // Forward map "-z, y, x":
        //   chip X → printer -Z  (printer_Z = -chip_X)
        //   chip Y → printer  Y  (printer_Y =  chip_Y)
        //   chip Z → printer  X  (printer_X =  chip_Z)
        let chip = [3.0_f64, 5.0, 7.0]; // arbitrary chip vector

        // Apply forward transform: printer = M * chip
        // printer_X = chip_Z = 7
        // printer_Y = chip_Y = 5
        // printer_Z = -chip_X = -3
        let printer = [chip[2], chip[1], -chip[0]];

        // Apply inverse matrix to recover chip vector.
        let inv = inverse_matrix(Some("-z, y, x")).expect("valid map");
        let recovered: Vec<f64> = (0..3)
            .map(|r| (0..3).map(|c| inv[r][c] * printer[c]).sum())
            .collect();

        for i in 0..3 {
            assert!(
                (recovered[i] - chip[i]).abs() < 1e-12,
                "axis {i}: got {}, expected {}",
                recovered[i],
                chip[i]
            );
        }
    }

    #[test]
    fn identity_and_invalid_maps_have_no_inverse() {
        assert_eq!(inverse_matrix(None), None);
        assert_eq!(inverse_matrix(Some("x, y, z")), None);
        assert_eq!(inverse_matrix(Some("x,y")), None);
        assert_eq!(inverse_matrix(Some("x,y,w")), None);
    }

    #[test]
    fn format_map_writes_comma_space_and_rejects_repeats() {
        assert_eq!(
            format_map(&[(X, false), (Z, true), (Y, false)]),
            Some("x, -z, y".to_owned())
        );
        assert_eq!(format_map(&[(X, false), (X, true), (Y, false)]), None);
    }

    #[test]
    fn config_check_ignores_spaces_and_identity() {
        // No configured map → NoneConfigured.
        assert_eq!(config_check(None, "x, -z, y"), ConfigCheck::NoneConfigured);
        // Identity configured map → NoneConfigured.
        assert_eq!(
            config_check(Some("x, y, z"), "x, -z, y"),
            ConfigCheck::NoneConfigured
        );
        // Matching map (spaces ignored) → AlreadyCorrect.
        assert_eq!(
            config_check(Some("x,-z,y"), "x, -z, y"),
            ConfigCheck::AlreadyCorrect
        );
        // Mismatching map → DoesNotMatch.
        assert_eq!(
            config_check(Some("-z, y, x"), "x, -z, y"),
            ConfigCheck::DoesNotMatch
        );
    }
}
