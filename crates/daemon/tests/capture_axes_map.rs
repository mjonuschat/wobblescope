mod common;

use common::fake_printer::fixture;
use wobblescope::capture::axes_map::{AxesMapRequest, ChipChoice, plan_axes_map};
use wobblescope::capture::meta::{ChipFacts, initial};
use wobblescope::capture::request::TestKind;
use wobblescope::moonraker::settings::{PrinterSettings, parse_settings};
use wobblescope_core::axes_map::Mounting;
use wobblescope_core::direction::Direction;
use wobblescope_session::meta::FORMAT_VERSION;

fn printer() -> PrinterSettings {
    parse_settings(
        &fixture("real_settings.json"),
        &fixture("real_printer_info.json"),
    )
    .unwrap()
}

fn request() -> AxesMapRequest {
    AxesMapRequest {
        chips: vec![
            ChipChoice {
                chip: "adxl345".to_owned(),
                mounting: Mounting::Toolhead,
            },
            ChipChoice {
                chip: "adxl345 bed".to_owned(),
                mounting: Mounting::Bed,
            },
        ],
        z_height: None,
        speed: None,
        accel: None,
        travel_speed: None,
    }
}

fn refused(request: &AxesMapRequest, printer: &PrinterSettings) -> String {
    plan_axes_map(request, printer, 600)
        .expect_err("the request must be refused")
        .to_string()
}

#[test]
fn defaults_match_shake_and_tune() {
    let plan = plan_axes_map(&request(), &printer(), 600).unwrap();
    assert_eq!(plan.kind, TestKind::AxesMap);
    let Some(axes_map) = &plan.axes_map else {
        panic!("not an axes-map plan");
    };
    assert_eq!(
        (
            axes_map.run.z_height,
            axes_map.run.speed,
            axes_map.run.accel,
            axes_map.run.travel_speed
        ),
        (20.0, 80.0, 1500.0, 120.0)
    );
    // The fixture's travel is X 0 to 301 and Y 0 to 308.
    let (mx, my) = (150.5, 154.0);
    assert_eq!(
        axes_map.points,
        [
            [mx - 15.0, my - 15.0, 20.0],
            [mx + 15.0, my - 15.0, 20.0],
            [mx + 15.0, my + 15.0, 20.0],
            [mx + 15.0, my + 15.0, 50.0],
        ]
    );
    assert_eq!(
        plan.segments.iter().map(|s| s.axis).collect::<Vec<_>>(),
        [
            Direction::Vector([1, 0, 0]),
            Direction::Vector([0, 1, 0]),
            Direction::Vector([0, 0, 1]),
        ]
    );
    assert_eq!(plan.segments[2].position, axes_map.points[2]);
    assert_eq!(plan.segments[2].target, Some(axes_map.points[3]));
}

#[test]
fn speed_under_20_is_refused_naming_the_field() {
    let mut r = request();
    r.speed = Some(19.0);
    assert!(refused(&r, &printer()).starts_with("speed:"));
    r.speed = None;
    r.accel = Some(99.0);
    assert!(refused(&r, &printer()).starts_with("accel:"));
    r.accel = None;
    r.travel_speed = Some(10.0);
    assert!(refused(&r, &printer()).starts_with("travel_speed:"));
}

#[test]
fn a_z_top_above_the_travel_is_refused() {
    let mut r = request();
    r.z_height = Some(280.0);
    assert!(refused(&r, &printer()).starts_with("z_height:"));
}

#[test]
fn missing_axis_limits_are_refused() {
    let mut p = printer();
    p.axis_limits = None;
    assert!(refused(&request(), &p).starts_with("axis_limits:"));
}

#[test]
fn an_empty_or_repeated_chip_list_is_refused() {
    let mut r = request();
    r.chips.clear();
    assert!(refused(&r, &printer()).starts_with("chips:"));
    let mut r = request();
    r.chips[1].chip = "adxl345".to_owned();
    assert!(refused(&r, &printer()).starts_with("chips:"));
}

#[test]
fn segment_script_is_exactly_the_documented_text() {
    let plan = plan_axes_map(&request(), &printer(), 600).unwrap();
    assert_eq!(
        plan.segment_script(&plan.segments[0]),
        "SAVE_GCODE_STATE NAME=wobblescope\nG91\nG4 P500\nG1 X30 Y0 Z0 F4800\nG4 P500\n\
         M400\nRESTORE_GCODE_STATE NAME=wobblescope"
    );
}

#[test]
fn segment_script_uses_signed_deltas_and_restores_state() {
    let plan = plan_axes_map(&request(), &printer(), 600).unwrap();
    let mut segment = plan.segments[0];
    segment.position = [10.0, 20.0, 30.0];
    segment.target = Some([5.0, 20.0, 32.5]);
    assert_eq!(
        plan.segment_script(&segment),
        "SAVE_GCODE_STATE NAME=wobblescope\nG91\nG4 P500\nG1 X-5 Y0 Z2.5 F4800\nG4 P500\n\
         M400\nRESTORE_GCODE_STATE NAME=wobblescope"
    );
}

#[test]
fn segment_script_requires_a_calibration_and_target() {
    let mut plan = plan_axes_map(&request(), &printer(), 600).unwrap();
    let mut segment = plan.segments[0];
    segment.target = None;
    assert!(plan.segment_script(&segment).is_empty());
    plan.axes_map = None;
    assert!(plan.segment_script(&plan.segments[0]).is_empty());
}

#[test]
fn meta_for_a_calibration_has_mounting_and_three_points_and_no_mode() {
    let plan = plan_axes_map(&request(), &printer(), 600).unwrap();
    let facts = |section: &str| ChipFacts {
        section: section.to_owned(),
        chip_type: wobblescope::moonraker::chips::ChipType::Adxl345,
        nominal_rate_hz: Some(3200.0),
        observed_rate_hz: 3200.0,
        axes_map: None,
        mcu: None,
    };
    let meta = initial(
        &plan,
        "axes-1",
        "2026-11-12T10:00:00Z",
        &[facts("adxl345"), facts("adxl345 bed")],
        None,
    );
    // The run settings and version 2 come with the first segment; the first journal is a
    // valid session without them.
    assert_eq!(meta.format_version, FORMAT_VERSION);
    assert!(meta.axes_map_run.is_none());
    meta.validate().unwrap();
    assert!(meta.mode.is_none() && meta.settings.is_none());
    assert_eq!(meta.probe_points.len(), 3);
    assert_eq!(meta.probe_points[1].position, Some([165.5, 139.0, 20.0]));
    assert_eq!(meta.chips[0].mounting, Some(Mounting::Toolhead));
    assert_eq!(meta.chips[1].mounting, Some(Mounting::Bed));
}
