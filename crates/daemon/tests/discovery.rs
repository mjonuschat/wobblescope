use std::path::PathBuf;

use serde_json::{Value, json};
use wobblescope::moonraker::chips::{Availability, ChipType};
use wobblescope::moonraker::discovery::{
    ChipList, ConfigfileError, RootsError, parse_config_root, parse_configfile,
};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/data/moonraker/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A query result whose raw config holds `sections`.
fn query(sections: Value) -> Value {
    json!({"status": {"configfile": {"config": sections}}})
}

fn parse(sections: Value) -> ChipList {
    parse_configfile(&query(sections)).unwrap()
}

fn only_chip(sections: Value) -> wobblescope::moonraker::chips::Chip {
    let mut list = parse(sections);
    assert_eq!(list.chips.len(), 1, "{list:?}");
    list.chips.remove(0)
}

#[test]
fn a_plain_adxl345_uses_the_mux_key_and_the_default_rate() {
    let chip = only_chip(json!({"adxl345": {"cs_pin": "PA4"}}));
    assert_eq!(chip.section, "adxl345");
    assert_eq!(chip.chip_type, ChipType::Adxl345);
    assert_eq!(chip.sensor.as_deref(), Some("adxl345"));
    assert_eq!(chip.dump_endpoint, "adxl345/dump_adxl345");
    assert_eq!(chip.nominal_rate_hz, Some(3200.0));
    assert_eq!(chip.availability, Availability::Unchecked);
}

#[test]
fn a_named_adxl345_takes_its_rate_option_and_an_unparseable_rate_gives_none() {
    let chip = only_chip(json!({"adxl345 nh36": {"rate": "1600"}}));
    assert_eq!(chip.sensor.as_deref(), Some("nh36"));
    assert_eq!(chip.nominal_rate_hz, Some(1600.0));
    let chip = only_chip(json!({"adxl345 nh36": {"rate": "fast"}}));
    assert_eq!(chip.nominal_rate_hz, None);
}

#[test]
fn a_lis2dw_runs_at_1600_hz() {
    let chip = only_chip(json!({"lis2dw ebb36": {}}));
    assert_eq!(chip.chip_type, ChipType::Lis2dw);
    assert_eq!(chip.sensor.as_deref(), Some("ebb36"));
    assert_eq!(chip.dump_endpoint, "lis2dw/dump_lis2dw");
    assert_eq!(chip.nominal_rate_hz, Some(1600.0));
}

#[test]
fn a_plain_beacon_has_no_sensor_and_no_rate() {
    let chip = only_chip(json!({"beacon": {}}));
    assert_eq!(chip.chip_type, ChipType::Beacon);
    assert_eq!(chip.dump_endpoint, "beacon/dump_accel");
    assert_eq!(chip.sensor, None);
    assert_eq!(chip.nominal_rate_hz, None);
}

#[test]
fn a_beacon_reads_its_axes_from_accel_axes_map() {
    let chip = only_chip(json!({"beacon": {"accel_axes_map": "x, z, -y", "axes_map": "y,x,z"}}));
    assert_eq!(chip.axes_map.as_deref(), Some("x,z,-y"));
}

#[test]
fn a_section_name_with_several_words_is_kept_and_the_last_word_is_the_sensor() {
    let chip = only_chip(json!({"adxl345 tool head": {}}));
    assert_eq!(chip.section, "adxl345 tool head");
    assert_eq!(chip.sensor.as_deref(), Some("head"));
}

#[test]
fn a_beacon_model_is_neither_a_chip_nor_skipped() {
    let list = parse(json!({"beacon model default": {}}));
    assert!(list.chips.is_empty());
    assert!(list.skipped.is_empty());
}

