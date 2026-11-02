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
fn the_release_info_names_the_project_asset_and_version_tag() {
    let value: serde_json::Value =
        serde_json::from_str(&release_info("wobblescope-aarch64", "0.1.0")).unwrap();
    assert_eq!(value["project_owner"], "mjonuschat");
    assert_eq!(value["project_name"], "wobblescope");
    assert_eq!(value["asset_name"], "wobblescope-aarch64");
    assert_eq!(value["version"], "v0.1.0");
}

const BLOCK_DIR: &str = "/home/pi/wobblescope";

#[test]
fn a_block_is_added_on_its_own_line_and_adding_it_twice_changes_nothing() {
    let block = update_block(Path::new(BLOCK_DIR));
    let once = with_block("[server]\nhost: 0.0.0.0", &block).unwrap();
    assert_eq!(once, format!("[server]\nhost: 0.0.0.0\n\n{block}"));
    assert!(block.contains("path: /home/pi/wobblescope\n"));
    assert!(block.contains("type: executable\nchannel: stable\nrepo: mjonuschat/wobblescope\n"));
    assert!(block.contains("managed_services: wobblescope\n"));
    assert_eq!(with_block(&once, &block), None);
}

#[test]
fn a_block_that_differs_is_replaced_and_the_text_around_it_is_untouched() {
    let old = update_block(Path::new("/old"));
    let new = update_block(Path::new(BLOCK_DIR));
    let conf = format!("[a]\nx: 1\n\n{old}\n[b]\ny: 2\n");
    let updated = with_block(&conf, &new).unwrap();
    assert_eq!(updated, format!("[a]\nx: 1\n\n{new}\n[b]\ny: 2\n"));
}

#[test]
fn removing_the_block_restores_the_original_text() {
    let block = update_block(Path::new(BLOCK_DIR));
    for original in ["[server]\nhost: x\n", "[a]\n\n", "", "[a]\nk: v\n\n[b]\n"] {
        let added = with_block(original, &block).unwrap();
        assert_eq!(
            without_block(&added).as_deref(),
            Some(original),
            "{original:?}"
        );
    }
    assert_eq!(without_block("[server]\n"), None);
}

#[test]
fn a_section_the_user_wrote_is_not_touched_by_removal() {
    let own = "[update_manager wobblescope]\ntype: web\n";
    assert_eq!(without_block(own), None);
    assert!(has_own_section(own));
    let block = update_block(Path::new(BLOCK_DIR));
    assert!(!has_own_section(&with_block("", &block).unwrap()));
    assert!(has_own_section(&with_block(own, &block).unwrap()));
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
    let (text, mode) = config_file(Some("http://127.0.0.1:7126"), Some("k\"ey"));
    assert_eq!(mode, 0o600);
    let parsed: toml::Table = text.parse().unwrap();
    assert_eq!(parsed["moonraker_api_key"].as_str(), Some("k\"ey"));
    assert_eq!(
        parsed["moonraker_url"].as_str(),
        Some("http://127.0.0.1:7126")
    );
}

#[test]
fn the_asset_name_follows_the_architecture() {
    assert_eq!(asset_for_arch("aarch64"), Some("wobblescope-aarch64"));
    assert_eq!(asset_for_arch("arm"), Some("wobblescope-armv7"));
    assert_eq!(asset_for_arch("x86_64"), Some("wobblescope-x86_64"));
    assert_eq!(asset_for_arch("riscv64"), None);
}
