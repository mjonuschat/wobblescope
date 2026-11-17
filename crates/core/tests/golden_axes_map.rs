//! Axes-map results against Shake&Tune's `AxesMapComputation`, through the public API.

mod common;

use common::{AxesMapCase, AxesMapSegment, axes_map_cases, close};
use wobblescope_core::RawSamples;
use wobblescope_core::axes_map::{
    AxesMapInput, AxesMapParams, AxesMapResult, AxisState, MapState, Mounting, NoiseVerdict,
    QualityStatus, analyze_axes_map,
};

fn raw(s: &AxesMapSegment) -> RawSamples {
    RawSamples {
        t: s.t.clone(),
        x: s.x.clone(),
        y: s.y.clone(),
        z: s.z.clone(),
    }
}

fn params(case: &AxesMapCase) -> AxesMapParams {
    AxesMapParams {
        speed: 80.0,
        accel: case.accel,
        max_z_velocity: None,
        max_z_accel: None,
    }
}

fn run(case: &AxesMapCase) -> AxesMapResult {
    let input = AxesMapInput {
        chip: "adxl345".to_owned(),
        segments: std::array::from_fn(|i| raw(&case.segments[i])),
        windows: [None, None, None],
        configured_map: case.configured_map.clone(),
        mounting: Mounting::Toolhead,
    };
    analyze_axes_map(&input, &params(case)).unwrap()
}

fn status_name(s: QualityStatus) -> &'static str {
    match s {
        QualityStatus::Ok => "ok",
        QualityStatus::Warning => "warning",
        QualityStatus::Error => "error",
    }
}

fn assert_matches_reference(name: &str, case: &AxesMapCase, got: &AxesMapResult) {
    let want = &case.expected;
    assert_eq!(got.state, MapState::Complete, "{name}: state");
    assert_eq!(
        got.map.as_deref(),
        Some(want.formatted.as_str()),
        "{name}: map"
    );
    let euler = got.euler_deg.expect("euler");
    let rotation = got.rotation.expect("rotation");
    for i in 0..3 {
        close(&format!("{name}: euler[{i}]"), euler[i], want.euler_deg[i]);
        for (j, (g, w)) in rotation[i].iter().zip(want.rotation_matrix[i]).enumerate() {
            close(&format!("{name}: rotation[{i}][{j}]"), *g, w);
        }
    }
    assert_eq!(
        status_name(got.quality.status),
        want.quality.status,
        "{name}: status"
    );
    assert_eq!(
        got.quality.messages, want.quality.messages,
        "{name}: messages"
    );
    close(&format!("{name}: noise"), got.noise, want.noise_level);
    close(
        &format!("{name}: gravity"),
        got.gravity * 1000.0,
        want.gravity,
    );
    for (i, seg) in want.per_segment.iter().enumerate() {
        let axis = &got.axes[i];
        let expected_virtual = want.extrapolated_axis == Some(i);
        if expected_virtual {
            assert_eq!(axis.state, AxisState::Virtual, "{name}: axis {i} state");
            assert_eq!(axis.confidence, 0.0, "{name}: axis {i} confidence");
        } else {
            assert_eq!(axis.state, AxisState::Measured, "{name}: axis {i} state");
            close(
                &format!("{name}: confidence {i}"),
                axis.confidence,
                seg.confidence,
            );
            close(&format!("{name}: tilt {i}"), axis.tilt_deg, seg.angle_error);
            for k in 0..3 {
                close(
                    &format!("{name}: peak {i}.{k}"),
                    axis.peaks[k],
                    seg.peak_velocities[k],
                );
            }
        }
    }
}

#[test]
fn toolhead_results_match_the_reference() {
    let golden = axes_map_cases();
    for (name, case) in &golden.cases {
        if name == "mirrored" || name == "duplicate_axis" {
            continue;
        }
        assert_matches_reference(name, case, &run(case));
    }
}

#[test]
fn the_noisy_case_is_flagged_by_the_verdict_too() {
    let golden = axes_map_cases();
    let got = run(&golden.cases["noisy_warning"]);
    assert_eq!(got.noise_verdict, NoiseVerdict::High);
    let got = run(&golden.cases["toolhead_identity"]);
    assert_eq!(got.noise_verdict, NoiseVerdict::Fine);
}

#[test]
fn mirrored_directions_are_rotated_like_the_reference() {
    // Deviation H, faithful: a mirrored chip is silently turned into a proper rotation by the
    // SVD step, and the map is still reported.
    let golden = axes_map_cases();
    let case = &golden.cases["mirrored"];
    assert_matches_reference("mirrored", case, &run(case));
}

#[test]
fn a_repeated_chip_axis_is_ambiguous_with_no_map() {
    let golden = axes_map_cases();
    let case = &golden.cases["duplicate_axis"];
    let got = run(case);
    assert_eq!(got.state, MapState::Ambiguous);
    assert_eq!(got.map, None);
    assert_eq!(got.rotation, None);
    assert_eq!(got.euler_deg, None);
    assert_eq!(
        status_name(got.quality.status),
        case.expected.quality.status
    );
    assert_eq!(got.quality.messages, case.expected.quality.messages);
}

#[test]
fn a_degenerate_cross_product_is_ambiguous_not_nan() {
    // Deviation E: X and Y sweeps show the same chip axis and Z has no signal, so the third
    // axis cannot be derived. The reference would divide by zero here.
    let golden = axes_map_cases();
    let case = &golden.cases["virtual_z"];
    let input = AxesMapInput {
        chip: "adxl345".to_owned(),
        segments: [
            raw(&case.segments[0]),
            raw(&case.segments[0]),
            raw(&case.segments[2]),
        ],
        windows: [None, None, None],
        configured_map: None,
        mounting: Mounting::Toolhead,
    };
    let got = analyze_axes_map(&input, &params(case)).unwrap();
    assert_eq!(got.state, MapState::Ambiguous);
    assert_eq!(got.map, None);
    assert_eq!(got.quality.status, QualityStatus::Error);
    assert!(
        got.axes
            .iter()
            .all(|a| a.confidence.is_finite() && a.tilt_deg.is_finite())
    );
}

/// Apply Klipper's `axes_map = -z, y, x`: out_x = -raw_z, out_y = raw_y, out_z = raw_x.
fn remap_minus_z_y_x(s: &AxesMapSegment) -> RawSamples {
    RawSamples {
        t: s.t.clone(),
        x: s.z.iter().map(|v| -v).collect(),
        y: s.y.clone(),
        z: s.x.clone(),
    }
}

#[test]
fn a_configured_map_gives_the_same_map_as_none() {
    let golden = axes_map_cases();
    let identity = &golden.cases["toolhead_identity"];
    let reference = run(identity);

    let input = AxesMapInput {
        chip: "adxl345".to_owned(),
        segments: std::array::from_fn(|i| remap_minus_z_y_x(&identity.segments[i])),
        windows: [None, None, None],
        configured_map: Some("-z, y, x".to_owned()),
        mounting: Mounting::Toolhead,
    };
    let got = analyze_axes_map(&input, &params(identity)).unwrap();
    assert_eq!(got.state, MapState::Complete);
    assert_eq!(got.map, reference.map);
    assert_eq!(got.configured_map.as_deref(), Some("-z, y, x"));
    for i in 0..3 {
        close(
            &format!("rotation row {i}"),
            got.rotation.unwrap()[i][0],
            reference.rotation.unwrap()[i][0],
        );
    }

    // The golden `configured_map` case itself matches the reference.
    let case = &golden.cases["configured_map"];
    assert_matches_reference("configured_map", case, &run(case));
}
