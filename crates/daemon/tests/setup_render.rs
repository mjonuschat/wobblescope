use std::path::Path;

use wobblescope::setup::render::*;

#[test]
fn the_unit_quotes_and_escapes_paths_and_orders_after_moonraker() {
    let text = unit(
        "pi",
        Path::new("/home/pi/my wobble%scope/wobblescope-aarch64"),
        Path::new("/home/pi/printer_data/config/wobblescope.toml"),
    );
    assert!(text.contains("User=pi\n"));
    assert!(text.contains("ExecStart=\"/home/pi/my wobble%%scope/wobblescope-aarch64\"\n"));
    assert!(text.contains(
        "Environment=\"WOBBLESCOPE_CONFIG=/home/pi/printer_data/config/wobblescope.toml\""
    ));
    assert!(text.contains("After=moonraker.service\nWants=moonraker.service\n"));
    assert!(text.contains("Restart=always\nRestartSec=5\n"));
    assert!(text.contains("WantedBy=multi-user.target\n"));
}

#[test]
fn the_service_list_edits_keep_order_and_ignore_repeats() {
    assert_eq!(
        with_service("a\nb", "wobblescope").unwrap(),
        "a\nb\nwobblescope\n"
    );
    assert_eq!(with_service("a\nwobblescope\n", "wobblescope"), None);
    assert_eq!(
        without_service("a\nwobblescope\nb\n", "wobblescope").unwrap(),
        "a\nb\n"
    );
    assert_eq!(without_service("a\n", "wobblescope"), None);
}

#[test]
fn the_config_text_and_mode_follow_the_key() {
    let (text, mode) = config_file(None, None);
    assert_eq!(mode, 0o644);
    assert!(!text.contains("moonraker_api_key"));
    let (text, mode) = config_file(Some("http://127.0.0.1:7140"), Some("k\"ey"));
    assert_eq!(mode, 0o600);
    let parsed: toml::Table = text.parse().unwrap();
    assert_eq!(parsed["moonraker_api_key"].as_str(), Some("k\"ey"));
    assert_eq!(
        parsed["moonraker_url"].as_str(),
        Some("http://127.0.0.1:7140")
    );
}