#[test]
fn sections_the_mvp_cannot_use_are_skipped_with_a_reason() {
    let list = parse(json!({
        "beacon sensor left": {},
        "lis3dh": {},
        "mpu9250 bed": {},
        "icm20948": {},
    }));
    assert!(list.chips.is_empty());
    let skipped: Vec<(&str, &str)> = list
        .skipped
        .iter()
        .map(|s| (s.section.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 4);
    for (section, reason) in skipped {
        let word = section.split(' ').next().unwrap();
        assert!(
            reason.to_lowercase().contains(word) || reason.contains("Beacon"),
            "{section}: {reason}"
        );
    }
}

#[test]
fn unrelated_sections_are_ignored_and_the_chips_around_them_still_parse() {
    let list = parse(json!({
        "adxl345": {},
        "stepper_x": {"step_pin": "PF13"},
        "gcode_macro PRINT_START": {"gcode": "G".repeat(200 * 1024)},
        "printer": {"kinematics": "corexy"},
        "lis2dw ebb36": {},
    }));
    let sections: Vec<&str> = list.chips.iter().map(|c| c.section.as_str()).collect();
    assert_eq!(sections, ["adxl345", "lis2dw ebb36"]);
    assert!(list.skipped.is_empty());
}

#[test]
fn accel_chips_wins_and_puts_every_listed_chip_on_both_axes() {
    let list = parse_configfile(&fixture("configfile_kalico_multi.json")).unwrap();
    let both = vec!["adxl345 nh36".to_owned(), "lis2dw ebb36".to_owned()];
    assert_eq!(list.defaults.x, both);
    assert_eq!(list.defaults.y, both);
    let sections: Vec<&str> = list.chips.iter().map(|c| c.section.as_str()).collect();
    assert_eq!(sections, ["adxl345 nh36", "beacon", "lis2dw ebb36"]);
    assert_eq!(list.skipped.len(), 1);
    assert_eq!(list.skipped[0].section, "lis3dh bed");
}

#[test]
fn per_axis_chips_set_their_own_axis() {
    let list = parse_configfile(&fixture("configfile_mainline_xy.json")).unwrap();
    assert_eq!(list.defaults.x, ["adxl345 nh36"]);
    assert_eq!(list.defaults.y, ["lis2dw ebb36"]);
}

#[test]
fn per_axis_names_are_trimmed() {
    let list = parse(json!({
        "adxl345": {},
        "resonance_tester": {"accel_chip_x": " adxl345 ", "accel_chip_y": " adxl345 "},
    }));
    assert_eq!(list.defaults.x, ["adxl345"]);
    assert_eq!(list.defaults.y, ["adxl345"]);
}

#[test]
fn an_x_default_without_a_y_default_gives_x_only() {
    let list = parse(json!({
        "adxl345": {},
        "resonance_tester": {"accel_chip_x": "adxl345"},
    }));
    assert_eq!(list.defaults.x, ["adxl345"]);
    assert!(list.defaults.y.is_empty());
}

#[test]
fn accel_chip_alone_covers_both_axes_and_no_resonance_tester_gives_none() {
    let list = parse(json!({
        "adxl345": {},
        "resonance_tester": {"accel_chip": "adxl345"},
    }));
    assert_eq!(list.defaults.x, ["adxl345"]);
    assert_eq!(list.defaults.y, ["adxl345"]);
    let list = parse(json!({"adxl345": {}}));
    assert!(list.defaults.x.is_empty() && list.defaults.y.is_empty());
}

#[test]
fn a_default_naming_a_skipped_section_is_left_out() {
    let list = parse(json!({
        "adxl345": {},
        "lis3dh": {},
        "resonance_tester": {"accel_chips": "lis3dh, adxl345"},
    }));
    assert_eq!(list.defaults.x, ["adxl345"]);
    assert_eq!(list.defaults.y, ["adxl345"]);
}

#[test]
fn the_roots_fixture_gives_the_config_root() {
    assert_eq!(
        parse_config_root(&fixture("files_roots.json")).unwrap(),
        PathBuf::from("/home/pi/printer_data/config")
    );
}

#[test]
fn roots_without_config_or_not_a_list_are_errors() {
    let no_config = json!([{"name": "gcodes", "path": "/g", "permissions": "rw"}]);
    assert_eq!(parse_config_root(&no_config), Err(RootsError::NoConfigRoot));
    assert_eq!(
        parse_config_root(&json!({"name": "config"})),
        Err(RootsError::NotAList)
    );
}

#[test]
fn a_query_without_configfile_or_config_is_an_error_not_an_empty_list() {
    assert_eq!(
        parse_configfile(&json!({"status": {}})),
        Err(ConfigfileError::NoConfig)
    );
    assert_eq!(
        parse_configfile(&json!({"status": {"configfile": {"settings": {}}}})),
        Err(ConfigfileError::NoConfig)
    );
}

#[test]
fn a_real_printers_config_gives_its_two_chips_and_the_accel_chip_default() {
    // Recorded from a real Moonraker, trimmed to a few sections around the accelerometers.
    let list = parse_configfile(&fixture("real_configfile.json")).unwrap();
    let sections: Vec<&str> = list.chips.iter().map(|c| c.section.as_str()).collect();
    assert_eq!(sections, ["adxl345", "lis2dw ebb36"]);
    assert_eq!(list.chips[0].nominal_rate_hz, Some(3200.0));
    assert_eq!(list.chips[1].sensor.as_deref(), Some("ebb36"));
    assert!(list.skipped.is_empty());
    assert_eq!(list.defaults.x, ["lis2dw ebb36"]);
    assert_eq!(list.defaults.y, ["lis2dw ebb36"]);
}

#[test]
fn a_real_beacon_config_gives_one_chip_with_its_axis_map_and_the_default() {
    // Recorded from a real Moonraker, trimmed to the `[beacon]` and `[resonance_tester]` sections.
    let list = parse_configfile(&fixture("real_configfile_beacon.json")).unwrap();
    assert_eq!(list.chips.len(), 1, "{list:?}");
    let chip = &list.chips[0];
    assert_eq!(chip.chip_type, ChipType::Beacon);
    assert_eq!(chip.sensor, None);
    assert_eq!(chip.axes_map.as_deref(), Some("x,z,-y"));
    assert!(list.skipped.is_empty());
    assert_eq!(list.defaults.x, ["beacon"]);
    assert_eq!(list.defaults.y, ["beacon"]);
}

#[test]
fn a_real_printers_file_roots_give_the_config_root() {
    assert_eq!(
        parse_config_root(&fixture("real_files_roots.json")).unwrap(),
        PathBuf::from("/home/pi/printer_data/config")
    );
}

#[test]
fn a_chip_is_wired_to_the_mcu_its_pin_or_i2c_option_names() {
    let mcu = |section: Value| only_chip(section).mcu;
    // A pin on another MCU is `<mcu>:<pin>`, with or without a space after the colon.
    assert_eq!(
        mcu(json!({"lis2dw ebb36": {"cs_pin": "toolhead:LIS2DW_CS"}})).as_deref(),
        Some("toolhead")
    );
    assert_eq!(
        mcu(json!({"lis2dw ebb36": {"cs_pin": "ebb36: PA4"}})).as_deref(),
        Some("ebb36")
    );
    // A bare pin is on the main MCU, and so is a chip that names nothing.
    assert_eq!(
        mcu(json!({"adxl345": {"cs_pin": "PA4"}})).as_deref(),
        Some("mcu")
    );
    assert_eq!(mcu(json!({"adxl345": {}})).as_deref(), Some("mcu"));
    assert_eq!(
        mcu(json!({"lis2dw": {"i2c_mcu": "rp2040"}})).as_deref(),
        Some("rp2040")
    );
    // A Beacon is its own device.
    assert_eq!(mcu(json!({"beacon": {}})), None);
}

#[test]
fn the_axes_map_is_kept_as_configured_without_spaces_or_left_unset() {
    let map = |section: Value| only_chip(section).axes_map;
    assert_eq!(
        map(json!({"adxl345": {"axes_map": "x, z, -y"}})).as_deref(),
        Some("x,z,-y")
    );
    assert_eq!(map(json!({"adxl345": {}})), None);
    assert_eq!(map(json!({"adxl345": {"axes_map": "  "}})), None);
}
