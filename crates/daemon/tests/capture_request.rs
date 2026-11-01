mod common;

use common::fake_printer::fixture;
use wobblescope::capture::request::{Overrides, TestRequest, plan};
use wobblescope::moonraker::settings::{PrinterSettings, parse_settings};
use wobblescope_core::fit::Axis;
use wobblescope_session::meta::Mode;

fn printer() -> PrinterSettings {
    let mut query = fixture("real_settings.json");
    // The real printer's period would make "pulse" send an extra parameter; most tests start
    // from a printer in pulse mode.
    query["status"]["configfile"]["settings"]["resonance_tester"]["sweeping_period"] =
        serde_json::json!(0.0);
    parse_settings(&query, &fixture("real_printer_info.json")).unwrap()
}

fn sweeping_printer() -> PrinterSettings {
    parse_settings(
        &fixture("real_settings.json"),
        &fixture("real_printer_info.json"),
    )
    .unwrap()
}

fn request() -> TestRequest {
    TestRequest {
        chips: vec!["adxl345".to_owned(), "lis2dw ebb36".to_owned()],
        axes: vec![Axis::X],
        mode: Mode::Pulse,
        overrides: Overrides::default(),
        points: None,
    }
}

fn refused(request: &TestRequest, printer: &PrinterSettings) -> String {
    plan(request, printer, 600)
        .expect_err("the request must be refused")
        .to_string()
}

#[test]
fn a_pulse_test_with_no_overrides_sends_no_frequency_parameters() {
    let plan = plan(&request(), &printer(), 600).unwrap();
    assert_eq!(plan.segments.len(), 1);
    assert_eq!(
        plan.command(&plan.segments[0], "test-20261101T121400Z"),
        "TEST_RESONANCES AXIS=X POINT=150.5,154,20 OUTPUT=raw_data NAME=test-20261101T121400Z \
         CHIPS=\"adxl345,lis2dw ebb36\""
    );
}

#[test]
fn an_override_adds_only_its_parameter_and_the_record_uses_it() {
    let mut r = request();
    r.overrides.freq_end = Some(100.0);
    let plan = plan(&r, &printer(), 600).unwrap();
    let command = plan.command(&plan.segments[0], "t");
    assert!(command.ends_with("FREQ_END=100"), "{command}");
    assert!(!command.contains("FREQ_START") && !command.contains("HZ_PER_SEC"));
    let recorded = plan.sweep_settings();
    assert_eq!(recorded.freq_end, 100.0);
    assert_eq!(recorded.freq_start, 5.0);
    assert_eq!(recorded.accel_per_hz, 60.0);
    // A pulse test stores no sweeping fields.
    assert_eq!(
        (recorded.sweeping_accel, recorded.sweeping_period),
        (None, None)
    );
}

#[test]
fn two_points_and_two_axes_give_four_segments_in_order() {
    let mut r = request();
    r.axes = vec![Axis::X, Axis::Y];
    r.points = Some(vec![[100.0, 100.0, 20.0], [200.0, 150.0, 20.0]]);
    let plan = plan(&r, &printer(), 600).unwrap();
    let order: Vec<(usize, Axis)> = plan.segments.iter().map(|s| (s.point, s.axis)).collect();
    assert_eq!(
        order,
        [(0, Axis::X), (0, Axis::Y), (1, Axis::X), (1, Axis::Y)]
    );
    assert!(
        plan.command(&plan.segments[3], "t")
            .contains("AXIS=Y POINT=200,150,20")
    );
}

#[test]
fn pulse_mode_switches_off_a_printer_period_above_zero_and_adds_nothing_otherwise() {
    let r = request();
    let on_sweeper = plan(&r, &sweeping_printer(), 600).unwrap();
    assert!(
        on_sweeper
            .command(&on_sweeper.segments[0], "t")
            .ends_with("SWEEPING_PERIOD=0")
    );
    let on_pulser = plan(&r, &printer(), 600).unwrap();
    assert!(
        !on_pulser
            .command(&on_pulser.segments[0], "t")
            .contains("SWEEPING")
    );
}

