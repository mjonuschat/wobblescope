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
