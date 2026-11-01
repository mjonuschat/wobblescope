use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wobblescope::config::{default_data_dir, load};

const MIB: u64 = 1024 * 1024;

fn load_text(text: &str) -> Result<wobblescope::config::Settings, String> {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("wobblescope.toml");
    std::fs::write(&path, text).unwrap();
    load(Some(&path)).map_err(|e| e.to_string())
}

fn refused_naming(text: &str, key: &str) {
    let err = load_text(text).expect_err("config must be refused");
    assert!(err.contains(key), "error does not name {key}: {err}");
}

#[test]
fn an_empty_file_gives_the_defaults() {
    let s = load_text("").unwrap();
    assert_eq!(s.storage_dir, None);
    assert_eq!(s.retention_cap_bytes, 2048 * MIB);
    assert_eq!(s.free_space_headroom_bytes, 256 * MIB);
}

#[test]
fn set_keys_are_converted_to_bytes() {
    let s = load_text(
        "storage_dir = \"/var/lib/ws\"\nretention_cap_mib = 512\nfree_space_headroom_mib = 0\n",
    )
    .unwrap();
    assert_eq!(s.storage_dir, Some(PathBuf::from("/var/lib/ws")));
    assert_eq!(s.retention_cap_bytes, 512 * MIB);
    assert_eq!(s.free_space_headroom_bytes, 0);
}

#[test]
fn an_unknown_key_is_refused_by_name() {
    refused_naming("retention_cap_mb = 100\n", "retention_cap_mb");
}

#[test]
fn a_zero_cap_is_refused_by_name() {
    refused_naming("retention_cap_mib = 0\n", "retention_cap_mib");
}

#[test]
fn a_cap_of_the_wrong_type_is_refused_by_name() {
    refused_naming("retention_cap_mib = \"big\"\n", "retention_cap_mib");
    refused_naming("retention_cap_mib = -1\n", "retention_cap_mib");
}

#[test]
fn a_relative_storage_dir_is_refused_by_name() {
    refused_naming("storage_dir = \"data\"\n", "storage_dir");
}

#[test]
fn a_missing_file_is_an_error_naming_the_path_and_no_path_gives_defaults() {
    let err = load(Some(Path::new("/nonexistent/ws.toml"))).unwrap_err();
    assert!(err.to_string().contains("/nonexistent/ws.toml"));
    let s = load(None).unwrap();
    assert_eq!(s.retention_cap_bytes, 2048 * MIB);
}

#[test]
fn the_default_data_dir_is_wobblescope_beside_the_config_root() {
    let want = PathBuf::from("/home/pi/printer_data/wobblescope");
    assert_eq!(
        default_data_dir(Path::new("/home/pi/printer_data/config")).unwrap(),
        want
    );
    assert_eq!(
        default_data_dir(Path::new("/home/pi/printer_data/config/")).unwrap(),
        want
    );
}

#[test]
fn a_config_root_without_a_parent_is_refused() {
    assert!(default_data_dir(Path::new("/")).is_err());
    assert!(default_data_dir(Path::new("")).is_err());
}

#[test]
fn the_moonraker_defaults_are_loopback_and_no_key() {
    let s = load_text("").unwrap();
    assert_eq!(s.moonraker_url.to_string(), "http://127.0.0.1:7125/");
    assert_eq!(s.moonraker_api_key, None);
    assert_eq!(s.moonraker_ws_url(), "ws://127.0.0.1:7125/websocket");
}

#[test]
fn the_url_path_is_moonrakers_route_prefix() {
    for url in [
        "http://printer.local:7125/moonraker/",
        "http://printer.local:7125/moonraker",
    ] {
        let s = load_text(&format!("moonraker_url = \"{url}\"\n")).unwrap();
        assert_eq!(
            s.moonraker_ws_url(),
            "ws://printer.local:7125/moonraker/websocket"
        );
    }
}

#[test]
fn a_url_without_a_port_or_with_an_ipv6_host_derives_its_websocket_url() {
    let s = load_text("moonraker_url = \"http://printer.local\"\n").unwrap();
    assert_eq!(s.moonraker_ws_url(), "ws://printer.local/websocket");
    let s = load_text("moonraker_url = \"http://[::1]:7125\"\n").unwrap();
    assert_eq!(s.moonraker_ws_url(), "ws://[::1]:7125/websocket");
}

#[test]
fn an_https_url_is_refused_as_not_supported() {
    let err = load_text("moonraker_url = \"https://printer.local:7130\"\n").unwrap_err();
    assert!(err.contains("moonraker_url"), "{err}");
    assert!(err.contains("https is not supported"), "{err}");
}

#[test]
fn urls_that_cannot_work_are_refused_by_name() {
    for url in [
        "ws://printer.local",
        "printer.local:7125",
        "http://pi:secret@printer.local",
        "http://printer.local/?a=1",
        "http://printer.local/#x",
    ] {
        refused_naming(&format!("moonraker_url = \"{url}\"\n"), "moonraker_url");
    }
}

#[test]
fn an_empty_api_key_is_refused_by_name() {
    refused_naming("moonraker_api_key = \"\"\n", "moonraker_api_key");
}

#[test]
fn the_api_key_never_appears_in_debug_output_or_errors() {
    let s = load_text("moonraker_api_key = \"s3cret-key\"\n").unwrap();
    assert_eq!(s.moonraker_api_key.as_deref(), Some("s3cret-key"));
    let debug = format!("{s:?}");
    assert!(!debug.contains("s3cret-key"), "{debug}");
    assert!(debug.contains("redacted"), "{debug}");
    let err = load_text(
        "moonraker_api_key = \"s3cret-key\"\nmoonraker_url = \"http://u:p@printer.local\"\n",
    )
    .unwrap_err();
    assert!(!err.contains("s3cret-key"), "{err}");
}

#[test]
fn a_misspelled_moonraker_key_is_refused_by_name() {
    refused_naming("moonraker_uri = \"http://x\"\n", "moonraker_uri");
}

#[test]
fn a_parse_error_names_the_file_and_line_but_never_echoes_the_line() {
    let err = load_text("moonraker_api_key = \"s3cret-key\" trailing\n").unwrap_err();
    assert!(err.contains("wobblescope.toml"), "{err}");
    assert!(err.contains("line 1"), "{err}");
    assert!(!err.contains("s3cret-key"), "{err}");
}

#[test]
fn a_wrongly_typed_api_key_is_refused_by_name() {
    refused_naming("moonraker_api_key = 5\n", "moonraker_api_key");
}

#[test]
fn max_segment_seconds_defaults_to_600_and_zero_is_refused() {
    assert_eq!(load_text("").unwrap().max_segment_seconds, 600);
    assert_eq!(
        load_text("max_segment_seconds = 90\n")
            .unwrap()
            .max_segment_seconds,
        90
    );
    refused_naming("max_segment_seconds = 0\n", "max_segment_seconds");
}