#[test]
fn sweep_mode_records_its_sweeping_settings_and_sends_none_by_default() {
    let mut r = request();
    r.mode = Mode::Sweep;
    let plan = plan(&r, &sweeping_printer(), 600).unwrap();
    assert!(!plan.command(&plan.segments[0], "t").contains("SWEEPING"));
    let recorded = plan.sweep_settings();
    assert_eq!(recorded.sweeping_accel, Some(400.0));
    assert_eq!(recorded.sweeping_period, Some(1.2));
}

#[test]
fn a_chip_name_with_a_quote_or_a_newline_is_refused() {
    for bad in ["bad\"name", "a\nTURN_OFF_HEATERS", "a,b", ""] {
        let mut r = request();
        r.chips = vec![bad.to_owned()];
        assert!(refused(&r, &printer()).starts_with("chips:"), "{bad:?}");
    }
    let mut r = request();
    r.chips = vec!["adxl345".to_owned()];
    let plan = plan(&r, &printer(), 600).unwrap();
    assert!(
        plan.command(&plan.segments[0], "t")
            .ends_with("CHIPS=adxl345")
    );
}

#[test]
fn each_range_violation_names_its_field() {
    type Set = fn(&mut Overrides);
    let cases: [(&str, Set); 4] = [
        ("hz_per_sec", |o| o.hz_per_sec = Some(2.5)),
        ("freq_end", |o| o.freq_end = Some(301.0)),
        ("freq_start", |o| o.freq_start = Some(0.5)),
        ("sweeping_period", |o| o.sweeping_period = Some(-1.0)),
    ];
    for (field, set) in cases {
        let mut r = request();
        set(&mut r.overrides);
        let message = refused(&r, &sweeping_printer());
        assert!(message.starts_with(&format!("{field}:")), "{message}");
    }
}

#[test]
fn a_point_outside_the_travel_names_the_axis_and_the_limit() {
    let mut r = request();
    r.points = Some(vec![[150.0, 400.0, 20.0]]);
    let message = refused(&r, &printer());
    assert!(
        message.contains("Y 400") && message.contains("0 to 308"),
        "{message}"
    );
}

#[test]
fn sweep_mode_with_no_effective_period_is_refused_naming_it() {
    let mut r = request();
    r.mode = Mode::Sweep;
    let message = refused(&r, &printer());
    assert!(message.starts_with("sweeping_period:"), "{message}");
    r.overrides.sweeping_period = Some(1.2);
    plan(&r, &printer(), 600).unwrap();
}

#[test]
fn a_segment_over_the_bound_is_refused_with_its_computed_length() {
    let mut r = request();
    r.overrides.freq_end = Some(300.0);
    r.overrides.freq_start = Some(1.0);
    r.overrides.hz_per_sec = Some(0.5);
    // (300 - 1) / 0.5 + 15 = 613 s
    assert!(plan(&r, &printer(), 700).is_ok());
    let message = refused(&r, &printer());
    assert!(message.contains("613"), "{message}");
}

#[test]
fn an_unreadable_needed_setting_blocks_until_it_is_overridden() {
    let mut query = fixture("real_settings.json");
    query["status"]["configfile"]["settings"]["resonance_tester"]
        .as_object_mut()
        .unwrap()
        .remove("max_freq");
    let unreadable = parse_settings(&query, &fixture("real_printer_info.json")).unwrap();
    let mut r = request();
    assert!(refused(&r, &unreadable).starts_with("freq_end:"));
    r.overrides.freq_end = Some(100.0);
    plan(&r, &unreadable, 600).unwrap();
}

#[test]
fn a_request_needs_chips_and_axes_and_points() {
    let mut r = request();
    r.chips.clear();
    assert!(refused(&r, &printer()).starts_with("chips:"));
    let mut r = request();
    r.axes.clear();
    assert!(refused(&r, &printer()).starts_with("axes:"));
    let mut r = request();
    r.points = Some(vec![]);
    assert!(refused(&r, &printer()).starts_with("points:"));
}

#[test]
fn the_request_json_has_only_the_given_overrides() {
    let r: TestRequest = serde_json::from_str(
        r#"{"chips":["adxl345"],"axes":["X","Y"],"mode":"Pulse","overrides":{"freq_end":100}}"#,
    )
    .unwrap();
    assert_eq!(r.overrides.freq_end, Some(100.0));
    assert_eq!(r.overrides.freq_start, None);
    assert_eq!(r.points, None);
}
